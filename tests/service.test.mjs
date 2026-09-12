import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'
import os from 'node:os'
import {execFile} from 'node:child_process'
import {promisify} from 'node:util'
import {create} from '../qml/lib/Service.mjs'
import * as Notes from '../qml/lib/Notes.mjs'
import * as Settings from '../qml/lib/Settings.mjs'
import * as Theme from '../qml/lib/Theme.mjs'
const execute = promisify(execFile)

async function fixture(t) {
    const dir = await fs.mkdtemp(path.join(os.tmpdir(),'omatate-service-'))
    t.after(async () => {
        // Finish any analysis job the test left running so its poll loop ends before the files go away.
        const workers=dir+'/runtime/omatate/workers'
        for (const name of await fs.readdir(workers).catch(()=>[])) await fs.writeFile(workers+'/'+name+'/status','1\n',{flag:'wx'}).catch(()=>{})
        await new Promise(r=>setTimeout(r,100))
        await fs.rm(dir,{recursive:true,force:true})
    })
    const environment = {HOME:dir,XDG_RUNTIME_DIR:dir+'/runtime',XDG_CONFIG_HOME:dir+'/.config',XDG_STATE_HOME:dir+'/.local/state'}
    await fs.mkdir(environment.XDG_RUNTIME_DIR)
    const warnings=[], panels=[]
    let sequence=0, focused=false, failure=null, stub=null, delays=true
    const io={
        async read(file,optional) { try { return await fs.readFile(file,'utf8') } catch(error) { if(optional&&error.code==='ENOENT') return null;throw error } },
        async write(file,text) {
            if(failure&&failure(file,text)) throw new Error('Injected write failure')
            const tmp=file+'.test-tmp'
            await fs.writeFile(tmp,text);await fs.rename(tmp,file)
        },
        async run(args,options={}) {
            if(stub) { const result=await stub(args,options);if(result) return result }
            try { const result=await execute(args[0],args.slice(1),{cwd:options.cwd||dir,timeout:options.timeout||10000});return {code:0,...result} }
            catch(error) { if(options.allowFailure) return {code:error.code||1,stdout:error.stdout||'',stderr:error.stderr||''};throw error }
        },
        delay: ms=>new Promise(resolve=>setTimeout(resolve,delays?ms:1)),
        uniqueId: ()=>'test-'+(++sequence), resource:p=>path.resolve(p), detach:()=>{}
    }
    let latest
    const hooks={changed:s=>latest=s,warning:m=>warnings.push(m),panel:cmd=>{panels.push(cmd);return Promise.resolve({ok:true})},noteFocused:()=>focused,isOpen:()=>panels.at(-1)==='open'}
    const service=create(io,n=>environment[n]||'',hooks)
    await service.start()
    const project=dir+'/project';await fs.mkdir(project)
    const owner=()=>({session:latest.session,token:latest.token})
    return {dir,service,project,owner,io,environment,hooks,warnings,panels,get state(){return latest},
        fail:fn=>failure=fn,stub:fn=>stub=fn,focus:()=>focused=true,fast:()=>delays=false}
}

