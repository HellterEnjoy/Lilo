# Lilo roadmap

Lilo is being developed as a fast, compact and keyboard-friendly note-taking widget. The roadmap is intentionally flexible: version contents may change as the application is tested in everyday use.

## 0.0.1 — Functional MVP ✅

The first published version establishes the technical foundation:

- Markdown vault and JSON settings;
- automatic saving, backups, Trash and recovery;
- live Markdown editor;
- wiki links and backlinks;
- nested folders, search, sorting and pinned notes;
- local, folder and global knowledge graphs;
- compact graph overlay;
- basic appearance, storage and Windows behaviour settings.

The MVP prioritises working behaviour over visual polish.

## 0.0.2 — Interface foundation ✅

- replace temporary letter and text controls with a consistent icon system;
- redesign the title bar, note list and folder actions;
- improve layouts for very small and expanded windows;
- add clearer confirmation, error and success messages;
- refine keyboard focus and navigation;
- establish reusable spacing, typography and colour rules.

## 0.0.3 — Editor workflow ✅

- improve Markdown editing around selections and paired markers;
- add convenient formatting actions without introducing a separate preview mode;
- improve list indentation and continuation;
- expand task-list interaction;
- make backlinks and unresolved links easier to inspect;
- profile and optimise very large notes.

## 0.0.4 — Knowledge navigation ✅

- improve graph layout quality and stability;
- add graph filtering and clearer node states;
- improve navigation between notes, folders and backlinks;
- provide better controls for dense local and folder graphs;
- explore tags and aliases as optional graph dimensions.

## 0.0.5 — Vault reliability ✅

- add a visual backup browser;
- improve conflict review for externally edited files;
- provide clearer import, export and vault-switching workflows;
- add recovery diagnostics for malformed Markdown metadata;
- expand automated tests for large and unusual vault structures.

## 0.1.0 — Stable MVP ✅

- complete the first coherent UI/UX pass;
- improve accessibility and keyboard-only use;
- measure startup, editor and graph performance on larger vaults;
- prepare a repeatable Windows packaging process;
- document installation, updates and data recovery.

All planned work through `0.1.0` is implemented. The `0.1.1` maintenance release adds the cross-platform foundation and tested Linux artifacts. See [CHANGELOG.md](CHANGELOG.md) for release summaries and [RELEASE.md](RELEASE.md) for installation and recovery guidance.

## How to read this roadmap

Roadmap versions describe complete user-facing capabilities, not deadlines or a required number of releases. Closely related milestones may be combined, and a version may be skipped when there is no useful standalone release to publish.

Lilo remains a single-author application. Ordinary Markdown is the source of truth, settings stay in JSON, and every core workflow must remain usable in the compact window.

Every published version must include:

- compatibility with vaults and settings created by earlier versions;
- backups and confirmation around destructive or vault-wide operations;
- automated storage tests and a manual keyboard-only UI pass;
- performance checks appropriate to the changed area;
- updated release notes and user documentation;
- reproducible release artifacts with published SHA-256 checksums.

## Cross-platform baseline before 0.2.0

Cross-platform support is an architectural requirement from the beginning of the `0.2.x` work, not a final port performed after the features are complete. Windows and Linux x86-64 are published binary platforms and required build-and-test targets for new development.

| Capability | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Application config | platform config directory resolved by `directories` | XDG config directory resolved by `directories` | platform config directory resolved by `directories` |
| Default vault | user Documents directory | user Documents directory when available | user Documents directory when available |
| Open a folder | Explorer | `xdg-open` | `open` |
| User autostart | `HKCU` Run registry value | `$XDG_CONFIG_HOME/autostart/lilo.desktop` | disabled until a native implementation is tested |
| Automated checks | required | required on Ubuntu | planned before an official macOS claim |
| Release artifact | Windows ZIP | Ubuntu and Arch x86-64 archives | not promised yet |

