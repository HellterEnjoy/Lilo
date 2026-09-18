# Lilo roadmap

Lilo is a compact, local-first Markdown notebook. The roadmap favors a fast editor, predictable files and a small interface over accounts, cloud services or a plugin platform.

## Current release line

Version `0.2.2` is the current application version.

| Version | Status | Main result |
| --- | --- | --- |
| 0.0.1–0.1.0 | Complete | Markdown vault, live editor, links, graph, recovery and the first stable interface. |
| 0.1.1 | Complete | Cross-platform foundation, Windows packaging and Linux release archives. |
| 0.1.9 | Complete | Daily Notes, templates, Quick Capture, command palette and structured search. |
| 0.2.0 | Complete | Hierarchical tags, saved searches, attachments, image paste and the connected knowledge workflow. |
| 0.2.1 | Complete | Explicit analytics consent, configurable autosave and responsiveness work. |
| 0.2.2 | Complete | Direct folder vaults, adaptive compact/expanded layouts, inline media and safe bulk changes. |

See [CHANGELOG.md](CHANGELOG.md) for the exact implemented changes in each release.

## 0.2.3 — Large-vault performance

- benchmark startup, search, tags and links with 1,000 and 10,000 notes;
- extend incremental indexing where measurements justify it;
- bound decoded-image memory and release unused resources;
- profile dense graph layout and interaction;
- publish repeatable fixtures and practical limits.

## 0.2.4 — Platform integration and distribution

The initial WinGet package is now live as `HellterEnjoy.Lilo`. Remaining work:

- keep WinGet updates aligned with GitHub releases;
- harden Windows global-hotkey registration and shutdown cleanup;
- evaluate a maintainable Linux capture shortcut without overstating Wayland support;
- improve Linux packaging based on actual demand;
- evaluate update notifications separately from automatic self-updating;
- continue publishing checksums and explicit support boundaries.

## 0.2.5 — Workflow polish

- improve first-run and empty-vault guidance;
- complete keyboard-focus and accessibility review for dialogs and overlays;
- refine Explorer and Inspector from real usage feedback;
- reduce duplicated controls through the shared command registry;
- prioritize reproducible bugs and common workflow requests;
- choose the `0.3.x` direction from actual use.

## Product rules

Every release should preserve:

- ordinary Markdown as the source of truth;
- backward-compatible vault and settings migration;
- backups and review before destructive or vault-wide changes;
- a usable compact workflow;
- Windows and Linux automated checks;
- reproducible artifacts with SHA-256 files;
- current user documentation.

## Not planned for 0.2.x

- cloud synchronization or user accounts;
- real-time collaboration;
- mobile or web clients;
- a second WYSIWYG editor separate from Markdown;
- arbitrary native-code plugins;
- built-in AI services;
- an embedded note database.

Suggestions and reproducible bug reports are welcome through [GitHub Issues](https://github.com/HellterEnjoy/Lilo/issues). The contribution policy is documented in [CONTRIBUTING.md](CONTRIBUTING.md).
