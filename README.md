# ui-notes

Floating review panel for commenting on full-screen UI mocks by voice. While a session
is active, every push-to-talk press (HOME) also takes a screenshot with the panel hidden
and sends it to Codex for a factual description of the screen. The description and your
spoken comment become one entry. You only say what you think; the
"what am I looking at" part is filled in for you.

## Install

```sh
./install.sh            # symlinks bin/ui-notes and bin/ui-notes-panel into ~/.local/bin
```

On Omarchy, the installer also registers a `theme-set` hook so a running panel restyles
itself when the desktop theme changes.

Hyprland (Lua config). This machine already has it wired: `~/.config/hypr/ui-notes.lua`
(required from `hyprland.lua`) holds the layer rule and the toggle bind, and the HOME
binds in `bindings.lua` call `ui-notes ptt` instead of `voxtype record` directly.

```lua
-- ~/.config/hypr/ui-notes.lua
hl.layer_rule({ match = { namespace = "ui-notes" }, no_anim = true, animation = "none" })  -- instant hide for screenshots
o.bind("SUPER + SHIFT + U", "UI notes session (toggle)", "ui-notes toggle")

-- ~/.config/hypr/bindings.lua
o.bind("HOME", "Start dictation (push-to-talk)", "ui-notes ptt start")
o.bind("HOME", "Stop dictation (push-to-talk)", "ui-notes ptt stop", { release = true })
```

With no session active, `ui-notes ptt` execs straight into `voxtype record`, so normal
dictation is unchanged.

## Use

1. Open the mock in Chrome, go full screen.
2. SUPER+SHIFT+U — the panel appears on the right (overlay layer, above full-screen windows).
3. Hold HOME, say your comment, release. The entry shows up with its screenshot; the
   screen description arrives after ~15–20 s; the transcript when voxtype finishes.
4. Use `+` to add sections and the folder button to relocate the note.
5. Edit any transcript in the panel if needed. SUPER+SHIFT+U again (or "End session") to stop.
   A session you never spoke into is discarded when it ends.

Run `ui-notes ai off` for plain transcript paragraphs without screenshots or analysis.
Use `ui-notes ai on` or `ui-notes ai toggle` to switch the mode.

## Output

Each session presents a single `notes.md`. Entry data, transcripts, context, and logs live
under the hidden `.data/` directory. Screenshots vanish from memory-backed runtime storage
five minutes after capture. Hand `notes.md` to
a coding agent to act on the comments.

`ui-notes help` lists every command. `ui-notes start <name>` names a session. `UI_NOTES_REASONING` (default `low`) and
`UI_NOTES_MODEL` tune the Codex call.

## Notes

- The panel re-execs itself with `LD_PRELOAD=/usr/lib/libgtk4-layer-shell.so`; Python/gi
  cannot become a layer surface without it.
- Hyprland's `no_screen_share` was tried for the panel layer: it blacks out the panel's
  rectangle in captures instead of showing what's underneath, so hide-before-capture is used.
