//! Operating-system capabilities and native integrations.

use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOs,
    Other,
}

impl OperatingSystem {
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Other => "this operating system",
        }
    }

    pub const fn supports_autostart(self) -> bool {
        matches!(self, Self::Windows | Self::Linux)
    }

    pub const fn autostart_label(self) -> &'static str {
        match self {
            Self::Windows => "Start Lilo with Windows",
            Self::Linux => "Start Lilo after signing in",
            Self::MacOs | Self::Other => "Autostart is not available on this platform",
        }
    }

    pub const fn markdown_path_hint(self) -> &'static str {
        match self {
            Self::Windows => r"C:\path\note.md",
            Self::Linux => "/home/user/note.md",
            Self::MacOs => "/Users/user/note.md",
            Self::Other => "/path/to/note.md",
        }
    }

    pub const fn export_path_hint(self) -> &'static str {
        match self {
            Self::Windows => r"C:\Exports",
            Self::Linux => "/home/user/Exports",
            Self::MacOs => "/Users/user/Exports",
            Self::Other => "/path/to/Exports",
        }
    }
}

pub fn open_folder(path: &Path) -> io::Result<()> {
    open_folder_impl(path)
}

#[cfg(target_os = "windows")]
fn open_folder_impl(path: &Path) -> io::Result<()> {
    std::process::Command::new("explorer").arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_folder_impl(path: &Path) -> io::Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_folder_impl(path: &Path) -> io::Result<()> {
    std::process::Command::new("open").arg(path).spawn()?;
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn open_folder_impl(_path: &Path) -> io::Result<()> {
    Err(io::Error::other(
        "Opening folders is not supported on this platform",
    ))
}

pub fn set_autostart(enabled: bool) -> io::Result<()> {
    set_autostart_impl(enabled)
}

#[cfg(target_os = "windows")]
fn set_autostart_impl(enabled: bool) -> io::Result<()> {
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    if !enabled {
        for value in ["Lilo", "RustWidgets"] {
            remove_windows_run_value(RUN_KEY, value)?;
        }
        return Ok(());
    }

    let exe_path = std::env::current_exe()?;
    let status = Command::new("reg")
        .args(["add", RUN_KEY, "/v", "Lilo", "/t", "REG_SZ", "/d"])
        .arg(format!("\"{}\"", exe_path.display()))
        .arg("/f")
        .status()?;
    if !status.success() {
        return Err(io::Error::other("Windows registry command failed"));
    }

    // Remove the pre-Lilo registry value after the new value is installed.
    let _ = Command::new("reg")
        .args(["delete", RUN_KEY, "/v", "RustWidgets", "/f"])
        .status();
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_windows_run_value(run_key: &str, value: &str) -> io::Result<()> {
    use std::process::Command;

    let deletion = Command::new("reg")
        .args(["delete", run_key, "/v", value, "/f"])
        .status()?;
    if deletion.success() {
        return Ok(());
    }

    // `reg delete` also fails when the value is already absent. Query it so a
    // real removal failure is not reported as a successful settings change.
    let query = Command::new("reg")
        .args(["query", run_key, "/v", value])
        .status()?;
    if query.success() {
        Err(io::Error::other(format!(
            "Could not remove the Windows autostart value {value}"
        )))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn set_autostart_impl(enabled: bool) -> io::Result<()> {
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| io::Error::other("Could not resolve the Linux config directory"))?;
    let executable = std::env::current_exe()?;
    set_linux_autostart_at(base_dirs.config_dir(), &executable, enabled)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn set_autostart_impl(_enabled: bool) -> io::Result<()> {
    Err(io::Error::other(
        "Autostart is not supported on this platform",
    ))
}

#[cfg(any(target_os = "linux", test))]
fn set_linux_autostart_at(config_dir: &Path, executable: &Path, enabled: bool) -> io::Result<()> {
    use std::fs;
    use std::io::Write;

    let autostart_dir = config_dir.join("autostart");
    let entry_path = autostart_dir.join("lilo.desktop");

    if fs::symlink_metadata(&entry_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::other(
            "Refusing to modify a symbolic link in the autostart directory",
        ));
    }

    if !enabled {
        return match fs::remove_file(&entry_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
    }

    fs::create_dir_all(&autostart_dir)?;
    let entry = linux_desktop_entry(executable)?;
    let temporary_path = autostart_dir.join(format!("lilo-{}.tmp", std::process::id()));
    let mut temporary = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    if let Err(error) = temporary
        .write_all(entry.as_bytes())
        .and_then(|()| temporary.sync_all())
        .and_then(|()| fs::rename(&temporary_path, &entry_path))
    {
        let _ = fs::remove_file(temporary_path);
        return Err(error);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn linux_desktop_entry(executable: &Path) -> io::Result<String> {
    let executable = executable.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "The executable path is not valid UTF-8",
        )
    })?;
    let executable = desktop_exec_argument(executable)?;
    Ok(format!(
        "[Desktop Entry]\nType=Application\nVersion=1.5\nName=Lilo\nComment=Compact Markdown notes\nExec={executable}\nTerminal=false\nHidden=false\n"
    ))
}

#[cfg(any(target_os = "linux", test))]
fn desktop_exec_argument(value: &str) -> io::Result<String> {
    if value.chars().any(|character| character.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Desktop entry paths cannot contain control characters",
        ));
    }

    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str(r"\\\\"),
            '"' => escaped.push_str("\\\\\""),
            '`' => escaped.push_str("\\\\`"),
            '$' => escaped.push_str("\\\\$"),
            '%' => escaped.push_str("%%"),
            _ => escaped.push(character),
        }
    }
    escaped.push('"');
    Ok(escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platform_has_expected_capabilities() {
        let platform = OperatingSystem::current();
        assert_eq!(
            platform.supports_autostart(),
            matches!(platform, OperatingSystem::Windows | OperatingSystem::Linux)
        );
        assert!(!platform.name().is_empty());
    }

    #[test]
    fn linux_desktop_entry_quotes_paths_and_escapes_field_codes() {
        let entry = linux_desktop_entry(Path::new("/opt/Lilo App/lilo%preview")).unwrap();

        assert!(entry.contains("Exec=\"/opt/Lilo App/lilo%%preview\""));
        assert!(entry.contains("Terminal=false"));
    }

    #[test]
    fn linux_exec_escapes_reserved_characters() {
        let argument = desktop_exec_argument("/opt/$Lilo`test\\binary").unwrap();

        assert!(argument.contains(r"\\$"));
        assert!(argument.contains(r"\\`"));
        assert!(argument.contains(r"\\\\"));
    }

    #[test]
    fn linux_autostart_entry_can_be_enabled_and_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let executable = Path::new("/opt/Lilo App/lilo");
        let entry = temp.path().join("autostart/lilo.desktop");

        set_linux_autostart_at(temp.path(), executable, true).unwrap();
        assert!(entry.is_file());
        assert!(
            std::fs::read_to_string(&entry)
                .unwrap()
                .contains("Name=Lilo")
        );

        set_linux_autostart_at(temp.path(), executable, false).unwrap();
        assert!(!entry.exists());
    }

    #[test]
    fn linux_desktop_entry_rejects_control_characters() {
        assert!(linux_desktop_entry(Path::new("/opt/lilo\nmalicious")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn linux_autostart_refuses_to_modify_a_symbolic_link() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let autostart = temp.path().join("autostart");
        std::fs::create_dir(&autostart).unwrap();
        let target = temp.path().join("unrelated.desktop");
        std::fs::write(&target, "preserve me").unwrap();
        let entry = autostart.join("lilo.desktop");
        symlink(&target, &entry).unwrap();

        assert!(set_linux_autostart_at(temp.path(), Path::new("/opt/lilo"), false).is_err());
        assert!(entry.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "preserve me");
    }
}
