# Lilo release guide

## Platform status

Windows and Linux x86-64 are published binary platforms. Linux archives are tested on current Arch Linux and Ubuntu 22.04 or newer. Windows and Ubuntu remain mandatory CI targets for ongoing development. macOS integration, release signing and notarisation are not currently claimed.

## Install on Windows

Download `Lilo-<version>-windows-x64-setup.exe` from the GitHub release and run it. The installer is per-user and does not require administrator access. The ZIP archive remains available as a portable build. Windows SmartScreen may warn because the binaries are not code-signed.

WinGet installation is prepared but is not advertised until the initial package has been accepted into the community repository. Once available, the package identifier will be `HellterEnjoy.Lilo`.

Lilo creates its default vault as `LiloVault` in the user's Documents directory. The active path is visible and can be changed immediately in **Settings → Storage**.

## Install on Linux

1. Download the Ubuntu or Arch `Lilo-<version>-<platform>-x86_64.tar.gz` archive and its `.sha256` file.
2. Run `sha256sum -c <archive>.sha256` in the download directory.
3. Extract the complete archive.
4. Install `Lilo` to a stable user-owned location such as `~/.local/bin/Lilo`.
5. Launch the installed executable before enabling autostart.

The Ubuntu artifact requires Ubuntu 22.04 or newer. The Arch artifact targets an up-to-date rolling installation. Both archives contain an unsigned dynamically linked x86-64 executable rather than a native distribution package.

## Update

Run the newer Setup.exe over an existing Windows installation. Portable Windows users can replace the files from the newer ZIP. On Linux, replace the installed executable while preserving its path so an existing autostart entry remains valid. The executable is separate from the vault, so updating or uninstalling the application does not remove notes or settings. Keep a vault export before an update when the data matters.

## Backup and recovery

- **Recovery → Trash** restores notes deleted inside Lilo.
- **Recovery → Backups** previews and restores rotating versions created during saving.
- **Recovery → Diagnostics** reports Markdown files whose metadata could not be read normally.
- **Settings → Storage → Export** copies Notes, Trash, and settings to a timestamped folder outside the active vault.

Lilo never requires a proprietary database: notes remain Markdown files under the vault's `Notes` directory. If the application cannot start, copy the whole vault before manually repairing any file.

## Build and package

The official Windows installer and ZIP are built by `.github/workflows/release.yml` on a clean GitHub-hosted Windows runner. For a local verification build, install the stable Rust toolchain and Inno Setup 6, then run:

```powershell
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

The script performs a locked release build and creates the Inno Setup installer, portable ZIP, and their SHA-256 checksums under `dist/`. Linux artifacts are built natively in current Arch Linux and Ubuntu 22.04 environments, packaged with the same documentation, and accompanied by SHA-256 checksums. Release artifacts are intentionally ignored by Git.

## Publishing releases and WinGet updates

Pushing a `v<version>` tag runs `.github/workflows/release.yml`. The workflow verifies that the tag matches `Cargo.toml`, runs the tests, builds all Windows artifacts from that exact tag, and creates or updates the GitHub release. Verified Linux archives are then uploaded to the same release.

The first WinGet version must be submitted once from the checked-in manifests:

```powershell
wingetcreate submit .\winget\0.1.0 --token $env:WINGET_GITHUB_TOKEN --no-open
```

After that PR has been accepted, add a repository Actions secret named `WINGET_GITHUB_TOKEN`. It should contain a GitHub token authorized to fork and open pull requests against `microsoft/winget-pkgs`. WinGet updates are then started explicitly through the release workflow's manual `publish_winget` option, so an unavailable community package cannot mark an otherwise valid GitHub release as failed. The `Retry WinGet submission` workflow can retry an update without rebuilding or replacing the release.
