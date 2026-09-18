use super::*;

pub fn save_note_with_backup(
    note: &Note,
    backups_dir: &Path,
    backup_limit: usize,
) -> StorageResult<()> {
    if note.file_path.exists() && backup_limit > 0 {
        fs::create_dir_all(backups_dir)?;
        let timestamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
        let backup = backups_dir.join(format!("{}-{timestamp}.md", note.id));
        fs::copy(&note.file_path, backup)?;
        prune_backups(backups_dir, note.id, backup_limit)?;
    }
    save_note(note)
}

#[derive(Debug)]
pub struct NoteSaveFailure {
    pub note_id: Uuid,
    pub title: String,
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct BatchSaveReport {
    pub saved_note_ids: Vec<Uuid>,
    pub failures: Vec<NoteSaveFailure>,
}

/// Saves every requested note independently so one inaccessible file cannot hide successful writes.
pub fn save_notes_with_report(
    notes: &[Note],
    note_ids: &HashSet<Uuid>,
    backups_dir: &Path,
    backups_enabled: bool,
    backup_limit: usize,
) -> BatchSaveReport {
    let mut report = BatchSaveReport::default();
    for note in notes.iter().filter(|note| note_ids.contains(&note.id)) {
        let result = if backups_enabled {
            save_note_with_backup(note, backups_dir, backup_limit)
        } else {
            save_note(note)
        };
        match result {
            Ok(()) => report.saved_note_ids.push(note.id),
            Err(error) => report.failures.push(NoteSaveFailure {
                note_id: note.id,
                title: display_note_title(note).to_owned(),
                path: note.file_path.clone(),
                error: error.to_string(),
            }),
        }
    }
    report
}

pub fn move_note_to_trash(note: &Note, paths: &StoragePaths) -> StorageResult<()> {
    let relative_path = note
        .file_path
        .strip_prefix(&paths.notes_dir)
        .map_err(|_| io::Error::other("Refusing to move a note outside the Notes directory"))?;
    if !is_safe_relative_path(relative_path) || relative_path.as_os_str().is_empty() {
        return Err(io::Error::other("Note has an unsafe relative path").into());
    }

    if !note.file_path.exists() {
        return Ok(());
    }

    let mut destination = paths.trash_dir.join(relative_path);
    if destination.exists() {
        let parent = destination.parent().unwrap_or(&paths.trash_dir);
        destination = parent.join(format!("{}.md", note.id));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&note.file_path, destination)?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct FolderTrashReport {
    pub trashed_note_ids: Vec<Uuid>,
    pub failures: Vec<NoteSaveFailure>,
    pub retained_files: Vec<PathBuf>,
}

/// Moves every note to Trash independently and removes only directories left completely empty.
pub fn delete_folder_with_trash(
    notes_dir: &Path,
    trash_dir: &Path,
    relative: &Path,
    notes: &[Note],
) -> StorageResult<FolderTrashReport> {
    if relative.as_os_str().is_empty() || !is_safe_relative_path(relative) {
        return Err(io::Error::other("The Notes root cannot be deleted").into());
    }
    let target = notes_dir.join(relative);
    if !target.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Folder does not exist").into());
    }
    if fs::symlink_metadata(&target)?.file_type().is_symlink() {
        return Err(io::Error::other("Refusing to delete a linked folder").into());
    }

    let mut report = FolderTrashReport::default();
    let paths = StoragePaths {
        settings_path: PathBuf::new(),
        vault_root: notes_dir.to_path_buf(),
        notes_dir: notes_dir.to_path_buf(),
        trash_dir: trash_dir.to_path_buf(),
        backups_dir: PathBuf::new(),
    };

    for note in notes {
        if let Ok(rel) = note.file_path.strip_prefix(notes_dir)
            && rel.starts_with(relative)
        {
            match move_note_to_trash(note, &paths) {
                Ok(()) => report.trashed_note_ids.push(note.id),
                Err(error) => report.failures.push(NoteSaveFailure {
                    note_id: note.id,
                    title: display_note_title(note).to_owned(),
                    path: note.file_path.clone(),
                    error: error.to_string(),
                }),
            }
        }
    }

    remove_empty_directory_tree(&target)?;
    if target.exists() {
        collect_retained_files(&target, &mut report.retained_files)?;
        report.retained_files.sort();
    }

    Ok(report)
}

fn remove_empty_directory_tree(directory: &Path) -> StorageResult<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let children = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    for child in children {
        if child.is_dir() && !fs::symlink_metadata(&child)?.file_type().is_symlink() {
            remove_empty_directory_tree(&child)?;
        }
    }
    if fs::read_dir(directory)?.next().transpose()?.is_none() {
        fs::remove_dir(directory)?;
    }
    Ok(())
}

