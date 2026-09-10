#!/usr/bin/env python3
"""Exercise install, update, settings preservation, and removal in temporary directories."""

import os
from pathlib import Path
import subprocess
import tempfile


source = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omatate-install-test-") as temporary:
    root = Path(temporary)
    home = root / "home"
    plugin = home / ".config/omarchy/plugins/cordrogue.omatate"
    launchers = home / ".local/bin"
    launchers.mkdir(parents=True)
    config = home / ".config/omatate/ai"
    config.parent.mkdir(parents=True)
    config.write_text("on\n")
    project = home / "Documents/omatate/existing"
    project.mkdir(parents=True)
    (project / "notes.md").write_text("Keep my notes.\n")
    env = dict(os.environ, HOME=str(home), OMATATE_PLUGIN_DIR=str(plugin),
               OMATATE_INSTALL_DIR=str(launchers), CARGO_NET_OFFLINE="true",
               RUSTUP_HOME=os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
               CARGO_HOME=os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))

    def run(script, option, success=True):
        result = subprocess.run([str(source / script), option], env=env,
                                capture_output=True, text=True)
        assert (result.returncode == 0) == success, result.stdout + result.stderr
        return result.stdout + result.stderr

    run("install.sh", "--no-enable")
    for name in ("omatate", "omatate-panel"):
        assert (launchers / name).resolve() == plugin / "bin" / name
    for name in ("omatate",):
        result = subprocess.run([str(launchers / name), "ai"], env=env,
                                capture_output=True, text=True, check=True)
        assert result.stdout == "on\n"
    run("install.sh", "--no-enable")
    edited = plugin / "README.md"
    edited.write_text(edited.read_text() + "\nMy local edit.\n")
    assert "installed files were edited" in run("install.sh", "--no-enable", False)
    unrelated = plugin / "personal.txt"
    unrelated.write_text("keep")
    run("uninstall.sh", "--no-disable")
    assert "My local edit." in edited.read_text()
    assert unrelated.read_text() == "keep"
    assert not (plugin / "target/release/omatate").exists()
    assert all(not p.is_symlink() for p in launchers.iterdir())
    assert config.read_text() == "on\n"
    assert (project / "notes.md").read_text() == "Keep my notes.\n"
    # An unrelated launcher must stop installation before touching its contents.
    (launchers / "omatate").write_text("unrelated executable")
    env["OMATATE_PLUGIN_DIR"] = str(root / "another-plugin")
    assert "refusing to replace a file" in run("install.sh", "--no-enable", False)
    assert (launchers / "omatate").read_text() == "unrelated executable"
print("PASS: install, update, settings preservation, edited-file protection, removal")
