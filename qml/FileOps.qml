import QtQuick
import Quickshell.Io

// All file jobs are asynchronous. Reads own a FileView until completion;
// writes and commands own a Process.
Item {
    id: root
    function read(path, optional) {
        return new Promise(function(resolve, reject) {
            var job = reader.createObject(root, {resolveJob: resolve, rejectJob: reject, optional: !!optional})
            job.path = path
        })
    }
    function write(path, text) {
        // Every write goes through a shell script rather than FileView, whose
        // atomic save resolves a symlink at the target and writes the file it
        // points to. The project directory is user controlled, so a planted
        // link at draft.txt or entries.jsonl would redirect the save. Here the
        // text lands in a private mktemp directory next to the target under
        // noclobber, so the open is an exclusive create, and mv -fT renames it
        // over the target name, replacing a link instead of following it.
        return run(["sh", "-c",
            "set -Ceu; target=$1; [ ! -e \"$target\" ] || [ -f \"$target\" ]; stage=$(mktemp -d \"${target}.XXXXXX\"); trap 'rm -rf -- \"$stage\"' EXIT; "
            + "cat > \"$stage/file\"; if [ -f \"$target\" ]; then chmod --reference=\"$target\" \"$stage/file\"; else chmod \"$(printf '%o' \"$((0666 & ~0$(umask)))\")\" \"$stage/file\"; fi; "
            + "sync -d \"$stage/file\"; mv -fT -- \"$stage/file\" \"$target\"",
            "omatate-write", path], {input: text}).then(function() {})
    }
    function run(args, options) {
        options = options || {}
        return new Promise(function(resolve, reject) {
            var job = processJob.createObject(root, {
                resolveJob: resolve, rejectJob: reject, command: args,
                workingDirectory: options.cwd || "", input: options.input || "",
                timeoutMs: options.timeout || 10000, allowFailure: !!options.allowFailure
            })
            job.running = true
        })
    }
    function delay(ms) {
        return new Promise(function(resolve) { delayJob.createObject(root, {interval: ms, resolveJob: resolve}).start() })
    }
    Component {
        id: reader
        FileView {
            property var resolveJob
            property var rejectJob
            property bool optional: false
            printErrors: false
            onLoaded: { resolveJob(text()); destroy() }
            onLoadFailed: error => {
                if (optional && error === FileViewError.FileNotFound) resolveJob(null)
                else rejectJob(new Error("Cannot read " + path + " (file error " + error + ")"))
                destroy()
            }
        }
    }
    Component {
        id: delayJob
        Timer { property var resolveJob; onTriggered: { resolveJob(); destroy() } }
    }
    Component {
        id: processJob
        Process {
            id: proc
            property var resolveJob
            property var rejectJob
            property string input: ""
            property int timeoutMs: 10000
            property bool allowFailure: false
            property bool finished: false
            property bool timedOut: false
            property int resultCode: 127
            stdinEnabled: true
            stdout: StdioCollector { id: output }
            stderr: StdioCollector { id: errors }
            property Timer deadline: Timer {
                interval: proc.timeoutMs
                onTriggered: { proc.timedOut = true; proc.signal(9) }
            }
            function finish(code) {
                if (finished) return
                finished = true; deadline.stop()
                var result = {code: code, stdout: output.text, stderr: errors.text}
                if (code === 0 || allowFailure) resolveJob(result)
                else rejectJob(new Error(command[0] + ": " + (timedOut ? "timed out" : (errors.text.trim() || "failed (" + code + ")"))))
                destroy()
            }
            onStarted: { deadline.start(); if (input) write(input); stdinEnabled = false }
            onExited: (code, status) => { resultCode = status === 0 ? code : 128 + code; Qt.callLater(function() { proc.finish(proc.resultCode) }) }
            // Failed-to-start does not emit exited on all Quickshell versions.
            onRunningChanged: if (!running && !finished) Qt.callLater(function() { if (!proc.finished) proc.finish(proc.resultCode) })
        }
    }
}
