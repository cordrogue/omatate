import QtQuick
import Quickshell
import Quickshell.Io
import "lib/Theme.mjs" as Theme

Item {
    id: root
    property var value: Theme.resolve("", "", "", "")
    property bool refreshing: false
    property var paths: Theme.sources(Quickshell.env("HOME"), Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config")
    FileOps { id: files }
    function refresh() {
        if (refreshing) return
        refreshing = true
        var home = Quickshell.env("HOME")
        Promise.all([
            files.read(paths[0], true).catch(() => null), files.read(paths[1], true).catch(() => null),
            files.read(home + "/.config/omarchy/shell.toml", true).catch(() => null),
            files.run(["timeout", "2s", "ghostty", "+show-config"], {allowFailure: true, timeout: 3000}),
            files.run(["timeout", "2s", "omarchy", "font", "current"], {allowFailure: true, timeout: 3000})
        ]).then(function(results) {
            var ghostty = results[3].code === 0 ? results[3].stdout : ""
            root.value = Theme.resolve(results[0] || results[1] || "", results[2] || "", ghostty, results[4].code === 0 ? results[4].stdout : "")
            var extra = Theme.configSources(ghostty, home)
            var next = root.paths.slice()
            extra.forEach(function(path) { if (next.indexOf(path) < 0) next.push(path) })
            if (next.length !== root.paths.length) root.paths = next
            root.refreshing = false
        }, function(error) { root.refreshing = false; console.warn("omatate theme:", error.message) })
    }
    Repeater {
        model: root.paths
        delegate: Item {
            required property string modelData
            FileView { path: modelData; preload: false; watchChanges: true; printErrors: false; onFileChanged: debounce.restart() }
        }
    }
    Timer { id: debounce; interval: 150; onTriggered: root.refresh() }
    Component.onCompleted: refresh()
}