Platform rules for all planned work:

- operating-system commands and capability checks belong in `platform.rs`;
- paths are represented by `Path` and `PathBuf`, never assembled with hard-coded separators;
- Markdown and JSON formats must remain identical across operating systems;
- unsupported controls are disabled with an explanation instead of failing after a click;
- a feature is not complete if it only compiles because the other platform's code was excluded;
- Windows and Linux must run the same storage and domain tests in CI;
- platform-specific UI text, file pickers and packaging instructions may differ without changing the underlying workflow.

## 0.1.1 — Cross-platform Stable MVP ✅

- separate the executable entry point, application orchestration and operating-system integration;
- provide explicit platform capability reporting and safe unsupported-platform behaviour;
- add Linux folder opening and XDG user autostart while retaining native Windows integration;
- run formatting, tests and Clippy on Windows and Ubuntu in CI;
- publish tested Arch Linux and Ubuntu 22.04+ x86-64 archives with SHA-256 checksums;
- document platform installation, runtime boundaries and the development plan through `0.2.5`.

No note, frontmatter or settings format changed in this maintenance release.

## 0.1.x — Stable MVP evolution

**Status:** complete with the `0.1.9` preview.

The `0.1.x` line established the released Stable MVP, its cross-platform foundation and the first complete daily-workflow implementation. Version `0.1.9` intentionally exposes that larger workflow before it is declared stable in `0.2.0`.

Further `0.1.x` releases are reserved for blocking data-safety, packaging or compatibility fixes found while testing `0.1.9`; no additional feature release is planned before `0.2.0`.

## 0.1.9 — Daily Workflow Preview ✅

- add local-date Daily Notes with configurable folders and portable date formats;
- add previous, today and next-day navigation directly in compact and expanded layouts;
- add declarative Markdown templates with a documented variable set;
- add Quick Capture targeting the current Daily Note, Inbox or a new timestamped note;
- add a fuzzy, keyboard-controlled Command Palette shared by existing and new actions;
- add structured `tag:`, `path:`, `link:` and `title:` search filters;
- add an expanded workspace with Explorer, contextual Inspector, heading outline and Zen mode;
- add native application branding and current interface screenshots;
- retain backwards-compatible Markdown notes and automatic JSON settings migration.

## 0.2.0 — Knowledge workflow ✅

**User value:** capture, organisation and navigation now form one coherent everyday workflow without abandoning portable Markdown.

- stabilise Daily Notes, declarative templates, Quick Capture and the Command Palette;
- add a configurable custom-note capture destination and a system-wide Windows capture shortcut;
- add hierarchical tags, usage counts, clickable filters and reusable searches;
- support comma-separated and negated tag search filters;
- paste or drag local images and files into a vault-contained attachments folder;
- render common local image formats while keeping relative Markdown links;
- detect orphaned attachments without automatic deletion or silent overwriting;
- add back and forward navigation and vault-wide unresolved-link inspection;
- preserve wiki-link headings, aliases and folder prefixes when a note is renamed;
- save every note affected by a vault-wide rename through the backup-aware storage path;
- keep settings migration automatic and note files compatible with `0.1.x`.

Templates remain declarative and attachments remain ordinary files. Neither system can execute scripts or introduce hidden note data.

**Platform acceptance:** the complete note, tag, search, attachment and navigation model is shared by Windows and Linux. The system-wide capture shortcut is explicitly Windows-only in this release; the ordinary in-app shortcut works on both supported platforms.

## 0.2.1 — Vault safety and migration

**Status:** planned from real `0.2.0` feedback.

**User value:** bulk changes and upgrades remain predictable even when files are unusual, externally edited or partially inaccessible.

- add a reviewed preview before a vault-wide tag or link rewrite;
- make partial write failures visible without hiding which files were saved;
- extend recovery diagnostics to missing and malformed attachment links;
- test upgrades from representative `0.1.x` vaults and settings files;
- test interrupted bulk operations and read-only files;
- clarify which recovery actions use Backups and which use Trash.

