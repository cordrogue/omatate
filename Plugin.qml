import QtQuick
import Quickshell
import "qml" as Omatate

// Resident entry point. Only the storage service lives here; the panel UI is
// instantiated on first open and released when the session ends.
Item {
    id: plugin
    property var shell: null
    property bool opened: false
    property bool ready: false
    property string status: ""
    property bool hasError: false
    readonly property var panel: ui.item

    function fail(message) {
        if (ui.item) ui.item.fail(message)
        else { status = message; hasError = true }
    }
    // Called by the shell's hide. Saves pending edits, then releases the panel;
    // the session stays active so the next open restores it.
    function close() {
        if (!opened) return
        if (ui.item) ui.item.saveAll(function(ok) { if (ok) plugin.release() })
        else release()
    }
    function release() {
        Qt.callLater(function() {
            if (!plugin.opened) return
            plugin.opened = false
            if (plugin.shell && plugin.shell.hide) plugin.shell.hide("cordrogue.omatate")
        })
    }
    function noteFocused() { return ui.item ? ui.item.noteFocused() : false }
    // Called by the service after pending editor writes have drained.
    function servicePanel(cmd) {
        if (cmd === "open" || cmd === "toggle-focus") opened = true
        if (ui.item) return ui.item.servicePanel(cmd)
        switch (cmd) {
        case "ping": return Promise.resolve({ok:false, pid:Quickshell.processId})
        case "focus": return Promise.resolve({ok:true, active:false, focus:"none"})
        // hide/show bracket a screen capture and only affect a loaded panel.
        case "flush": case "reload": case "restyle": case "hide": case "show": return Promise.resolve({ok:true})
        case "quit": release(); return Promise.resolve({ok:true})
        default: return Promise.reject(new Error("Unknown panel command: " + cmd))
        }
    }
    // Shell entry point for `omarchy-shell shell summon`.
    function open(payloadJson) {
        var payload = {}
        try { payload = payloadJson ? JSON.parse(String(payloadJson)) : {} } catch (error) { fail("Invalid command payload"); return }
        var args = payload.args || (payload.cmd && payload.cmd !== "open" ? ["panel", payload.cmd] : ["open"])
        function reply(response) {
            if (payload.reply) storage.reply(payload.reply, response).catch(error => plugin.fail(error.message))
            else if (!response.ok) plugin.fail(response.error)
        }
        function drained(ok) {
            if (!ok) { reply({ok:false, error:ui.item.status}); return }
            storage.command(args).then(function(output) { reply({ok:true, output:output || ""}) },
                function(error) { reply({ok:false, error:error.message}) })
        }
        storage.start().then(function() {
            if (ui.item) ui.item.saveAll(drained); else drained(true)
        }, function(error) { reply({ok:false, error:error.message}) })
    }

    Omatate.OmatateService {
        id: storage
        host: plugin
        onWarning: message => plugin.fail(message)
        Component.onCompleted: start().then(function() { plugin.ready = true }, function() { plugin.ready = true })
    }
    // A URL source keeps the panel file and its Qt modules unparsed until the
    // first open. Properties are passed at creation so the panel can seed
    // itself from the current service snapshot in Component.onCompleted.
    Loader { id: ui }
    onOpenedChanged: {
        if (!opened) { ui.source = ""; return }
        ui.setSource(Qt.resolvedUrl("qml/OmatatePanel.qml"), {service: storage, shell: shell, initialError: hasError ? status : ""})
        ui.item.closed.connect(plugin.release)
        status = ""; hasError = false
    }
}
