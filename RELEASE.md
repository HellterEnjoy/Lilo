# Lilo release and recovery guide

## Supported packages

Lilo publishes x86-64 builds for Windows, Ubuntu 22.04+ and current Arch Linux. macOS has no official package. Release binaries are currently unsigned.

## Windows installation

Lilo is available from the official WinGet community source as `HellterEnjoy.Lilo`:

```powershell
winget install --id HellterEnjoy.Lilo --exact
```

Upgrade when a newer catalog version is available:

```powershell
winget upgrade --id HellterEnjoy.Lilo --exact
```

Check the version currently published by WinGet with:

```powershell
winget show --id HellterEnjoy.Lilo --exact
```

The WinGet catalog can lag behind a new GitHub release. [GitHub Releases](https://github.com/HellterEnjoy/Lilo/releases/latest) always contains the latest per-user Setup executable, portable ZIP and SHA-256 files. The installer does not require administrator access. Windows SmartScreen may warn because the binaries are not code-signed.

## Linux installation

Download the Ubuntu or Arch archive and its `.sha256` file from GitHub Releases, then verify and install it:

Ubuntu and related distributions need the native windowing dependencies:

```bash
sudo apt-get update
sudo apt-get install libssl-dev libwayland-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev \
  libxkbcommon-x11-dev pkg-config
```

```bash
sha256sum -c Lilo-<version>-<platform>-x86_64.tar.gz.sha256
tar -xzf Lilo-<version>-<platform>-x86_64.tar.gz
cd Lilo-<version>-<platform>-x86_64
install -Dm755 Lilo "$HOME/.local/bin/Lilo"
```

Launch the executable from its final location before enabling autostart. Linux packages are dynamically linked archives rather than native distribution packages.

## Updating and uninstalling

WinGet users can use the upgrade command above. Windows installer users can run the newer Setup executable over the existing installation. Portable and Linux users can replace the executable while keeping its path stable.

Application files, settings and vault data are separate. Updating or uninstalling Lilo does not remove notes. Keep a current export when the vault matters.

## Vaults and recovery

New vaults use the directory selected by the user as the Markdown root. Lilo stores its recoverable application data under `.lilo/`:

- `Trash/` contains notes deleted inside Lilo and supports restoration;
- `Backups/` contains rotating versions saved before overwriting a note;
- `cache/` contains rebuildable data.

Vaults created with Lilo 0.2.1 or earlier retain their legacy `Notes`, `Trash` and `Backups` layout. Upgrading does not move those files.

Recovery tools are available inside the application:

- **Trash & Backups → Trash** restores deleted notes;
- **Trash & Backups → Backups** previews and restores earlier versions;
- **Diagnostics** reports malformed notes and unsafe, missing or malformed attachment links without rewriting files;
- **Settings → Files & Storage → Export** creates a timestamped copy of notes, recovery data and settings.

If Lilo cannot start, copy the complete vault before repairing files manually. Notes are ordinary Markdown and do not depend on an embedded database.

## Build and package

Run the release checks before packaging:

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

The packaging script creates the Windows installer, portable ZIP and SHA-256 files under `dist/`. Inno Setup 6 is required. Linux artifacts are built natively in their target environments.

Pushing a `v<version>` tag runs `.github/workflows/release.yml`. The workflow verifies the tag against `Cargo.toml`, runs checks, publishes Windows artifacts and submits the matching WinGet update when `WINGET_GITHUB_TOKEN` is configured. Linux archives are uploaded to the same release after native verification.

The initial WinGet package is already published. A manually dispatched release can include or skip WinGet with its `publish_winget` option. If an automatic submission needs to be retried, run `.github/workflows/winget.yml` with the released version. Both paths use `wingetcreate update HellterEnjoy.Lilo` and require the `WINGET_GITHUB_TOKEN` Actions secret.