test('Markdown matches the project format, including sections and AI context',()=>{
    assert.equal(Notes.render('/projects/20260910-130000-demo',[
        {id:1,kind:'section',title:'Header'},
        {id:2,ai:false,transcript:'Fix spacing',asset:'assets/clip.png'},
        {id:3,context:{title:'Login',summary:'Form'},transcript:'First\nSecond'}
    ]),'# UI review — demo · 2026-09-10\n\n## Header\n\n![Screen capture](assets/clip.png)\n\nFix spacing\n\n### 3. Login\nForm\n\n> First\n> Second\n')
})
test('bad records and unsafe ids are rejected',()=>{
    for(const text of ['{}','{"id":1}\n{"id":1}','{"id":9007199254740992}','not json']) assert.throws(()=>Notes.parseEntries(text))
    assert.throws(()=>Notes.nextId([{id:Number.MAX_SAFE_INTEGER}]))
})
test('keyboard config accepts overrides, comments and multiline arrays',()=>{
    const keys=Settings.keys("[keys]\nsearch = [\n'<Control>g', # comment\n]\nprojects=[]\n")
    assert.deepEqual(keys.search,['Ctrl+G']);assert.deepEqual(keys.projects,[])
    assert.deepEqual(keys.expand_note,['Shift+Alt+Return'])
    for(const text of ["[keys]\nsearch=['Ctrl+N']","[keys]\nsearch=['x']","[keys]\nsearch=['Ctrl+Escape']","[keys]\nsearch=['Ctrl+F']\nsearch=[]"]) assert.throws(()=>Settings.keys(text))
})
test('Ghostty palette preserves terminal typography and alpha',()=>{
    const theme=Theme.resolve('', '', 'background = #111111\nforeground = #eeeeee\nfont-family = Example\nfont-size = 13\nselection-background = #aabbcc88\npalette = 4=#55aaff', '')
    assert.equal(theme.fontPointSize,13);assert.equal(theme.smallPointSize,13);assert.equal(theme.accent,'#55aaff');assert.equal(theme.selectionBg,'#88aabbcc')
})
test('Omarchy accent survives a different Ghostty blue',()=>{
    const palette='background = "#05182e"\nforeground = "#f6dcac"\naccent = "#faa968"'
    const terminal='background = #05182e\nforeground = #f6dcac\npalette = 4=#3f8f8a'
    const theme=Theme.resolve(palette,'',terminal,'')
    assert.equal(theme.accent,'#faa968')
    assert.equal(theme.background,'#05182e')
})
test('new project, concurrent saves, sections, deletion and restart',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project])
    await Promise.all(Array.from({length:20},(_,i)=>f.service.request('note',{...f.owner(),text:'Note '+i})))
    assert.equal(f.state.entries.length,20);assert.equal(new Set(f.state.entries.map(e=>e.id)).size,20)
    await f.service.command(['section','Second'])
    await f.service.request('note',{...f.owner(),text:'Last'})
    await f.service.request('delete-section',{...f.owner(),entryId:21})
    assert.equal(f.state.entries.at(-1).transcript,'Last')
    await f.service.request('draft',{...f.owner(),text:'Unfinished\nλ'})
    const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    assert.equal(restarted.snapshot().draft,'Unfinished\nλ');assert.equal(restarted.snapshot().entries.length,21)
})
test('switch rejects stale owners and preserves each project draft',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);const old=f.owner()
    await f.service.request('draft',{...old,text:'First draft'})
    const next=f.dir+'/second';await fs.mkdir(next)
    await f.service.command(['select-project',next,f.project])
    await assert.rejects(f.service.request('note',{...old,text:'wrong project'}),/changed/)
    await f.service.command(['select-project',f.project,next]);assert.equal(f.state.draft,'First draft')
})
test('existing unrelated files and invalid project layouts survive rejection',async t=>{
    const f=await fixture(t);await fs.writeFile(f.project+'/notes.md','User document')
    await assert.rejects(f.service.command(['open-project',f.project]),/already contains/)
    assert.equal(await fs.readFile(f.project+'/notes.md','utf8'),'User document')
    await assert.rejects(f.service.command(['create-project',f.dir,'../escape','']),/single nonempty/)
})
test('failed draft clear rolls back note insertion and allows one retry',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.request('draft',{...f.owner(),text:'Keep me'})
    f.fail((file,text)=>file.endsWith('/draft.txt')&&text==='')
    await assert.rejects(f.service.request('note',{...f.owner(),text:'Keep me'}),/Injected/)
    assert.equal(await fs.readFile(f.project+'/.data/entries.jsonl','utf8'),'')
    assert.equal(f.state.draft,'Keep me');f.fail(null)
    await f.service.request('note',{...f.owner(),text:'Keep me'});assert.equal(f.state.entries.length,1)
})
test('Markdown failure warns after a successful save without duplicating it',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);f.fail(file=>file.endsWith('/notes.md'))
    await f.service.request('note',{...f.owner(),text:'Saved'})
    assert.equal(f.state.entries.length,1);assert.match(f.warnings.at(-1),/Entries saved/)
})
test('failed note write retains editor draft and records',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.request('draft',{...f.owner(),text:'Keep draft'})
    f.fail((file,text)=>file.endsWith('/entries.jsonl')&&text.includes('Keep draft'))
    await assert.rejects(f.service.request('note',{...f.owner(),text:'Keep draft'}));assert.equal(f.state.draft,'Keep draft')
})
test('replacing a project identity blocks delayed writes',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await fs.writeFile(f.project+'/.data/id','replacement\n')
    await assert.rejects(f.service.request('draft',{...f.owner(),text:'stale'}),/identity changed/)
})
test('AI is opt-in and optional tools are not used for plain notes',async t=>{
    const f=await fixture(t);f.stub(async args=>{if(['codex','voxtype','grim','slurp'].includes(args[0])) throw new Error('Unexpected optional tool')})
    assert.equal(await f.service.command(['ai']),'off');await f.service.command(['open-project',f.project])
    await f.service.request('note',{...f.owner(),text:'Plain'})
    assert.equal(await f.service.command(['ai','on']),'on');assert.equal(await f.service.command(['ai','off']),'off')
})
test('capture cancellation restores the panel and leaves no entry',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);f.fast()
    f.stub(async args=>args[0]==='slurp'?{code:1,stdout:'',stderr:'selection cancelled'}:null)
    await f.service.command(['clip']);assert.equal(f.state.entries.length,0);assert.deepEqual(f.panels.slice(-2),['hide','show'])
})
test('capture persists relative asset and dimensions',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);f.fast()
    f.stub(async args=>{
        if(args[0]==='slurp') return {code:0,stdout:'-40,20 100x80\n',stderr:''}
        if(args[0]==='grim') {await fs.writeFile(args.at(-1),'fake png');return {code:0,stdout:'',stderr:''}}
    })
    await f.service.command(['clip']);assert.deepEqual(f.state.entries[0].asset_size,{width:100,height:80})
    assert.equal(await fs.readFile(f.project+'/'+f.state.entries[0].asset,'utf8'),'fake png')
})
test('dictation results return to their owning note and block switches in flight',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);f.focus();f.fast()
    let transcript
    f.stub(async args=>{
        if(args[0]==='voxtype') {
            if(args[2]==='start') transcript=args[3].slice('--file='.length)
            if(args[2]==='stop') {await fs.writeFile(transcript,'Voice note');await fs.mkdir(f.environment.XDG_RUNTIME_DIR+'/voxtype',{recursive:true});await fs.writeFile(f.environment.XDG_RUNTIME_DIR+'/voxtype/state','idle')}
            return {code:0,stdout:'',stderr:''}
        }
    })
    await f.service.command(['ptt','start'])
    await assert.rejects(f.service.command(['select-project',f.dir,f.project]),/Finish recording/)
    await f.service.command(['ptt','stop'])
    for(let i=0;i<100&&f.state.entries[0].status!=='done';i++) await new Promise(r=>setTimeout(r,10))
    assert.equal(f.state.entries[0].transcript,'Voice note');assert.equal(f.state.entries[0].status,'done')
})
test('relocation preserves unrelated files and rejects occupied destinations',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.request('note',{...f.owner(),text:'Moving'})
    await fs.writeFile(f.project+'/unrelated.txt','Keep');const dest=f.dir+'/destination';await fs.mkdir(dest)
    await fs.writeFile(dest+'/notes.md','Occupied');await assert.rejects(f.service.command(['relocate',dest]),/already contains/)
    await fs.unlink(dest+'/notes.md');await f.service.command(['relocate',dest]);assert.equal(f.state.session,dest)
    assert.equal(await fs.readFile(f.project+'/unrelated.txt','utf8'),'Keep');assert.match(await fs.readFile(dest+'/notes.md','utf8'),/Moving/)
})

