# Omatate component contract

The Quickshell frontend is a keep-loaded Omarchy panel. Rust owns persistence,
CLI operations, dictation, captures, and analysis. Dictation is delegated to the
`voxtype` command, captures to `grim` and `slurp`, and analysis to `codex exec`
running in read-only sandbox mode with the screenshot attached and the project
folder as its working directory. Those tools are the only paths by which data
leaves the machine, and each is optional. The migration keeps the project
storage format and existing `ui-notes` commands.

## Components

- `manifest.json` declares `cordrogue.omatate`, kind `panel`, entry point
  `Plugin.qml`, and `keepLoaded: true`.
- `Plugin.qml` and `qml/` run in the existing `omarchy-shell` process. They own
  windows, focus, editor state, search, shortcuts, and the panel socket.
- `src/bin/omatate.rs` provides the CLI and invokes `src/backend.rs` for
  `omatate backend`, a persistent stdin/stdout JSON process owned by QML.
- `src/core.rs` holds shared paths, atomic persistence, locking, and Markdown
  rendering. `src/keyboard.rs` loads shortcut overrides.
- `src/bin/omatate-panel.rs` summons this plugin through Omarchy shell IPC.
  It does not start another Quickshell process.
- `bin/ui-notes` and `bin/ui-notes-panel` are regular wrapper files that run the
  corresponding binary under `target/release/`. Plugin folders cannot contain
  symlinks. External CLI links in `~/.local/bin` point to these wrappers.

The frontend normally resolves its backend relative to the plugin directory.
`UI_NOTES_EXECUTABLE` can override that backend path for development.

## Project storage

Each selected project contains:

| Path | Meaning |
| --- | --- |
| `notes.md` | Derived Markdown deliverable |
| `.data/entries.jsonl` | Authoritative note and section records |
| `.data/id` | Stable session identity, derived from PID and creation time |
| `.data/draft.txt` | Unfinished new note, excluded from Markdown |
| `.data/transcripts/`, `.data/context/`, `.data/logs/` | Worker output |
| `assets/clip-<timestamp>.png` | Permanent rectangle captures |

An entry has a positive integer `id`, timestamp, transcript, and status. AI
entries can also carry a screen image path, window information, analysis status,
and structured context. Plain entries use `ai: false`. A section has the same
ID sequence with `kind: "section"` and a title. Section deletion preserves its
notes. Clip assets use paths relative to the project, keeping folders portable.

Every writer of `entries.jsonl` holds the shared runtime flock, reads current
records, updates the intended ID, and atomically replaces the file. Worker
output files and external tool output are written outside that lock. Markdown renders from those
records. Drafts save separately. The backend requires both expected project path
and session token for every editor mutation, so a delayed save cannot land in a
different project. Project switching saves pending edits first and rejects an
outgoing recording or transcription.

The selected project must have a valid Omatate session or be free of conflicting
`notes.md` and `.data` content. Creation and switching preserve unrelated files.
Existing GTK sessions use this same format and need no data migration.

## Runtime and user state

Runtime files live under `$XDG_RUNTIME_DIR/ui-notes`, or `/tmp/ui-notes-$UID` when
XDG runtime storage is unset. `session` holds the active absolute project path,
`lock` coordinates writers, and `panel.sock` carries CLI requests to QML.
Full-screen AI images live under `shots/`. A detached cleanup process deletes
each one about five minutes after capture; the deletion is best effort and a
failure before the analysis worker launches can leave a file for `start` to
clear later. They do not become permanent project assets. The CLI's `start`
command clears old runtime captures. `stop` never erases notes or clips in a
project you chose; it does remove an auto-named project under
`~/Documents/ui-notes` when that project has no records, draft, or assets.

Known projects live at `$XDG_STATE_HOME/ui-notes/projects.json`, defaulting to
`~/.local/state/ui-notes/projects.json`. AI mode lives at
`~/.config/ui-notes/ai`. Shortcut overrides use
`$XDG_CONFIG_HOME/ui-notes/keys.toml`, defaulting to
`~/.config/ui-notes/keys.toml`. Only an explicit `on` value enables AI. Missing, unreadable, or invalid configuration means disabled.

## Backend JSON protocol

