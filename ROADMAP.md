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

## 0.1.x — Stable MVP maintenance

**Status:** active maintenance.

The `0.1.x` line exists to support the released Stable MVP. Patch releases will be created only for concrete fixes discovered through real use, not to manufacture progress through version numbers.

Likely work includes data-safety fixes, Windows integration issues, compact-layout corrections, accessibility improvements and internal refactoring that does not alter visible behaviour or the vault format. New product areas listed below do not need to be forced into separate `0.1.x` releases.

## 0.2.0 — Daily workflow

**Status:** next feature milestone.

**User value:** Lilo becomes useful as something opened every day, not only as a place to store isolated notes.

- open or create today's note with one command and configurable shortcut;
- configure the daily-note folder and date-based title format;
- create notes from ordinary Markdown templates;
- support a small, documented variable set for date, time, title and cursor position;
- choose an optional default template for daily notes;
- add a keyboard-first quick-capture flow;
- introduce a command palette shared by these actions and existing application commands.

Templates remain declarative. They cannot execute scripts or hide required data outside the generated Markdown file.

**Platform acceptance:** date and timezone behaviour is tested on Windows and Linux; daily and template folders use relative `PathBuf` values; no template or shortcut stores an operating-system-specific absolute path in note metadata.

**Ready when:** the same local calendar day always resolves to the same note, templates produce portable Markdown, and all creation flows work without using the mouse.

## 0.2.1 — Find and organise

**Status:** planned.

**User value:** a growing vault remains easy to search and reorganise.

- add a tag browser with usage counts and nested tag names such as `rust/ownership`;
- filter the note list by one or more tags;
- add focused search operators such as `tag:`, `path:` and `link:` while preserving simple text search;
- make tags clickable in note properties, search results and graph details;
- provide recent searches or saved filters only if they remain understandable in the compact UI;
- preview tag rename or removal across the vault before writing changes;
- create backups for every affected note during a bulk metadata operation.

**Ready when:** a user can locate and safely reorganise related notes without opening files one by one or editing frontmatter manually.

## 0.2.2 — Attachments and Markdown interoperability

**Status:** planned.

**User value:** screenshots, diagrams and reference files can live with notes without sacrificing the plain-file vault.

- paste or drag an image into a note and store it in a predictable attachments directory;
- attach an existing file using a relative Markdown link;
- open or reveal an attachment through the operating system;
- include attachments in vault export and recovery diagnostics;
- detect missing and clearly orphaned attachments without deleting them automatically;
- document paths and naming rules so notes remain usable in other Markdown applications.

Lilo will not embed binary data in Markdown, become a media library, or introduce a proprietary attachment database.

**Platform acceptance:** paste, drag-and-drop and reveal-in-folder behaviour may use different native integrations, but stored Markdown links remain relative and portable between Windows and Linux.

**Ready when:** an exported vault preserves working relative links and no attachment operation can silently overwrite or delete an existing user file.

## 0.2.3 — Knowledge navigation

**Status:** planned.

**User value:** moving through a connected vault becomes faster and safer than manually following filenames.

- add back and forward navigation history;
- add a compact heading outline for the current note;
- make note renaming preserve link integrity through reviewed link updates or retained aliases;
- improve unresolved-link and duplicate-title resolution;
- add useful graph grouping or colouring by folder and tag;
- allow a small number of saved graph filters or views;
- keep graph interaction responsive with vaults containing thousands of notes.

This release improves navigation, not graph decoration. Visual effects without navigational value are out of scope.

**Ready when:** renaming and traversing notes does not unexpectedly break connections, and the graph helps answer where a note belongs or what connects to it.

## 0.2.4 — Personalisation and platform distribution

**Status:** planned; builds on the cross-platform baseline established before `0.2.0`.

**User value:** Lilo can follow the user between machines and adapt to their environment without changing note data.

- provide a small maintained set of accessible light and dark themes;
- retain system-theme following, accent colour, interface density and editor typography controls;
- add an explicit portable mode with settings and a default vault beside the executable;
- clearly separate installed and portable storage so the two modes cannot be confused silently;
- show the active mode and all resolved storage paths in diagnostics;
- maintain tested Linux archives and evaluate native distribution packages only where they improve installation and updates;
- keep macOS source-compatible and document its status without promising signing or notarisation;
- publish separate installed-mode and portable Windows packages with checksums.

Arbitrary CSS-like skinning and per-screen theme overrides are out of scope.

**Ready when:** a portable package can be moved between user-writable folders and reopen its vault without touching installed settings, while every supported theme and platform passes the compact-layout smoke test.

## 0.2.5 — Safe automation experiment

**Status:** exploratory; inclusion depends on a real workflow validated in earlier releases.

**User value:** repetitive personal workflows can be extended without giving unknown code unrestricted access to the vault or computer.

- route built-in actions and shortcuts through an internal command registry;
- define a versioned, declarative extension manifest;
- allow experimental commands composed from safe built-in actions and templates;
- expose no unrestricted process, filesystem or network execution;
- isolate malformed or incompatible extensions and report them through diagnostics;
- provide a global safe mode that starts Lilo with extensions disabled;
- document the format as experimental and unstable before `1.0`.

Native dynamic libraries, arbitrary scripts, an online marketplace and third-party API stability guarantees are not planned for `0.2.x`.

**Ready when:** a broken extension cannot block access to the vault, disabling the experiment leaves every note untouched, and the feature is useful for at least one real workflow rather than existing only as an API demonstration.

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