test('save receipt prevents duplicate draft recovery after interrupted cleanup',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.request('draft',{...f.owner(),text:'Only once'})
    let insertionWritten=false
    f.fail((file,text)=>{
        if(file.endsWith('/entries.jsonl')&&text.includes('Only once')) insertionWritten=true
        return (file.endsWith('/draft.txt')&&text==='') || (insertionWritten&&file.endsWith('/entries.jsonl')&&text==='')
    })
    await f.service.request('note',{...f.owner(),text:'Only once'});assert.equal(f.state.entries.length,1)
    f.fail(null);const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    assert.equal(restarted.snapshot().draft,'');assert.equal(restarted.snapshot().entries.length,1)
    await restarted.request('draft',{...f.owner(),text:'Only once'})
    assert.equal((await restarted.request('snapshot',{})).draft,'Only once')
})
test('a failed draft save keeps the receipt that suppresses duplicate recovery',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.request('draft',{...f.owner(),text:'Only once'})
    let insertionWritten=false
    f.fail((file,text)=>{
        if(file.endsWith('/entries.jsonl')&&text.includes('Only once')) insertionWritten=true
        return (file.endsWith('/draft.txt')&&text==='') || (insertionWritten&&file.endsWith('/entries.jsonl')&&text==='')
    })
    await f.service.request('note',{...f.owner(),text:'Only once'})
    f.fail(file=>file.endsWith('/draft.txt'))
    await assert.rejects(f.service.request('draft',{...f.owner(),text:'Replacement'}),/Injected/)
    f.fail(null);const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    assert.equal(restarted.snapshot().draft,'');assert.equal(restarted.snapshot().entries.length,1)
})
test('invalid command arity cannot create project files',async t=>{
    const f=await fixture(t)
    await assert.rejects(f.service.command(['create-project',f.dir,'mistake']),/Invalid arguments/)
    await assert.rejects(fs.stat(f.dir+'/mistake'),{code:'ENOENT'})
})
test('missing active project does not prevent opening another folder',async t=>{
    const f=await fixture(t);await fs.writeFile(f.environment.XDG_RUNTIME_DIR+'/omatate/session',f.dir+'/gone\n')
    const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    await restarted.command(['open-project',f.project]);assert.equal(restarted.snapshot().session,f.project)
})
test('corrupt registry is preserved while notes remain usable',async t=>{
    const f=await fixture(t);await fs.mkdir(f.environment.XDG_STATE_HOME+'/omatate',{recursive:true})
    const registry=f.environment.XDG_STATE_HOME+'/omatate/projects.json';await fs.writeFile(registry,'broken')
    const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    await restarted.command(['open-project',f.project]);assert.equal(await fs.readFile(registry,'utf8'),'broken')
})

