# Omatate 0.3.0

UI Notes is now Omatate, universal annotation for Omarchy.

- The plugin ID is `cordrogue.omatate`; the commands are `omatate` and
  `omatate-panel`.
- Existing `ui-notes` commands remain compatibility aliases. Storage paths,
  project files, shortcuts, and the layer namespace remain compatible.
- Installation disables the old UI Notes plugin and preserves its directory.
- AI screen analysis is off unless the user has explicitly saved `on`.
  Existing explicit preferences remain in effect.
- Backend snapshots reuse mutation results, file replacement preserves private
  permissions, and mouse focus can leave the panel without hiding it.

## Verification

Checked September 10, 2026 on Omarchy 4.0.3 with Qt 6.11.2:

- All 56 Rust tests passed, including explicit AI opt-in, dictation without
  capture or Codex, session recovery, concurrent writes, and permissions.
- Rust formatting, Clippy with warnings denied, and the locked release build
  passed.
- All 12 headless QML test cases passed. Manifest validation passed.
- `tests/check-install.py` passed fresh installation, repeat installation,
  legacy aliases and settings, modified-file protection, unrelated-launcher
  protection, and uninstall without deleting notes or settings.
- The installed plugin passed open, hide/show, focus transfer, keyboard note
  saving, search, project switching, capture cancellation, shell restart and
  draft recovery, and disable/re-enable checks. Plugin loading after enable is
  asynchronous; readiness was checked after loading.
- The preview is a capture of the actual panel with disposable sample notes.

QML lint exits successfully but reports Quickshell type-metadata warnings,
dynamic delegate-property warnings, and unqualified-access warnings. The live
shell log showed no Omatate QML load or binding errors. Portal registration and
socket peer-close warnings remain in the shared shell log.

Dictation command behavior is covered with a stubbed Voxtype process. Microphone
transcription accuracy and authenticated cloud AI analysis were not exercised
for this release. Those optional features depend on the user's external tools
and account configuration.

## Install

```sh
git clone https://github.com/cordrogue/omatate.git
cd omatate
./install.sh
omatate open
```

Rust 1.92 or newer and Cargo are required. Omarchy's plugin installer does not
compile the backend. Run `./install.sh` after each plugin update.
