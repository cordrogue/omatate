import QtQuick
import Quickshell.Io

// All file jobs are asynchronous. Each job owns its FileView until completion.
Item {
    id: root
    function read(path, optional) {
        return new Promise(function(resolve, reject) {
            var job = reader.createObject(root, {resolveJob: resolve, rejectJob: reject, optional: !!optional})
            job.path = path
        })
    }
    function write(path, text) {
        // FileView.setText("") is a no-op on a fresh FileView. Use an empty
        // sibling temporary file so clears still replace the target atomically.
        if (text === "") return run(["sh", "-c",
            "set -eu; target=$1; [ ! -e \"$target\" ] || [ -f \"$target\" ]; tmp=$(mktemp \"${target}.XXXXXX\"); trap 'rm -f -- \"$tmp\"' EXIT; if [ -f \"$target\" ]; then chmod --reference=\"$target\" \"$tmp\"; fi; sync -d \"$tmp\"; mv -fT -- \"$tmp\" \"$target\"",
            "omatate-empty-file", path]).then(function() {})
        return new Promise(function(resolve, reject) {
            var job = writer.createObject(root, {resolveJob: resolve, rejectJob: reject, path: path, expectedText: text})
            job.setText(text)
        })
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
        id: writer
        FileView {
            property var resolveJob
            property var rejectJob
            property string expectedText: ""
            preload: false
            atomicWrites: true
            blockWrites: false
            printErrors: false
            onSaved: {
                var resolve = resolveJob, reject = rejectJob, expected = expectedText
                // Verify the on-disk result, including a failed atomic rename
                // reported only as a warning by some FileView versions.
                root.read(path, false).then(function(actual) {
                    if (actual === expected) resolve()
                    else reject(new Error("Saved file did not match the requested contents"))
                }, reject)
                destroy()
            }
            onSaveFailed: error => { rejectJob(new Error("Cannot save " + path + " (file error " + error + ")")); destroy() }
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