fn collect_retained_files(directory: &Path, output: &mut Vec<PathBuf>) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || file_type.is_file() {
            output.push(entry.path());
        } else if file_type.is_dir() {
            collect_retained_files(&entry.path(), output)?;
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct TrashEntry {
    pub relative_path: PathBuf,
    pub display_name: String,
}

#[derive(Clone)]
pub struct BackupEntry {
    pub relative_path: PathBuf,
    pub note_id: Uuid,
    pub title: String,
    pub created_label: String,
    pub size: u64,
}

pub fn list_trash(paths: &StoragePaths) -> StorageResult<Vec<TrashEntry>> {
    let mut entries = Vec::new();
    let mut directories = vec![paths.trash_dir.clone()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                let relative_path = entry.path().strip_prefix(&paths.trash_dir)?.to_path_buf();
                let display_name = load_note(&entry.path())
                    .map(|note| display_note_title(&note).to_owned())
                    .unwrap_or_else(|_| {
                        entry
                            .path()
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    });
                entries.push(TrashEntry {
                    relative_path,
                    display_name,
                });
            }
        }
    }
    entries.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(entries)
}

pub fn list_backups(paths: &StoragePaths) -> StorageResult<Vec<BackupEntry>> {
    fs::create_dir_all(&paths.backups_dir)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&paths.backups_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let Ok(note) = load_note(&path) else {
            continue;
        };
        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::now());
        let created_label = DateTime::<Local>::from(modified)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        entries.push(BackupEntry {
            relative_path: path.strip_prefix(&paths.backups_dir)?.to_path_buf(),
            note_id: note.id,
            title: display_note_title(&note).to_owned(),
            created_label,
            size: metadata.len(),
        });
    }
    entries.sort_by(|left, right| right.created_label.cmp(&left.created_label));
    Ok(entries)
}

pub fn backup_preview(paths: &StoragePaths, relative: &Path) -> StorageResult<String> {
    let path = safe_managed_file(&paths.backups_dir, relative)?;
    Ok(load_note(&path)?.content)
}

pub fn restore_backup(
    note: &mut Note,
    paths: &StoragePaths,
    relative: &Path,
    backup_limit: usize,
) -> StorageResult<()> {
    let backup_path = safe_managed_file(&paths.backups_dir, relative)?;
    let mut restored = load_note(&backup_path)?;
    if restored.id != note.id {
        return Err(io::Error::other("Backup belongs to a different note").into());
    }
    save_note_with_backup(note, &paths.backups_dir, backup_limit.max(1))?;
    restored.file_path = note.file_path.clone();
    restored.updated_at = Local::now();
    restored.refresh_search_text();
    save_note(&restored)?;
    *note = restored;
    Ok(())
}

pub fn restore_from_trash(paths: &StoragePaths, relative: &Path) -> StorageResult<PathBuf> {
    if !is_safe_relative_path(relative) || relative.as_os_str().is_empty() {
        return Err(io::Error::other("Trash path is unsafe").into());
    }
    let source = paths.trash_dir.join(relative);
    if !source.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Trash item does not exist").into());
    }
    let destination = paths.notes_dir.join(relative);
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note already exists at the original path",
        )
        .into());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(source, &destination)?;
    Ok(destination)
}

fn prune_backups(backups_dir: &Path, note_id: Uuid, limit: usize) -> StorageResult<()> {
    let prefix = format!("{note_id}-");
    let mut backups = fs::read_dir(backups_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".md"))
        })
        .collect::<Vec<_>>();
    backups.sort();
    let excess = backups.len().saturating_sub(limit);
    for path in backups.into_iter().take(excess) {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn display_note_title(note: &Note) -> &str {
    if note.title.trim().is_empty() {
        note.content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Untitled")
    } else {
        note.title.as_str()
    }
}
