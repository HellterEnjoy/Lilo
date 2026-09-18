use super::*;

pub fn import_markdown(
    source: &Path,
    paths: &StoragePaths,
    target_relative: &Path,
) -> StorageResult<Note> {
    if !source.is_file()
        || !source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err(io::Error::other("Import source must be a Markdown file").into());
    }
    if source.starts_with(&paths.notes_dir) {
        return Err(io::Error::other("The selected file is already inside this vault").into());
    }

    let destination_dir = ensure_note_folder(&paths.notes_dir, target_relative)?;
    let title = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut note = match load_note(source) {
        Ok(mut note) => {
            note.id = Uuid::new_v4();
            note.created_at = Local::now();
            note.updated_at = note.created_at;
            note
        }
        Err(_) => {
            let mut note = Note::new_named(&destination_dir, &title);
            note.content = fs::read_to_string(source)?.replace("\r\n", "\n");
            note
        }
    };
    if note.title.trim().is_empty() {
        note.title = title;
    }
    note.file_path = destination_dir.join(note_file_name(&note.title, note.id));
    note.refresh_search_text();
    save_note(&note)?;
    Ok(note)
}

pub fn export_vault(paths: &StoragePaths, destination_root: &Path) -> StorageResult<PathBuf> {
    if destination_root.as_os_str().is_empty() {
        return Err(io::Error::other("Export destination cannot be empty").into());
    }
    fs::create_dir_all(destination_root)?;
    let destination_root = destination_root.canonicalize()?;
    let vault_root = paths.vault_root.canonicalize()?;
    if destination_root.starts_with(&vault_root) {
        return Err(io::Error::other("Export destination must be outside the active vault").into());
    }

    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let destination = destination_root.join(format!("Lilo-Vault-{timestamp}"));
    if destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Export already exists").into());
    }
    fs::create_dir(&destination)?;
    if paths.notes_dir == paths.vault_root {
        copy_directory_excluding(&paths.notes_dir, &destination.join("Notes"), &[".lilo"])?;
    } else {
        copy_directory(&paths.notes_dir, &destination.join("Notes"))?;
    }
    copy_directory(&paths.trash_dir, &destination.join("Trash"))?;
    copy_directory(&paths.backups_dir, &destination.join("Backups"))?;
    fs::copy(&paths.settings_path, destination.join("settings.json"))?;
    Ok(destination)
}

fn copy_directory(source: &Path, destination: &Path) -> StorageResult<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_directory_excluding(
    source: &Path,
    destination: &Path,
    excluded_names: &[&str],
) -> StorageResult<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if excluded_names.iter().any(|name| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        }) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
