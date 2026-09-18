<div align="center">

<img src="assets/logo.png" alt="Lilo logo" width="120" />

# Lilo

### A compact Markdown notebook for quick thoughts and connected knowledge.

[![Release](https://img.shields.io/github/v/release/HellterEnjoy/Lilo?style=flat-square&color=9b6aa9)](https://github.com/HellterEnjoy/Lilo/releases/latest)
[![Build](https://img.shields.io/github/actions/workflow/status/HellterEnjoy/Lilo/cross-platform-ci.yml?branch=main&style=flat-square)](https://github.com/HellterEnjoy/Lilo/actions/workflows/cross-platform-ci.yml)
[![WinGet](https://img.shields.io/badge/WinGet-HellterEnjoy.Lilo-7f5af0?style=flat-square)](#install)
[![License](https://img.shields.io/badge/license-PolyForm%20Noncommercial-70517c?style=flat-square)](LICENSE)

```powershell
winget install --id HellterEnjoy.Lilo --exact
```

<img src="assets/Screen4.png" alt="Lilo expanded workspace" width="100%" />

</div>

Lilo is a fast desktop note-taking widget built with Rust and egui. It opens as a small writing surface and expands into a full workspace with folders, tags, backlinks, search and a knowledge graph. Notes remain ordinary local Markdown files in a directory you choose.

## Screenshots

<table>
  <tr>
    <td align="center" width="50%">
      <img src="assets/Screen1.png" alt="Quick Capture in compact Lilo" width="100%" />
      <br /><strong>Quick Capture</strong>
      <br /><sub>Send a thought to Today, Inbox, a new note or an existing note.</sub>
    </td>
    <td align="center" width="50%">
      <img src="assets/Screen3.png" alt="Lilo compact widget editor" width="100%" />
      <br /><strong>Compact widget</strong>
      <br /><sub>Keep one note beside your work and write without changing context.</sub>
    </td>
  </tr>
</table>

<img src="assets/Screen2.png" alt="Lilo expanded editor with hierarchical tags" width="100%" />

## What Lilo includes

- one live Markdown editor with headings, lists, tasks, code, links and inline local images;
- Daily Notes, templates and Quick Capture with a global Windows shortcut;
- Obsidian-style `[[wiki links]]`, aliases, backlinks and unresolved-link inspection;
- hierarchical tags, structured search and reusable saved searches;
- local, folder and vault knowledge graphs;
- any directory as a vault, with quick switching between saved vaults;
- autosave, rotating backups, recoverable Trash and external-change protection;
- compact and expanded layouts, themes, typography, Zen mode and configurable shortcuts.

## Local Markdown, by design

Lilo does not put note content in an embedded database. A new vault uses the selected directory directly:

```text
My Vault/
├── Daily/
├── Templates/
├── Attachments/
├── Project notes.md
└── .lilo/
    ├── Backups/
    ├── Trash/
    └── cache/
```

Existing vaults from Lilo 0.2.1 and earlier keep their original `Notes`, `Trash` and `Backups` layout. Lilo does not move those files automatically.

## Install

### Windows

Install from the official WinGet community source:

```powershell
winget install --id HellterEnjoy.Lilo --exact
```

Update later with:

```powershell
winget upgrade --id HellterEnjoy.Lilo --exact
```

The WinGet catalog can briefly lag behind the newest GitHub release. The latest Windows installer and portable ZIP are always available from [GitHub Releases](https://github.com/HellterEnjoy/Lilo/releases/latest). The current binaries are unsigned, so Windows SmartScreen may show a warning.

### Linux

GitHub Releases provide x86-64 archives for Ubuntu 22.04+ and current Arch Linux. Download the matching archive and `.sha256` file, verify it, then install the executable in a stable location such as `~/.local/bin/Lilo`.

## Keyboard shortcuts

| Action | Default |
| --- | --- |
| Search and commands | `Ctrl+K` or `Ctrl+P` |
| Quick Capture | `Ctrl+Shift+C` |
| Today's note | `Alt+D` |
| New note | `Ctrl+N` |
| Save | `Ctrl+S` |
| Editor / Notes / Graph | `Ctrl+1` / `Ctrl+2` / `Ctrl+3` |
| Explorer / Inspector | `Ctrl+Shift+B` / `Ctrl+Shift+I` |
| Zen mode | `F11` |

Shortcuts can be changed in Settings. The system-wide Quick Capture shortcut is currently Windows-only.

## Build from source

Install the stable Rust toolchain, then run:

```bash
git clone https://github.com/HellterEnjoy/Lilo.git
cd Lilo
cargo run --release
```

Project checks:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Ubuntu also requires the native Wayland/X11 development packages listed in [RELEASE.md](RELEASE.md).

## Privacy

Lilo works without an account. Notes, titles, paths, tags, searches and attachments stay on the device. Optional usage analytics are disabled until explicit consent and contain only a random installation ID, app version, local date and counters from a public feature whitelist. See [PRIVACY.md](PRIVACY.md).

## Project information

- [CHANGELOG.md](CHANGELOG.md) — release history;
- [ROADMAP.md](ROADMAP.md) — planned direction;
- [RELEASE.md](RELEASE.md) — installation, updates, packaging and recovery;
- [PERFORMANCE.md](PERFORMANCE.md) — performance guardrails;
- [CONTRIBUTING.md](CONTRIBUTING.md) — feedback and contribution policy.

Lilo is maintained as a single-author project. Bug reports and product suggestions are welcome through [GitHub Issues](https://github.com/HellterEnjoy/Lilo/issues); code pull requests are not accepted into the official repository.

## License

Lilo is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE). Non-commercial use, study, modification and redistribution are allowed under its terms. Commercial use requires separate permission.

Copyright © 2026 Kyrylo Yazynin
