# Publishing Omatate

`cordrogue.omatate` is published from https://github.com/cordrogue/omatate.
The 0.3.0 release checks are recorded in [release-0.3.0.md](release-0.3.0.md).
The project uses the MIT license, recorded in the root `LICENSE`,
`manifest.json`, and `Cargo.toml`. Marketplace listing requires maintainer approval.

Before submitting:

1. Test a fresh install on Omarchy Quattro, including explicit Rust compilation,
   open/hide/toggle, shell restart, project switching, draft recovery, search,
   clip cancellation, dictation, AI-disabled use, and uninstall.
2. Run Rust checks, `omarchy plugin validate .`, and QML validation with the
   installed Omarchy imports. Check the runtime log for QML errors.
3. Confirm `REPORT.md` and `docs/investigations/` remain untracked. The root
   `.gitignore` excludes these machine-specific diagnostics so local copies
   can stay in the checkout without entering the public repository. Do not
   force-add them.
4. Commit and push the release to the public repository. Verify that its root
   contains the manifest, README, license, QML, and Rust source. No built binaries
   are committed. Pick a stable version and optionally add a screenshot.
5. Open the marketplace submission form with the repository URL. Use the
   Productivity category and the permitted `quickshell` and `ai` tags. The
   [submission draft](marketplace-submission.md) contains the proposed title
   and complete issue body. Follow the
   [CLI and AI agent submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md).
   Confirm every checklist statement and public visibility with the owner,
   and obtain approval of the title and body before creating the issue.

The README documents the separate build step because Omarchy's plugin installer
never runs plugin install hooks. The plugin runs as the logged-in user and starts
its Rust backend inside the shared shell. The backend reads and writes selected
project folders and Omatate configuration/state. Optional screen capture uses
`grim` and `slurp`; optional dictation uses `voxtype`; AI analysis invokes `codex`
and sends the screen image and window context to its configured service. There
is no privileged installer or separate system service.

The marketplace requires a public GitHub repository, a valid root manifest,
README, license, and safe install/removal behavior. Current registry policy
requires an exact-commit scan and maintainer approval before a listing appears.
Check the policy again at submission time.

Publishing and submission rules checked September 10, 2026:

- [Marketplace publishing guide](https://plugins.omarchy.org/publish.html)
- [Marketplace submission and review policy](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/README.md)
- [CLI and AI agent submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md)

Shell and development references checked September 6, 2026:

- [Omarchy shell manifest and installation reference](https://github.com/omacom/omarchy/blob/quattro/docs/omarchy-shell.md)
- [Plugin development guide](https://plugins.omarchy.org/develop.html)
