#!/usr/bin/env python3
"""Exercise the production QML service with isolated storage and no desktop UI."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omatate-qml-") as temporary:
    root = Path(temporary)
    shutil.copytree(repo / "qml", root / "qml")
    shutil.copytree(repo / "share", root / "share")
    for name in ["home", "runtime", "project", "other"]:
        (root / name).mkdir(mode=0o700)
    (root / "shell.qml").write_text('''import QtQuick
import Quickshell
import "qml" as Notes
ShellRoot {
    id: root
    property bool opened: false
    property var owner: ({})
    property string base: Quickshell.env("OMATATE_TEST_ROOT")
    property string large: "λ line of text that repeats until the write no longer fits one pipe buffer\n".repeat(20000)
    function servicePanel(command) { return Promise.resolve({ok:true}) }
    function noteFocused() { return false }
    function require(value, message) { if (!value) throw new Error(message) }
    Notes.FileOps { id: files }
    Notes.OmatateService { id: service; host: root; onWarning: message => console.error("WARNING", message) }
    Timer {
        interval: 50; running: true
        onTriggered: {
            service.command(["open-project", root.base + "/project"])
            .then(() => service.request("snapshot", {}))
            .then(function(state) {
                root.owner = {session:state.session, token:state.token}
                return service.request("draft", Object.assign({text:"An unfinished draft"}, root.owner))
            }).then(() => service.request("note", Object.assign({text:"Saved with QML λ"}, root.owner)))
            .then(function(state) {
                root.require(state.entries.length === 1 && state.draft === "", "Note or draft mismatch")
                return Promise.all([service.request("edit", Object.assign({entryId:1,text:"Updated"}, root.owner)),
                    service.request("draft", Object.assign({text:"Keep this draft"}, root.owner))])
            }).then(() => service.command(["select-project", root.base + "/other", root.base + "/project"]))
            .then(function() {
                return service.request("note", Object.assign({text:"Stale"}, root.owner))
                    .then(function() { throw new Error("Stale mutation was accepted") }, function(error) { root.require(error.message.indexOf("changed") >= 0, error.message) })
            }).then(() => service.command(["select-project", root.base + "/project", root.base + "/other"]))
            .then(() => service.request("snapshot", {}))
            .then(function(state) {
                root.require(state.draft === "Keep this draft" && state.entries[0].transcript === "Updated", "Draft or edit lost")
                return files.run(["sh", "-c", "printf '%s' \\\"$1\\\"; printf '%s' error >&2; exit 7", "test", "literal $() ' \\\" λ"], {allowFailure:true})
            }).then(function(result) {
                root.require(result.code === 7 && result.stderr === "error" && result.stdout.indexOf("literal $()") === 0, "Process result lost")
                return files.run(["/does/not/exist"], {allowFailure:true})
            }).then(function(result) {
                root.require(result.code !== 0, "Missing command returned success")
                return files.write(root.base + "/empty", "")
            }).then(() => files.read(root.base + "/empty"))
            .then(function(text) {
                root.require(text === "", "Empty file write failed")
                // A symlink planted at a write target is replaced, never followed.
                return files.run(["sh", "-c", "printf keep > \\"$1/victim\\" && ln -s \\"$1/victim\\" \\"$1/link\\" && ln -s \\"$1/missing\\" \\"$1/dangling\\"", "test", root.base])
            }).then(() => files.write(root.base + "/link", "changed"))
            .then(() => files.write(root.base + "/dangling", "created"))
            .then(() => files.write(root.base + "/large", root.large))
            .then(() => files.read(root.base + "/large"))
            .then(function(text) {
                root.require(text === root.large, "Large write was truncated or altered")
                console.log("SERVICE_OK"); Qt.quit()
            })
            .catch(function(error) { console.error("SERVICE_FAIL", error.message); Qt.quit() })
        }
    }
    Timer { interval: 15000; running: true; onTriggered: { console.error("SERVICE_TIMEOUT"); Qt.quit() } }
}
''')
    env = os.environ.copy()
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "QSG_RHI_BACKEND"]:
        env.pop(name, None)
    env.update({
        "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "home/.config"),
        "XDG_STATE_HOME": str(root / "home/.local/state"), "XDG_CACHE_HOME": str(root / "home/.cache"), "XDG_RUNTIME_DIR": str(root / "runtime"),
        "OMATATE_TEST_ROOT": str(root), "QT_QPA_PLATFORM": "offscreen", "QT_QPA_PLATFORMTHEME": "",
        "QT_STYLE_OVERRIDE": "Fusion", "QT_QUICK_BACKEND": "software", "QT_QUICK_CONTROLS_STYLE": "Basic",
    })
    result = subprocess.run(["qs", "-p", str(root / "shell.qml"), "--no-color"], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20)
    if result.returncode or "SERVICE_OK" not in result.stdout or "SERVICE_FAIL" in result.stdout:
        raise SystemExit(result.stdout)
    assert (root / "project/.data/draft.txt").read_text() == "Keep this draft"
    assert "Updated" in (root / "project/notes.md").read_text()
    assert (root / "victim").read_text() == "keep", "write followed a symlink"
    assert not (root / "link").is_symlink() and (root / "link").read_text() == "changed"
    assert not (root / "dangling").is_symlink() and (root / "dangling").read_text() == "created"
    mask = os.umask(0); os.umask(mask)
    assert (root / "dangling").stat().st_mode & 0o777 == 0o666 & ~mask, "new file mode does not follow the umask"
    assert not [n for n in os.listdir(root) if ".XXXXXX" in n or n.startswith(("link.", "dangling.", "large."))], "stage directory left behind"
    print("Quickshell service integration passed")
