# ui-notes — component contract (v6)

Goal: a small floating notes panel over a full-screen Chrome page showing a mock HTML UI.
Holding HOME dictates (voxtype). While a ui-notes session is active, each HOME press also
takes a screenshot (with the panel hidden), sends it to Codex for a factual description of
the screen, and stores description + spoken comment as one entry. The user only speaks
opinions; the screen context is filled in automatically. Output is one Markdown file.

Two components, each built by a separate delegate. This file is the seam.

## Paths

- Project: `~/Work/ui-notes/`
  - `bin/ui-notes` — CLI (Python 3, stdlib only, shebang `#!/usr/bin/python3`)
  - `bin/ui-notes-panel` — GTK4 panel (Python 3, `#!/usr/bin/python3`, gi Gtk 4.0 +
    Gtk4LayerShell 1.0; only /usr/bin/python3 has gi, NOT the mise python). It re-execs
    itself with `LD_PRELOAD=/usr/lib/libgtk4-layer-shell.so`, which Python/gi needs.
  - `share/analyze-prompt.md`, `share/analyze-schema.json` — Codex prompt + output schema
  - `share/ui-notes-theme-hook` — Omarchy theme-set hook, runs `ui-notes panel restyle`
  - `install.sh` — symlinks both bins into `~/.local/bin`, installs the hook via
    `omarchy hook install theme-set`
- Sessions: `~/Documents/ui-notes/<YYYYMMDD-HHMMSS>[-<name>]/` (or wherever `relocate` moved it)
  - `notes.md` — the deliverable, rendered from entries.jsonl. The session dir should look
    like "one Markdown file"; everything else is hidden:
  - `.data/entries.jsonl` — one JSON object per line, one per record id, **the source of truth**
  - `.data/id` — random session token written by `start`; lets detached workers follow a `relocate` (below)
  - `.data/transcripts/<NNN>.txt`, `.data/context/<NNN>.json`, `.data/logs/`
  - Screenshots are never written under the session dir.
