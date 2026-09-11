# Omatate

A floating notes panel for Omarchy. Keep notes beside whatever you are working
on, group them into sections, and attach screen clips.

![Omatate showing project notes](preview.png)

Each project is a folder you choose. Omatate writes a readable `notes.md`, note
records under `.data/`, and screen clips under `assets/`.

## Install

Omatate needs Omarchy Quattro with its Quickshell shell running. There is no
build step. Only a small storage service stays loaded in the shell; the panel
itself loads when you open it and unloads when you close it.

```sh
omarchy plugin add https://github.com/cordrogue/omatate.git --enable
```

Open the panel from the shell's plugin controls, or from a terminal:

```sh
~/.config/omarchy/plugins/cordrogue.omatate/bin/omatate open
```

The examples below use this alias:

```sh
alias omatate="$HOME/.config/omarchy/plugins/cordrogue.omatate/bin/omatate"
```

Typing notes works with no extra tools. Optional features need these on your
PATH:

| Feature | Tools |
| --- | --- |
| Screen clips | `grim`, `slurp` |
| Dictation | `voxtype`, configured for your microphone |
| AI screen descriptions | `grim`, `hyprctl`, an authenticated `codex` CLI |

## Use

`omatate open` restores the last project or shows the project selector. Omatate
saves after a short typing pause, and before you switch projects or close the
panel.

| Action | Shortcut |
| --- | --- |
| Save a new note | Ctrl+Return or Ctrl+S |
| Delete the focused note | Ctrl+Delete |
| Search notes and descriptions | Ctrl+F |
| Show projects | Ctrl+P |
| Add a section | Ctrl+Shift+S |
| Capture a screen clip | Ctrl+Shift+C |
| Expand the focused note | Alt+Shift+Return |
| Cycle panel opacity | Ctrl+Shift+O |
| Show help | Ctrl+H or F1 |

Useful commands:

```sh
omatate open-project /path/to/project   # open a folder as a project
omatate clip                            # pick a rectangle; Escape cancels
omatate stop                            # save and end the session
omatate help                            # list every command
```

A clip is saved inside the project and linked from `notes.md`. Deleting a note
keeps its image file.

### Bind keys in Hyprland

Add these to your Hyprland Lua configuration. Drop the HOME lines if you do not
dictate.

```lua
local omatate = os.getenv("HOME") .. "/.config/omarchy/plugins/cordrogue.omatate/bin/omatate"
hl.layer_rule({ match = { namespace = "omatate" }, no_anim = true, animation = "none" })
o.bind("SUPER + SHIFT + U", "Omatate session", omatate .. " toggle")
o.bind("SUPER + U", "Omatate keyboard focus", omatate .. " focus-toggle")
o.bind("HOME", "Start dictation", omatate .. " ptt start")
o.bind("HOME", "Stop dictation", omatate .. " ptt stop", { release = true })
```

Super+U moves keyboard focus between Omatate and the previous window. HOME
dictates into Omatate when its new-note editor has focus, and behaves as plain
Voxtype push-to-talk everywhere else.

## Configure

Colors and fonts come from Ghostty's effective configuration, then from the
Omarchy theme. Icons use the Omarchy theme's accent color, with Ghostty's blue
as a fallback when no accent is defined. The panel reloads when those files change.

To change shortcuts, copy [share/keys.toml](share/keys.toml) to
`~/.config/omatate/keys.toml` and keep only the actions you want to override.
An empty array disables a shortcut. Invalid or conflicting entries fall back to
the defaults.

## Privacy

Omatate runs as your user inside the Omarchy shell. It does not change desktop
configuration or install a service. Screen clips capture only the rectangle you
select. Dictation runs `voxtype record`, so your Voxtype configuration decides
whether audio stays local.

AI descriptions are off by default. Turn them on with `omatate ai on`. When on,
a push-to-talk press in the new-note editor screenshots the whole focused
monitor, including other windows, with the panel hidden. Omatate sends that
screenshot to `codex exec` in a read-only sandbox with the project folder as
the working directory, using your Codex CLI account. The screenshot is deleted
about five minutes later and never becomes a project asset.

Set `OMATATE_MODEL` and `OMATATE_REASONING` in the shell's environment to
override the defaults, `gpt-5.6-sol` and `low`.

## Files

| Location | Contents |
| --- | --- |
| `<project>/notes.md` | Markdown generated from the records |
| `<project>/.data/` | Note records, draft, transcripts, analysis results, logs |
| `<project>/assets/` | Screen clips |
| `~/.config/omatate/ai` | AI opt-in |
| `~/.config/omatate/keys.toml` | Shortcut overrides |
| `~/.local/state/omatate/projects.json` | Known projects |
| `$XDG_RUNTIME_DIR/omatate/` | Active project pointer and temporary files |

Omatate honors `XDG_CONFIG_HOME` and `XDG_STATE_HOME` when set. Keep `.data/`
with `notes.md` when you move or back up a project. Edit notes in the panel.
Omatate overwrites `notes.md` on the next save.

## Update and remove

```sh
omarchy plugin update cordrogue.omatate
omarchy plugin remove cordrogue.omatate
```

Finish dictation and analysis before you remove the plugin. Removal leaves your
project folders and preferences in place. Remove any alias or keybindings you
added.

## Development

[docs/testing.md](docs/testing.md) covers the service, QML, and Quickshell
checks and the startup benchmark. [CONTRACT.md](CONTRACT.md) describes the
file format and service behavior.

MIT licensed. Icon attribution is in [qml/icons/README.md](qml/icons/README.md).
