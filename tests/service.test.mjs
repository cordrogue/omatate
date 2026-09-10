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
    t.after(() => fs.rm(dir,{recursive:true,force:true}))
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

test('AI capture uses read-only Codex, returns context, and survives a project switch',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project]);await f.service.command(['ai','on']);f.focus();f.fast()
    let analysisArgs
    f.io.detach=args=>{if(args[3]==='omatate-analysis') analysisArgs=args}
    f.stub(async args=>{
        if(args[0]==='voxtype') return {code:0,stdout:'',stderr:''}
        if(args[0]==='hyprctl') return {code:0,stdout:JSON.stringify(args[1]==='monitors'?[{name:'TEST',focused:true}]:{class:'test',title:'Window'}),stderr:''}
        if(args[0]==='grim') {await fs.writeFile(args.at(-1),'fake png');return {code:0,stdout:'',stderr:''}}
    })
    await f.service.command(['ptt','start'])
    assert.ok(analysisArgs.includes('read-only'));assert.ok(analysisArgs.includes('codex'))
    const pending=f.state.entries[0]
    // Complete transcription so selecting another project is allowed while AI runs.
    await fs.writeFile(f.project+'/.data/transcripts/001.txt','Spoken note')
    await fs.mkdir(f.environment.XDG_RUNTIME_DIR+'/voxtype',{recursive:true});await fs.writeFile(f.environment.XDG_RUNTIME_DIR+'/voxtype/state','idle')
    await f.service.command(['ptt','stop'])
    for(let i=0;i<100&&f.state.entries[0].status!=='done';i++) await new Promise(r=>setTimeout(r,10))
    const other=f.dir+'/other';await fs.mkdir(other);await f.service.command(['select-project',other,f.project])
    const context={title:'Test screen',summary:'A form',regions:[],notable:[]}
    await fs.writeFile(pending.analysis.output,JSON.stringify(context));await fs.writeFile(pending.analysis.status,'0\n')
    let entries
    for(let i=0;i<100;i++) {entries=Notes.parseEntries(await fs.readFile(f.project+'/.data/entries.jsonl','utf8'));if(entries[0].context_status==='done') break;await new Promise(r=>setTimeout(r,10))}
    assert.deepEqual(entries[0].context,context);assert.equal(entries[0].transcript,'Spoken note')
    assert.equal(f.state.session,other);assert.equal(f.state.entries.length,0)
})
test('shell restart reconnects to analysis completion files',async t=>{
    const f=await fixture(t);await f.service.command(['open-project',f.project])
    const output=f.environment.XDG_RUNTIME_DIR+'/omatate/workers/resumed.json'
    await fs.mkdir(path.dirname(output),{recursive:true})
    const context={title:'Recovered',summary:'Completed during reload',regions:[],notable:[]}
    const entry={id:1,ts:Notes.timestamp(),status:'done',transcript:'Preserved',context_status:'pending',shot:f.environment.XDG_RUNTIME_DIR+'/omatate/shots/test.png',
        analysis:{output,status:output+'.status',started:Date.now()}}
    await fs.writeFile(f.project+'/.data/entries.jsonl',Notes.encodeEntries([entry]));await fs.writeFile(output,JSON.stringify(context));await fs.writeFile(output+'.status','0\n')
    const restarted=create(f.io,n=>f.environment[n]||'',f.hooks);await restarted.start()
    for(let i=0;i<100&&restarted.snapshot().entries[0].context_status!=='done';i++) await new Promise(r=>setTimeout(r,10))
    assert.deepEqual(restarted.snapshot().entries[0].context,context)
})
