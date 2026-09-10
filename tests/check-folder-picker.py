#!/usr/bin/env python3
"""Open the production picker in a disposable Wayland panel for about two seconds.

Uses the existing compositor, with no Omatate backend or user project access.
This checks dialog behavior and geometry; it does not simulate keyboard input.
"""

import os
from pathlib import Path
import subprocess
import tempfile


source = (Path(__file__).resolve().parents[1] / "Plugin.qml").read_text()
start = source.index("    FolderDialog {")
end = source.index("\n    }", start) + len("\n    }")
dialog = source[start:end]

fixture = """import QtQuick
import QtQuick.Controls as QQC
import QtQuick.Dialogs
import QtTest
import Quickshell
import Quickshell.Wayland
ShellRoot {
    id: root
    property int step: 0
    property bool newFolder: false
    property string parentFolder: ""
    property string selectedProject: ""
    property int createCount: 0
    function localPath(url) { return decodeURIComponent(String(url).replace(/^file:\\/\\//, "")) }
    function selectProject(path) { selectedProject = path }
    function require(condition, message) {
        if (!condition) { console.error("PICKER_FAIL " + message); Qt.quit() }
        return condition
    }
    QtObject { id: newProjectDialog; function open() { ++root.createCount } }
    PanelWindow {
        id: panel
        visible: true
        implicitWidth: 180
        implicitHeight: 120
        anchors { top: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "omatate-picker-test"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
        Rectangle { anchors.fill: parent; color: "#333333" }
    }
    TestCase { id: inspector; name: "Inspector"; when: false }
DIALOG
    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            ++root.step
            if (root.step === 1) {
                if (!root.require(!!(folderDialog.options & FolderDialog.DontUseNativeDialog), "native dialog enabled")) return
                folderDialog.currentFolder = "file:///tmp"
                folderDialog.open()
            } else if (root.step === 2) {
                if (!root.require(folderDialog.visible && !!folderDialog.parentWindow, "picker did not open with a parent")) return
                var list = inspector.findChild(panel.contentItem.Window.window, "folderDialogListView")
                if (!root.require(!!list, "Qt Quick folder list missing")) return
                var popup = list.Window.window
                if (!root.require(popup !== folderDialog.parentWindow && popup.width > panel.width && popup.height > panel.height, "picker clipped to panel")) return
                console.log("PICKER_GEOMETRY", popup.width, popup.height, "parent", panel.width, panel.height)
                folderDialog.selectedFolder = "file:///tmp"
                folderDialog.accept()
                if (!root.require(root.selectedProject === "/tmp" && !folderDialog.visible, "existing project acceptance failed")) return
                root.newFolder = true
                folderDialog.open()
            } else if (root.step === 3) {
                folderDialog.selectedFolder = "file:///tmp"
                folderDialog.accept()
                if (!root.require(root.parentFolder === "/tmp" && root.createCount === 1, "new project acceptance failed")) return
                folderDialog.open()
            } else if (root.step === 4) {
                folderDialog.reject()
                if (!root.require(!folderDialog.visible && root.createCount === 1, "cancel changed project state")) return
                console.log("PICKER_PASS open, existing/new acceptance, cancel, separate popup")
                Qt.quit()
            }
        }
    }
}
""".replace("DIALOG", dialog)

env = os.environ.copy()
display = env.get("WAYLAND_DISPLAY", "")
if not display:
    raise SystemExit("A running Wayland session is required.")
if not os.path.isabs(display):
    display = str(Path(env["XDG_RUNTIME_DIR"]) / display)
with tempfile.TemporaryDirectory(prefix="omatate-picker-test-") as temporary:
    directory = Path(temporary)
    runtime = directory / "runtime"
    runtime.mkdir(mode=0o700)
    config = directory / "shell.qml"
    config.write_text(fixture)
    env.pop("QSG_RHI_BACKEND", None)
    env.update(
        QT_QPA_PLATFORM="wayland",
        QT_QPA_PLATFORMTHEME="",
        QT_STYLE_OVERRIDE="Fusion",
        QT_QUICK_BACKEND="software",
        QT_QUICK_CONTROLS_STYLE="Basic",
        WAYLAND_DISPLAY=display,
        XDG_RUNTIME_DIR=str(runtime),
        XDG_CACHE_HOME=str(directory / "cache"),
    )
    result = subprocess.run(
        ["quickshell", "--no-color", "--path", str(config)],
        env=env, capture_output=True, text=True, timeout=10,
    )
    output = result.stdout + result.stderr
    print(output, end="")
    if result.returncode or "PICKER_FAIL" in output or "PICKER_PASS" not in output:
        raise SystemExit(result.returncode or 1)
