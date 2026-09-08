# Marketplace submission draft

Title: `[Plugin]: UI Notes`

This is a proposed issue body. Do not submit it until the owner confirms every
checklist statement, confirms the repository is public, and approves this title
and body. The checked boxes below are part of the proposed submission, pending
that confirmation.

```markdown
### Repository URL

https://github.com/cordrogue/ui-notes

### Category

Productivity

### Tags

quickshell, ai

### Suggest a missing tag

_No response_

### Maintainer notes

UI Notes requires Rust 1.92+ and Cargo. Run ./install.sh to build after cloning
or using omarchy plugin add. Please apply the manual-setup label.

The panel runs inside the shared Quickshell shell. Installation requires no
privileges and does not overwrite user configuration. Optional screen capture
uses grim and slurp; dictation uses voxtype.

AI analysis is enabled by default. Push-to-talk while the new-note editor has
focus invokes the authenticated codex CLI with a screenshot of the focused
monitor and the project folder as its working directory. Run ui-notes ai off
to disable AI analysis.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
```

The format follows the marketplace's
[CLI and AI agent submission guide](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md),
checked September 8, 2026.
