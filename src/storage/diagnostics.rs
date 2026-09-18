use super::*;

pub fn vault_diagnostics(
    paths: &StoragePaths,
    settings: &AppSettings,
) -> StorageResult<Vec<String>> {
    let excluded_directories = managed_note_exclusions(&paths.notes_dir, settings);
    let (notes, warnings, _) = load_notes_excluding(&paths.notes_dir, &excluded_directories)?;
    let mut diagnostics = warnings;
    diagnostics.extend(crate::attachments::AttachmentManager::diagnostics(
        &notes,
        &paths.notes_dir,
    ));
    for directory in [&paths.notes_dir, &paths.trash_dir, &paths.backups_dir] {
        if !directory.is_dir() {
            diagnostics.push(format!(
                "Missing managed directory: {}",
                directory.display()
            ));
        }
    }
    Ok(diagnostics)
}
