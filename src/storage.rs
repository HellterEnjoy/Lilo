mod diagnostics;
mod fs_io;
mod notes;
mod recovery;
mod settings;
mod transfer;
mod vault;

use chrono::{DateTime, Local};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub use diagnostics::vault_diagnostics;
use fs_io::{atomic_write, safe_managed_file};
pub use notes::{
    AppData, Note, create_note_folder, ensure_note_folder, move_note_to_folder, rename_folder,
    rename_note_file, save_note,
};
use notes::{
    is_safe_relative_path, load_note, load_notes_excluding, managed_note_exclusions, note_file_name,
};
#[cfg(test)]
use notes::{load_notes, sanitize_file_stem};
pub use recovery::{
    backup_preview, delete_folder_with_trash, list_backups, list_trash, move_note_to_trash,
    restore_backup, restore_from_trash, save_note_with_backup, save_notes_with_report,
};
pub use settings::{
    AppSettings, GraphNodeOffset, NoteSort, QuickCaptureTarget, SearchPreset, ThemeChoice,
    save_settings,
};
use settings::{load_settings, migrate_settings};
pub use transfer::{export_vault, import_markdown};
#[cfg(test)]
use vault::vault_directories;
pub use vault::{
    LoadedStorage, StoragePaths, VaultEntry, VaultLayout, load_storage, reload_notes,
    set_vault_path, vault_name, vault_snapshot,
};