// Starts an AI note with stubbed capture tools and finishes its transcription.
// Returns the pending entry and the detached analysis command.
async function startAnalysis(f) {
    await f.service.command(['open-project',f.project]);await f.service.command(['ai','on']);f.focus();f.fast()
    let analysisArgs
    f.io.detach=args=>{if(args[3]==='omatate-analysis') analysisArgs=args}
    f.stub(async args=>{
        if(args[0]==='voxtype') return {code:0,stdout:'',stderr:''}
        if(args[0]==='hyprctl') return {code:0,stdout:JSON.stringify(args[1]==='monitors'?[{name:'TEST',focused:true}]:{class:'test',title:'Window'}),stderr:''}
        if(args[0]==='grim') {await fs.writeFile(args.at(-1),'fake png');return {code:0,stdout:'',stderr:''}}
    })
    await f.service.command(['ptt','start'])
    const pending=f.state.entries[0]
    await fs.writeFile(f.project+'/.data/transcripts/001.txt','Spoken note')
    await fs.mkdir(f.environment.XDG_RUNTIME_DIR+'/voxtype',{recursive:true});await fs.writeFile(f.environment.XDG_RUNTIME_DIR+'/voxtype/state','idle')
    await f.service.command(['ptt','stop'])
    for(let i=0;i<100&&f.state.entries[0].status!=='done';i++) await new Promise(r=>setTimeout(r,10))
    return {pending,analysisArgs}
}
async function fileWhen(file) {
    for(let i=0;i<200;i++) {try {return await fs.readFile(file,'utf8')} catch(error) {if(error.code!=='ENOENT') throw error;await new Promise(r=>setTimeout(r,10))}}
    return fs.readFile(file,'utf8')
}
async function warningWhen(f,text) {
    for(let i=0;i<200&&!f.warnings.some(w=>w.includes(text));i++) await new Promise(r=>setTimeout(r,10))
    return f.warnings.some(w=>w.includes(text))
}
async function entriesWhen(f,done) {
    let entries
    for(let i=0;i<200;i++) {entries=Notes.parseEntries(await fs.readFile(f.project+'/.data/entries.jsonl','utf8'));if(done(entries)) break;await new Promise(r=>setTimeout(r,10))}
    return entries
}
// Runs the real detached wrapper with a fake analysis command in place of codex,
// against a job directory the service is not following.
let isolated=0
async function isolatedJob(f) {
    const dir=f.environment.XDG_RUNTIME_DIR+'/omatate/workers/job-isolated'+(++isolated)
    await fs.mkdir(dir,{recursive:true,mode:0o700});return {dir,output:dir+'/output.json',status:dir+'/status',log:dir+'/log'}
}
function runWrapper(analysisArgs,command,job) {
    const [sh,flag,script,name,root,,input]=analysisArgs
    return execute(sh,[flag,script,name,root,path.basename(job.dir),input,'sh','-c',command]).then(()=>0,error=>error.code)
}

