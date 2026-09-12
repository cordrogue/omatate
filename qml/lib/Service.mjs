import * as Notes from './Notes.mjs'
import * as Settings from './Settings.mjs'

// One instance owns all writes. Slow external work runs outside this queue and
// returns an immutable project path + identity token with its result.
export function create(io, env, hooks) {
    var home = env('HOME'), runtime = env('XDG_RUNTIME_DIR') + '/omatate'
    var config = (env('XDG_CONFIG_HOME') || home + '/.config') + '/omatate'
    var registry = (env('XDG_STATE_HOME') || home + '/.local/state') + '/omatate/projects.json'
    var state = {session:'', token:'', entries:[], draft:'', projects:[], ai:false, keys:Settings.keys(''), keysWarning:'', runtimeDir:runtime}
    var queue = Promise.resolve(), initialized = false, recording = null, jobs = {}, transcriptJobs = {}, startupError = '', registryError = ''
    var service = {}
    function run(args, options) { return io.run(args, options) }
    function exists(path) { return run(['test', '-e', path], {allowFailure:true}).then(r => r.code === 0) }
    function isLink(path) { return run(['test', '-L', path], {allowFailure:true}).then(r => r.code === 0) }
    function mkdir(path) { return run(['mkdir', '-p', '--', path]) }
    function remove(path) { return run(['rm', '-f', '--', path]) }
    function serial(items, fn) { return items.reduce((p, item) => p.then(() => fn(item)), Promise.resolve()) }
    function assertPath(path) { if (typeof path !== 'string' || path[0] !== '/' || path.indexOf('\0') >= 0) throw new Error('Expected an absolute path'); return path }
    function canonical(path) {
        if (path === '~') path = home
        else if (path.indexOf('~/') === 0) path = home + path.slice(1)
        assertPath(path)
        return run(['realpath', '-e', '--', path]).then(r => r.stdout.replace(/[\r\n]+$/, ''))
    }
    function directory(path) { return run(['test', '-d', path]).then(() => path) }
    function owner() { return {session:state.session, token:state.token} }
    function checkOwner(expected) {
        if (!expected.session || !expected.token) return Promise.reject(new Error('Inactive project'))
        // Read the token from the verified directory, including when handling
        // failed analysis publication. A matching token behind a symlink does
        // not authorize a completion write through that symlink.
        return run(['sh', '-c',
            'set -eu; cd -- "$1"; [ "$(pwd -P)" = "$1" ] || { printf "%s\\n" "Project directory changed; refusing a stale write" >&2; exit 1; }; cat -- id',
            'omatate-owner', expected.session + '/.data']).then(function(result) {
            if (result.stdout.trim() !== expected.token) throw new Error('Project identity changed; refusing a stale write')
        })
    }
    function checkActive(expected) {
        if (state.session !== expected.session || state.token !== expected.token) return Promise.reject(new Error('Active project changed; reload before saving'))
        return checkOwner(expected)
    }
    function snapshot() { return Notes.clone(state) }
    function publish() { hooks.changed(snapshot()) }
    function enqueue(fn) {
        var result = queue.then(function() { if (startupError) throw new Error(startupError); return fn() })
        queue = result.then(function() {}, function() {})
        return result
    }
    function readProject(path) {
        return Promise.all([io.read(path + '/.data/id'), io.read(path + '/.data/entries.jsonl'),
            io.read(path + '/.data/draft.txt', true), io.read(path + '/.data/pending-note.json', true)]).then(function(values) {
            if (!values[0].trim()) throw new Error('Empty project identity')
            var entries=Notes.parseEntries(values[1]), draft=values[2] || ''
            if (values[3]) {
                var receipt=JSON.parse(values[3])
                if (receipt.token === values[0].trim() && receipt.text === draft
                        && entries.some(e => e.id === receipt.id && e.transcript === receipt.text)) draft=''
            }
            return {session:path, token:values[0].trim(), entries:entries, draft:draft}
        })
    }
    function remember(path) {
        if (registryError) { hooks.warning(registryError); return Promise.resolve() }
        state.projects = [path].concat(state.projects.filter(p => p !== path))
        return mkdir(registry.slice(0, registry.lastIndexOf('/')))
            .then(() => io.write(registry, JSON.stringify(state.projects) + '\n'))
            .catch(error => hooks.warning('Project saved, but cannot remember it: ' + error.message))
    }
    function refreshSettings() {
        return Promise.all([io.read(config + '/ai', true).catch(() => null), io.read(config + '/keys.toml', true)]).then(function(values) {
            state.ai = (values[0] || '').trim() === 'on'
            try { state.keys = Settings.keys(values[1] || ''); state.keysWarning = '' }
            catch (error) { state.keys = Settings.keys(''); state.keysWarning = 'Invalid keyboard config: ' + error.message + '. Using defaults.' }
        }).catch(function(error) { state.keys = Settings.keys(''); state.keysWarning = error.message })
    }
    function commit(expected, entries) {
        return checkOwner(expected).then(() => io.write(expected.session + '/.data/entries.jsonl', Notes.encodeEntries(entries)))
            .then(function() {
                if (state.session === expected.session && state.token === expected.token) state.entries = entries
                return io.write(expected.session + '/notes.md', Notes.render(expected.session, entries))
                    .catch(error => hooks.warning('Entries saved, but cannot render notes.md: ' + error.message))
            }).then(publish)
    }
    function updateEntry(expected, id, update) {
        return enqueue(function() {
            return checkOwner(expected).then(() => io.read(expected.session + '/.data/entries.jsonl')).then(function(text) {
                var entries = Notes.parseEntries(text), entry = entries.find(e => e.id === id)
                if (!entry) throw new Error('Entry disappeared')
                update(entry)
                return commit(expected, entries)
            })
        })
    }
    function idleForSwitch() {
        if (recording || state.entries.some(e => ['recording','transcribing'].indexOf(e.status) >= 0))
            throw new Error('Finish recording and transcription before switching projects')
    }
    function select(base, name, expected, resumeOnly) {
        if (expected !== undefined && expected !== state.session) throw new Error('Active project changed; reload projects')
        idleForSwitch()
        if (name !== undefined && !Notes.validName(name)) throw new Error('Project name must be a single nonempty folder name')
        var path, created = false
        return canonical(base).then(directory).then(function(parent) {
            path = name === undefined ? parent : parent + '/' + name
            if (expected === undefined && state.session && state.session !== path) throw new Error('Another session is active; stop it before opening this project')
            if (name !== undefined) return run(['mkdir', '--', path])
        }).then(function() {
            return Promise.all([exists(path + '/.data'), isLink(path + '/.data'), exists(path + '/notes.md'), isLink(path + '/notes.md')])
        }).then(function(found) {
            if (found[0] || found[1]) return readProject(path)
            if (resumeOnly) throw new Error('Missing .data/entries.jsonl')
            if (found[2] || found[3]) throw new Error('Project already contains notes.md without a valid notes session')
            // mkdir without -p reserves the metadata directory. Never overwrite another creator.
            return run(['mkdir', '--', path + '/.data']).then(function() {
                created = true
                return serial(['transcripts','context','logs'], n => mkdir(path + '/.data/' + n))
            }).then(() => io.write(path + '/.data/id', io.uniqueId() + '\n'))
                .then(() => io.write(path + '/.data/entries.jsonl', ''))
                .then(() => io.write(path + '/notes.md', Notes.render(path, [])))
                .then(() => readProject(path))
        }).then(function(project) {
            return io.write(runtime + '/session', path + '\n').then(function() {
                Object.assign(state, project)
                return recover(project).then(() => remember(path))
            })
        }).then(function() { publish(); return path })
            .catch(function(error) {
                // A partial new layout is retained for recovery; no existing project is removed.
                if (created) error.message += '. New project files were retained at ' + path
                throw error
            })
    }
    function recover(project) {
        var changed = false, transcribing = [], analyses = []
        var pending = project.entries.filter(e => (e.status === 'recording' || e.status === 'transcribing') && !transcriptJobs[workerKey(project,e.id)])
        return (pending.length ? io.read(env('XDG_RUNTIME_DIR') + '/voxtype/state',true) : Promise.resolve(null)).then(function(voxtype) {
            return serial(pending,function(entry) {
                var transcript = project.session + '/.data/transcripts/' + String(entry.id).padStart(3,'0') + '.txt'
                var record = {owner:{session:project.session,token:project.token},id:entry.id,transcript:transcript}
                if (entry.status === 'recording' && (voxtype || '').trim() === 'recording' && project.session === state.session) {
                    recording = record
                    return
                }
                return io.read(transcript,true).then(function(text) {
                    if ((text || '').trim()) { entry.transcript = text.trim(); entry.status = 'done' }
                    else { entry.status = 'transcribing'; transcribing.push(record) }
                    changed = true
                })
            })
        }).then(function() {
            project.entries.forEach(function(entry) {
                if (entry.context_status !== 'pending') return
                if (validAnalysis(entry.analysis)) analyses.push(entry)
                else { entry.context_status = 'error'; changed = true }
            })
            return changed ? commit(project,project.entries) : undefined
        }).then(function() {
            transcribing.forEach(ingest)
            analyses.forEach(entry => followAnalysis({session:project.session,token:project.token},entry))
        })
    }
    function initialize() {
        if (!env('XDG_RUNTIME_DIR')) return Promise.reject(new Error('XDG_RUNTIME_DIR is required'))
        return mkdir(runtime).then(() => run(['chmod','700','--',runtime]))
            .then(() => Promise.all([io.read(runtime + '/session', true), io.read(registry, true), refreshSettings()]))
            .then(function(values) {
                if (values[1]) {
                    try {
                        var paths = JSON.parse(values[1])
                        if (!Array.isArray(paths) || !paths.every(p => typeof p === 'string' && p[0] === '/')) throw new Error('Invalid project registry')
                        state.projects = paths
                    } catch (error) { registryError = 'Cannot update project list: ' + error.message; hooks.warning(registryError) }
                }
                var path = (values[0] || '').replace(/[\r\n]+$/, '')
                if (!path) return
                return canonical(path).then(readProject).then(function(project) { Object.assign(state, project); return recover(project) })
                    .catch(function(error) {
                        state.session='';state.token='';state.entries=[];state.draft=''
                        hooks.warning('Cannot restore the active project: '+error.message)
                    })
            }).then(function() {
                initialized = true; publish()
                state.projects.filter(path => path !== state.session).forEach(function(path) {
                    enqueue(() => readProject(path).then(recover)).catch(function() {})
                })
            })
    }
    function restore() {
        if (state.session || !state.projects.length) return Promise.resolve()
        return select(state.projects[0], undefined, undefined, true).catch(error => hooks.warning('Cannot restore the last project: ' + error.message))
    }
    function mutate(cmd, request) {
        var expected = {session:request.session, token:request.token}
        return checkActive(expected).then(function() {
            if (cmd === 'draft') {
                if (typeof request.text !== 'string') throw new Error('Missing draft text')
                // The receipt goes only after the new draft is on disk: a failed
                // write must not expose the previous note's text to draft recovery.
                return io.write(state.session + '/.data/draft.txt', request.text).then(() => remove(state.session+'/.data/pending-note.json')).then(function() { state.draft = request.text })
            }
            return io.read(state.session + '/.data/entries.jsonl').then(function(text) {
                var previous = Notes.parseEntries(text), entries = Notes.mutate(previous, cmd, request)
                if (cmd !== 'note') return commit(expected, entries)
                // A receipt prevents duplicate submission if the shell exits between
                // persisting the note and clearing its draft.
                var receipt=state.session+'/.data/pending-note.json',keepReceipt=false
                return io.write(receipt,JSON.stringify({id:entries[entries.length-1].id,text:request.text,token:expected.token})+'\n')
                    .then(function() {
                        return io.write(state.session + '/.data/entries.jsonl', Notes.encodeEntries(entries))
                            .then(() => io.write(state.session + '/.data/draft.txt', ''))
                            .catch(function(error) {
                                return io.write(state.session + '/.data/entries.jsonl',Notes.encodeEntries(previous))
                                    .then(function() { return remove(receipt).then(function() { throw error }) },function(rollback) {
                                        return io.read(state.session+'/.data/entries.jsonl').then(function(actual) {
                                            if (actual !== Notes.encodeEntries(entries)) throw error
                                            keepReceipt=true
                                            hooks.warning('Note saved, but draft cleanup failed. Its save receipt prevents duplicate recovery: '+rollback.message)
                                        })
                                    })
                            })
                    }).then(function() {
                        return keepReceipt ? undefined : remove(receipt).catch(error => hooks.warning('Cannot remove save receipt: '+error.message))
                    }).then(function() {
                        state.draft = ''; state.entries = entries
                        // Retaining the receipt is harmless; it matches only this
                        // exact project, note ID, and draft text.
                        return io.write(expected.session + '/notes.md', Notes.render(expected.session, entries))
                            .catch(error => hooks.warning('Entries saved, but cannot render notes.md: ' + error.message))
                    })

            })
        }).then(function() {
            // commit() already published edits and deletions.
            if (cmd === 'draft' || cmd === 'note') publish()
            return snapshot()
        })
    }
    function clip() {
        var expected = owner(), captured = '', asset = '', dimensions, hidden = false
        idleForSwitch()
        return checkActive(expected).then(() => hooks.panel('hide')).then(function() {
            hidden = true
            return io.delay(50)
        }).then(() => run(['slurp'], {allowFailure:true,timeout:120000})).then(function(result) {
            var region = result.stdout.trim()
            if (result.code !== 0 || !region) {
                if (result.stderr.trim() && result.stderr.indexOf('selection cancelled') < 0) throw new Error(result.stderr.trim())
                return
            }
            var match = /^-?\d+,-?\d+ (\d+)x(\d+)$/.exec(region)
            if (!match || +match[1] < 1 || +match[2] < 1) throw new Error('Invalid capture dimensions')
            dimensions = {width:+match[1],height:+match[2]}
            captured = runtime + '/captures/' + io.uniqueId() + '.png'
            return mkdir(runtime + '/captures').then(() => run(['grim','-g',region,captured]))
        }).then(function() { if (hidden) return hooks.panel('show') }).then(function() {
            hidden = false
            if (!captured) return
            return checkActive(expected).then(() => isLink(expected.session + '/assets')).then(function(link) {
                if (link) throw new Error('Project assets must be a directory inside the project')
                return mkdir(expected.session + '/assets')
            }).then(function() {
                asset = 'assets/clip-' + Notes.filenameStamp() + '-' + io.uniqueId() + '.png'
                return run(['cp','--no-clobber','--',captured,expected.session + '/' + asset])
            }).then(function() {
                var entries = Notes.clone(state.entries), id = Notes.nextId(entries)
                entries.push({id:id,ts:Notes.timestamp(),ai:false,status:'done',transcript:'',asset:asset,asset_size:dimensions})
                return commit(expected, entries).then(() => String(id))
            })
        }).then(function(result) { return captured ? remove(captured).then(() => result) : result }, function(error) {
            return (hidden ? hooks.panel('show') : Promise.resolve()).catch(function() {})
                .then(() => captured ? remove(captured) : undefined).then(function() { throw error })
        })
    }
    function workerKey(expected, id) { return expected.token + ':' + id }
    function validAnalysis(job) {
        if (!job || typeof job.dir !== 'string' || !Number.isFinite(job.started)) return false
        var prefix = runtime + '/workers/'
        return job.dir.indexOf(prefix) === 0 && /^job-[A-Za-z0-9]+$/.test(job.dir.slice(prefix.length))
            && job.output === job.dir + '/output.json' && job.status === job.dir + '/status' && job.log === job.dir + '/log'
    }
    function removeJob(job) {
        // Deletes relative to the verified workers directory, like the wrapper.
        return run(['sh','-c','root=$(cd -- "$1" && pwd -P) && cd -- "$root/omatate/workers" && [ "$(pwd -P)" = "$root/omatate/workers" ] && rm -rf -- "./$2"',
            'omatate-cleanup',env('XDG_RUNTIME_DIR'),job.dir.slice(job.dir.lastIndexOf('/')+1)])
    }
    function publishFile(expected, sub, name, source) {
        // Detached jobs only ever write inside their private runtime directory.
        // Results are copied into the project by the service: every directory
        // we own is entered and verified with pwd -P, the file is staged in a
        // private mktemp directory, the project token is read again right
        // before the atomic rename, and noclobber keeps every redirection an
        // exclusive create that never follows a symlink.
        return checkOwner(expected).then(() => run(['sh','-c',
            'set -Cu; umask 077; data=$1; sub=$2; name=$3; token=$4; source=$5; '
            + 'cd -- "$data" && [ "$(pwd -P)" = "$data" ] && mkdir -p -- "$sub" && cd -- "$sub" && [ "$(pwd -P)" = "$data/$sub" ] '
            + '&& stage=$(mktemp -d ./.publish.XXXXXX) && trap \'rm -rf -- "$stage"\' EXIT && cat -- "$source" > "$stage/file" '
            + '&& [ "$(cat ../id)" = "$token" ] && mv -fT -- "$stage/file" "$name"',
            'omatate-publish', expected.session + '/.data', sub, name, expected.token, source]))
    }
    function followAnalysis(expected, entry) {
        var id=entry.id,key=workerKey(expected,id),job=entry.analysis
        if (jobs[key]) return
        jobs[key]=true
        function poll() {
            return io.read(job.status,true).then(function(status) {
                if (status === null && Date.now()-job.started < 250000) return io.delay(250).then(poll)
                if (status === null) throw new Error('Analysis timed out')
                if (status.trim() !== '0') throw new Error('Analysis command failed; see the project analysis log')
                return io.read(job.output).then(Notes.context)
            })
        }
        // The log is published before the pending metadata is cleared, so a
        // reload in between can still reconnect to the job.
        function publishLog() {
            return publishFile(expected,'logs','analyze-'+String(id).padStart(3,'0')+'.log',job.log)
                .catch(error => hooks.warning('Cannot save the analysis log: '+error.message))
        }
        poll().then(function(context) {
            return io.write(job.dir+'/context.json',JSON.stringify(context,null,2)+'\n')
                .then(() => publishFile(expected,'context',String(id).padStart(3,'0')+'.json',job.dir+'/context.json'))
                .then(publishLog)
                .then(() => updateEntry(expected,id,function(e) { e.context=context;e.context_status='done';delete e.analysis }))
        }).catch(function(error) {
            return publishLog().then(() => updateEntry(expected,id,function(e) { delete e.context;delete e.analysis;e.context_status='error' }))
                .then(() => hooks.warning('Analysis failed: '+error.message))
        }).then(function() {
            delete jobs[key];if (validAnalysis(job)) removeJob(job).catch(function() {})
        },function(error) { delete jobs[key];hooks.warning(error.message) })
    }
    function analyze(expected, entry) {
        var job
        // Every file the detached job writes lives in a private directory that
        // mktemp creates exclusively (mode 700) under the runtime directory.
        return mkdir(runtime+'/workers').then(() => run(['mktemp','-d','--',runtime+'/workers/job-XXXXXXXX'])).then(function(result) {
            var dir=result.stdout.replace(/[\r\n]+$/,'')
            job={dir:dir,output:dir+'/output.json',status:dir+'/status',log:dir+'/log',started:Date.now()}
            if (!validAnalysis(job)) throw new Error('Unexpected analysis directory')
            entry.analysis=job
            var entries=Notes.clone(state.entries),target=entries.find(e=>e.id===entry.id)
            target.analysis=job
            return commit(expected,entries)
        }).then(function() {
            // The external tool survives shell reloads. Its completion file is
            // polled by the service, and all note writes still use our queue.
            // The wrapper works relative to its job directory, verified with
            // pwd -P against the canonical runtime root so no owned component
            // is a symlink. noclobber makes each redirection an exclusive
            // create that never follows a symlink; mv -T keeps the rename on
            // the status name itself. Nothing here writes into the project.
            io.detach(['sh','-c',
                'set -Cu; umask 077; root=$1; name=$2; input=$3; shift 3; '
                + 'root=$(cd -- "$root" && pwd -P) && cd -- "$root/omatate/workers/$name" && [ "$(pwd -P)" = "$root/omatate/workers/$name" ] || exit 1; '
                + '"$@" < "$input" > log 2>&1; result=$?; '
                + 'tmp=$(mktemp -u ./status.XXXXXX) && printf "%s\\n" "$result" > "$tmp" && mv -fT -- "$tmp" status',
                'omatate-analysis',env('XDG_RUNTIME_DIR'),job.dir.slice(job.dir.lastIndexOf('/')+1),io.resource('share/analyze-prompt.md'),
                'timeout','--kill-after=2s','240s','codex','exec','--skip-git-repo-check','--sandbox','read-only',
                '-C',expected.session,'-i',entry.shot,'--output-schema',io.resource('share/analyze-schema.json'),'-o',job.output,
                '-c','model_reasoning_effort='+JSON.stringify(env('OMATATE_REASONING')||'low'),
                '-m',env('OMATATE_MODEL')||'gpt-5.6-sol','-'])
            followAnalysis(expected,entry)
        })
    }
    function scheduleShotCleanup(path) {
        // Cleanup is independent of the QML object's lifetime, and only unlinks
        // the exact temporary capture this service created.
        var prefix=runtime+'/shots/'
        if (typeof path !== 'string' || path.indexOf(prefix)!==0 || !/^[A-Za-z0-9-]+\.png$/.test(path.slice(prefix.length))) return
        io.detach(['sh','-c','sleep 300; rm -f -- "$1"','omatate-cleanup',path])
    }
    function pttStart() {
        if (recording) throw new Error('Recording is already active')
        if (!state.session || !hooks.noteFocused()) return run(['voxtype','record','start']).then(() => '')
        var expected = owner(), id = Notes.nextId(state.entries), ai = state.ai
        var entry = {id:id,ts:Notes.timestamp(),status:'recording'}
        if (ai) entry.context_status = 'pending'; else entry.ai = false
        var transcript = expected.session + '/.data/transcripts/' + String(id).padStart(3,'0') + '.txt'
        return checkActive(expected).then(() => commit(expected,state.entries.concat([entry])))
            .then(() => run(['voxtype','record','start','--file=' + transcript]))
            .then(function() {
                recording = {owner:expected,id:id,transcript:transcript}
                if (!ai) return
                var hidden = false
                entry.shot = runtime + '/shots/' + io.uniqueId() + '.png'
                return mkdir(runtime + '/shots').then(() => run(['hyprctl','activewindow','-j']))
                    .then(function(result) { var window = JSON.parse(result.stdout); entry.window = {class:window.class || '',title:window.title || ''}; return run(['hyprctl','monitors','-j']) })
                    .then(function(result) {
                        var monitors = JSON.parse(result.stdout), monitor = monitors.find(m => m.focused) || monitors[0]
                        if (!monitor) throw new Error('No monitor')
                        return hooks.panel('hide').then(function() { hidden = true; return io.delay(50) })
                            .then(() => run(['grim','-o',monitor.name,entry.shot]))
                    }).then(() => hooks.panel('show')).then(function() {
                        hidden = false
                        scheduleShotCleanup(entry.shot)
                        var entries = Notes.clone(state.entries), target = entries.find(e => e.id === id)
                        target.shot = entry.shot; target.window = entry.window
                        return commit(expected,entries)
                    }).then(function() { return analyze(expected,entry) })
                    .catch(function(error) {
                        return (hidden ? hooks.panel('show') : Promise.resolve()).catch(function() {})
                            .then(function() { if (entry.shot) scheduleShotCleanup(entry.shot); throw error })
                    })
            }).then(() => String(id)).catch(function(error) {
                recording = null
                return run(['voxtype','record','cancel'],{allowFailure:true}).then(() => commit(expected,state.entries.filter(e => e.id !== id)))
                    .then(function() { throw error })
            })
    }
    function ingest(record) {
        var key=workerKey(record.owner,record.id)
        if (transcriptJobs[key]) return
        transcriptJobs[key]=true
        var started = Date.now(), idleSince = 0, expected = record.owner
        function poll() {
            return Promise.all([io.read(record.transcript,true), io.read(env('XDG_RUNTIME_DIR') + '/voxtype/state',true)])
                .then(function(values) {
                    var text = (values[0] || '').trim(), idle = (values[1] || '').trim() === 'idle'
                    if (idle && !idleSince) idleSince = Date.now()
                    if (!idle) idleSince = 0
                    if ((text && idle) || Date.now() - started >= 120000
                            || (idleSince && Date.now()-idleSince >= 1500 && Date.now()-started >= 3000))
                        return updateEntry(expected,record.id,function(e) { if (text && idle) e.transcript = text; e.status = text && idle ? 'done' : 'no-transcript' })
                    return io.delay(200).then(poll)
                })
        }
        poll().catch(error => updateEntry(expected,record.id,e => { e.status = 'no-transcript' }).then(() => hooks.warning(error.message)))
            .catch(error => hooks.warning(error.message)).then(function() { delete transcriptJobs[key] })
    }
    function pttStop() {
        if (!recording) return run(['voxtype','record','stop']).then(() => '')
        var record = recording; recording = null
        return run(['voxtype','record','stop']).then(function() {
            var entries = Notes.clone(state.entries); entries.find(e => e.id === record.id).status = 'transcribing'
            return commit(record.owner,entries).then(function() { ingest(record); return '' })
        }).catch(function(error) {
            var entries = Notes.clone(state.entries); entries.find(e => e.id === record.id).status = 'no-transcript'
            return commit(record.owner,entries).then(function() { throw error })
        })
    }
    function relocate(destination) {
        idleForSwitch()
        if (Object.keys(jobs).length) throw new Error('Finish analysis before relocating the project')
        var source = state.session, expected = owner(), moved = [], target
        return checkActive(expected).then(() => canonical(destination)).then(directory).then(function(path) {
            target = path
            if (path === source || path.indexOf(source + '/') === 0) throw new Error('Destination must not be inside the current project')
            return serial(['notes.md','.data','assets'], function(name) {
                return Promise.all([exists(target+'/'+name),isLink(target+'/'+name)]).then(function(found) {
                    if (found[0] || found[1]) throw new Error('Destination already contains '+name)
                })
            })
        }).then(() => run(['find',source+'/.data','-type','l','-print','-quit']))
            .then(function(result) { if (result.stdout) throw new Error('Project metadata contains a symlink'); return isLink(source+'/assets') })
            .then(function(link) { if (link) throw new Error('Project assets are a symlink'); return serial(['notes.md','.data','assets'], function(name) {
                return exists(source+'/'+name).then(function(found) {
                    if (!found) return
                    return run(['mv','--no-clobber','--',source+'/'+name,target+'/'+name]).then(function() {
                        return exists(source+'/'+name).then(function(remains) { if (remains) throw new Error('Destination changed during relocation'); moved.push(name) })
                    })
                })
            }) }).then(() => io.write(runtime+'/session',target+'\n'))
            .catch(function(error) {
                return serial(moved.reverse(), name => run(['mv','--no-clobber','--',target+'/'+name,source+'/'+name]))
                    .then(function() { throw error })
            }).then(function() { state.session=target; return remember(target) }).then(function() { publish(); return target })
    }
    function command(args) {
        if (!Array.isArray(args) || !args.every(arg => typeof arg === 'string')) throw new Error('Command arguments must be strings')
        var cmd = args[0] || 'open'
        var counts={open:[1], 'focus-toggle':[1],toggle:[1],panel:[2],status:[1],projects:[1],ai:[1,2],
            'open-project':[2],resume:[2],'select-project':[3],'create-project':[4],start:[1,2],stop:[1],ptt:[2],clip:[1],relocate:[2],render:[1],section:[1,2]}
        if (!counts[cmd] || counts[cmd].indexOf(args.length || 1)<0) throw new Error('Invalid arguments for '+cmd)
        if (cmd === 'open') return restore().then(() => hooks.panel('open')).then(() => '')
        if (cmd === 'focus-toggle') return restore().then(() => hooks.panel('toggle-focus')).then(() => '')
        if (cmd === 'toggle') return hooks.isOpen() ? command(['stop']) : command(['open'])
        if (cmd === 'panel') return hooks.panel(args[1]).then(result => JSON.stringify(result))
        if (cmd === 'status') return state.session || 'inactive'
        if (cmd === 'projects') return JSON.stringify(state.projects)
        if (cmd === 'ai') {
            var mode = args[1]
            if (mode === undefined) return state.ai ? 'on' : 'off'
            if (['on','off','toggle'].indexOf(mode) < 0) throw new Error('Expected ai on, off, or toggle')
            var enabled = mode === 'toggle' ? !state.ai : mode === 'on'
            return mkdir(config).then(() => io.write(config+'/ai',enabled ? 'on\n' : 'off\n'))
                .then(function() { state.ai=enabled; publish(); return enabled ? 'on' : 'off' })
        }
        if (cmd === 'open-project' || cmd === 'resume') return select(args[1],undefined,undefined,cmd === 'resume').then(path => hooks.panel('open').then(() => path))
        if (cmd === 'select-project') return select(args[1],undefined,args[2])
        if (cmd === 'create-project') return select(args[1],args[2],args[3])
        if (cmd === 'start') {
            if (state.session) return state.session
            var parent = home+'/Documents/omatate', name = Notes.filenameStamp()
            if (args[1]) {
                var suffix = args[1].replace(/[^A-Za-z0-9._-]+/g,'-').replace(/^[-.]+|[-.]+$/g,'')
                if (!suffix) throw new Error('Session name contains no usable characters')
                name += '-'+suffix
            }
            return mkdir(parent).then(() => exists(parent+'/'+name)).then(function(found) {
                return select(parent,found ? name+'-'+io.uniqueId() : name,undefined)
            }).then(path => hooks.panel('open').then(() => path))
        }
        if (cmd === 'stop') {
            idleForSwitch()
            return (state.session ? io.write(state.session+'/notes.md',Notes.render(state.session,state.entries)) : Promise.resolve())
                .then(() => remove(runtime+'/session')).then(function() {
                    state.session='';state.token='';state.entries=[];state.draft='';publish()
                    return hooks.panel('quit')
                }).then(() => '')
        }
        if (cmd === 'ptt' && args[1] === 'start') return pttStart()
        if (cmd === 'ptt' && args[1] === 'stop') return pttStop()
        if (!state.session) throw new Error('Inactive project')
        if (cmd === 'clip') return clip()
        if (cmd === 'relocate') return relocate(args[1])
        if (cmd === 'render') return io.write(state.session+'/notes.md',Notes.render(state.session,state.entries)).then(() => '')
        if (cmd === 'section') {
            var entries=Notes.clone(state.entries), id=Notes.nextId(entries)
            entries.push({id:id,kind:'section',ts:Notes.timestamp(),title:args[1] === undefined ? 'Section '+(entries.filter(e=>e.kind==='section').length+1) : args[1]})
            return commit(owner(),entries).then(() => String(id))
        }
        throw new Error('Unknown command: '+cmd)
    }
    service.start = function() {
        queue = initialize().catch(function(error) { startupError=error.message; hooks.warning(error.message) })
        return queue.then(function() { if (startupError) throw new Error(startupError) })
    }
    service.request = function(cmd, data) {
        return enqueue(function() {
            if (!initialized) throw new Error('Notes are not ready')
            // A snapshot request waits for earlier queued writes; state is in memory.
            if (cmd === 'snapshot') return snapshot()
            return mutate(cmd,data)
        })
    }
    service.command = args => enqueue(() => command(args))
    service.snapshot = snapshot
    service.refresh = function() { return enqueue(() => refreshSettings().then(publish)) }
    return service
}
