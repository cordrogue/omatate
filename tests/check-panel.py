#!/usr/bin/env python3
"""Check the real panel and CLI in a disposable Quickshell instance on Wayland."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
if not os.environ.get("WAYLAND_DISPLAY"):
    raise SystemExit("This check needs a running Wayland desktop")
with tempfile.TemporaryDirectory(prefix="omatate-panel-") as temporary:
    root = Path(temporary)
    for name in ["qml", "share", "bin"]:
        shutil.copytree(repo / name, root / name)
    shutil.copy(repo / "Plugin.qml", root / "Plugin.qml")
    for name in ["home", "runtime", "project with spaces", "other", "tools"]:
        (root / name).mkdir(mode=0o700)
    (root / "shell.qml").write_text('''import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    id: root
    Plugin { id: plugin }
    TestCase { id: assertions; when: false }
    IpcHandler {
        target: "shell"
        function summon(id: string, payload: string): string { plugin.open(payload); return "ok" }
        function hide(id: string): string { plugin.close(); return "ok" }
        function state(): string { var p=plugin.panel; return JSON.stringify({ready:plugin.ready,session:p?p.session:"",entries:p?p.entries:[],error:p?(p.hasError?p.status:""):(plugin.hasError?plugin.status:""),opened:plugin.opened}) }
        function note(text: string): void {
            var field=assertions.findChild(plugin,"omatate-draft")
            if (!field) throw new Error("Draft field not found")
            field.text=text;field.textEdited();plugin.panel.submitNote()
        }
        function quit(): void { Qt.quit() }
    }
}
''')
    shim = root / "tools/omarchy-shell"
    shim.write_text('#!/bin/sh\nexec qs ipc -n -p "$OMATATE_TEST_ROOT/shell.qml" call -- "$@"\n')
    shim.chmod(0o755)
    env = os.environ.copy()
    display = env["WAYLAND_DISPLAY"]
    if not display.startswith("/"):
        display = env["XDG_RUNTIME_DIR"] + "/" + display
    env.update({"HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "home/.config"),
        "XDG_STATE_HOME": str(root / "home/.local/state"), "XDG_CACHE_HOME": str(root / "home/.cache"), "XDG_RUNTIME_DIR": str(root / "runtime"),
        "WAYLAND_DISPLAY": display, "OMATATE_TEST_ROOT": str(root), "PATH": str(root / "tools") + ":" + env["PATH"],
        "QT_QPA_PLATFORM": "wayland", "QT_QPA_PLATFORMTHEME": "", "QT_STYLE_OVERRIDE": "Fusion", "QT_QUICK_CONTROLS_STYLE": "Basic"})
    def ipc(method, *args):
        return subprocess.check_output(["qs", "ipc", "-n", "-p", str(root / "shell.qml"), "call", "shell", method, *args], env=env, text=True, stderr=subprocess.STDOUT, timeout=5).strip()
    def cli(*args):
        result = subprocess.run([str(root / "bin/omatate"), *args], env=env, text=True, capture_output=True, timeout=10)
        if result.returncode:
            raise AssertionError(f"CLI {args}: {result.stderr} {result.stdout}")
        return result.stdout.strip()
    with (root / "log").open("w+") as log:
        process = subprocess.Popen(["qs", "-p", str(root / "shell.qml"), "--no-color"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError("Panel process exited during startup")
                try:
                    state=json.loads(ipc("state"))
                    if state["ready"]: break
                except (subprocess.SubprocessError,json.JSONDecodeError): pass
                time.sleep(.05)
            else: raise AssertionError("Panel did not become ready")
            project=str(root / "project with spaces")
            assert cli("open-project",project)==project
            text="A QML note with literal $() and 'quotes' λ"
            ipc("note",text)
            for _ in range(100):
                state=json.loads(ipc("state"))
                if state["entries"]: break
                time.sleep(.05)
            assert state["entries"][0]["transcript"]==text, state
            assert not state["error"], state
            assert cli("section","Details")=="2"
            assert cli("status")==project
            # The shell's hide closes the panel; the session survives and the next open restores it.
            ipc("hide","cordrogue.omatate")
            for _ in range(100):
                state=json.loads(ipc("state"))
                if not state["opened"]: break
                time.sleep(.05)
            assert not state["opened"] and state["session"]=="", state
            assert cli("status")==project
            cli("open")
            state=json.loads(ipc("state"))
            assert state["opened"] and state["session"]==project and state["entries"][0]["transcript"]==text, state
            cli("select-project",str(root / "other"),project)
            cli("select-project",project,str(root / "other"))
            assert json.loads(ipc("state"))["entries"][0]["transcript"]==text
            cli("stop")
            assert not json.loads(ipc("state"))["opened"]
            assert text in (root / "project with spaces/notes.md").read_text()
            ipc("quit")
            process.wait(timeout=5)
        except BaseException:
            log.seek(0)
            print(log.read())
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
    print("Panel and CLI integration passed")
