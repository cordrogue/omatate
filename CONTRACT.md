# Omatate behavior

## Project files

Each project owns `notes.md`, `.data/`, and optional `assets/`. A project may
contain unrelated files, which Omatate preserves. Opening a folder refuses to
overwrite an unrelated `notes.md` or invalid `.data/` directory.

`.data/entries.jsonl` is authoritative. Each line is a JSON object with a unique
positive integer `id`. Notes have a local ISO timestamp `ts`, `transcript`, and
`status`. Plain notes have `ai: false`. Sections have `kind: "section"` and
`title`. Screen clips have a relative `asset` path and logical `asset_size`.
AI notes may contain `context`, `context_status`, `shot`, `window`, and pending
analysis job metadata. Context has `title`, `summary`, `regions`, and `notable`.

`.data/id` identifies the project independently of its path. `.data/draft.txt`
stores the unfinished editor text. Transcripts, structured context, and logs
live under their corresponding `.data/` subdirectories.

`notes.md` is generated from the records, ordered by entry ID. Deleting a
section preserves its notes. Deleting a note preserves its captured asset.

## Storage service

`qml/lib/Service.mjs` owns project state. `qml/OmatateService.qml` supplies the
Quickshell environment and `qml/FileOps.qml` supplies asynchronous file and
process operations. The UI calls the service directly.

A lifetime file lock prevents another shell instance from writing concurrently.
The service serializes mutations, project switches, and CLI commands. Every
editor mutation checks its expected project path and identity token. External
work returns to its original project and note, even if another project is
active when it finishes. A replaced identity rejects stale results.

Files are replaced atomically. A note is persisted before its draft is cleared;
a failed clear rolls back the insertion. A save receipt prevents duplicate draft
recovery if the shell exits between those steps. A Markdown failure after saving records
reports a warning without reporting the note as unsaved. The UI retains pending
edits when a write fails. Project switches drain editor saves first.

Filesystem arguments and user text are passed as separate process arguments or
through stdin. They are never interpolated into shell command source. Paths for
project operations must resolve to directories. Relocation rejects occupied
destinations and preserves unrelated source files.

## Commands and shell lifecycle

`bin/omatate` sends argument arrays through `omarchy-shell shell summon`.
The launcher creates a private temporary reply directory under
`$XDG_RUNTIME_DIR`; the service writes success or failure after the operation
completes. The launcher prints the result, returns a failing status on errors,
and removes the reply directory. Its only direct feature action is forwarding
push-to-talk to Voxtype when there is no active Omatate session.

The plugin stays loaded when hidden. The UI preserves the command socket at
`$XDG_RUNTIME_DIR/omatate/panel.sock` for panel controls. Pending edits save
before hiding, closing, or changing projects. Screen capture waits until panel,
preview, and help windows are hidden.

Project state lives on disk. The service restores the active project when the
shell starts and the last known project when the user opens the panel. It
recovers available transcripts and reconnects to pending analysis completion
files. Unfinished work reports a visible failure instead of writing to a new
project or silently deleting the note.

## Optional work

Capture calls `slurp` and `grim`, with Escape treated as cancellation.
Dictation calls Voxtype and checks its state and transcript output. Recording
and transcription prevent project switching. The panel remains usable while
waiting for transcription or analysis.

AI runs only after explicit opt-in and a push-to-talk press while the new-note
editor is focused. It captures the focused monitor and runs the authenticated
Codex CLI with a schema, read-only sandbox, and project working directory.
The external process has a timeout and survives shell reloads. QML handles
completion and performs all note mutations. Relocation waits for outstanding
analysis to finish. Temporary screenshots have detached cleanup jobs.

## Settings

AI is enabled only by the literal value `on` in `~/.config/omatate/ai`.
Shortcut overrides use a `[keys]` table of string arrays in
`$XDG_CONFIG_HOME/omatate/keys.toml`. Multiline arrays, comments, Qt shortcut
spelling, and angle-bracket modifiers are supported. Unknown actions,
conflicting shortcuts, unsupported syntax, and reserved typing/navigation keys
fall back to default bindings with a warning.

Theme resolution uses Ghostty's effective configuration, then Omarchy theme
colors and font. Theme file changes trigger asynchronous refreshes. Slow tools
have bounded execution time and do not block the UI thread.
