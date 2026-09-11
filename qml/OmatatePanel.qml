import QtQuick
import QtQuick.Controls as QQC
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQml.Models
import Qt5Compat.GraphicalEffects
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import "lib/Settings.mjs" as Settings

Item {
    id: root
    property var service: null
    property var shell: null
    property string initialError: ""
    signal closed()
    property bool shown: false
    property bool minimized: false
    property bool choosing: false
    property bool searching: false
    property bool helping: false
    property bool busy: false
    property bool restoring: false
    property bool draftDirty: false
    property bool focusAllowed: true
    property bool hiddenFocus: false
    property string session: ""
    property string token: ""
    property bool ai: false
    property var projects: []
    property var entries: []
    property var dirtyEdits: ({})
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
    property var hideReply: null
    property int hideRetries: 0
    property bool newFolder: false
    property string parentFolder: ""
    property var keys: Settings.defaults()
    property string keysWarning: ""
    property int opacityStep: 0
    readonly property var opacityLevels: [1.0, 0.8, 0.6, 0.4]
    readonly property real panelOpacity: opacityLevels[opacityStep]
    readonly property var actions: [
        {id:"save_note", label:"Save note"},
        {id:"delete_note", label:"Delete focused note"},
        {id:"focus_note", label:"Focus note"},
        {id:"search", label:"Search"},
        {id:"projects", label:"Projects"},
        {id:"new_project", label:"New project"},
        {id:"open_folder", label:"Open folder"},
        {id:"add_section", label:"Add section"},
        {id:"clip", label:"Clip"},
        {id:"toggle_ai", label:"Toggle AI"},
        {id:"cycle_opacity", label:"Change opacity"},
        {id:"minimize", label:"Minimize or expand"},
        {id:"end_session", label:"End session"},
        {id:"previous_entry", label:"Previous entry"},
        {id:"next_entry", label:"Next entry"},
        {id:"preview", label:"Preview"},
        {id:"expand_note", label:"Expand or collapse note"},
        {id:"help", label:"Help"}
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
    readonly property var theme: appearance.value
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
    function withAlpha(colorValue, opacity) { var c = Qt.tint("transparent", colorValue); return Qt.rgba(c.r, c.g, c.b, opacity) }
    function fail(message) { status = message; hasError = true }
    function request(cmd, data, done) {
        // Every mutation publishes its state through onChanged before resolving.
        service.request(cmd, data).then(function() {
            if (done) done(true)
        }, function(error) {
            root.fail(error.message)
            if (done) done(false)
        })
    }

    function applyState(state) {
        var switched = session !== state.session || token !== state.token
        if (switched && (draftDirty || Object.keys(dirtyEdits).length)) {
            fail("The active project changed while edits were pending. Save or copy your text before reopening.")
            return
        }
        restoring = true
        session = state.session
        token = state.token
        ai = state.ai
        projects = state.projects || []
        keys = state.keys || Settings.defaults()
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
                entryStatus:e.status || "", contextStatus:e.context_status || "", asset:e.asset || "", aiEntry:e.ai !== false}
            if (dirtyEdits[e.id] !== undefined) row.entryText = dirtyEdits[e.id]
            // Both lists are sorted by unique id and removed ids are gone, so a
            // mismatch here is always a new entry.
            if (i >= entryModel.count) entryModel.append(row)
            else if (entryModel.get(i).entryId !== e.id) entryModel.insert(i, row)
            else entryModel.set(i, row)
        }
        restoring = false
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
            // The service processes requests in order. This reply also waits for
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
        service.command(args).then(function() { if (done) done(true) }, function(error) { root.fail(error.message); if (done) done(false) })
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
    function takeFocus() {
        releaseTimer.stop()
        focusAllowed = true
        shown = true
        Qt.callLater(function() {
            if (!shown || !focusAllowed || screensaverVisible) return
            panelFocusGrab.active = true
            focusNote()
        })
    }
    function noteFocused() { return shown && panel.contentItem.Window.active && draft.activeFocus }
    // Called after pending editor writes have drained. Never request storage
    // from these hooks: the service may already own the command queue.
    function servicePanel(cmd) {
        return new Promise(function(resolve, reject) {
            switch (cmd) {
            case "open": root.takeFocus(); resolve({ok:true}); break
            case "ping": resolve({ok:true,pid:Quickshell.processId}); break
            case "focus": resolve({ok:true,active:root.shown && panel.contentItem.Window.active,focus:root.noteFocused() ? "note" : "none"}); break
            case "flush": case "reload": resolve({ok:true}); break
            case "restyle": appearance.refresh(); resolve({ok:true}); break
            case "hide":
                root.hiddenFocus = panel.contentItem.Window.active; root.shown = false; panelFocusGrab.active = false
                root.previewAsset = ""; root.helping = false
                // The service serializes panel commands, so one hide is pending at a time.
                root.hideReply = function(reply) { if (reply.ok) resolve(reply); else reject(new Error(reply.error)) }
                root.hideRetries = 0; hideTimer.restart(); break
            case "show":
                root.focusAllowed = root.hiddenFocus; root.shown = true
                if (root.hiddenFocus) Qt.callLater(function() { if (root.shown && root.focusAllowed && !root.screensaverVisible) panelFocusGrab.active = true })
                else releaseTimer.restart()
                resolve({ok:true}); break
            case "toggle-focus":
                if (panel.contentItem.Window.active && root.shown) { root.focusAllowed = false; panelFocusGrab.active = false; root.helping = false; root.previewAsset = ""; releaseTimer.restart() }
                else root.takeFocus()
                resolve({ok:true}); break
            case "quit":
                panelFocusGrab.active = false; root.shown = false; root.previewAsset = ""; root.helping = false
                root.closed()
                resolve({ok:true}); break
            default: reject(new Error("Unknown panel command: " + cmd))
            }
        })
    }
    function stopSession() {
        if (busy) return
        busy = true
        saveAll(function(ok) {
            if (!ok) { busy = false; return }
            // A successful stop releases this panel through the service's quit hook.
            if (session) cli(["stop"], function(success) { if (!success) busy = false })
            else { panelFocusGrab.active = false; shown = false; previewAsset = ""; helping = false; closed() }
        })
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
            if (matches[index] !== true) continue
            notes.positionViewAtIndex(index, ListView.Contain)
            var row = notes.itemAtIndex(index)
            if (row) {
                row.editor.forceActiveFocus()
                return
            }
        }
        focusNote()
    }
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
        case "cycle_opacity": opacityStep = (opacityStep + 1) % opacityLevels.length; break
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

    // Warnings arrive through the resident root, which forwards them to fail().
    Connections {
        target: root.service
        function onChanged(state) { root.applyState(state) }
    }
    OmatateTheme { id: appearance }
    Timer { id: autosave; interval: 450; onTriggered: root.saveAll() }

    Timer { id: releaseTimer; interval: 180; onTriggered: root.focusAllowed = true }
    HyprlandFocusGrab { id: panelFocusGrab; windows: [panel] }
    // Grab only long enough to enter the panel. OnDemand then lets the mouse
    // transfer focus to another window while the notes stay visible.
    Timer {
        interval: 100
        running: panelFocusGrab.active && panel.contentItem.Window.active
        onTriggered: panelFocusGrab.active = false
    }
    onScreensaverVisibleChanged: {
        if (screensaverVisible) {
            restoreFocusAfterScreensaver = panel.contentItem.Window.active
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
            var visible = panel.backingWindowVisible || previewWindow.backingWindowVisible || helpWindow.backingWindowVisible
            if (visible && ++root.hideRetries * interval < 1200) { restart(); return }
            var reply = root.hideReply; root.hideReply = null
            if (reply) reply(visible ? {ok:false, error:"panel did not hide"} : {ok:true})
        }
    }
    ListModel { id: entryModel }
    FolderDialog {
        id: folderDialog
        // GTK's folder picker can abort the shell while enumerating network locations.
        options: FolderDialog.DontUseNativeDialog
        parentWindow: panel.contentItem.Window.window
        popupType: QQC.Popup.Window
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
        WlrLayershell.namespace: "omatate"
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
                                        captureControlDelete: root.keys.delete_note.indexOf("Ctrl+Delete") >= 0
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
                    objectName: "omatate-draft"
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
                sequences: root.keys[modelData.id]
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
        WlrLayershell.namespace: "omatate"
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
        WlrLayershell.namespace: "omatate"
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
                            Text { text: root.keys[modelData.id].join(", ") || "Disabled"; Layout.maximumWidth: helpWindow.width * 0.48; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignRight; color: root.theme.accent; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
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
                    Text { text: "Run omatate help for session, project, capture, AI, dictation, and panel commands."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: root.theme.foreground; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                    Text { visible: !!root.keysWarning; text: root.keysWarning; Layout.fillWidth: true; Layout.topMargin: 4; wrapMode: Text.Wrap; color: root.theme.red; font.family: root.theme.fontFamily; font.pointSize: root.theme.fontPointSize }
                }
            }
        }
    }
    Component.onCompleted: {
        applyState(service.model.snapshot())
        if (initialError) fail(initialError)
    }
}
