import QtQuick
import QtQuick.Controls as QQC
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQml.Models
import Qt5Compat.GraphicalEffects
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import "qml"

Item {
    id: root
    property string omarchyPath: ""
    property var shell: null
    property var manifest: null
    property var pluginRegistry: null
    property var barWidgetRegistry: null
    property string executable: Quickshell.env("UI_NOTES_EXECUTABLE") || localPath(Qt.resolvedUrl("target/release/ui-notes"))
    property bool shown: false
    property bool opened: false
    property bool minimized: false
    property bool choosing: false
    property bool searching: false
    property bool helping: false
    property bool busy: false
    property bool ready: false
    property bool restoring: false
    property bool draftDirty: false
    property bool focusAllowed: true
    property bool hiddenFocus: false
    property string session: ""
    property string token: ""
    property string runtimeDir: ""
    property bool ai: true
    property var projects: []
    property var entries: []
    property var dirtyEdits: ({})
    property var callbacks: ({})
    property int requestId: 0
    property string status: ""
    property bool hasError: false
    property var currentEditor: null
    property int currentEntry: -1
    property string previewAsset: ""
    readonly property size previewLogicalSize: {
        var entry = entries.find(function(entry) { return entry.asset === previewAsset })
        var size = entry && entry.asset_size
        if (size && Number.isFinite(size.width) && size.width > 0 && Number.isFinite(size.height) && size.height > 0)
            return Qt.size(size.width, size.height)
        return Qt.size(0, 0)
    }
    property real previewY: 0
    property var hideReplies: []
    property int hideRetries: 0
    property var commandQueue: []
    property var commandDone: null
    property string commandError: ""
    property bool newFolder: false
    property string parentFolder: ""
    property var keys: ({})
    property string keysWarning: ""
    property int opacityStep: 0
    readonly property var opacityLevels: [1.0, 0.8, 0.6, 0.4]
    readonly property real panelOpacity: opacityLevels[opacityStep]
    readonly property var actions: [
        {id:"save_note", label:"Save note", fallback:["Ctrl+Return", "Ctrl+S"]},
        {id:"delete_note", label:"Delete focused note", fallback:["Ctrl+Delete"]},
        {id:"focus_note", label:"Focus note", fallback:["Ctrl+N"]},
        {id:"search", label:"Search", fallback:["Ctrl+F"]},
        {id:"projects", label:"Projects", fallback:["Ctrl+P"]},
        {id:"new_project", label:"New project", fallback:["Ctrl+Shift+N"]},
        {id:"open_folder", label:"Open folder", fallback:["Ctrl+O"]},
        {id:"add_section", label:"Add section", fallback:["Ctrl+Shift+S"]},
        {id:"clip", label:"Clip", fallback:["Ctrl+Shift+C"]},
        {id:"toggle_ai", label:"Toggle AI", fallback:["Ctrl+Shift+A"]},
        {id:"cycle_opacity", label:"Change opacity", fallback:["Ctrl+Shift+O"]},
        {id:"minimize", label:"Minimize or expand", fallback:["Ctrl+M"]},
        {id:"end_session", label:"End session", fallback:["Ctrl+Shift+W"]},
        {id:"previous_entry", label:"Previous entry", fallback:["Alt+Up"]},
        {id:"next_entry", label:"Next entry", fallback:["Alt+Down"]},
        {id:"preview", label:"Preview", fallback:["Alt+Return"]},
        {id:"expand_note", label:"Expand or collapse note", fallback:["Shift+Alt+Return"]},
        {id:"help", label:"Help", fallback:["Ctrl+H", "F1"]}
    ]
    readonly property var bar: shell && shell.bar ? shell.bar : null
    readonly property var panelMonitor: panel.screen ? Hyprland.monitorFor(panel.screen) : null
    readonly property bool fullscreen: panelMonitor && panelMonitor.activeWorkspace && panelMonitor.activeWorkspace.hasFullscreen
    readonly property bool screensaverVisible: ToplevelManager.toplevels.values.some(function(toplevel) {
        return toplevel.appId === "org.omarchy.screensaver"
    })
    property bool restoreFocusAfterScreensaver: false
    readonly property real topGap: 4 + (!fullscreen && bar && !bar.barHidden && bar.position === "top" ? bar.barSize : 0)
    readonly property real rightGap: 12 + (!fullscreen && bar && !bar.barHidden && bar.position === "right" ? bar.barSize : 0)
    property var theme: ({background:"#3a332a", control:"#373028", foreground:"#f5ecd9", muted:"#b0a48f", accent:"#d6a06b", red:"#ed806b", yellow:"#dab46b", green:"#83baa1", hair:"#40f5ecd9", selectionBg:"#73d6a06b", selectionFg:"#f5ecd9", cursor:"#d6a06b", fontFamily:"JetBrainsMono Nerd Font", fontPointSize:11, smallPointSize:9.02})
    readonly property var matches: {
        var query = search.text.trim().toLowerCase()
        if (!query) return entries.map(function() { return true })
        var result = []
        var sectionIndex = -1
        var sectionMatch = false
        for (var i = 0; i < entries.length; ++i) {
            var entry = entries[i]
            if (entry.kind === "section") {
                sectionIndex = i
                var title = dirtyEdits[entry.id] !== undefined ? dirtyEdits[entry.id] : entry.title
                sectionMatch = String(title || "").toLowerCase().indexOf(query) >= 0
                result.push(sectionMatch)
                continue
            }
            var transcript = dirtyEdits[entry.id] !== undefined ? dirtyEdits[entry.id] : entry.transcript
            var context = entry.context || {}
            var description = [context.title, context.summary].concat(context.notable || [],
                (context.regions || []).map(function(region) { return (region.name || "") + " " + (region.contents || "") }))
            var noteMatch = sectionMatch
                || String(transcript || "").toLowerCase().indexOf(query) >= 0
                || description.join("\n").toLowerCase().indexOf(query) >= 0
            result.push(noteMatch)
            if (noteMatch && sectionIndex >= 0) result[sectionIndex] = true
        }
        return result
    }
    readonly property int matchedNotes: entries.filter(function(entry, index) { return entry.kind !== "section" && root.matches[index] }).length
    readonly property int matchedSections: entries.filter(function(entry, index) { return entry.kind === "section" && root.matches[index] }).length

    function localPath(url) { return decodeURIComponent(String(url).replace(/^file:\/\//, "")) }
    function fileUrl(path) { return "file://" + path.split("/").map(encodeURIComponent).join("/") }
    function projectLabel(path) { var home = Quickshell.env("HOME") || ""; return home && String(path).indexOf(home + "/") === 0 ? "~/" + String(path).slice(home.length + 1) : String(path) }
    function name(path) { return String(path).replace(/\/$/, "").split("/").pop() }
    function withAlpha(colorValue, opacity) { var c = Qt.tint("transparent", colorValue); return Qt.rgba(c.r, c.g, c.b, opacity) }
    function fail(message) { status = message; hasError = true }
    function request(cmd, data, done) {
        if (!backend.running) { fail("Notes backend is unavailable. Reopen the plugin to retry."); if (done) done(false); return }
        var id = ++requestId
        var message = Object.assign({id:id, cmd:cmd}, data || {})
        callbacks[id] = done || function() {}
        backend.write(JSON.stringify(message) + "\n")
    }
    function applyState(state) {
        var switched = session !== state.session || token !== state.token
        if (switched && (draftDirty || Object.keys(dirtyEdits).length)) {
            fail("The active project changed while edits were pending. Save or copy your text before reopening.")
            return
        }
        restoring = true
        if (state.theme) theme = state.theme
        session = state.session
        token = state.token
        runtimeDir = state.runtimeDir || runtimeDir
        ai = state.ai
        projects = state.projects || []
        keys = state.keys || {}
        keysWarning = state.keysWarning || ""
        entries = (state.entries || []).slice().sort(function(a, b) { return a.id - b.id })
        if (!draftDirty && draft.text !== state.draft) draft.text = state.draft || ""
        if (switched) { status = state.draft ? "Draft restored" : ""; hasError = false; entryModel.clear(); previewAsset = ""; currentEntry = -1; currentEditor = null; choosing = !session }
        var entryIds = ({})
        entries.forEach(function(entry) { entryIds[entry.id] = true })
        for (var removed = entryModel.count - 1; removed >= 0; --removed)
            if (!entryIds[entryModel.get(removed).entryId]) entryModel.remove(removed)
        for (var i = 0; i < entries.length; ++i) {
            var e = entries[i]
            var row = {entryId:e.id, kind:e.kind || "note", entryText:e.kind === "section" ? (e.title || "") : (e.transcript || ""),
                title:e.context && e.context.title ? e.context.title : "",
                summary:e.context && e.context.summary ? e.context.summary : "",
                entryStatus:e.status || "", contextStatus:e.context_status || "", asset:e.asset || "", aiEntry:e.ai !== false}
            if (dirtyEdits[e.id] !== undefined) row.entryText = dirtyEdits[e.id]
            if (i >= entryModel.count) entryModel.append(row)
            else {
                if (entryModel.get(i).entryId !== e.id) {
                    var previous = i + 1
                    while (previous < entryModel.count && entryModel.get(previous).entryId !== e.id) ++previous
                    if (previous < entryModel.count) entryModel.move(previous, i, 1)
                    else entryModel.insert(i, row)
                }
                entryModel.set(i, row)
            }
        }
        if (entryModel.count > entries.length) entryModel.remove(entries.length, entryModel.count - entries.length)
        restoring = false
        ready = true
    }
    function markEdit(id, text) {
        if (restoring) return
        dirtyEdits[id] = text
        hasError = false
        autosave.restart()
    }
    function saveAll(done) {
        autosave.stop()
        var writes = []
        var owner = {session:session, token:token}
        if (draftDirty) writes.push({cmd:"draft", text:draft.text})
        Object.keys(dirtyEdits).forEach(function(id) { writes.push({cmd:"edit", entryId:Number(id), text:dirtyEdits[id]}) })
        if (!writes.length) {
            // The backend processes lines in order. This reply also waits for
            // writes already sent by an autosave or note submission.
            request("snapshot", {}, function(ok) {
                if (ok && (draftDirty || Object.keys(dirtyEdits).length)) saveAll(done)
                else if (done) done(ok)
            })
            return
        }
        var remaining = writes.length, success = true
        writes.forEach(function(write) {
            request(write.cmd, Object.assign({}, owner, write), function(ok) {
                if (ok) {
                    if (write.cmd === "draft" && draft.text === write.text) draftDirty = false
                    if (write.cmd === "edit" && dirtyEdits[write.entryId] === write.text) {
                        delete dirtyEdits[write.entryId]
                        dirtyEditsChanged()
                    }
                } else success = false
                if (--remaining === 0) {
                    if (success) {
                        if (writes.some(function(write) { return write.cmd === "draft" })) status = draft.text ? "Draft saved" : ""
                        hasError = false
                        // Typing may have continued while these writes were in
                        // flight. A hide or project switch must drain it too.
                        saveAll(done)
                    } else if (done) done(false)
                }
            })
        })
    }
    function cli(args, done) {
        commandQueue.push({args:args, done:done})
        nextCommand()
    }
    function nextCommand() {
        if (command.running || !commandQueue.length) return
        var next = commandQueue.shift()
        commandDone = next.done || null
        commandError = ""
        command.command = [executable].concat(next.args)
        command.running = true
    }
    function actionCommand(args, done) {
        if (busy) return
        busy = true
        saveAll(function(ok) {
            if (!ok) { busy = false; return }
            cli(args, function(success) {
                request("snapshot", {}, function() { busy = false; if (done) done(success) })
            })
        })
    }
    function focusNote() { minimized = false; choosing = !session; if (session) draft.forceActiveFocus(); else projectButton.forceActiveFocus() }
    function takeFocus(project) {
        releaseTimer.stop()
        opened = true; focusAllowed = true
        shown = true
        Qt.callLater(function() {
            if (!shown || !focusAllowed || screensaverVisible) return
            panelFocusGrab.active = true
            if (project || !session) projectButton.forceActiveFocus(); else focusNote()
        })
    }
    function open(payloadJson) {
        if (!backend.running) backend.running = true
        var payload = {}
        try { payload = payloadJson ? JSON.parse(String(payloadJson)) : {} } catch (e) {}
        if (payload.cmd && payload.cmd !== "open" && payload.cmd !== "ping") { handleCommand(payload.cmd, function() {}); return }
        var wasShown = shown
        request("snapshot", {}, function() { takeFocus(wasShown) })
        takeFocus(wasShown)
    }
    function close() { saveAll(function(ok) { if (ok) { panelFocusGrab.active = false; opened = false; shown = false; previewAsset = ""; helping = false } }) }
    function stopSession() {
        if (!session) { close(); if (shell && shell.hide) shell.hide("cordrogue.ui-notes"); return }
        actionCommand(["stop"], function(ok) { if (ok) { panelFocusGrab.active = false; shown = false; if (shell && shell.hide) shell.hide("cordrogue.ui-notes") } })
    }
    function chooseFolder(create) { if (busy) return; newFolder = create; folderDialog.title = create ? "Choose a parent for the new project" : "Choose a project folder"; folderDialog.open() }
    function selectProject(path) {
        actionCommand(["select-project", path, session], function(ok) { if (ok) { choosing = false; focusNote() } })
    }
    function submitNote() {
        if (!session || busy) return
        if (currentEditor && currentEditor.activeFocus && currentEntry >= 0) { saveAll(); return }
        if (!draft.text.trim()) { saveAll(); return }
        busy = true
        var text = draft.text
        saveAll(function(ok) {
            if (!ok) { busy = false; return }
            request("note", {session:session, token:token, text:text}, function(success) {
                busy = false
                if (success) {
                    if (draft.text === text) { restoring = true; draft.text = ""; restoring = false; draftDirty = false }
                    status = ""; focusNote(); notes.positionViewAtEnd()
                }
            })
        })
    }
    function deleteFocusedNote() {
        if (busy || minimized || helping || choosing || newProjectDialog.visible || folderDialog.visible
                || !session || !currentEditor || !currentEditor.entryFocused || currentEntry < 0) return
        var ownerSession = session
        var ownerToken = token
        var id = currentEntry
        var entry = entries.find(function(candidate) { return candidate.id === id })
        if (!entry || entry.kind === "section") return
        busy = true
        saveAll(function(ok) {
            if (!ok || session !== ownerSession || token !== ownerToken) { busy = false; return }
            request("delete-note", {session:ownerSession, token:ownerToken, entryId:id}, function(success) {
                busy = false
                if (!success || session !== ownerSession || token !== ownerToken) return
                currentEditor = null
                currentEntry = -1
                previewAsset = ""
                Qt.callLater(root.focusNote)
            })
        })
    }
    function matched(index) {
        return matches[index] === true
    }
    function preview(editor, asset, toggle) {
        if (!asset || (toggle && previewAsset === asset)) { previewAsset = ""; return }
        previewAsset = asset
        previewY = Math.max(0, editor.mapToItem(panel.contentItem, 0, 0).y)
    }
    function moveEntry(delta) {
        var index = -1
        for (var i = 0; i < entryModel.count; ++i) if (entryModel.get(i).entryId === currentEntry) index = i
        if (index < 0) index = delta < 0 ? entryModel.count : -1
        for (index += delta; index >= 0 && index < entryModel.count; index += delta) {
            if (!matched(index)) continue
            notes.positionViewAtIndex(index, ListView.Contain)
            var row = notes.itemAtIndex(index)
            if (row) {
                row.editor.forceActiveFocus()
                return
            }
        }
        focusNote()
    }
    function cycleOpacity() { opacityStep = (opacityStep + 1) % opacityLevels.length }
    function runAction(action) {
        switch (action) {
        case "save_note": submitNote(); break
        case "delete_note": deleteFocusedNote(); break
        case "focus_note": focusNote(); break
        case "search": minimized = false; searching = true; search.forceActiveFocus(); break
        case "projects": choosing = !choosing; break
        case "new_project": chooseFolder(true); break
        case "open_folder": chooseFolder(false); break
        case "add_section": if (session) actionCommand(["section"]); break
        case "clip": if (session) actionCommand(["clip"], function(ok) { if (ok) { minimized = false; currentEntry = -1; notes.positionViewAtEnd(); moveEntry(-1) } }); break
        case "toggle_ai": if (session) actionCommand(["ai", "toggle"]); break
        case "cycle_opacity": cycleOpacity(); break
        case "minimize": minimized = !minimized; previewAsset = ""; break
        case "end_session": stopSession(); break
        case "previous_entry": moveEntry(-1); break
        case "next_entry": moveEntry(1); break
        case "preview": if (currentEditor) preview(currentEditor, currentEditor.clipAsset || "", true); break
        case "expand_note": if (currentEditor) currentEditor.toggleExpanded(); break
        case "help": helping = !helping; break
        }
    }
    function escapePanel() {
        if (helping) helping = false
        else if (previewAsset) previewAsset = ""
        else if (searching) { searching = false; search.text = ""; focusNote() }
        else if (choosing && session) { choosing = false; focusNote() }
        else focusNote()
    }
    function handleCommand(cmd, reply) {
        switch (cmd) {
        case "ping": reply({ok:opened, pid:Quickshell.processId}); break
        case "focus": reply({ok:true, active:shown && panel.contentItem.Window.active, focus:draft.activeFocus ? "note" : (currentEditor && currentEditor.activeFocus ? (currentEditor.section ? "section" : "entry") : "none"), id:currentEditor && currentEditor.activeFocus ? currentEntry : null}); break
        case "flush": saveAll(function(ok) { reply({ok:ok, error:ok ? undefined : status}) }); break
        case "hide":
            saveAll(function(ok) {
                if (!ok) { reply({ok:false,error:status}); return }
                hiddenFocus = panel.contentItem.Window.active; shown = false; panelFocusGrab.active = false; previewAsset = ""; helping = false
                hideReplies.push(reply); hideRetries = 0; hideTimer.restart()
            }); break
        case "show":
            focusAllowed = hiddenFocus; shown = true
            if (hiddenFocus) Qt.callLater(function() { if (shown && focusAllowed && !screensaverVisible) panelFocusGrab.active = true })
            if (!hiddenFocus) releaseTimer.restart()
            request("snapshot")
            reply({ok:true}); break
        case "toggle-focus":
            if (panel.contentItem.Window.active && shown) saveAll(function(ok) {
                if (ok) { focusAllowed = false; panelFocusGrab.active = false; helping = false; previewAsset = ""; releaseTimer.restart() }
                reply({ok:ok,focused:!ok,error:ok ? undefined : status})
            })
            else { takeFocus(false); reply({ok:true,focused:true}) }
            break
        case "open": takeFocus(true); request("snapshot"); reply({ok:true}); break
        case "reload": request("snapshot", {}, function(ok) { reply({ok:ok}) }); break
        case "restyle": reply({ok:true}); break
        case "quit": saveAll(function(ok) {
            if (ok) { panelFocusGrab.active = false; shown = false; previewAsset = ""; helping = false }
            reply({ok:ok,error:ok ? undefined : status})
            // Socket.flush() runs in reply(). Remove the listener only on the
            // next event-loop turn so the client receives the acknowledgement.
            if (ok) Qt.callLater(function() {
                opened = false
                if (shell && shell.hide) shell.hide("cordrogue.ui-notes")
            })
        }); break
        default: reply({ok:false,error:"unknown command"})
        }
    }

    Process {
        id: backend
        command: [root.executable, "backend"]
        stdinEnabled: true
        running: true
        onStarted: root.request("snapshot", {}, function(ok) { if (ok) { root.hasError = false } })
        stdout: SplitParser {
            onRead: data => {
                try {
                    var response = JSON.parse(data)
                    if (response.ok && response.state) root.applyState(response.state)
                    if (!response.ok) root.fail(response.error || "Could not save notes")
                    var callback = root.callbacks[response.id]
                    delete root.callbacks[response.id]
                    if (callback) callback(response.ok)
                    if (response.warning) root.fail(response.warning)
                } catch (e) { root.fail("Invalid backend response: " + e) }
            }
        }
        stderr: SplitParser { onRead: data => console.warn("ui-notes:", data) }
        onExited: (code, status) => {
            root.ready = false
            root.fail("Notes backend stopped. Reopen the plugin to retry.")
            var pending = root.callbacks; root.callbacks = ({})
            Object.keys(pending).forEach(function(id) { pending[id](false) })
        }
    }
    Process {
        id: command
        stdout: SplitParser { onRead: data => {} }
        stderr: SplitParser { onRead: data => { root.commandError += data + "\n" } }
        onExited: (code, status) => {
            var done = root.commandDone; root.commandDone = null
            if (code !== 0) root.fail(root.commandError.trim() || "Command failed")
            if (done) done(code === 0)
            Qt.callLater(root.nextCommand)
        }
        onRunningChanged: {
            if (running || !root.commandDone) return
            var done = root.commandDone; root.commandDone = null
            root.fail(root.commandError.trim() || "Command failed to start")
            done(false)
            Qt.callLater(root.nextCommand)
        }
    }
    SocketServer {
        active: root.ready && root.opened && root.runtimeDir.length > 0
        path: root.runtimeDir + "/panel.sock"
        handler: Socket {
            id: connection
            parser: SplitParser {
                onRead: data => {
                    try { root.handleCommand(JSON.parse(data).cmd, function(reply) { if (connection.connected) { connection.write(JSON.stringify(reply) + "\n"); connection.flush() } }) }
                    catch (e) { connection.write(JSON.stringify({ok:false,error:String(e)}) + "\n"); connection.flush() }
                }
            }
        }
    }
    Timer { id: autosave; interval: 450; onTriggered: root.saveAll() }
    Timer { interval: 1500; running: root.ready && root.shown; repeat: true; onTriggered: if (!root.busy && !Object.keys(root.callbacks).length) root.request("snapshot", {poll:true}) }
    Timer { id: releaseTimer; interval: 180; onTriggered: root.focusAllowed = true }
    HyprlandFocusGrab { id: panelFocusGrab; windows: [panel] }
    onScreensaverVisibleChanged: {
        if (screensaverVisible) {
            restoreFocusAfterScreensaver = panelFocusGrab.active
            panelFocusGrab.active = false
        } else if (restoreFocusAfterScreensaver && shown && focusAllowed) {
            restoreFocusAfterScreensaver = false
            Qt.callLater(function() { if (!root.screensaverVisible && root.shown && root.focusAllowed) panelFocusGrab.active = true })
        } else {
            restoreFocusAfterScreensaver = false
        }
    }
    Timer {
        id: hideTimer
        interval: 48
        onTriggered: {
            if (panel.backingWindowVisible || previewWindow.backingWindowVisible || helpWindow.backingWindowVisible) {
                if (++root.hideRetries * interval < 1200) { restart(); return }
                var failed = root.hideReplies; root.hideReplies = []
                failed.forEach(function(reply) { reply({ok:false, error:"panel did not hide"}) })
                return
            }
            var replies = root.hideReplies; root.hideReplies = []
            replies.forEach(function(reply) { reply({ok:true}) })
        }
    }
    ListModel { id: entryModel }
    FolderDialog {
        id: folderDialog
        onAccepted: {
            var folder = root.localPath(selectedFolder)
            if (root.newFolder) { root.parentFolder = folder; newProjectDialog.open() }
            else root.selectProject(folder)
        }
    }

    PanelWindow {
        id: panel
        visible: root.shown
        color: "transparent"
        anchors { top: true; right: true }
        margins { top: root.topGap; right: root.rightGap }
        implicitWidth: Math.min(toolbar.implicitWidth, screen ? screen.width - 24 : toolbar.implicitWidth)
        implicitHeight: Math.min(panelContent.implicitHeight, (screen ? screen.height : 900) - root.topGap - 12)
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "ui-notes"
        WlrLayershell.layer: root.screensaverVisible ? WlrLayer.Top : WlrLayer.Overlay
        WlrLayershell.keyboardFocus: !root.screensaverVisible && root.focusAllowed ? (panelFocusGrab.active ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.OnDemand) : WlrKeyboardFocus.None
        contentItem.opacity: root.panelOpacity

        ColumnLayout {
            id: panelContent
            anchors { top: parent.top; left: parent.left; right: parent.right }
            spacing: 4
            RowLayout {
                id: toolbar
                Layout.alignment: Qt.AlignRight
                spacing: 4
                NoteField { id: search; theme: root.theme; visible: root.searching; Layout.preferredWidth: 180; placeholderText: "Search notes…" }
                NoteButton { theme: root.theme; iconName: "search"; tooltipText: "Search · Ctrl+F"; onClicked: { if (root.searching) { root.searching = false; search.text = ""; root.focusNote() } else root.runAction("search") } }
                NoteButton { id: projectButton; theme: root.theme; iconName: "folder"; tooltipText: root.session || "Choose project · Ctrl+P"; onClicked: root.choosing = !root.choosing }
                NoteButton { theme: root.theme; iconName: "capture"; enabled: !!root.session && !root.busy; tooltipText: "Select a screen area, then type a note or hold HOME to narrate · Ctrl+Shift+C"; onClicked: root.runAction("clip") }
                NoteButton { theme: root.theme; iconName: "sparkle"; selected: root.ai; enabled: !!root.session && !root.busy; tooltipText: "Screenshots and screen analysis · Ctrl+Shift+A"; onClicked: root.runAction("toggle_ai") }
                NoteButton { theme: root.theme; iconName: "plus"; enabled: !!root.session && !root.busy; tooltipText: "Add section · Ctrl+Shift+S"; onClicked: root.runAction("add_section") }
                NoteButton { theme: root.theme; iconName: root.minimized ? "expand" : "collapse"; tooltipText: (root.minimized ? "Expand" : "Minimize") + " · Ctrl+M"; onClicked: root.runAction("minimize") }
            }
            Rectangle {
                visible: root.hasError
                Layout.fillWidth: true
                implicitHeight: projectStatus.implicitHeight + 8
                color: root.theme.background
                Text { id: projectStatus; anchors { left: parent.left; right: parent.right; top: parent.top; margins: 6; topMargin: 4 } text: root.status; wrapMode: Text.Wrap; color: root.theme.red; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
            }
            Rectangle {
                visible: !!search.text && !root.minimized
                Layout.fillWidth: true
                implicitHeight: searchCount.implicitHeight + 4
                color: root.theme.background
                Text { id: searchCount; anchors.centerIn: parent; text: root.matchedNotes + " notes · " + root.matchedSections + " sections"; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize }
            }
            ListView {
                id: notes
                readonly property real fadeStart: Math.max(0, Math.min(height, (panel.screen ? panel.screen.height : 800) * 0.5 - (root.topGap + y)))
                readonly property bool fadesAtBottom: visible && height > 0 && root.topGap + y + height > (panel.screen ? panel.screen.height : 800) * 0.5
                visible: !root.minimized && !!root.session && (entryModel.count > 0 || !!search.text)
                Layout.fillWidth: true
                implicitHeight: Math.min(contentHeight, Math.max(40, (panel.screen ? panel.screen.height : 800) * 0.70))
                spacing: 0
                clip: true
                model: entryModel
                cacheBuffer: 20000
                QQC.ScrollBar.vertical: QQC.ScrollBar { policy: QQC.ScrollBar.AlwaysOff }
                layer.enabled: fadesAtBottom
                layer.effect: OpacityMask {
                    maskSource: Rectangle {
                        width: notes.width
                        height: notes.height
                        gradient: Gradient {
                            orientation: Gradient.Vertical
                            GradientStop { position: notes.height > 0 ? notes.fadeStart / notes.height : 1; color: "white" }
                            GradientStop { position: 1; color: "transparent" }
                        }
                    }
                }
                onContentYChanged: if (root.currentEditor && root.previewAsset) root.preview(root.currentEditor, root.previewAsset, false)
                header: Rectangle {
                    width: notes.width
                    height: visible ? noResults.implicitHeight + 4 : 0
                    visible: !!search.text && root.matchedNotes + root.matchedSections === 0
                    color: root.theme.background
                    Text { id: noResults; anchors.centerIn: parent; text: "No matching notes or sections"; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                }
                delegate: Item {
                    id: row
                    required property int index
                    required property int entryId
                    required property string kind
                    required property string entryText
                    required property string title
                    required property string summary
                    required property string entryStatus
                    required property string contextStatus
                    required property string asset
                    required property bool aiEntry
                    readonly property bool section: kind === "section"
                    readonly property string statusText: contextStatus === "error" ? "ERROR" : entryStatus === "recording" ? "REC" : entryStatus === "transcribing" ? "TRANSCRIBING" : entryStatus === "no-transcript" ? "NO TRANSCRIPT" : contextStatus === "pending" ? "ANALYZING" : ""
                    readonly property color statusColor: contextStatus === "error" || entryStatus === "recording" ? root.theme.red : entryStatus === "transcribing" || contextStatus === "pending" ? root.theme.yellow : entryStatus === "no-transcript" ? root.theme.muted : root.theme.green
                    readonly property color barColor: statusText ? statusColor : asset && root.previewAsset !== asset ? "#6aa9ff" : statusColor
                    property bool expanded: false
                    property alias editor: editor
                    function toggleExpanded() {
                        if (section) return
                        expanded = !expanded
                        if (expanded) Qt.callLater(function() { notes.positionViewAtIndex(row.index, ListView.Contain) })
                    }
                    width: ListView.view.width
                    visible: root.matches[index] === true
                    height: visible ? body.implicitHeight + (index < entryModel.count - 1 ? 4 : 0) : 0
                    HoverHandler { id: rowHover }
                    Rectangle {
                        id: card
                        anchors { top: parent.top; left: parent.left; right: parent.right; leftMargin: row.section ? parent.width * 0.05 : 0; rightMargin: row.section ? parent.width * 0.05 : 0 }
                        height: body.implicitHeight
                        color: row.section ? Qt.tint(root.theme.control, root.withAlpha(root.theme.accent, 0.22)) : root.theme.control
                        border.width: row.section ? 0 : 1
                        border.color: root.theme.hair
                        QQC.Control {
                            id: statusBar
                            visible: !row.section
                            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                            width: 2
                            padding: 0
                            hoverEnabled: true
                            focusPolicy: Qt.StrongFocus
                            function togglePreview() {
                                if (!row.asset) return
                                var wasOpen = root.previewAsset === row.asset
                                editor.forceActiveFocus()
                                root.preview(editor, wasOpen ? "" : row.asset, false)
                            }
                            Accessible.role: Accessible.Button
                            Accessible.name: row.asset ? (root.previewAsset === row.asset ? "Hide clip preview" : "Show clip preview") + "; double click to toggle full note" : "Double click to toggle full note"
                            onActiveFocusChanged: if (activeFocus) { root.currentEditor = editor; root.currentEntry = row.entryId }
                            Accessible.onPressAction: togglePreview()
                            Keys.onSpacePressed: togglePreview()
                            Keys.onReturnPressed: togglePreview()
                            background: Rectangle { color: row.barColor }
                            contentItem: Item {}
                            QQC.ToolTip.visible: barMouse.containsMouse
                            QQC.ToolTip.text: row.asset ? "Toggle clip preview · Alt+Enter · Double click to expand note" : "Double click to expand note"
                            Timer {
                                id: previewClickTimer
                                interval: Qt.styleHints.mouseDoubleClickInterval
                                onTriggered: statusBar.togglePreview()
                            }
                        }
                    }
                    // Hit area for the status bar: twice the bar's width, layered above the body so the
                    // extra width isn't swallowed by the editor. The visible bar itself stays 2px.
                    MouseArea {
                        id: barMouse
                        visible: !row.section
                        z: 1
                        anchors { left: card.left; top: card.top; bottom: card.bottom }
                        width: statusBar.width * 2
                        hoverEnabled: true
                        cursorShape: row.asset ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onPressed: { root.currentEditor = editor; root.currentEntry = row.entryId }
                        onClicked: previewClickTimer.restart()
                        onDoubleClicked: {
                            previewClickTimer.stop()
                            row.toggleExpanded()
                        }
                    }
                    Item {
                        id: body
                        x: card.x
                        width: card.width
                        implicitHeight: cardContent.implicitHeight + 2
                        height: implicitHeight
                        ColumnLayout {
                            id: cardContent
                            x: row.section ? 0 : 2
                            y: 1
                            width: parent.width - (row.section ? 0 : 3)
                            spacing: 4
                            RowLayout {
                                id: entryHeader
                                visible: !row.section && (row.aiEntry || !!row.statusText)
                                Layout.fillWidth: true
                                spacing: 4
                                Text { visible: row.aiEntry; text: row.title; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize }
                                Text { text: row.statusText; textFormat: Text.PlainText; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 4
                                QQC.ScrollView {
                                    id: entryScroll
                                    Layout.fillWidth: true
                                    implicitHeight: {
                                        if (row.section) return Math.max(18, editor.contentHeight + 2)
                                        if (!row.expanded) return Math.min(122, editor.implicitHeight)
                                        var headerHeight = entryHeader.visible ? entryHeader.implicitHeight + cardContent.spacing : 0
                                        // Keep an expanded card above the list's bottom fade.
                                        var maxExpandedHeight = Math.max(122, (panel.screen ? panel.screen.height : 800) * 0.5 - root.topGap - notes.y - headerHeight - 2 - (row.index < entryModel.count - 1 ? 4 : 0))
                                        return Math.min(Math.max(22, editor.implicitHeight), maxExpandedHeight)
                                    }
                                    contentWidth: availableWidth
                                    clip: true
                                    QQC.ScrollBar.horizontal.policy: QQC.ScrollBar.AlwaysOff
                                    QQC.ScrollBar.vertical.policy: row.expanded && !row.section ? QQC.ScrollBar.AsNeeded : QQC.ScrollBar.AlwaysOff
                                    NoteEditor {
                                        id: editor
                                        theme: root.theme
                                        section: row.section
                                        background: Item {}
                                        property string clipAsset: row.asset
                                        readonly property bool entryFocused: activeFocus || statusBar.activeFocus
                                        function toggleExpanded() {
                                            if (activeFocus || statusBar.activeFocus) row.toggleExpanded()
                                        }
                                        width: entryScroll.availableWidth
                                        text: row.entryText
                                        enabled: !root.busy && row.entryStatus !== "recording" && row.entryStatus !== "transcribing"
                                        captureControlDelete: (root.keys.delete_note !== undefined ? root.keys.delete_note : ["Ctrl+Delete"]).indexOf("Ctrl+Delete") >= 0
                                        onControlDeletePressed: root.deleteFocusedNote()
                                        placeholderText: activeFocus || row.section ? "" : row.entryStatus === "recording" ? "listening…" : row.entryStatus === "transcribing" ? "transcribing…" : row.entryStatus === "no-transcript" ? "no transcript" : row.asset ? "Type a note or hold HOME to narrate" : ""
                                        onTextChanged: if (activeFocus && text !== row.entryText) root.markEdit(row.entryId, text)
                                        onActiveFocusChanged: if (activeFocus) { root.currentEditor = editor; root.currentEntry = row.entryId; root.preview(editor, row.asset, false) }
                                    }
                                }
                                NoteButton { visible: row.section; theme: root.theme; iconName: "close"; bare: true; implicitWidth: 24; implicitHeight: 20; opacity: rowHover.hovered || activeFocus ? 1 : 0; tooltipText: "Delete section heading; keep its notes"; onClicked: root.saveAll(function(ok) { if (ok) root.request("delete-section", {session:root.session,token:root.token,entryId:row.entryId}) }) }
                            }
                        }
                    }
                }
            }
            QQC.ScrollView {
                id: draftScroll
                visible: !root.minimized && !!root.session
                Layout.fillWidth: true
                implicitHeight: Math.min(122, draft.implicitHeight)
                contentWidth: availableWidth
                clip: true
                QQC.ScrollBar.horizontal.policy: QQC.ScrollBar.AlwaysOff
                QQC.ScrollBar.vertical.policy: QQC.ScrollBar.AlwaysOff
                NoteEditor {
                    id: draft
                    theme: root.theme
                    bordered: true
                    width: draftScroll.availableWidth
                    placeholderText: "new note"
                    enabled: !root.busy
                    onTextChanged: if (!root.restoring) { root.draftDirty = true; root.status = "Saving draft…"; root.hasError = false; autosave.restart() }
                    onActiveFocusChanged: if (activeFocus) { root.currentEditor = null; root.currentEntry = -1; root.previewAsset = "" }
                }
            }
            Rectangle {
                visible: !root.hasError && !!root.status
                Layout.fillWidth: true
                implicitHeight: draftStatus.implicitHeight + 6
                color: root.theme.background
                Text { id: draftStatus; anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 5; rightMargin: 5; topMargin: 3 } text: root.status; wrapMode: Text.Wrap; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize }
            }
        }
        QQC.Popup {
            id: projectPopup
            popupType: QQC.Popup.Window
            parent: panel.contentItem
            x: panel.width - width
            y: toolbar.height
            width: Math.max(276, panel.width)
            padding: 3
            opacity: root.panelOpacity
            visible: root.choosing && root.shown && !root.minimized
            closePolicy: QQC.Popup.CloseOnEscape | QQC.Popup.CloseOnPressOutside
            onClosed: root.choosing = false
            background: Rectangle { color: root.theme.background; border.width: 1; border.color: root.theme.hair }
            component ProjectChoice: NoteButton {
                theme: root.theme
                Layout.fillWidth: true
                leftPadding: 8
                rightPadding: 8
                topPadding: 2
                bottomPadding: 2
                implicitHeight: Math.max(28, implicitContentHeight + 4)
                background: Rectangle { color: parent.selected ? root.theme.selectionBg : (parent.hovered ? root.withAlpha(root.theme.foreground, 0.08) : "transparent") }
                contentItem: Text { text: parent.text; color: parent.selected ? root.theme.selectionFg : parent.textColor; font: parent.font; elide: Text.ElideMiddle; verticalAlignment: Text.AlignVCenter }
            }
            contentItem: ColumnLayout {
                spacing: 0
                ProjectChoice { text: "Choose project"; textColor: root.theme.foreground; selected: !root.session; onClicked: root.choosing = false }
                Repeater {
                    model: root.projects
                    ProjectChoice {
                        required property string modelData
                        text: root.projectLabel(modelData)
                        textColor: root.theme.foreground
                        tooltipText: modelData
                        selected: root.session === modelData
                        enabled: !root.busy
                        onClicked: root.selectProject(modelData)
                    }
                }
                ProjectChoice { text: "New project"; textColor: root.theme.accent; onClicked: { root.choosing = false; root.chooseFolder(true) } }
                ProjectChoice { text: "Use existing folder"; textColor: root.theme.accent; onClicked: { root.choosing = false; root.chooseFolder(false) } }
            }
        }

        Instantiator {
            model: root.actions
            delegate: Shortcut {
                required property var modelData
                sequences: root.keys[modelData.id] !== undefined ? root.keys[modelData.id] : modelData.fallback
                context: Qt.WindowShortcut
                autoRepeat: modelData.id !== "delete_note"
                enabled: root.shown && panel.contentItem.Window.active && !newProjectDialog.visible && !folderDialog.visible
                onActivated: root.runAction(modelData.id)
            }
        }
        Shortcut { sequence: "Escape"; enabled: root.shown && panel.contentItem.Window.active && !newProjectDialog.visible && !folderDialog.visible; onActivated: root.escapePanel() }
        QQC.Dialog {
            id: newProjectDialog
            popupType: QQC.Popup.Window
            anchors.centerIn: parent
            width: Math.min(404, (panel.screen ? panel.screen.width : 440) - 36)
            modal: true
            opacity: root.panelOpacity
            title: "New project"
            onOpened: projectName.forceActiveFocus()
            onAccepted: if (projectName.text.trim()) root.actionCommand(["create-project", root.parentFolder, projectName.text.trim(), root.session], function(ok) { if (ok) { root.choosing = false; root.focusNote() } else newProjectDialog.open() })
            background: Rectangle { color: root.theme.background; border.width: 1; border.color: root.theme.hair }
            contentItem: ColumnLayout {
                Text { text: root.parentFolder; Layout.fillWidth: true; wrapMode: Text.WrapAnywhere; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                NoteField { id: projectName; theme: root.theme; Layout.fillWidth: true; placeholderText: "Project name"; onAccepted: if (text.trim()) newProjectDialog.accept() }
            }
            footer: RowLayout {
                Item { Layout.fillWidth: true }
                NoteButton { theme: root.theme; text: "Cancel"; onClicked: newProjectDialog.reject() }
                NoteButton { theme: root.theme; text: "Create"; enabled: !!projectName.text.trim(); onClicked: newProjectDialog.accept() }
            }
        }
    }
    PanelWindow {
        id: previewWindow
        visible: root.shown && !root.minimized
        screen: panel.screen
        anchors { top: true; right: true }
        margins { top: Math.min(root.topGap + root.previewY, Math.max(root.topGap, (screen ? screen.height : 900) - implicitHeight - 12)); right: Math.min(root.rightGap + panel.width + 4, Math.max(root.rightGap, (screen ? screen.width : 1400) - implicitWidth - 12)) }
        implicitWidth: root.previewAsset ? Math.min(clipImage.width + 2, Math.max(1, (screen ? screen.width : 1400) - root.rightGap - 12)) : 1
        implicitHeight: root.previewAsset ? Math.min(clipImage.height + 2, Math.max(1, (screen ? screen.height : 900) - root.topGap - 12)) : 1
        color: "transparent"
        mask: Region {
            width: root.previewAsset ? previewWindow.width : 0
            height: root.previewAsset ? previewWindow.height : 0
        }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "ui-notes"
        WlrLayershell.layer: root.screensaverVisible ? WlrLayer.Top : WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        contentItem.opacity: root.panelOpacity
        Rectangle {
            anchors.fill: parent
            visible: !!root.previewAsset
            border.width: 1
            border.color: root.theme.hair
            color: root.theme.background
            QQC.ScrollView {
                anchors.fill: parent
                anchors.margins: 1
                clip: true
                contentWidth: clipImage.width
                contentHeight: clipImage.height
                QQC.ScrollBar.horizontal.policy: QQC.ScrollBar.AlwaysOff
                QQC.ScrollBar.vertical.policy: QQC.ScrollBar.AlwaysOff
                Image {
                    id: clipImage
                    source: root.previewAsset ? root.fileUrl(root.session + "/" + root.previewAsset) : ""
                    asynchronous: true
                    width: root.previewLogicalSize.width > 0 ? root.previewLogicalSize.width : implicitWidth / (previewWindow.screen && previewWindow.screen.devicePixelRatio > 0 ? previewWindow.screen.devicePixelRatio : 1)
                    height: root.previewLogicalSize.height > 0 ? root.previewLogicalSize.height : implicitHeight / (previewWindow.screen && previewWindow.screen.devicePixelRatio > 0 ? previewWindow.screen.devicePixelRatio : 1)
                }
            }
        }
    }
    PanelWindow {
        id: helpWindow
        visible: root.shown && root.helping && !root.minimized
        screen: panel.screen
        anchors { top: true; right: true }
        margins { top: root.topGap; right: Math.min(root.rightGap + panel.width + 4, Math.max(root.rightGap, (screen ? screen.width : 1400) - implicitWidth - 12)) }
        implicitWidth: Math.min(460, Math.max(1, (screen ? screen.width : 1400) - root.rightGap - 12))
        implicitHeight: Math.min(helpContent.implicitHeight, 520, Math.max(1, (screen ? screen.height : 900) - root.topGap - 30)) + 18
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "ui-notes"
        WlrLayershell.layer: root.screensaverVisible ? WlrLayer.Top : WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        contentItem.opacity: root.panelOpacity
        Rectangle {
            anchors.fill: parent
            color: root.theme.background
            border.width: 1
            border.color: root.theme.hair
            QQC.ScrollView {
                anchors.fill: parent
                anchors.margins: 9
                clip: true
                contentWidth: availableWidth
                QQC.ScrollBar.horizontal.policy: QQC.ScrollBar.AlwaysOff
                QQC.ScrollBar.vertical.policy: QQC.ScrollBar.AsNeeded
                ColumnLayout {
                    id: helpContent
                    width: parent.width
                    spacing: 3
                    Text { text: "Keyboard shortcuts"; Layout.fillWidth: true; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize; font.bold: true }
                    Repeater {
                        model: root.actions
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { text: modelData.label; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                            Text { text: (root.keys[modelData.id] !== undefined ? root.keys[modelData.id] : modelData.fallback).join(", ") || "Disabled"; Layout.maximumWidth: helpWindow.width * 0.48; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignRight; color: root.theme.accent; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                        }
                    }
                    Text { text: "Global bindings"; Layout.fillWidth: true; Layout.topMargin: 7; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize; font.bold: true }
                    Repeater {
                        model: [
                            {label:"Open or end session", control:"Super+Shift+U"},
                            {label:"Enter or release keyboard focus", control:"Super+U"},
                            {label:"Hold to dictate; release to stop", control:"Home"}
                        ]
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { text: modelData.label; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                            Text { text: modelData.control; Layout.maximumWidth: helpWindow.width * 0.48; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignRight; color: root.theme.accent; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                        }
                    }
                    Text { text: "Suggested Hyprland bindings; configure them in your desktop settings. Dictation targets the focused editor."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize }
                    Text { text: "Editor and mouse"; Layout.fillWidth: true; Layout.topMargin: 7; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize; font.bold: true }
                    Repeater {
                        model: [
                            "New drafts and edits save automatically after a typing pause.",
                            "Click a note's status bar to show or hide its clip preview.",
                            "Focus the status bar and press Space or Return to toggle its preview.",
                            "Double-click a status bar to expand or collapse the note.",
                            "Edit section titles in place; hover a section and click × to remove only its heading.",
                            "Escape closes help, a clip preview, search, or the project menu."
                        ]
                        Text {
                            required property string modelData
                            text: "• " + modelData
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: root.theme.foreground
                            font.family: root.theme.fontFamily
                            font.pointSize: root.theme.fontPointSize
                        }
                    }
                    Text { text: "Projects"; Layout.fillWidth: true; Layout.topMargin: 7; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize; font.bold: true }
                    Text { text: "Open Projects to switch among known folders, create a project, or use an existing folder. Pending edits save before switching."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                    Text { text: "Command line"; Layout.fillWidth: true; Layout.topMargin: 7; color: root.theme.muted; font.family: root.theme.fontFamily; font.pointSize: root.theme.smallPointSize; font.bold: true }
                    Text { text: "Run ui-notes help for session, project, capture, AI, dictation, and panel commands."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                    Text { visible: !!root.keysWarning; text: root.keysWarning; Layout.fillWidth: true; Layout.topMargin: 4; wrapMode: Text.Wrap; color: root.theme.red; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                }
            }
        }
    }
}
