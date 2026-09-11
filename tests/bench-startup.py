#!/usr/bin/env python3
"""Measure what Omatate costs the Omarchy shell at startup.

Runs a disposable Quickshell instance on the current Wayland desktop with an
isolated home, runtime directory, and a seeded project registry. It reports:

  instantiate  synchronous QML instantiation of Plugin.qml (blocks the shell)
  ready        time from instantiation until the service has restored state
  processes    child processes still alive one second after ready
  rss          resident memory of the shell with and without the plugin

Usage: tests/bench-startup.py [--runs N] [--projects N] [--entries N]
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--runs", type=int, default=5)
parser.add_argument("--projects", type=int, default=4)
parser.add_argument("--entries", type=int, default=25)
args = parser.parse_args()
if not os.environ.get("WAYLAND_DISPLAY"):
    raise SystemExit("This benchmark needs a running Wayland desktop")

SHELL = '''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    id: root
    property real instantiateMs: -1
    property real readyMs: -1
    property real loadedAt: 0
    Loader { id: loader; active: false; source: "Plugin.qml" }
    IpcHandler {
        target: "bench"
        function load(): string {
            var t = Date.now(); loader.active = true; root.loadedAt = Date.now()
            root.instantiateMs = root.loadedAt - t
            loader.item.readyChanged.connect(function() { if (loader.item.ready && root.readyMs < 0) root.readyMs = Date.now() - root.loadedAt })
            if (loader.item.ready) root.readyMs = 0
            return "ok"
        }
        function state(): string { return JSON.stringify({instantiate: root.instantiateMs, ready: root.readyMs, hasError: loader.item ? loader.item.hasError : false, status: loader.item ? loader.item.status : ""}) }
        function quit(): void { Qt.quit() }
    }
}
'''

def rss_kib(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    return 0

def descendants(pid):
    try:
        out = subprocess.check_output(["ps", "-o", "pid=,args=", "--ppid", str(pid)], text=True)
    except subprocess.CalledProcessError:
        return []
    result = []
    for line in out.splitlines():
        child, _, cmd = line.strip().partition(" ")
        result.append(cmd)
        result += descendants(int(child))
    return result

def seed(root):
    projects = []
    for i in range(args.projects):
        project = root / f"project-{i}"
        (project / ".data").mkdir(parents=True)
        for sub in ["transcripts", "context", "logs"]:
            (project / ".data" / sub).mkdir()
        (project / ".data/id").write_text(f"token-{i}\n")
        lines = [json.dumps({"id": n + 1, "ts": "2026-09-01T10:00:00", "ai": False, "status": "done",
                             "transcript": f"Note {n + 1} in project {i}"}) for n in range(args.entries)]
        (project / ".data/entries.jsonl").write_text("".join(line + "\n" for line in lines))
        (project / "notes.md").write_text("# notes\n")
        projects.append(str(project))
    state = root / "home/.local/state/omatate"
    state.mkdir(parents=True)
    (state / "projects.json").write_text(json.dumps(projects) + "\n")
    if projects:
        (root / "runtime/omatate").mkdir(mode=0o700)
        (root / "runtime/omatate/session").write_text(projects[0] + "\n")

results = {"instantiate": [], "ready": [], "processes": [], "rss_base": [], "rss_plugin": []}
for run in range(args.runs):
    with tempfile.TemporaryDirectory(prefix="omatate-bench-") as temporary:
        root = Path(temporary)
        for name in ["qml", "share", "bin"]:
            shutil.copytree(repo / name, root / name)
        shutil.copy(repo / "Plugin.qml", root / "Plugin.qml")
        for name in ["home", "runtime"]:
            (root / name).mkdir(mode=0o700)
        (root / "shell.qml").write_text(SHELL)
        seed(root)
        env = os.environ.copy()
        display = env["WAYLAND_DISPLAY"]
        if not display.startswith("/"):
            display = env["XDG_RUNTIME_DIR"] + "/" + display
        env.update({"HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "home/.config"),
                    "XDG_STATE_HOME": str(root / "home/.local/state"), "XDG_CACHE_HOME": str(root / "home/.cache"),
                    "XDG_RUNTIME_DIR": str(root / "runtime"), "WAYLAND_DISPLAY": display,
                    "QT_QPA_PLATFORM": "wayland", "QT_QPA_PLATFORMTHEME": "", "QT_STYLE_OVERRIDE": "Fusion",
                    "QT_QUICK_CONTROLS_STYLE": "Basic"})
        def ipc(method):
            return subprocess.check_output(["qs", "ipc", "-n", "-p", str(root / "shell.qml"), "call", "bench", method],
                                           env=env, text=True, stderr=subprocess.STDOUT, timeout=5).strip()
        log = open(root / "log", "w+")
        process = subprocess.Popen(["qs", "-p", str(root / "shell.qml"), "--no-color"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            for _ in range(200):
                try:
                    json.loads(ipc("state")); break
                except (subprocess.SubprocessError, json.JSONDecodeError):
                    time.sleep(0.05)
            else:
                raise AssertionError("Shell did not start")
            time.sleep(1.5)
            results["rss_base"].append(rss_kib(process.pid))
            ipc("load")
            state = None
            for _ in range(400):
                state = json.loads(ipc("state"))
                if state["ready"] >= 0: break
                time.sleep(0.02)
            else:
                raise AssertionError(f"Plugin did not become ready: {state}")
            if state["hasError"]:
                raise AssertionError(f"Plugin reported an error: {state['status']}")
            time.sleep(1.0)
            resident = descendants(process.pid)
            results["rss_plugin"].append(rss_kib(process.pid))
            results["instantiate"].append(state["instantiate"])
            results["ready"].append(state["ready"])
            results["processes"].append(len(resident))
            if run == 0:
                print("Child processes still running one second after ready (first run):")
                for cmd in sorted(resident):
                    print("  " + cmd[:110])
            ipc("quit")
            process.wait(timeout=5)
            if run == 0:
                log.seek(0)
                noise = [line.rstrip() for line in log if "WARN" in line or "rror" in line]
                if noise:
                    print("Shell warnings (first run):")
                    for line in noise: print("  " + line[:160])
        except BaseException:
            log.seek(0); print(log.read()); raise
        finally:
            if process.poll() is None:
                process.terminate(); process.wait(timeout=5)

def med(key): return statistics.median(results[key])
print(f"\nRuns: {args.runs}, registry projects: {args.projects}, entries each: {args.entries}")
print(f"instantiate  median {med('instantiate'):6.0f} ms   (all: {results['instantiate']})")
print(f"ready        median {med('ready'):6.0f} ms   (all: {results['ready']})")
print(f"resident     median {med('processes'):6.0f} processes (all: {results['processes']})")
print(f"rss          shell {med('rss_base') / 1024:.1f} MiB -> with plugin {med('rss_plugin') / 1024:.1f} MiB (+{(med('rss_plugin') - med('rss_base')) / 1024:.1f} MiB)")
