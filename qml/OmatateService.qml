import QtQuick
import Quickshell
import Quickshell.Io
import "lib/Service.mjs" as Service

Item {
    id: root
    property var panel
    property var model: null
    property var startup: null
    property int sequence: 0
    property var leaseReady: null
    property var leaseFailed: null
    property bool leased: false
    signal changed(var state)
    signal warning(string message)
    FileOps { id: files }
    function uniqueId() { return Quickshell.processId.toString(16) + "-" + Date.now().toString(16) + "-" + (++sequence).toString(16) }
    function resource(path) { return decodeURIComponent(String(Qt.resolvedUrl("../" + path)).replace(/^file:\/\//, "")) }
    function start() {
        if (startup) return startup
        model = Service.create({
            read: function(path, optional) { return files.read(path, optional) },
            write: function(path, text) { return files.write(path, text) },
            run: function(args, options) { return files.run(args, options) },
            delay: function(ms) { return files.delay(ms) },
            uniqueId: uniqueId, resource: resource,
            detach: function(args) { Quickshell.execDetached(args) }
        }, function(name) { return Quickshell.env(name) || "" }, {
            changed: function(state) { root.changed(state) },
            warning: function(message) { root.warning(message) },
            panel: function(command) { return panel.servicePanel(command) },
            noteFocused: function() { return panel.noteFocused() },
            isOpen: function() { return panel.opened }
        })
        var runtime = Quickshell.env("XDG_RUNTIME_DIR")
        startup = Promise.resolve().then(function() {
            if (!runtime) throw new Error("XDG_RUNTIME_DIR is required")
            return files.run(["mkdir", "-p", "--", runtime + "/omatate"])
        }).then(function() {
            return new Promise(function(resolve, reject) {
                root.leaseReady = resolve; root.leaseFailed = reject
                lease.command = ["flock", "--timeout", "3", runtime + "/omatate/lock", "sh", "-c", "printf 'ready\\n'; cat >/dev/null"]
                lease.running = true
            })
        }).then(function() { return root.model.start() })
        return startup
    }
    function request(command, data) { return start().then(function() { if (!root.leased) throw new Error("Notes service lost its storage lock"); return root.model.request(command, data || {}) }) }
    function command(args) { return start().then(function() { if (!root.leased) throw new Error("Notes service lost its storage lock"); return root.model.command(args) }) }
    function reply(path, response) {
        var base = Quickshell.env("XDG_RUNTIME_DIR") + "/omatate-client."
        if (typeof path !== "string" || path.indexOf(base) !== 0 || !/^[A-Za-z0-9]+\/reply$/.test(path.slice(base.length)))
            return Promise.reject(new Error("Invalid command reply path"))
        return files.write(path, JSON.stringify(response) + "\n")
    }
    // Holding the process's stdin open holds flock for this service's lifetime.
    // Another shell instance cannot concurrently mutate the same project files.
    Process {
        id: lease
        stdinEnabled: true
        stdout: SplitParser {
            onRead: line => {
                if (line !== "ready" || root.leased) return
                root.leased = true
                var ready = root.leaseReady; root.leaseReady = null; root.leaseFailed = null
                if (ready) ready()
            }
        }
        stderr: SplitParser { onRead: line => console.warn("omatate lock:", line) }
        onRunningChanged: if (!running) Qt.callLater(function() {
            if (root.leaseFailed) {
                var failed = root.leaseFailed; root.leaseFailed = null; root.leaseReady = null
                failed(new Error("Another Omatate service holds the storage lock, or flock is unavailable"))
            } else if (root.leased) {
                root.leased = false
                root.warning("Notes service lost its storage lock. Reload the plugin.")
            }
        })
    }
    FileView {
        path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/omatate/keys.toml"
        watchChanges: true
        printErrors: false
        onFileChanged: settingsChanged.restart()
    }
    FileView {
        path: Quickshell.env("HOME") + "/.config/omatate/ai"
        watchChanges: true
        printErrors: false
        onFileChanged: settingsChanged.restart()
    }
    Timer {
        id: settingsChanged
        interval: 100
        onTriggered: if (root.model) root.model.refresh().catch(error => root.warning(error.message))
    }
    Component.onCompleted: start().catch(function(error) { root.warning(error.message) })
}
