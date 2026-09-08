# UI Notes

A floating notes panel for Omarchy Quattro. You keep it open beside a mockup,
type notes, group them into sections, and capture rectangles of the screen to
annotate. Optional push-to-talk dictation turns speech into notes, and optional
AI analysis attaches a description of what was on screen when you spoke.

Quickshell draws the panel inside the running `omarchy-shell`. A Rust backend
owns the command line, project storage, dictation, captures, and analysis. The
plugin ID is `cordrogue.ui-notes`.
The panel stays above regular and fullscreen applications. Omarchy's screensaver
covers it; notes and editor state remain open behind the saver and return when
it closes. The panel sits 4 pixels from the screen edge in fullscreen mode. With
the Omarchy bar visible, it sits 4 pixels below a top bar or 12 pixels left of a
right bar.

Every project is a folder you choose. Notes land in `notes.md` in that folder,
with the authoritative records under `.data/` and captures under `assets/`.
Nothing is stored in a database or a hidden cache.

## What it does on your machine

Read this section before installing. Everything runs as your user inside the
shell you already have. There is no service, no privileged step, and no network
code in this repository. The two optional features that reach outside the
machine do so through tools you install and configure yourself.

Screen capture. `ui-notes clip` runs `slurp` so you pick a rectangle, then
`grim` saves it into the project's `assets/` folder. Nothing captures without
you drawing a rectangle.

Dictation. Pressing the push-to-talk key runs `voxtype record start`, and
releasing it runs `voxtype record stop`. Voxtype owns the microphone and the
transcription. Whether audio stays local depends on how you configured
Voxtype. UI Notes never opens the microphone itself and has no setting to
disable dictation. If you do not want dictation, do not add the push-to-talk
binding.

AI screen analysis. This is on by default and only fires when the panel's new
note editor has focus and you press push-to-talk. UI Notes then screenshots the
entire focused monitor with `grim`, hides the panel first, and runs
`codex exec` in read-only sandbox mode with the screenshot attached and the
project folder as the working directory. Codex uses `gpt-5.6-sol` by default
and the account you configured for the `codex` CLI. Because the project
folder is the working directory, the Codex agent can also read files in that
folder if it decides to. Anything visible on that monitor, including other
windows, is in the image. The screenshot lives under the runtime directory and
is deleted about five minutes after capture. It never becomes a project asset.
Turn this off with:

```sh
ui-notes ai off
```

That writes `off` to `~/.config/ui-notes/ai`. With AI off, push-to-talk still
dictates, but no screenshot is taken and Codex is never run. `ui-notes ai`
prints the current mode.

Files created outside your project folders:

| Path | Purpose |
| --- | --- |
| `~/.config/omarchy/plugins/cordrogue.ui-notes/` | The installed plugin |
| `~/.local/bin/ui-notes`, `~/.local/bin/ui-notes-panel` | Links to the plugin's launchers |
| `~/.local/state/ui-notes/projects.json` | Paths of projects you have opened |
| `~/.config/ui-notes/ai` | AI mode, only after you change it |
| `~/.config/ui-notes/keys.toml` | Shortcut overrides, only if you create it |
| `~/Documents/ui-notes/` | Auto-named projects from `ui-notes start` |
| `$XDG_RUNTIME_DIR/ui-notes/` | Socket, lock, and temporary captures. Falls back to `/tmp/ui-notes-<uid>` |

The installer does not edit Hyprland bindings, install packages, or create
services. `$XDG_STATE_HOME` and `$XDG_CONFIG_HOME` are honored for the
projects list and shortcut file.

## Requirements

- Omarchy Quattro with its Quickshell shell running.
- Rust 1.92 or newer and Cargo, to build the backend.
- Qt 6 Declarative and Qt Test modules, to run the QML component tests.
- Bash, coreutils, findutils, `jq`, and the `omarchy` and `omarchy-shell`
  commands, for the installer.
- `grim` and `slurp` for screen capture. Install with `omarchy pkg add grim slurp`.
- Voxtype for dictation, optional. The `voxtype-bin` package provides the
  `voxtype` command. Configure it for your microphone before use.
- An authenticated `codex` CLI for AI analysis, optional.

## Install

Clone the repository and run its installer:

```sh
git clone https://github.com/cordrogue/ui-notes.git
cd ui-notes
./install.sh
ui-notes open
```

The installer runs `omarchy plugin validate` on the source, builds both Rust
executables with `cargo build --locked --release`, copies the plugin files
including source and docs to the plugin directory, links the two launchers into
`~/.local/bin`, rescans plugins, and enables the panel. Add `~/.local/bin` to
your PATH if it is not there.

It refuses to touch a plugin directory it did not create, and refuses to
replace a launcher link that points somewhere other than a UI Notes checkout.
On an update it checks the installed files against recorded checksums and
stops if you edited any of them. If a panel is running it asks it to save and
close before building. A failed build leaves the installed files as they were.
Re-run the installer from the checkout after pulling changes.

Omarchy's git installer can also fetch the checkout, but it does not build:

```sh
omarchy plugin add https://github.com/cordrogue/ui-notes.git --yes
cd ~/.config/omarchy/plugins/cordrogue.ui-notes
./install.sh
```

After `omarchy plugin update cordrogue.ui-notes`, run `./install.sh` in that
directory again.

`./install.sh --no-enable` builds and copies without talking to the running
shell. `UI_NOTES_PLUGIN_DIR` and `UI_NOTES_INSTALL_DIR` override the plugin and
launcher directories for staging.

## Use

