# Omatate

Annotate anything on your screen with a floating notes panel for Omarchy Quattro.
Keep notes beside a mockup, website, or application, group them into sections,
and capture screen regions to reference later.

![Omatate showing project notes](preview.png)

- Write and search project notes without switching away from your work.
- Capture a rectangle of the screen and attach it to a note.
- Dictate with an optional push-to-talk binding.
- Add AI descriptions of the screen when you dictate, off by default.

Every project is a folder you choose. Omatate writes `notes.md` there, keeps
its authoritative records under `.data/`, and saves clips under `assets/`.
You can read and share the Markdown and images without Omatate.

[Install](#install) · [Use](#use) · [Update](#update) ·
[Privacy and files](#privacy-and-files) · [Remove](#remove)

## Install

You need Omarchy Quattro with its Quickshell shell running, Git, Rust 1.92 or
newer with Cargo, and a C linker. The installer also uses Bash, coreutils,
findutils, `jq`, `omarchy`, and `omarchy-shell`.

If you do not already have a Rust toolchain and build tools:

```sh
omarchy pkg add rust base-devel
```

Clone and build Omatate, then open the panel:

```sh
git clone https://github.com/cordrogue/omatate.git
cd omatate
./install.sh
omatate open
```

The installer builds the Rust backend with `cargo build --locked --release`,
installs the plugin as `cordrogue.omatate`, links its commands into
`~/.local/bin`, and enables it. Cargo may download build dependencies.
Keep the checkout for updates, and make sure `~/.local/bin` is on your PATH.
The installer itself does not install packages, edit keybindings, or create a
system service.

Typing notes works without dictation or AI. For screen clips, install
`grim` and `slurp` if they are missing:

```sh
omarchy pkg add grim slurp
```

For dictation, configure Voxtype for your microphone and add the
[push-to-talk bindings](#use). The `voxtype-bin` package provides the
`voxtype` command. AI descriptions additionally require an authenticated
`codex` CLI. Read [Privacy and files](#privacy-and-files) before enabling them.

### Using Omarchy's plugin manager

Omarchy's `plugin add` command clones and validates a plugin; it does not run
`install.sh` or compile Rust. Omatate therefore needs a build step even when
installed from the marketplace.

As an alternative to the checkout above, run:

```sh
omarchy plugin add https://github.com/cordrogue/omatate.git
```

Accept the clone prompt and choose **No** when asked to enable the plugin.
Then build it in the installed checkout:

```sh
cd ~/.config/omarchy/plugins/cordrogue.omatate
./install.sh
omatate open
```

`install.sh` enables the plugin after the build succeeds. Using `plugin add`
with `--enable` alone leaves the backend and command launchers missing.
If you already added the plugin, skip `plugin add` and run the build steps
above. Choose one installation method; a separate checkout cannot overwrite
an existing installation owned by the plugin manager.

### Installer behavior

The installer validates the source, builds both Rust executables, and installs
source and docs alongside the binaries. When run in the plugin manager's
checkout, it builds in place.

It refuses to touch a plugin directory it did not create, and refuses to
replace a launcher link that points somewhere other than an Omatate checkout.
On an update to a copied installation, it checks the installed files against
recorded checksums and stops if you edited any of them. If a panel is running,
it asks it to save and close before building. A failed build does not replace
the installed binaries.

`./install.sh --no-enable` builds and installs without talking to the running
shell. `OMATATE_PLUGIN_DIR` and `OMATATE_INSTALL_DIR` override the plugin and
launcher directories for staging.

## Use

`omatate open` restores the last project or shows the project selector.
`omatate open-project /path/to/project` opens a folder from the command line.
The panel's project controls switch folders or create a project.

Drafts save after a typing pause. Ctrl+Return or Ctrl+S saves a new note.
Existing notes save as you edit them. Search filters note text, descriptions,
and section titles. Ctrl+Delete removes the focused saved note. Ctrl+H or F1
opens help with the current shortcuts.
Ctrl+Shift+O cycles the panel, clip preview, and popup controls through 100%,
80%, 60%, and 40% opacity. The setting lasts for the current plugin session.

`omatate clip` opens the rectangle picker. Escape cancels. Clips are saved
under the project's `assets/` folder and appear as relative image links in
`notes.md`. `omatate stop` saves the draft and ends the session.
`omatate help` lists every command.

Add bindings like these to your Hyprland Lua configuration. Skip the two HOME
lines if you do not want dictation.

```lua
hl.layer_rule({ match = { namespace = "omatate" }, no_anim = true, animation = "none" })
o.bind("SUPER + SHIFT + U", "Omatate session (toggle)", "omatate toggle")
o.bind("SUPER + U", "Omatate keyboard focus", "omatate focus-toggle")
o.bind("HOME", "Start dictation (push-to-talk)", "omatate ptt start")
o.bind("HOME", "Stop dictation (push-to-talk)", "omatate ptt stop", { release = true })
```

Super+U transfers keyboard focus between Omatate and the previously focused
Hyprland application without hiding the panel. Super+Shift+U still opens or
ends the notes session.
The mouse can also transfer focus to another window while the notes stay visible.

With those bindings, HOME dictates into Omatate when the new note editor has
focus and behaves as plain Voxtype dictation everywhere else.

## Update

For the recommended installation, run these commands from your original
`omatate` checkout:

```sh
git pull --ff-only && ./install.sh
```

For an installation made with `omarchy plugin add`, run:

```sh
omarchy plugin update cordrogue.omatate &&
  "$HOME/.config/omarchy/plugins/cordrogue.omatate/install.sh"
```

Both methods need `install.sh` to rebuild the backend and refresh launchers.
`omarchy plugin update` only updates git-managed installations; it cannot
update the copy created by the recommended installer.

After a successful build, reopen with `omatate open`. Current Quattro builds
reload local plugin code when it changes. If the panel still shows the old
version, save and close it with `omatate stop`, run `omarchy restart shell`,
then run `omatate open` again. Restarting the shell also reloads the bar and
other plugins.

## Configuration

`OMATATE_MODEL` and `OMATATE_REASONING` set the Codex model and reasoning
effort for AI analysis. The model defaults to `gpt-5.6-sol`, and reasoning
defaults to `low`.

Shortcut defaults are in [share/keys.toml](share/keys.toml). Copy it to
`$XDG_CONFIG_HOME/omatate/keys.toml`, or `~/.config/omatate/keys.toml` when
that variable is unset, and keep only the actions you want to change.
Press Alt+Shift+Enter to expand or collapse the focused saved note. Customize
this shortcut with `expand_note`; it also appears in the help menu.
Customize the opacity shortcut with `cycle_opacity`.

Colors and font come from Ghostty's effective configuration when Ghostty is
installed, with the current Omarchy theme and font as fallbacks. The panel
refreshes when those source files change.

The panel stays above regular and fullscreen applications. Omarchy's
screensaver covers it, and notes and editor state return when it closes.
The panel adjusts its position for a top or right Omarchy bar.

## Privacy and files

Quickshell draws the panel inside the running `omarchy-shell`. A Rust backend
handles project storage, dictation, captures, and analysis. Both run as your
user. Omatate has no separate system service or direct network client;
optional dictation and AI use tools you install and configure yourself.

Screen capture. `omatate clip` runs `slurp` so you pick a rectangle, then
`grim` saves it into the project's `assets/` folder. Nothing captures without
you drawing a rectangle.

Dictation. Pressing the push-to-talk key runs `voxtype record start`, and
releasing it runs `voxtype record stop`. Voxtype owns the microphone and the
transcription. Whether audio stays local depends on how you configured
Voxtype. Omatate never opens the microphone itself and has no setting to
disable dictation. If you do not want dictation, do not add the push-to-talk
binding.

AI screen analysis. This is off by default. Enable it explicitly with
`omatate ai on`. When enabled, it only fires when the panel's new
note editor has focus and you press push-to-talk. Omatate then screenshots the
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
omatate ai off
```

That writes `off` to `~/.config/omatate/ai`. With AI off, push-to-talk still
dictates, but no screenshot is taken and Codex is never run. `omatate ai`
prints the current mode.

Files created outside your project folders:

| Path | Purpose |
| --- | --- |
| `~/.config/omarchy/plugins/cordrogue.omatate/` | The installed plugin |
| `~/.local/bin/omatate`, `~/.local/bin/omatate-panel` | Main commands |
| `~/.local/state/omatate/projects.json` | Paths of projects you have opened |
| `~/.config/omatate/ai` | AI mode, only after you change it |
| `~/.config/omatate/keys.toml` | Shortcut overrides, only if you create it |
| `~/Documents/omatate/` | Auto-named projects from `omatate start` |
| `$XDG_RUNTIME_DIR/omatate/` | Socket, lock, and temporary captures. Falls back to `/tmp/omatate-<uid>` |

The installer does not edit Hyprland bindings, install packages, or create
services. `$XDG_STATE_HOME` and `$XDG_CONFIG_HOME` are honored for the
projects list and shortcut file.

## Remove

For either installation method, run the installed uninstaller:

```sh
"$HOME/.config/omarchy/plugins/cordrogue.omatate/uninstall.sh"
```

It saves and closes a running panel, disables the plugin, and removes its
launcher links. For a copied installation, it removes files whose checksums
still match the installed version, preserving and reporting edited or
unrelated files.

For a plugin-manager installation, the checkout and build files remain.
After the uninstaller completes, remove that checkout with:

```sh
omarchy plugin remove cordrogue.omatate
```

Run the uninstaller first so launcher links are cleaned up before the plugin
directory disappears. `--no-disable` skips shell commands for staging.

Your notes, captures, settings, and project history remain. Project folders,
including anything under `~/Documents/omatate/`, are yours to keep or delete.
The [file locations](#privacy-and-files) list identifies the remaining settings
and state.

## Development

Development additionally requires the Qt 6 Declarative and Qt Test modules
for the QML component tests.

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
omarchy plugin validate .
/usr/lib/qt6/bin/qmllint -I /usr/share/omarchy/shell Plugin.qml qml/*.qml
./tests/run-qml-tests.sh
python3 tests/check-install.py
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
omarchy-shell shell summon cordrogue.omatate '{}'
omarchy-shell shell hide cordrogue.omatate
omarchy plugin list --json
qs log -p "$OMARCHY_PATH/shell" --tail 100
```

Current Quattro builds watch local plugin files and reload them on changes.
`omarchy-shell shell rescanPlugins` requests a rescan and reload. On older
builds, or if changed QML is still stale, use `omarchy restart shell` after
saving and closing the panel. A copied installation needs `./install.sh`
after source edits. An installed git checkout can be edited in place, but Rust
changes still need a rebuild. The panel shares Omarchy's Quickshell process,
so never launch a second Quickshell for it. See the
[Omarchy plugin guide](https://plugins.omarchy.org/develop.html) and the
[shell reference](https://github.com/omacom/omarchy/blob/quattro/docs/omarchy-shell.md).

[CONTRACT.md](CONTRACT.md) describes the storage format and the boundary
between QML and the Rust backend. [docs/publishing.md](docs/publishing.md) is
the release checklist. [PERFORMANCE.md](PERFORMANCE.md) holds benchmarks of an
earlier GTK implementation and does not describe this version.

## License

Omatate is licensed under the [MIT License](LICENSE). The icons under
`qml/icons/` are from Lucide and carry their own [ISC and MIT notices](qml/icons/LICENSE).