test('AI capture uses read-only Codex, returns context, and survives a project switch',async t=>{
    const f=await fixture(t);const {pending,analysisArgs}=await startAnalysis(f)
    assert.ok(analysisArgs.includes('read-only'));assert.ok(analysisArgs.includes('codex'))
    const other=f.dir+'/other';await fs.mkdir(other);await f.service.command(['select-project',other,f.project])
    const context={title:'Test screen',summary:'A form',regions:[],notable:[]}
    await fs.writeFile(pending.analysis.log,'codex said hi\n')
    await fs.writeFile(pending.analysis.output,JSON.stringify(context));await fs.writeFile(pending.analysis.status,'0\n')
    const entries=await entriesWhen(f,e=>e[0].context_status==='done')
    assert.deepEqual(entries[0].context,context);assert.equal(entries[0].transcript,'Spoken note')
    assert.equal(f.state.session,other);assert.equal(f.state.entries.length,0)
    assert.equal(await fileWhen(f.project+'/.data/logs/analyze-001.log'),'codex said hi\n')
    for(let i=0;i<100;i++) {try {await fs.stat(pending.analysis.dir)} catch(error) {break} await new Promise(r=>setTimeout(r,10))}
    await assert.rejects(fs.stat(pending.analysis.dir),'the private job directory is removed after publication')
})
test('detached analysis only writes inside its private runtime job directory',async t=>{
    const f=await fixture(t);const {pending,analysisArgs}=await startAnalysis(f)
    const workers=f.environment.XDG_RUNTIME_DIR+'/omatate/workers/'
    assert.match(pending.analysis.dir,new RegExp('^'+workers.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')+'job-[A-Za-z0-9]+$'))
    assert.equal((await fs.stat(pending.analysis.dir)).mode&0o777,0o700)
    assert.ok(!analysisArgs.some(a=>a.includes(f.project)&&!a.startsWith('-C')&&a!==f.project),'the wrapper receives the project only as the codex -C argument')
    assert.equal(analysisArgs.indexOf(f.project),analysisArgs.indexOf('-C')+1)
    const job=await isolatedJob(f)
    assert.equal(await runWrapper(analysisArgs,'cat >/dev/null; echo hello; echo oops >&2; exit 3',job),0)
    assert.equal(await fs.readFile(job.log,'utf8'),'hello\noops\n');assert.equal(await fs.readFile(job.status,'utf8'),'3\n')
    assert.deepEqual((await fs.readdir(job.dir)).sort(),['log','status'],'no predictable temporary name is left behind')
    assert.equal(await runWrapper(analysisArgs,'exit 0',pending.analysis),0)
    const entries=await entriesWhen(f,e=>e[0].context_status==='error')
    assert.equal(entries[0].context_status,'error');assert.ok(await warningWhen(f,'Analysis failed'))
    assert.equal(await fileWhen(f.project+'/.data/logs/analyze-001.log'),'')
    assert.equal((await fs.stat(f.project+'/.data/logs/analyze-001.log')).mode&0o777,0o600)
})
test('detached analysis refuses symlinked log and status targets',async t=>{
    const f=await fixture(t);const {analysisArgs}=await startAnalysis(f);const job=await isolatedJob(f)
    const victimLog=f.dir+'/victim-log',victimStatus=f.dir+'/victim-status'
    await fs.writeFile(victimLog,'keep');await fs.writeFile(victimStatus,'keep')
    await fs.symlink(victimLog,job.log);await fs.symlink(victimStatus,job.status)
    await runWrapper(analysisArgs,'exit 0',job)
    assert.equal(await fs.readFile(victimLog,'utf8'),'keep');assert.equal(await fs.readFile(victimStatus,'utf8'),'keep')
    assert.ok((await fs.lstat(job.status)).isFile(),'the status rename replaces the planted symlink instead of following it')
    assert.ok((await fs.lstat(job.log)).isSymbolicLink());assert.notEqual(await fs.readFile(job.status,'utf8'),'0\n')
    // A symlink to a directory at the status name is replaced, not entered.
    const decoy=f.dir+'/decoy';await fs.mkdir(decoy);await fs.rm(job.status);await fs.symlink(decoy,job.status)
    await fs.rm(job.log);await runWrapper(analysisArgs,'exit 0',job)
    assert.deepEqual(await fs.readdir(decoy),[]);assert.ok((await fs.lstat(job.status)).isFile());assert.equal(await fs.readFile(job.status,'utf8'),'0\n')
    // A symlink planted at the job directory name itself is rejected by the pwd -P check.
    const planted=f.dir+'/planted';await fs.mkdir(planted)
    await fs.rm(job.dir,{recursive:true});await fs.symlink(planted,job.dir)
    assert.equal(await runWrapper(analysisArgs,'echo escaped > escaped',job),1)
    assert.deepEqual(await fs.readdir(planted),[])
})
test('analysis log publication never follows symlinks in the project',async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    const victim=f.dir+'/victim';await fs.writeFile(victim,'keep')
    await fs.symlink(victim,f.project+'/.data/logs/analyze-001.log')
    await fs.writeFile(pending.analysis.log,'log text\n');await fs.writeFile(pending.analysis.status,'1\n')
    await entriesWhen(f,e=>e[0].context_status==='error')
    for(let i=0;i<100&&(await fs.lstat(f.project+'/.data/logs/analyze-001.log')).isSymbolicLink();i++) await new Promise(r=>setTimeout(r,10))
    assert.equal(await fs.readFile(victim,'utf8'),'keep')
    assert.ok((await fs.lstat(f.project+'/.data/logs/analyze-001.log')).isFile())
    assert.equal(await fs.readFile(f.project+'/.data/logs/analyze-001.log','utf8'),'log text\n')
    assert.deepEqual((await fs.readdir(f.project+'/.data/logs')).filter(n=>n.startsWith('.publish')),[])
})
test('analysis log publication refuses a symlinked log directory and a changed project identity',async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    const outside=f.dir+'/outside';await fs.mkdir(outside)
    await fs.rm(f.project+'/.data/logs',{recursive:true});await fs.symlink(outside,f.project+'/.data/logs')
    await fs.writeFile(pending.analysis.log,'log text\n');await fs.writeFile(pending.analysis.status,'1\n')
    await entriesWhen(f,e=>e[0].context_status==='error')
    assert.ok(await warningWhen(f,'Cannot save the analysis log'));assert.deepEqual(await fs.readdir(outside),[])
})
test('analysis context publication refuses a symlinked context directory',async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    const outside=f.dir+'/outside';await fs.mkdir(outside)
    await fs.rm(f.project+'/.data/context',{recursive:true});await fs.symlink(outside,f.project+'/.data/context')
    await fs.writeFile(pending.analysis.output,JSON.stringify({title:'T',summary:'S',regions:[],notable:[]}));await fs.writeFile(pending.analysis.status,'0\n')
    const entries=await entriesWhen(f,e=>e[0].context_status!=='pending')
    assert.equal(entries[0].context_status,'error');assert.equal(entries[0].context,undefined);assert.deepEqual(await fs.readdir(outside),[])
    assert.ok(await warningWhen(f,'Analysis failed'))
})
test('analysis publication verifies .data before creating a missing target directory',async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    // .data is swapped for a symlink after the project was opened; its token still matches.
    const outside=f.dir+'/outside';await fs.rename(f.project+'/.data',outside);await fs.symlink(outside,f.project+'/.data')
    await fs.rm(outside+'/logs',{recursive:true})
    await fs.writeFile(pending.analysis.log,'log text\n');await fs.writeFile(pending.analysis.status,'1\n')
    assert.ok(await warningWhen(f,'Cannot save the analysis log'));await assert.rejects(fs.stat(outside+'/logs'))
})
for (const status of ['0', '1']) test('analysis completion preserves records after .data becomes a symlink, status '+status,async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    const outside=f.dir+'/outside';await fs.rename(f.project+'/.data',outside);await fs.symlink(outside,f.project+'/.data')
    const before=await fs.readFile(outside+'/entries.jsonl','utf8')
    await fs.writeFile(pending.analysis.output,JSON.stringify({title:'T',summary:'S',regions:[],notable:[]}))
    await fs.writeFile(pending.analysis.log,'log text\n');await fs.writeFile(pending.analysis.status,status+'\n')
    const refused=await warningWhen(f,'Project directory changed')
    assert.equal(await fs.readFile(outside+'/entries.jsonl','utf8'),before,'completion must not rewrite records through the rejected directory')
    assert.ok(refused,'completion reports the invalid project directory')
    assert.deepEqual(await fs.readdir(outside+'/context'),[])
    assert.deepEqual(await fs.readdir(outside+'/logs'),[])
})
test('analysis log publication re-reads the project identity right before renaming',async t=>{
    const f=await fixture(t);const {pending}=await startAnalysis(f)
    // Change the project identity after the service's own check but before the publish script runs.
    const previous=f.io.run
    f.io.run=async(args,options)=>{if(args[3]==='omatate-publish') await fs.writeFile(f.project+'/.data/id','replaced\n');return previous(args,options)}
    await fs.writeFile(pending.analysis.log,'log text\n');await fs.writeFile(pending.analysis.status,'1\n')
    assert.ok(await warningWhen(f,'Cannot save the analysis log'))
    assert.deepEqual(await fs.readdir(f.project+'/.data/logs'),[])
})
test('shell restart reconnects to analysis completion files',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project])
    const dir=f.environment.XDG_RUNTIME_DIR+'/omatate/workers/job-resumed'
    await fs.mkdir(dir,{recursive:true})
    const context={title:'Recovered',summary:'Completed during reload',regions:[],notable:[]}
    const entry={id:1,ts:Notes.timestamp(),status:'done',transcript:'Preserved',context_status:'pending',shot:f.environment.XDG_RUNTIME_DIR+'/omatate/shots/test.png',
        analysis:{dir,output:dir+'/output.json',status:dir+'/status',log:dir+'/log',started:Date.now()}}
    await fs.writeFile(f.project+'/.data/entries.jsonl',Notes.encodeEntries([entry]));await fs.writeFile(dir+'/output.json',JSON.stringify(context));await fs.writeFile(dir+'/status','0\n')
    const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    for(let i=0;i<100&&restarted.snapshot().entries[0].context_status!=='done';i++) await new Promise(r=>setTimeout(r,10))
    assert.deepEqual(restarted.snapshot().entries[0].context,context)
})
