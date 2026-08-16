# Changelog

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