pub type StorageResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const SETTINGS_VERSION: u32 = 10;
const MIN_SUPPORTED_SETTINGS_VERSION: u32 = 6;
const ROOT_VAULT_LAYOUT_VERSION: u32 = 9;
pub const MIN_AUTOSAVE_INTERVAL_SECONDS: u64 = 15;
pub const MAX_AUTOSAVE_INTERVAL_SECONDS: u64 = 10 * 60;
pub const DEFAULT_AUTOSAVE_INTERVAL_SECONDS: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &Path) -> StoragePaths {
        let paths = StoragePaths {
            settings_path: root.join("settings.json"),
            vault_root: root.to_path_buf(),
            notes_dir: root.join("Notes"),
            trash_dir: root.join("Trash"),
            backups_dir: root.join("Backups"),
        };
        for directory in [&paths.notes_dir, &paths.trash_dir, &paths.backups_dir] {
            fs::create_dir_all(directory).expect("create managed directory");
        }
        fs::write(&paths.settings_path, "{}").expect("create settings file");
        paths
    }

    #[test]
    fn markdown_note_round_trips() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        fs::create_dir_all(&notes_dir).expect("create Notes directory");

        let mut note = Note::new(&notes_dir);
        note.title = "Ownership".to_owned();
        note.content = "Ownership is connected to [[Borrowing]].".to_owned();
        note.tags = vec!["rust".to_owned(), "learning".to_owned()];
        note.pinned = true;
        note.mark_as_updated();

        save_note(&note).expect("save Markdown note");
        let loaded = load_note(&note.file_path).expect("load Markdown note");
        let markdown = fs::read_to_string(&note.file_path).expect("read Markdown note");

        assert_eq!(loaded.id, note.id);
        assert_eq!(loaded.title, note.title);
        assert_eq!(loaded.content, note.content);
        assert_eq!(loaded.tags, note.tags);
        assert!(loaded.pinned);
        assert!(markdown.starts_with("---\n"));
        assert!(markdown.contains("\n---\n\nOwnership is connected"));
        assert!(!markdown.contains("search_text"));
    }

    #[test]
    fn windows_file_names_are_sanitized() {
        let stem = sanitize_file_stem("  CON:<bad>/name?  ");
        assert!(!stem.contains(['<', '>', ':', '/', '?']));
        assert!(!stem.ends_with(['.', ' ']));
    }

    #[test]
    fn nested_notes_and_empty_folders_are_discovered() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let nested = ensure_note_folder(&notes_dir, Path::new("Programming/Rust"))
            .expect("create nested folder");
        ensure_note_folder(&notes_dir, Path::new("Empty")).expect("create empty folder");
        let note = Note::new_named(&nested, "Ownership");
        save_note(&note).expect("save nested note");

        let (notes, warnings, folders) = load_notes(&notes_dir).expect("load nested notes");

        assert!(warnings.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Ownership");
        assert!(folders.contains(&PathBuf::from("Programming")));
        assert!(folders.contains(&PathBuf::from("Programming/Rust")));
        assert!(folders.contains(&PathBuf::from("Empty")));
    }

    #[test]
    fn templates_and_attachments_are_not_loaded_as_notes() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let templates_dir = notes_dir.join("Templates");
        let attachments_dir = notes_dir.join("Attachments");
        fs::create_dir_all(&templates_dir).expect("create templates directory");
        fs::create_dir_all(&attachments_dir).expect("create attachments directory");
        fs::write(templates_dir.join("Daily.md"), "# {{date}}")
            .expect("write template without note metadata");
        fs::write(attachments_dir.join("Reference.md"), "attachment text")
            .expect("write markdown attachment");
        let note = Note::new_named(&notes_dir, "Real note");
        save_note(&note).expect("save real note");

        let settings = AppSettings::default();
        let exclusions = managed_note_exclusions(&notes_dir, &settings);
        let (notes, warnings, folders) =
            load_notes_excluding(&notes_dir, &exclusions).expect("load note documents");

        assert!(warnings.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Real note");
        assert!(!folders.contains(&PathBuf::from("Templates")));
        assert!(!folders.contains(&PathBuf::from("Attachments")));
    }

    #[test]
    fn nested_note_keeps_its_relative_path_in_trash() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let trash_dir = temp.path().join("Trash");
        let nested = ensure_note_folder(&notes_dir, Path::new("Biologia/Anathomia"))
            .expect("create nested folder");
        let note = Note::new_named(&nested, "Bones");
        save_note(&note).expect("save nested note");
        let file_name = note.file_path.file_name().expect("note file name");
        let paths = StoragePaths {
            settings_path: temp.path().join("settings.json"),
            vault_root: temp.path().to_path_buf(),
            notes_dir,
            trash_dir: trash_dir.clone(),
            backups_dir: temp.path().join("Backups"),
        };

        move_note_to_trash(&note, &paths).expect("move nested note to Trash");

        assert!(
            trash_dir
                .join("Biologia/Anathomia")
                .join(file_name)
                .exists()
        );
        assert!(!note.file_path.exists());
    }

    #[test]
    fn note_can_move_between_real_vault_folders() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let source =
            ensure_note_folder(&notes_dir, Path::new("Inbox")).expect("create source folder");
        let mut note = Note::new_named(&source, "Rust Tips");
        save_note(&note).expect("save source note");
        let old_path = note.file_path.clone();
        let paths = StoragePaths {
            settings_path: temp.path().join("settings.json"),
            vault_root: temp.path().to_path_buf(),
            notes_dir: notes_dir.clone(),
            trash_dir: temp.path().join("Trash"),
            backups_dir: temp.path().join("Backups"),
        };

        move_note_to_folder(&mut note, &paths, Path::new("Programming"))
            .expect("move note to Programming");

        assert!(!old_path.exists());
        assert!(note.file_path.exists());
        assert_eq!(
            note.file_path.parent(),
            Some(notes_dir.join("Programming").as_path())
        );
        assert_eq!(
            load_note(&note.file_path).expect("load moved note").id,
            note.id
        );
    }

    #[test]
    fn unsafe_folder_names_and_parent_paths_are_rejected() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");

        assert!(ensure_note_folder(&notes_dir, Path::new("../Outside")).is_err());
        assert!(create_note_folder(&notes_dir, Path::new(""), "CON").is_err());
        assert!(create_note_folder(&notes_dir, Path::new(""), "bad/name").is_err());
    }

    #[test]
    fn folders_can_be_renamed_and_deleted_with_trash() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let trash_dir = temp.path().join("Trash");
        let folder = ensure_note_folder(&notes_dir, Path::new("Programming/Rust"))
            .expect("create nested folder");
        let note = Note::new_named(&folder, "Ownership");
        save_note(&note).expect("save note");

        let renamed = rename_folder(&notes_dir, Path::new("Programming/Rust"), "Rust Notes")
            .expect("rename folder");
        assert_eq!(renamed, PathBuf::from("Programming/Rust Notes"));

        // Delete with trash moves note to trash
        let (notes, _, _) = load_notes(&notes_dir).expect("reload notes after rename");
        let report = delete_folder_with_trash(&notes_dir, &trash_dir, &renamed, &notes)
            .expect("delete with trash");
        assert_eq!(report.trashed_note_ids.len(), 1);
        assert!(report.failures.is_empty());
        assert!(!notes_dir.join(&renamed).exists());
    }

    #[test]
    fn folder_trash_never_deletes_unmanaged_files() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let trash_dir = temp.path().join("Trash");
        let folder =
            ensure_note_folder(&notes_dir, Path::new("Project")).expect("create project folder");
        let note = Note::new_named(&folder, "Plan");
        save_note(&note).expect("save note");
        fs::write(folder.join("diagram.bin"), b"keep me").expect("write unmanaged file");

        let report =
            delete_folder_with_trash(&notes_dir, &trash_dir, Path::new("Project"), &[note])
                .expect("move managed notes");

        assert_eq!(report.trashed_note_ids.len(), 1);
        assert_eq!(report.retained_files.len(), 1);
        assert!(folder.join("diagram.bin").exists());
    }

    #[test]
    fn trash_item_can_be_restored() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let trash_dir = temp.path().join("Trash");
        let backups_dir = temp.path().join("Backups");
        let folder = ensure_note_folder(&notes_dir, Path::new("Inbox")).expect("create Inbox");
        let note = Note::new_named(&folder, "Restore me");
        save_note(&note).expect("save note");
        let paths = StoragePaths {
            settings_path: temp.path().join("settings.json"),
            vault_root: temp.path().to_path_buf(),
            notes_dir: notes_dir.clone(),
            trash_dir,
            backups_dir,
        };

        move_note_to_trash(&note, &paths).expect("trash note");
        let entry = list_trash(&paths)
            .expect("list Trash")
            .pop()
            .expect("trash item");
        let restored = restore_from_trash(&paths, &entry.relative_path).expect("restore note");

        assert!(restored.starts_with(&notes_dir));
        assert!(restored.exists());
        assert_eq!(
            load_note(&restored).expect("load restored note").id,
            note.id
        );
    }

    #[test]
    fn corrupt_frontmatter_is_recovered_without_rewriting_source() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        fs::create_dir_all(&notes_dir).expect("create Notes");
        let path = notes_dir.join("broken.md");
        let original = "---\ninvalid: [yaml\n---\n\n# Preserved body";
        fs::write(&path, original).expect("write corrupt note");

        let (notes, warnings, _) = load_notes(&notes_dir).expect("load vault");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].content, "# Preserved body");
        assert_eq!(fs::read_to_string(path).expect("source remains"), original);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn backups_are_pruned_per_note() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        let backups_dir = temp.path().join("Backups");
        fs::create_dir_all(&notes_dir).expect("create Notes");
        let mut note = Note::new_named(&notes_dir, "Backed up");
        save_note(&note).expect("initial save");

        for index in 0..4 {
            note.content = format!("version {index}");
            save_note_with_backup(&note, &backups_dir, 2).expect("save with backup");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        assert_eq!(fs::read_dir(backups_dir).expect("list backups").count(), 2);
    }

    #[test]
    fn backup_browser_previews_and_restores_an_older_version() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());
        let mut note = Note::new_named(&paths.notes_dir, "History");
        note.content = "first version".to_owned();
        save_note(&note).expect("save first version");
        note.content = "second version".to_owned();
        save_note_with_backup(&note, &paths.backups_dir, 5).expect("create backup");

        let entry = list_backups(&paths)
            .expect("list backups")
            .pop()
            .expect("backup entry");
        assert_eq!(entry.note_id, note.id);
        assert_eq!(
            backup_preview(&paths, &entry.relative_path).unwrap(),
            "first version"
        );

        restore_backup(&mut note, &paths, &entry.relative_path, 5).expect("restore backup");
        assert_eq!(note.content, "first version");
        assert_eq!(load_note(&note.file_path).unwrap().content, "first version");
    }

    #[test]
    fn plain_markdown_import_gets_managed_metadata_without_changing_source() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(&temp.path().join("vault"));
        let source = temp.path().join("Useful idea.md");
        fs::write(&source, "# Useful idea\n\nPlain Markdown.").expect("write import source");

        let imported = import_markdown(&source, &paths, Path::new("Inbox")).expect("import note");

        assert_eq!(imported.title, "Useful idea");
        assert_eq!(imported.content, "# Useful idea\n\nPlain Markdown.");
        assert!(
            imported
                .file_path
                .starts_with(paths.notes_dir.join("Inbox"))
        );
        assert!(
            fs::read_to_string(&source)
                .unwrap()
                .starts_with("# Useful idea")
        );
        assert!(
            fs::read_to_string(&imported.file_path)
                .unwrap()
                .starts_with("---\n")
        );
    }

    #[test]
    fn vault_export_copies_notes_trash_and_settings_outside_the_vault() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(&temp.path().join("vault"));
        let export_root = temp.path().join("exports");
        let note = Note::new_named(&paths.notes_dir, "Exported");
        save_note(&note).expect("save note");
        fs::write(paths.trash_dir.join("recoverable.md"), "recoverable").unwrap();

        let exported = export_vault(&paths, &export_root).expect("export vault");

        assert!(
            exported
                .join("Notes")
                .join(note.file_path.file_name().unwrap())
                .exists()
        );
        assert!(exported.join("Trash/recoverable.md").exists());
        assert!(exported.join("settings.json").exists());
        assert!(export_vault(&paths, &paths.notes_dir).is_err());
    }

    #[test]
    fn version_six_settings_load_and_ignore_removed_navigation_fields() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("settings.json");
        let json = r#"{
            "version": 6,
            "vault_path": "C:/Vault",
            "selected_note_id": null,
            "legacy_migration_completed": true,
            "font_size": 13.0,
            "editor_font_size": 15.0,
            "toolbar_placement": "Floating",
            "toolbar_expanded": true,
            "floating_toolbar_vertical": true,
            "floating_toolbar_position": [100.0, 200.0],
            "selected_folder": "",
            "collapsed_folders": []
        }"#;

        fs::write(&path, json).expect("write version 6 settings");
        let mut settings = load_settings(&path).expect("load version 6 settings");
        migrate_settings(&mut settings, Path::new("C:/DefaultVault"));

        assert_eq!(settings.version, SETTINGS_VERSION);
        assert_eq!(settings.vault_layout, VaultLayout::LegacyNotesDirectory);
        assert_eq!(settings.note_sort, NoteSort::Updated);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert!(settings.autosave_enabled);
        assert_eq!(
            settings.autosave_interval_seconds,
            DEFAULT_AUTOSAVE_INTERVAL_SECONDS
        );
        assert!(settings.backups_enabled);
        assert_eq!(settings.shortcuts.graph_overlay, "Ctrl+Shift+G");
        assert_eq!(settings.editor_font_size, 15.0);
        assert_eq!(settings.ui_font_size, 14.0);
        assert_eq!(settings.sidebar_width, 260.0);
        assert_eq!(settings.daily_notes_folder, PathBuf::from("Daily"));
        assert_eq!(settings.daily_note_format, "%Y-%m-%d");
        assert_eq!(settings.templates_folder, PathBuf::from("Templates"));
        assert_eq!(settings.quick_capture_target, QuickCaptureTarget::DailyNote);
    }

    #[test]
    fn settings_migration_preserves_legacy_vault_layouts() {
        for version in [6, 7, 8] {
            let json = format!(r#"{{"version": {version}, "vault_path": "C:/ExistingLiloVault"}}"#);
            let mut settings: AppSettings = serde_json::from_str(&json).expect("old settings");
            migrate_settings(&mut settings, Path::new("C:/DefaultVault"));
            assert_eq!(settings.version, SETTINGS_VERSION);
            assert_eq!(settings.vault_layout, VaultLayout::LegacyNotesDirectory);
            assert_eq!(settings.vault_path, PathBuf::from("C:/ExistingLiloVault"));
        }
    }

    #[test]
    fn current_settings_round_trip_without_touching_markdown() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let settings_path = temp.path().join("settings.json");
        let note_path = temp.path().join("portable.md");
        let markdown = "# Привет\n\nОбычный [[Markdown]] файл.\n";
        fs::write(&note_path, markdown).expect("write Markdown note");
        let settings = AppSettings {
            vault_path: temp.path().to_path_buf(),
            editor_font_size: 18.0,
            ..AppSettings::default()
        };

        save_settings(&settings_path, &settings).expect("save current settings");
        let loaded = load_settings(&settings_path).expect("load current settings");

        assert_eq!(loaded.version, SETTINGS_VERSION);
        assert_eq!(loaded.editor_font_size, 18.0);
        assert_eq!(fs::read_to_string(note_path).unwrap(), markdown);
    }

    #[test]
    fn corrupt_settings_are_backed_up_and_replaced_with_defaults() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let settings_path = temp.path().join("settings.json");
        fs::write(&settings_path, b"{ not valid json").expect("write corrupt settings");

        let settings = load_settings(&settings_path).expect("recover corrupt settings");
        let backups = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("settings.json.invalid-") && name.ends_with(".bak"))
            .collect::<Vec<_>>();

        assert_eq!(settings.version, SETTINGS_VERSION);
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(temp.path().join(&backups[0])).unwrap(),
            b"{ not valid json"
        );
    }

    #[test]
    fn version_nine_root_vault_stays_root_after_registry_upgrade() {
        let json = r#"{"version":9,"vault_path":"C:/DirectNotes","vault_layout":"Root"}"#;
        let mut settings: AppSettings = serde_json::from_str(json).unwrap();
        migrate_settings(&mut settings, Path::new("C:/DefaultVault"));
        assert_eq!(settings.vault_layout, VaultLayout::Root);
        assert_eq!(settings.vaults.len(), 1);
        assert_eq!(settings.vaults[0].path, settings.vault_path);
    }

    #[test]
    fn switching_vaults_restores_each_vaults_navigation_state() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("Work");
        let second = temp.path().join("Personal");
        let mut settings = AppSettings::default();
        set_vault_path(&mut settings, first.to_string_lossy().as_ref()).unwrap();
        let note_id = Uuid::new_v4();
        settings.selected_note_id = Some(note_id);
        settings.selected_folder = PathBuf::from("Projects");
        settings.collapsed_folders = vec![PathBuf::from("Archive")];
        settings.recent_note_ids = vec![note_id];

        set_vault_path(&mut settings, second.to_string_lossy().as_ref()).unwrap();
        assert_eq!(settings.selected_note_id, None);
        assert!(settings.selected_folder.as_os_str().is_empty());
        assert!(settings.recent_note_ids.is_empty());
        assert_eq!(settings.vaults.len(), 2);
        assert_eq!(settings.vaults[0].name(), "Personal");

        set_vault_path(&mut settings, first.to_string_lossy().as_ref()).unwrap();
        assert_eq!(settings.selected_note_id, Some(note_id));
        assert_eq!(settings.selected_folder, PathBuf::from("Projects"));
        assert_eq!(settings.collapsed_folders, vec![PathBuf::from("Archive")]);
        assert_eq!(settings.recent_note_ids, vec![note_id]);
        assert_eq!(settings.vaults[0].name(), "Work");
    }

    #[test]
    fn newly_selected_folder_becomes_the_note_root() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let selected = temp.path().join("Obsidian-style vault");
        let mut settings = AppSettings::default();

        set_vault_path(&mut settings, selected.to_string_lossy().as_ref())
            .expect("select root vault");

        assert_eq!(settings.vault_layout, VaultLayout::Root);
        assert_eq!(settings.vault_path, selected);
        let (notes, trash, backups) = vault_directories(&selected, settings.vault_layout);
        assert_eq!(notes, selected);
        assert_eq!(trash, notes.join(".lilo/Trash"));
        assert_eq!(backups, notes.join(".lilo/Backups"));
    }

    #[test]
    fn selecting_an_existing_lilo_vault_keeps_its_layout() {
        let temp = tempfile::tempdir().expect("temporary directory");
        fs::create_dir_all(temp.path().join("Notes")).unwrap();
        fs::create_dir_all(temp.path().join("Backups")).unwrap();
        let mut settings = AppSettings::default();

        set_vault_path(&mut settings, temp.path().to_string_lossy().as_ref())
            .expect("select legacy vault");

        assert_eq!(settings.vault_layout, VaultLayout::LegacyNotesDirectory);
    }

    #[test]
    fn batch_save_reports_partial_read_only_failure() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let backups = temp.path().join("Backups");
        let mut writable = Note::new_named(temp.path(), "Writable");
        let mut read_only = Note::new_named(temp.path(), "Read only");
        save_note(&writable).unwrap();
        save_note(&read_only).unwrap();
        let original_permissions = fs::metadata(&read_only.file_path).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&read_only.file_path, permissions).unwrap();
        writable.content = "saved".to_owned();
        read_only.content = "must fail".to_owned();
        let ids = HashSet::from([writable.id, read_only.id]);

        let report = save_notes_with_report(
            &[writable.clone(), read_only.clone()],
            &ids,
            &backups,
            true,
            5,
        );

        assert_eq!(report.saved_note_ids, vec![writable.id]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].note_id, read_only.id);
        assert!(
            fs::read_to_string(&writable.file_path)
                .unwrap()
                .contains("saved")
        );
        assert!(
            !fs::read_to_string(&read_only.file_path)
                .unwrap()
                .contains("must fail")
        );

        fs::set_permissions(&read_only.file_path, original_permissions).unwrap();
    }

    #[test]
    fn interrupted_atomic_write_preserves_original_file() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("settings.json");
        fs::write(&path, "original").unwrap();
        fs::create_dir(path.with_extension("lilo-tmp")).unwrap();

        assert!(atomic_write(&path, b"replacement").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn vault_snapshot_detects_empty_folders_and_ignores_cache() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let notes_dir = temp.path().join("Notes");
        fs::create_dir_all(&notes_dir).expect("create Notes");
        let before = vault_snapshot(&notes_dir).expect("initial snapshot");
        fs::create_dir(notes_dir.join("Empty")).expect("create empty folder");
        let after = vault_snapshot(&notes_dir).expect("updated snapshot");

        assert_ne!(before, after);

        // Cache folder should be ignored
        fs::create_dir(notes_dir.join("Cache")).expect("create cache folder");
        let after_cache = vault_snapshot(&notes_dir).expect("snapshot with cache");
        assert_eq!(after, after_cache);
    }

    #[test]
    fn large_nested_vault_loads_completely_with_bounded_diagnostics() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());
        for folder_index in 0..12 {
            let folder = ensure_note_folder(
                &paths.notes_dir,
                Path::new(&format!("Area {folder_index}/Topic")),
            )
            .expect("create nested folder");
            for note_index in 0..25 {
                let mut note =
                    Note::new_named(&folder, &format!("Note {folder_index}-{note_index}"));
                note.content = format!(
                    "# Test\n\n[[Note {}-{}]] #performance",
                    folder_index,
                    (note_index + 1) % 25
                );
                save_note(&note).expect("save generated note");
            }
        }
        fs::write(
            paths.notes_dir.join("Area 0/Topic/broken.md"),
            "---\ninvalid: [yaml\n---\n\nBody survives",
        )
        .expect("write malformed note");

        let started = std::time::Instant::now();
        let (notes, warnings, folders) = load_notes(&paths.notes_dir).expect("load large vault");
        let elapsed = started.elapsed();

        assert_eq!(notes.len(), 301);
        assert_eq!(warnings.len(), 1);
        assert!(folders.len() >= 24);
        assert!(elapsed < std::time::Duration::from_secs(10));
    }

    #[test]
    fn quick_capture_target_serialization_and_defaults() {
        let targets = vec![
            QuickCaptureTarget::DailyNote,
            QuickCaptureTarget::Inbox,
            QuickCaptureTarget::NewNote,
            QuickCaptureTarget::CustomNote("Meeting Notes".to_owned()),
        ];

        for target in targets {
            let json = serde_json::to_string(&target).expect("serialize target");
            let parsed: QuickCaptureTarget =
                serde_json::from_str(&json).expect("deserialize target");
            assert_eq!(parsed, target);
        }

        // Check deserializing empty settings retains default daily template and global shortcut
        let parsed_settings: AppSettings =
            serde_json::from_str("{}").expect("parse default settings");
        assert_eq!(parsed_settings.default_daily_template, "Daily");
        assert_eq!(
            parsed_settings.global_quick_capture_shortcut,
            "Ctrl+Shift+C"
        );
        assert!(parsed_settings.global_quick_capture_enabled);
        assert_eq!(
            parsed_settings.quick_capture_target,
            QuickCaptureTarget::DailyNote
        );
    }
}