QML starts `omatate backend` with stdin enabled. Each request and reply occupies
one JSON line. Requests carry an `id`, echoed in their reply. Success uses
`ok: true`; failures use `ok: false` and `error`. A successful write may include a
`warning` when entries saved but Markdown rendering or state refresh failed.
The frontend must not retry an already saved note solely because of that warning.

| Command | Request fields | Behavior |
| --- | --- | --- |
| `snapshot` | `id`, `cmd`, optional `poll` | Read current project, token, entries, draft, AI mode, known projects, shortcuts, runtime path, theme |
| `draft` | `session`, `token`, `text` | Atomically persist the unfinished editor text |
| `note` | `session`, `token`, `text` | Add a plain note and clear its draft |
| `edit` | `session`, `token`, `entryId`, `text` | Change transcript or section title |
| `delete-section` | `session`, `token`, `entryId` | Remove only the section heading |
| `delete-note` | `session`, `token`, `entryId` | Remove a finished note while retaining its stored assets |

Write requests also include `id` and `cmd`. A snapshot is returned as `state`
when available. Empty notes, stale project identity, missing records, and edits
to recording/transcribing notes fail without switching projects.

With `poll: true`, a snapshot checks a metadata stamp against the last successful
full snapshot, including snapshots attached to mutation replies. If unchanged,
the reply contains `ok: true` and `unchanged: true` with no `state`; otherwise it
returns a full snapshot. Snapshots without `poll: true` always return `state` on
success.

Operations already exposed by the CLI, such as select/create project, clips,
sections, AI mode, and stopping, use argument arrays through Quickshell's Process
API. User text and paths must not be interpolated into a shell command.

## Panel socket and shell lifecycle

The panel socket accepts one JSON request per line and returns one JSON reply.
The public CLI uses this socket for its existing commands.

- `ping` reports readiness and the shared Quickshell process ID.
- `focus` reports whether the panel holds keyboard focus and which editor is
  active. AI dictation captures only when the new-note editor is active.
- `hide` hides the panel and preview before acknowledging capture readiness.
  The CLI also waits briefly for the compositor before running `grim`.
- `show` restores the panel and previous focus intent after capture.
- `toggle-focus` keeps the panel visible while transferring keyboard focus
  between Omatate and the previously focused Hyprland application.
- `reload` refreshes the backend snapshot; `restyle` remains a compatibility call.
- `flush` saves pending drafts and edits.
- `quit` saves before hiding. A failed save returns `ok: false` and leaves the
  panel available. It never quits the shared Omarchy shell.
- `open` focuses the project controls and refreshes state.

Omarchy invokes the plugin's `open(payloadJson)` and `close()` lifecycle methods.
`omarchy-shell shell summon cordrogue.omatate '{}'` opens it and
`omarchy-shell shell hide cordrogue.omatate` hides it. `keepLoaded` leaves the
backend loaded while the window is closed; the socket listens only while the
plugin is open. Disable unloads the plugin.
Install and uninstall ask the current panel to save before modifying loaded code.

## Appearance and shortcuts

The panel preserves the GTK layout and resolves the same colors and typography
in Rust through `src/theme.rs`. Ghostty's effective configuration takes precedence,
with the current Omarchy palette and font as fallbacks. The backend caches this
result and refreshes it when a source file fingerprint changes. Snapshot `theme`
contains Qt-compatible colors, font family, and point sizes. No GTK dependency or
CSS provider is required. The installer adds no theme hooks.

All panel windows use the overlay layer so they remain visible over ordinary
fullscreen applications. While Omarchy's `org.omarchy.screensaver` window is
present, they move to the top layer and release their keyboard focus grab. This
places them behind the native fullscreen screensaver without closing the panel
or discarding editor state; the overlay layer and prior focus intent return when
the screensaver closes. On the panel's monitor, a fullscreen workspace ignores
Omarchy's top and right bar reservations, using a 4 pixel top gap and 12 pixel right gap.
Otherwise, a visible top or right bar adds its size to the matching gap.

Shortcuts retain `share/keys.toml` compatibility. Rust translates GTK accelerator
notation into Qt sequences for the frontend. Invalid override files fall back to
defaults and expose a warning. The QML help panel shows effective shortcuts.
`cycle_opacity` defaults to Ctrl+Shift+O and cycles all panel windows and popup
controls through 100%, 80%, 60%, and 40% opacity without persisting the value.
Theme and font updates use the original source files; runtime behavior should be
verified on the supported Omarchy version before release.
