use super::*;

pub(super) fn safe_managed_file(root: &Path, relative: &Path) -> StorageResult<PathBuf> {
    if !is_safe_relative_path(relative) || relative.as_os_str().is_empty() {
        return Err(io::Error::other("Managed file path is unsafe").into());
    }
    let path = root.join(relative);
    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Managed file does not exist").into());
    }
    Ok(path)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("lilo-tmp");
    let replaced = path.with_extension("lilo-replaced");
    fs::write(&temporary, bytes)?;
    if path.exists() {
        if replaced.exists() {
            fs::remove_file(&replaced)?;
        }
        fs::rename(path, &replaced)?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::rename(&replaced, path);
            return Err(error.into());
        }
        fs::remove_file(replaced)?;
    } else {
        fs::rename(temporary, path)?;
    }
    Ok(())
}
