# Changelog

## 0.2.1 — Transparent Analytics

- added explicit first-run consent for optional, privacy-preserving usage analytics;
- added a Settings control that disables analytics and queues deletion of previously collected installation rows;
- restricted usage reporting to a public whitelist of aggregate daily feature counters;
- kept note contents, titles, paths, tags, search queries and device information out of analytics payloads;
- moved HTTPS delivery to a bounded background worker so unavailable analytics cannot delay the editor or application startup;
- added a Cloudflare Worker and D1 schema with strict validation, prepared statements, 90-day installation-row retention and no public read endpoint;
- added a protected GitHub Actions collector that archives the repository's rolling Traffic API data without publishing it in the repository;
- documented collection, consent, retention, processing and deletion in `PRIVACY.md`.

## 0.2.0 — Knowledge Workflow

> [!WARNING]
> Lilo is still under active development. Although this release passed automated checks on Windows, Arch Linux and Ubuntu 22.04, bugs may remain and some functionality may behave differently or fail on particular systems, desktop environments or vaults. Keep a current vault backup and report reproducible problems through GitHub Issues.

- stabilised Daily Notes, templates, Quick Capture and the Command Palette introduced in `0.1.9`;
- added Quick Capture destinations for a selected custom note and improved target switching;
- added an optional system-wide Quick Capture shortcut on Windows while retaining the in-app shortcut on Linux;
- added hierarchical tag indexing, usage counts, tag filtering and vault-wide tag renaming;
- expanded structured search with comma-separated and negated tag filters plus reusable saved searches;
- added image paste, drag-and-drop file import, relative attachment links and inline local image rendering;
- added attachment collision protection, vault-contained attachment paths and an orphan-file inspector;
- added back and forward note navigation, improved heading and link inspection, and vault-wide unresolved-link discovery;
- preserved headings, aliases and folder prefixes when updating wiki links after a note rename;
- improved Daily Note date parsing and relative-day navigation for flat and nested date paths;
- ensured vault-wide tag and link changes persist every affected note through the existing backup-aware save path;
- reduced image-loader dependencies by excluding unused HTTP loading;
- expanded automated coverage to 89 passing Windows tests and 90 passing Linux tests while keeping the Markdown vault and JSON settings backwards-compatible.

## 0.1.9 — Daily Workflow Preview

- added local-date Daily Notes with configurable folders, filename formats and previous/next-day navigation;
- added Quick Capture with selectable Daily Note, Inbox or timestamped-note destinations;
- added declarative Markdown templates for creating notes and inserting reusable content;
- added `{{title}}`, `{{date}}`, `{{time}}`, `{{datetime}}`, `{{yesterday}}`, `{{tomorrow}}` and `{{cursor}}` template variables;
- added a keyboard-first Command Palette with fuzzy command matching and grouped application actions;
- added structured search operators for tags, folders, outgoing links and titles while preserving ordinary text search;
- expanded the desktop workspace with collapsible Explorer and Inspector sidebars, Zen mode and editor zoom controls;
- added a Markdown heading outline, word and character counts, and improved editor line spacing;
- expanded settings for the daily workflow, editor typography, interface sizing and Quick Capture behaviour;
- added application branding to the native window and Windows executable;
- added a redesigned project README with current screenshots and workflow documentation;
- preserved the existing Markdown vault structure and automatic settings migration.

## 0.1.1 — Cross-platform Stable MVP

- split the application UI and orchestration out of `main.rs` while keeping the executable entry point small;
- isolated operating-system integration in a dedicated platform module with explicit capability reporting;
- added native Linux folder opening and XDG user autostart alongside the existing Windows integrations;
- hardened Linux autostart generation with command escaping, validation, atomic replacement and symbolic-link protection;
- added Windows and Ubuntu CI checks for formatting, tests and Clippy;
- added a per-user Inno Setup installer, portable Windows ZIP and automated release workflow;
- prepared WinGet manifests and update automation while keeping manual downloads available;
- added tested x86-64 release archives for current Arch Linux and Ubuntu 22.04 or newer;
- documented platform-specific building, installation and support boundaries;
- expanded the development roadmap through `0.2.5` without changing the Markdown vault format.

## 0.1.0 — Stable MVP

- introduced a coherent adaptive interface with icon navigation and movable toolbar placement;
- expanded the live Markdown editor with selection formatting, list continuation and task actions;
- added cached highlighting for responsive editing of large notes;
- improved backlinks, unresolved links, tags and aliases;
- stabilised graph placement and added title, alias and tag filters plus density controls;
- added a visual backup browser, conflict review, diagnostics, Markdown import, vault export and immediate vault switching;
- expanded keyboard navigation and automated coverage for recovery and graph behaviour;
- added repeatable Windows ZIP packaging and recovery documentation.

## 0.0.1 — Functional MVP

- first public functional build.