**Ready when:** every multi-file operation explains its scope, preserves recoverable input and reports partial failure accurately.

## 0.2.2 — Large-vault performance

**Status:** planned; implementation depends on measured bottlenecks.

**User value:** Lilo remains responsive when a personal vault grows from dozens to thousands of notes.

- benchmark startup, search, tag indexing and link indexing with 1,000 and 10,000 notes;
- avoid rebuilding vault-wide indexes when one note changes if profiling shows meaningful cost;
- bound decoded-image memory and release unused image resources;
- profile graph layout and interaction on dense vaults;
- keep editor highlighting and command matching responsive for unusually large notes;
- document repeatable performance fixtures and acceptable limits.

**Ready when:** published performance measurements identify the remaining limits and ordinary editing does not stall behind avoidable vault-wide work.

## 0.2.3 — Platform integration and distribution

**Status:** planned; features may differ where Windows, X11 and Wayland provide different capabilities.

**User value:** supported packages behave like native desktop applications and are easier to install and update safely.

- harden Windows global-hotkey registration, conflict reporting and shutdown cleanup;
- investigate a maintainable Linux capture shortcut without claiming unsupported Wayland behaviour;
- preserve native Windows and XDG autostart implementations;
- improve Ubuntu and Arch packaging based on actual download and support demand;
- complete the initial WinGet submission when the package is accepted;
- evaluate update notification separately from automatic self-updating;
- continue publishing checksums and explicit platform support boundaries.

**Ready when:** installation, shortcuts, startup and removal are predictable on every advertised platform, and unsupported integration is clearly identified in the UI.

## 0.2.4 — Workflow polish

**Status:** planned after reliability and performance work.

**User value:** the existing feature set becomes easier to discover and operate without making the compact window crowded.

- complete a keyboard-focus and screen-reader pass for dialogs and overlays;
- improve first-run and empty-vault guidance;
- make destructive confirmations and status messages consistent;
- reduce duplicated controls by routing actions through the command registry;
- improve compact layouts at minimum supported sizes;
- refine Explorer and Inspector information hierarchy from user feedback.

Arbitrary CSS-like skinning and per-screen theme overrides remain out of scope.

**Ready when:** a new user can discover capture, search, recovery and navigation without consulting source code or fighting keyboard focus.

## 0.2.5 — Feedback-driven consolidation

**Status:** intentionally not feature-filled in advance.

**User value:** the last planned `0.2.x` milestone addresses demonstrated problems instead of adding speculative surface area.

- prioritise reproducible bugs and repeated workflow requests from Issues and Discussions;
- simplify or remove interactions that users consistently misunderstand;
- finish accessibility, documentation and packaging gaps found in `0.2.x`;
- decide the `0.3.x` direction from actual usage: capture speed, knowledge navigation or portable-vault workflow;
- publish explicit non-goals before beginning another large feature cycle.

Plugins, arbitrary scripts and an extension marketplace are not promised for `0.2.x`.

**Ready when:** the project has a documented, evidence-based direction for `0.3.x` and no known blocking data-safety regression remains in the `0.2.x` line.

## Beyond 0.2.5

The following areas are intentionally deferred until the `0.2.x` direction has been validated through real use:

- cloud synchronisation and user accounts;
- real-time collaboration;
- mobile or web applications;
- a WYSIWYG rich-text editor separate from Markdown source;
- arbitrary native-code plugins or a public plugin marketplace;
- built-in AI services;
- encrypted vaults and an embedded database.

Deferring these features keeps Lilo fast, understandable and safe to maintain. They can be reconsidered from real usage rather than added only to make the feature list longer.

Suggestions are welcome through GitHub Issues. See [CONTRIBUTING.md](CONTRIBUTING.md) for the project contribution policy.