`ui-notes open` restores the last project or shows the project selector.
`ui-notes open-project /path/to/project` opens a folder from the command line.
The panel's project controls switch folders or create a project.

Drafts save after a typing pause. Ctrl+Return or Ctrl+S saves a new note.
Existing notes save as you edit them. Search filters note text, descriptions,
and section titles. Ctrl+Delete removes the focused saved note. Ctrl+H or F1
opens help with the current shortcuts.
Ctrl+Shift+O cycles the panel, clip preview, and popup controls through 100%,
80%, 60%, and 40% opacity. The setting lasts for the current plugin session.

`ui-notes clip` opens the rectangle picker. Escape cancels. Clips are saved
under the project's `assets/` folder and appear as relative image links in
`notes.md`. `ui-notes stop` saves the draft and ends the session.
`ui-notes help` lists every command.

Add bindings like these to your Hyprland Lua configuration. Skip the two HOME
lines if you do not want dictation.

```lua
hl.layer_rule({ match = { namespace = "ui-notes" }, no_anim = true, animation = "none" })
o.bind("SUPER + SHIFT + U", "UI notes session (toggle)", "ui-notes toggle")
o.bind("SUPER + U", "UI notes keyboard focus", "ui-notes focus-toggle")
o.bind("HOME", "Start dictation (push-to-talk)", "ui-notes ptt start")
o.bind("HOME", "Stop dictation (push-to-talk)", "ui-notes ptt stop", { release = true })
```

Super+U transfers keyboard focus between UI Notes and the previously focused
Hyprland application without hiding the panel. Super+Shift+U still opens or
ends the notes session.

With those bindings, HOME dictates into UI Notes when the new note editor has
focus and behaves as plain Voxtype dictation everywhere else.

## Configuration

`UI_NOTES_MODEL` and `UI_NOTES_REASONING` set the Codex model and reasoning
effort for AI analysis. The model defaults to `gpt-5.6-sol`, and reasoning
defaults to `low`.

Shortcut defaults are in [share/keys.toml](share/keys.toml). Copy it to
`$XDG_CONFIG_HOME/ui-notes/keys.toml`, or `~/.config/ui-notes/keys.toml` when
that variable is unset, and keep only the actions you want to change.
Press Alt+Shift+Enter to expand or collapse the focused saved note. Customize
this shortcut with `expand_note`; it also appears in the help menu.
Customize the opacity shortcut with `cycle_opacity`.

Colors and font come from Ghostty's effective configuration when Ghostty is
installed, with the current Omarchy theme and font as fallbacks. The panel
refreshes when those source files change.

## Remove

```sh
./uninstall.sh
```

This asks a running panel to save, disables the plugin, removes the two
launcher links if they point at this plugin, and removes the installed files
whose checksums still match what the installer wrote. Edited files, unrelated
files, and the plugin directory itself when not empty are kept and reported.
For a git-installed plugin the checkout stays; remove it with
`omarchy plugin remove cordrogue.ui-notes`. `--no-disable` skips the shell
commands for a staging directory.

The uninstaller never touches your notes or settings. To remove those too:

```sh
rm -r ~/.local/state/ui-notes ~/.config/ui-notes
rm -r "$XDG_RUNTIME_DIR/ui-notes"
```

Project folders, including anything under `~/Documents/ui-notes/`, are yours
to delete or keep.

## Development

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
omarchy plugin validate .
/usr/lib/qt6/bin/qmllint -I /usr/share/omarchy/shell Plugin.qml qml/*.qml
./tests/run-qml-tests.sh
```

Rust tests run the built binary against a temporary HOME and stub out the
external tools. They do not touch your real configuration.

Use [tests/run-qml-tests.sh](tests/run-qml-tests.sh) for QML component tests,
including runner options such as help:

```sh
./tests/run-qml-tests.sh -help
```

The wrapper gives `qmltestrunner` a process-scoped offscreen platform, neutral
platform theme, Fusion style, software Qt Quick backend, and Basic Controls
style. These settings keep the headless component tests in
[tests/qml](tests/qml) independent of the desktop display and OpenGL. They do
not test plugin loading in Omarchy's Wayland shell or GPU-rendered effects.

`omarchy plugin validate` checks the manifest, entry point paths, and absence
of symlinks. It does not run the QML or backend. Exercise the installed plugin
with:

```sh
omarchy-shell shell summon cordrogue.ui-notes '{}'
omarchy-shell shell hide cordrogue.ui-notes
omarchy plugin list --json
qs log -p "$OMARCHY_PATH/shell" --tail 100
```

The running shell keeps the QML it already compiled, so QML edits take effect
only after `omarchy-restart-shell`. A copied installation needs `./install.sh`
after source edits. An installed git checkout can be edited in place, but Rust
changes still need a rebuild. `omarchy-shell shell rescanPlugins` forces
discovery. The panel shares Omarchy's Quickshell process, so never launch a
second Quickshell for it. See the
[Omarchy plugin guide](https://plugins.omarchy.org/develop.html) and the
[shell reference](https://github.com/omacom/omarchy/blob/quattro/docs/omarchy-shell.md).

[CONTRACT.md](CONTRACT.md) describes the storage format and the boundary
between QML and the Rust backend. [docs/publishing.md](docs/publishing.md) is
the release checklist. [PERFORMANCE.md](PERFORMANCE.md) holds benchmarks of an
earlier GTK implementation and does not describe this version.

## License

UI Notes is licensed under the [MIT License](LICENSE). The icons under
`qml/icons/` are from Lucide and carry their own [ISC and MIT notices](qml/icons/LICENSE).
