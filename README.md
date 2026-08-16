<div align="center">

# Lilo

**A compact Markdown note-taking widget with a built-in knowledge graph.**

[![Version](https://img.shields.io/badge/version-0.1.1-6f42c1?style=flat-square)](https://github.com/HellterEnjoy/Lilo/releases)
[![Status](https://img.shields.io/badge/status-stable%20MVP-2ea44f?style=flat-square)](ROADMAP.md)
[![Rust](https://img.shields.io/badge/Rust-2024-b7410e?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![egui](https://img.shields.io/badge/UI-egui-4b8bbe?style=flat-square)](https://github.com/emilk/egui)
[![Windows](https://img.shields.io/badge/platform-Windows-0078d4?style=flat-square&logo=windows11&logoColor=white)](#building-from-source)
[![Linux](https://img.shields.io/badge/platform-Linux-fcc624?style=flat-square&logo=linux&logoColor=black)](#installing-on-linux)
[![Linux CI](https://github.com/HellterEnjoy/Lilo/actions/workflows/cross-platform-ci.yml/badge.svg)](https://github.com/HellterEnjoy/Lilo/actions/workflows/cross-platform-ci.yml)
[![Markdown](https://img.shields.io/badge/storage-Markdown-24292f?style=flat-square&logo=markdown&logoColor=white)](#storage)
[![License](https://img.shields.io/badge/license-PolyForm%20Noncommercial-5c6ac4?style=flat-square)](LICENSE)

</div>

Lilo is a compact desktop note-taking widget built with Rust and egui. It keeps notes as ordinary Markdown files and provides a small knowledge graph for navigating connections between them. Windows and Linux x86-64 release builds are available.

> **Stable MVP:** version `0.1.1` adds tested Linux distribution builds and a cross-platform application foundation to the editor, knowledge-navigation and recovery workflow. It is still a small preview application rather than a finished commercial product.

Lilo is non-commercial source-available software.

## Current features

- Markdown notes with YAML frontmatter;
- one live editor instead of separate edit and preview modes;
- headings, lists, task checkboxes, links, inline code, code blocks and other common Markdown formatting;
- Obsidian-style `[[wiki links]]`, aliases and backlinks;
- nested vault folders;
- note search, sorting and pinning;
- adaptive icon navigation with top, side and floating toolbar layouts;
- keyboard-only view switching, list navigation and editor formatting;
- safe deletion to Trash with restoration;
- automatic creation and modification dates;
- local, folder and global graph scopes;
- graph zooming, panning and movable nodes;
- stable graph placement, filters, tag/alias context and node-density controls;
- a temporary graph overlay for switching notes without leaving the editor;
- automatic saving, a visual rotating-backup browser and external conflict review;
- Markdown import, full-vault export and immediate vault switching;
- recovery diagnostics for malformed Markdown metadata;
- configurable theme, accent colour, editor font size, shortcuts and always-on-top mode;
- native user autostart through the Windows registry or a Linux XDG desktop entry.

## Storage

Lilo stores note content as `.md` files. Application preferences and UI state remain in `settings.json`.

A vault contains three application-managed directories:

```text
Vault/
├── Notes/       Markdown notes and user folders
├── Trash/       Recoverable deleted notes
└── Backups/     Rotating note backups
```

The vault location can be changed immediately in Settings. Because notes are regular Markdown files, they can also be inspected or edited with another text editor; Lilo detects external changes and avoids silently overwriting conflicting local edits.

## Installing on Windows

After the package has been accepted into the WinGet community repository, install or update Lilo with:

```powershell
winget install --id HellterEnjoy.Lilo --exact
winget upgrade --id HellterEnjoy.Lilo --exact
```

Alternatively, download `Lilo-0.1.1-windows-x64-setup.exe` from [GitHub Releases](https://github.com/HellterEnjoy/Lilo/releases). The per-user installer does not require administrator access and creates a Start menu shortcut. A ZIP archive and matching SHA-256 file remain available for portable use. The preview binary is not code-signed, so Windows SmartScreen may display a warning.

Updates do not require changing the vault: close Lilo and replace the extracted application files. See [RELEASE.md](RELEASE.md) for installation, updating, export and recovery instructions.

## Installing on Linux

Download the x86-64 archive matching the distribution from [GitHub Releases](https://github.com/HellterEnjoy/Lilo/releases):

- `Lilo-0.1.1-ubuntu-22.04-x86_64.tar.gz` supports Ubuntu 22.04 or newer;
- `Lilo-0.1.1-arch-x86_64.tar.gz` targets an up-to-date Arch Linux installation.

Verify the accompanying SHA-256 file, extract the archive and install the executable for the current user:

```bash
sha256sum -c Lilo-0.1.1-<platform>-x86_64.tar.gz.sha256
tar -xzf Lilo-0.1.1-<platform>-x86_64.tar.gz
cd Lilo-0.1.1-<platform>-x86_64
install -Dm755 Lilo "$HOME/.local/bin/Lilo"
"$HOME/.local/bin/Lilo"
```

Install the executable in its final location before enabling autostart so the generated desktop entry retains a valid command path.

## Default shortcuts

| Action | Shortcut |
| --- | --- |
| Create note | `Ctrl+N` |
| Search notes | `Ctrl+P` |
| Open graph | `Ctrl+G` |
| Toggle graph overlay | `Ctrl+Shift+G` |
| Save immediately | `Ctrl+S` |
| Return to editor or close an overlay | `Escape` |

Shortcuts can be changed in Settings.

## Building from source

To build Lilo locally, install the stable Rust toolchain and run:

```powershell
git clone https://github.com/HellterEnjoy/Lilo.git
cd Lilo
cargo run --release
```

On Ubuntu and related distributions, install the native windowing dependencies before building:

```bash
sudo apt-get install libssl-dev libwayland-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev \
  libxkbcommon-x11-dev pkg-config
```

Linux autostart follows the [freedesktop Autostart Specification](https://specifications.freedesktop.org/autostart/0.5/) and writes `lilo.desktop` below the user's XDG config directory. Its launch command follows the [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/). Windows uses the current user's `Run` registry key. Unsupported platform integrations are disabled in the interface rather than attempted at runtime.

Run the project checks with:

```powershell
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

On Windows, install [Inno Setup 6](https://jrsoftware.org/isinfo.php), then create the installer, portable ZIP, and SHA-256 checksums with:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

## Stable MVP limitations

- Windows and Linux x86-64 are the current binary platforms; macOS remains source-compatible but untested and unpublished;
- there is no automatic updater or code signing, and Linux currently uses manually installed archives;
- graph density is capped by a configurable node limit to keep the compact widget responsive;
- very large notes deliberately fall back to lightweight plain-text layout instead of expensive live highlighting.

See [ROADMAP.md](ROADMAP.md) for the planned direction. The roadmap describes intent rather than fixed deadlines or guaranteed release scope.

The implemented release history is recorded in [CHANGELOG.md](CHANGELOG.md), and the current performance guardrails are described in [PERFORMANCE.md](PERFORMANCE.md).

## Contributions

Bug reports, feature requests, UI/UX feedback and technical suggestions are welcome through GitHub Issues.

Lilo is intentionally maintained as a single-author project, so code pull requests are not accepted. Example code and pseudocode may still be shared as suggestions or references.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution policy.

## License

Lilo is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE).

You may use, study, modify, fork and redistribute Lilo and modified versions for non-commercial purposes, subject to the license terms. Commercial use of Lilo or any modified or derived version is not permitted under this license and requires separate permission from the copyright holder.

The copyright holder retains the right to sell Lilo, issue separate commercial licenses, offer future versions under a different licensing model and distribute Lilo under other terms. Redistribution must preserve the license and all required copyright and attribution notices.

Copyright 2026 Kyrylo Yazynin. Project: https://github.com/HellterEnjoy/Lilo