- Runtime: `$XDG_RUNTIME_DIR/ui-notes/` (fallback `/tmp/ui-notes-$UID`)
  - `session` — text file with the absolute path of the active session dir. Absent = inactive.
  - `panel.sock` — Unix stream socket the panel listens on
  - `lock` — flock file guarding entries.jsonl rewrites
  - `shots/<session-basename>/<NNN>.png` — screenshots (tmpfs) while they are needed (Codex
    input, the panel's transient preview popup). **TTL: 5 minutes.** Each `analyze` worker,
    after finishing, sleeps until 300 s after the shot's mtime, then unlinks it and removes
    its parent dir if empty. `stop` does not delete shots. `start` deletes everything under `shots/`.

## entries.jsonl records

Entry:
```json
{"id": 3, "ts": "2026-09-03T23:58:10", "shot": "/run/user/1000/ui-notes/shots/20260904-001106-e2e/003.png",
 "window": {"class": "chromium", "title": "Dashboard mock - Chromium"},
 "status": "recording|transcribing|done|no-transcript",
 "context_status": "pending|done|error",
 "context": {"title": "...", "summary": "...", "regions": [{"name": "...", "contents": "..."}], "notable": ["..."]},
 "transcript": "what the user said (may be edited in the panel)"}
```
Section (shares the id sequence and the file; groups the entries that follow it in id order):
```json
{"id": 4, "kind": "section", "ts": "2026-09-04T01:20:00", "title": "Checkout flow"}
```
A record without `kind` is an entry. `id` is a 1-based integer, zero-padded to 3 digits in
filenames. Fields are absent until known. `shot` is an ABSOLUTE path and the file may be gone.

AI toggle (v5): `~/.config/ui-notes/ai` holds `on` or `off` (missing = on). When off, an
entry is a plain note: `{"id": 5, "ts": "...", "ai": false, "status": "recording|transcribing|done|no-transcript", "transcript": "..."}`
— no `shot`, no `window`, no `context_status`, no `context`.

### Rewrite protocol (both sides)

Any writer MUST: `flock` the runtime `lock` file, read all lines, replace every line with the
matching `id` (collapsing duplicates) or append, write `entries.jsonl.tmp`, `os.replace` over
`entries.jsonl`, release. Never append blindly; never truncate other ids. The only allowed
deletion is a section record (entries are never deleted). The CLI regenerates `notes.md`
after each rewrite it performs; the panel spawns `ui-notes render` (detached) after each
rewrite it performs, so notes.md always matches entries.jsonl.

## Panel socket protocol

Line-delimited JSON over `panel.sock`; one JSON reply per request.

- `{"cmd":"hide"}` → panel unmaps its surface AND the screenshot popup (must be invisible to
  grim); replies `{"ok":true}` only after GTK has unmapped (iterate the main context until
  idle). The CLI still sleeps ~50 ms afterwards so the compositor renders a frame without
  the layer before grim reads it.
- `{"cmd":"show"}` → remaps the panel (not the popup), `{"ok":true}`.
- `{"cmd":"reload"}` → re-read entries.jsonl, `{"ok":true}` (belt-and-braces; the panel also
  watches the `.data` dir with Gio.FileMonitor).
- `{"cmd":"restyle"}` → re-resolve the Omarchy theme and reapply CSS, `{"ok":true}`.
- `{"cmd":"quit"}` → flush pending edits synchronously, exit.
- `{"cmd":"ping"}` → `{"ok":true,"pid":N}`.

## CLI commands (`ui-notes`)

- `start [name]` — clear `shots/`, create the session dir (`notes.md`, `.data/…`, `.data/id`), write the
  runtime `session` file atomically, launch the panel if `ping` fails (two 250 ms attempts;
  panel spawned detached with stdio → DEVNULL). Also launches the panel when called on an
  already-active session.
- `stop` — remove `session`, send `quit` to the panel. If the session dir is directly under
  `~/Documents/ui-notes`, is named `<YYYYMMDD-HHMMSS>…`, and has no entries and no transcript
  files, the whole dir is deleted (an untouched session leaves nothing behind). Never deletes
  a relocated session.
- `toggle` — start if inactive else stop. (Bound to SUPER+SHIFT+U.)
- `status` — print the active session path or `inactive`; exit 1 if inactive.
- `ptt start` / `ptt stop` — bound to HOME press / release.
  - Inactive: `os.execvp` into `voxtype record start|stop` before any heavy import. This
    path must stay fast and must never break plain dictation.
  - Active, AI off: allocate id N; `voxtype record start --file=…/NNN.txt`; write the plain
    entry (`ai: false`, status recording). No hide/grim/show, no analyze. `stop` is unchanged.
  - Active, AI on, `start`: allocate id N; `voxtype record start --file=<session>/.data/transcripts/NNN.txt`
    FIRST (so speech is not lost); panel `hide` → sleep 50 ms → `grim -o <focused monitor>
    <runtime>/shots/<session>/NNN.png` (the `hyprctl monitors -j` entry with `focused: true`, else the first) → panel `show`; read `hyprctl activewindow -j`; write
    the entry (status recording, context_status pending); spawn detached `analyze N`. If
    anything fails after voxtype started and before the entry is written, run
    `voxtype record cancel` (best effort) and re-raise; a failed `analyze` spawn marks
    context_status error.
  - Active, `stop`: `voxtype record stop` (on failure mark the latest recording ENTRY
    no-transcript); set it transcribing; spawn detached `ingest N`.
- `analyze N` — `codex exec --skip-git-repo-check --sandbox read-only -C <session> -i <shot>
  --output-schema` (`<shot>` is the entry's own `shot` path) ` share/analyze-schema.json -o .data/context/NNN.json -c model_reasoning_effort="low" - < share/analyze-prompt.md`
  with a 240 s timeout. Parse the JSON (extract the first `{…}` block if Codex wrote prose);
  set context + context_status done, or context_status error (dropping any stale context).
  Refuses a section id or a missing entry. Then the 5-minute shot TTL wait + delete.
  Env `UI_NOTES_REASONING` overrides the effort; `UI_NOTES_MODEL` adds `-m`.
- `ingest N` — poll every 200 ms: success when `.data/transcripts/NNN.txt` is non-empty and
  voxtype's state file (`$XDG_RUNTIME_DIR/voxtype/state`) reads `idle` → set transcript,
  status done. Fast fail: state has read `idle` continuously for 1.5 s with no non-empty file
  and ≥3 s elapsed → status no-transcript (voxtype writes no file for an empty
  transcription). Hard cap 120 s → no-transcript. Logs the branch taken.
- `render` — regenerate notes.md from entries.jsonl.
- `section [title]` — append a section record (default title `Section K`, K = existing
  sections + 1), render, print the new id.
- `relocate <dir>` — move the active session (`notes.md` and `.data/`) into `<dir>` (an
  existing directory outside the session that holds neither `notes.md` nor `.data`), point
  the runtime `session` file at `<dir>` atomically, print the new path; all of it under the
  runtime `lock`. Absolute `shot` paths are untouched; new shots go under `shots/<new basename>/`.
  The panel follows via its 1 s poll.
  Detached workers (`analyze`, `ingest`) are spawned with `UI_NOTES_SESSION_PATH` and
  `UI_NOTES_SESSION_ID` (the `.data/id` token). Before writing, a worker re-resolves its session:
  the pinned path if it still has `.data/entries.jsonl`, else the runtime `session` path when its
  `.data/id` matches the token. So a relocate while analysis or transcription is in flight loses
  nothing.
- `ai [on|off|toggle]` — read or set `~/.config/ui-notes/ai`; always prints `on` or `off`.
  Exit 0. (No argument = print current state.)
- `panel hide|show|reload|restyle|quit|ping` — thin socket client.
- `help` / `-h` / `--help` — usage. An unknown command or bad arity prints `ui-notes: invalid
  command: …` plus `run 'ui-notes help'` to stderr and exits 2.

## notes.md format (simple, with optional sections)

```
# UI review — <session name> · <YYYY-MM-DD>

## <section title>                       (one per section record, in id order)

### 1. <context.title, or "Entry 1" while pending>
<context.summary>            (one paragraph; "_analysis pending…_" / "_analysis failed_" otherwise)

> <transcript>               (blockquote; "_transcribing…_" or "_no transcript_" otherwise)
```

Entries use `##` when the file has no section records at all, `###` otherwise. Entries that
precede the first section render before any section header. A plain entry (`ai: false`)
renders as just its transcript as a normal paragraph (no heading, no blockquote; the
placeholders `_transcribing…_` / `_no transcript_` still apply). Nothing else: no images, no
window titles, no regions, no notable lists (those stay in `.data/context/NNN.json`).

## Omarchy theming (panel)

Resolve the palette at startup and on `restyle`:
1. `name` = output of `omarchy theme current`; `slug` = name lower-cased, spaces → `-`.
2. colors file = first existing of `~/.config/omarchy/themes/<slug>/colors.toml`,
   `/usr/share/omarchy/themes/<slug>/colors.toml`, `$(omarchy theme dir <name>)/colors.toml`.
   Parse with `tomllib`. Keys used: `background`, `dark_background`, `darker_background`,
   `lighter_background`, `foreground`, `dark_foreground`, `muted`, `accent`, `selection`,
   `red`, `yellow`, `green`, `blue`.
3. Font: `omarchy font current`; base size from `[font] base-size` in
   `~/.config/omarchy/shell.toml`, default 11. Subprocess timeout 2 s.
4. If anything fails, fall back to the Gand values hard-coded as defaults; never crash.

Look (v4, "sharp + small"): solid `background` panel (alpha 1.0), solid `lighter_background`
cards, 0 radius everywhere, 1 px hairlines in `foreground` at ~0.25 alpha, no fills on idle
controls, accent only on focus. **Size:** 320 px wide, anchored TOP+RIGHT only (12 px margins);
height follows content up to 70 % of the monitor height, then the list scrolls. Header: four
glyph buttons right-aligned — AI toggle (nf-md-creation U+F0674; a `Gtk.ToggleButton`
whose active state mirrors `~/.config/ui-notes/ai`, accent-colored glyph when on, muted
when off; clicking runs `ui-notes ai toggle`; the panel re-reads the file on its 1 s poll),
`+` new section (runs `ui-notes section`), folder (native `Gtk.FileDialog.select_folder`,
then `ui-notes relocate <dir>`), `✕` end session. A plain entry card (`ai: false`) has no
title row: just the stripe, the transient status word, and the transcript editor. Section
rows: a flat hairline row with the title in an inline editable field (saved via the rewrite
protocol) and a small `✕` that deletes the section record. Cards (5 px padding): 2 px left status stripe
(recording → `red`, transcribing/pending → `yellow`, done → `green`, no transcript → `muted`,
error → `red`) + a small muted uppercase status word only while transient or wrong (REC,
TRANSCRIBING, ANALYZING, NO TRANSCRIPT, ERROR; none when finished), context title (small,
`muted`, ≤2 lines; the user's comment is the primary text), transcript editor (`foreground`,
base size). While the transcript is empty the editor is one line tall and shows a muted
placeholder (`listening…` / `transcribing…` / `no transcript`) that disappears on text or
focus; with text it grows to 120 px then scrolls. No thumbnails in cards. Empty state: one
muted line, `no session`.

**Focus:** the panel opens with nothing focused (GTK's map-time grab is cleared, so no field
looks pre-selected). `hide`/`show` restores whatever had focus before the capture. A section
record that arrives more than 2 s after the panel opened (i.e. the user pressed `+`) gets its
title focused with the text selected, so typing replaces `Section K`. The section `✕` is only
visible while the pointer is over the row.

**Screenshot popup:** when a reload brings in a new entry whose `shot` file exists, a second
layer-shell surface (namespace `ui-notes-shot`, OVERLAY, anchored TOP+RIGHT, right margin =
panel width + 24 so it sits just left of the panel) shows the screenshot ~360 px wide inside
a hairline, stays ~4 s, fades out (CSS opacity transition), then unmaps — like a notification.
Entries present at startup never pop up. Hyprland layer rule (`~/.config/hypr/ui-notes.lua`):
`namespace = "^ui-notes"` gets `no_anim` so both surfaces unmap instantly for captures.
