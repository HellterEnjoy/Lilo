use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultLayout {
    /// Current layout: Markdown files live directly in the selected vault root.
    #[default]
    Root,
    /// Compatibility layout used by Lilo 0.2.1 and earlier.
    LegacyNotesDirectory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEntry {
    pub path: PathBuf,
    pub layout: VaultLayout,
    pub selected_note_id: Option<Uuid>,
    pub selected_folder: PathBuf,
    pub collapsed_folders: Vec<PathBuf>,
    pub recent_note_ids: Vec<Uuid>,
}

impl VaultEntry {
    pub fn name(&self) -> String {
        vault_name(&self.path)
    }
}

pub fn vault_name(path: &Path) -> String {
    path.file_name()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub struct StoragePaths {
    pub settings_path: PathBuf,
    pub vault_root: PathBuf,
    pub notes_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub backups_dir: PathBuf,
}

pub struct LoadedStorage {
    pub data: AppData,
    pub settings: AppSettings,
    pub paths: StoragePaths,
    pub warnings: Vec<String>,
    /// Paths relative to `Notes`; empty means the root.
    pub folder_paths: Vec<PathBuf>,
}

pub fn load_storage() -> StorageResult<LoadedStorage> {
    let project_dirs = ProjectDirs::from("com", "HellterEnjoy", "Lilo")
        .ok_or_else(|| io::Error::other("Failed to resolve application directories"))?;
    // An explicit data directory enables portable storage and isolated UI review.
    let portable = std::env::var_os("LILO_DATA_DIR").map(PathBuf::from);
    if portable.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(io::Error::other("LILO_DATA_DIR must be an absolute path").into());
    }
    let config_dir = portable
        .clone()
        .unwrap_or_else(|| project_dirs.config_dir().to_path_buf());
    fs::create_dir_all(&config_dir)?;
    let settings_path = config_dir.join("settings.json");
    let default_vault_path = portable.as_ref().map_or_else(
        || default_vault_path(&config_dir),
        |path| path.join("Vault"),
    );
    let mut settings = load_settings(&settings_path)?;
    migrate_settings(&mut settings, &default_vault_path);

    let vault_root = settings.vault_path.clone();
    let (notes_dir, trash_dir, backups_dir) = vault_directories(&vault_root, settings.vault_layout);
    let templates_dir = if settings.templates_folder.as_os_str().is_empty() {
        notes_dir.join("Templates")
    } else {
        notes_dir.join(&settings.templates_folder)
    };
    let daily_dir = if settings.daily_notes_folder.as_os_str().is_empty() {
        notes_dir.join("Daily")
    } else {
        notes_dir.join(&settings.daily_notes_folder)
    };

    fs::create_dir_all(&notes_dir)?;
    fs::create_dir_all(&trash_dir)?;
    fs::create_dir_all(&backups_dir)?;
    let _ = fs::create_dir_all(&templates_dir);
    let _ = fs::create_dir_all(&daily_dir);

    // Bootstrap default Daily template if missing
    let daily_template_path = templates_dir.join("Daily.md");
    if !daily_template_path.exists() {
        let default_template_content = "---\ntags:\n  - daily\n---\n# {{date}}\n\n## 🎯 Focus\n- [ ] {{cursor}}\n\n## 📋 Tasks\n- [ ] \n\n## 📝 Notes & Log\n";
        let _ = atomic_write(&daily_template_path, default_template_content.as_bytes());
    }
    if settings.default_daily_template.trim().is_empty() {
        settings.default_daily_template = "Daily".to_owned();
    }

    // Vault hidden .lilo/cache folder with .gitignore
    let vault_lilo = settings.vault_path.join(".lilo");
    let _ = fs::create_dir_all(vault_lilo.join("cache"));
    let _ = fs::write(vault_lilo.join(".gitignore"), "*\n");

    let excluded_directories = managed_note_exclusions(&notes_dir, &settings);
    let (notes, warnings, folder_paths) = load_notes_excluding(&notes_dir, &excluded_directories)?;

    if !is_safe_relative_path(&settings.selected_folder)
        || !folder_paths.contains(&settings.selected_folder)
    {
        settings.selected_folder = PathBuf::new();
    }
    settings
        .collapsed_folders
        .retain(|path| is_safe_relative_path(path) && folder_paths.contains(path));

    let mut data = AppData {
        notes,
        selected_note_id: settings.selected_note_id,
    };
    data.normalize_selection();
    settings.selected_note_id = data.selected_note_id;
    settings.remember_active_vault();
    save_settings(&settings_path, &settings)?;

    Ok(LoadedStorage {
        data,
        settings,
        paths: StoragePaths {
            settings_path,
            vault_root,
            notes_dir,
            trash_dir,
            backups_dir,
        },
        warnings,
        folder_paths,
    })
}

pub(super) fn vault_directories(root: &Path, layout: VaultLayout) -> (PathBuf, PathBuf, PathBuf) {
    match layout {
        VaultLayout::Root => (
            root.to_path_buf(),
            root.join(".lilo/Trash"),
            root.join(".lilo/Backups"),
        ),
        VaultLayout::LegacyNotesDirectory => {
            (root.join("Notes"), root.join("Trash"), root.join("Backups"))
        }
    }
}

pub fn reload_notes(
    paths: &StoragePaths,
    settings: &AppSettings,
) -> StorageResult<(Vec<Note>, Vec<String>, Vec<PathBuf>)> {
    let excluded_directories = managed_note_exclusions(&paths.notes_dir, settings);
    load_notes_excluding(&paths.notes_dir, &excluded_directories)
}

pub fn vault_snapshot(notes_dir: &Path) -> StorageResult<HashSet<(PathBuf, u128)>> {
    let mut snapshot = HashSet::new();
    let mut directories = vec![notes_dir.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str.eq_ignore_ascii_case("cache") {
                    continue;
                }
                snapshot.insert((entry.path(), 0));
                directories.push(entry.path());
                continue;
            }
            let path = entry.path();
            if file_type.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                let modified = entry
                    .metadata()?
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                snapshot.insert((path, modified));
            }
        }
    }
    Ok(snapshot)
}

pub fn set_vault_path(settings: &mut AppSettings, value: &str) -> StorageResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(io::Error::other("Vault path cannot be empty").into());
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(io::Error::other("Vault path must be absolute").into());
    }
    fs::create_dir_all(&path)?;
    let layout = settings
        .vaults
        .iter()
        .find(|vault| vault.path == path)
        .map_or_else(|| detect_vault_layout(&path), |vault| vault.layout);
    settings.activate_vault(path, layout);
    Ok(())
}

fn detect_vault_layout(path: &Path) -> VaultLayout {
    let has_legacy_notes = path.join("Notes").is_dir();
    let has_legacy_recovery = path.join("Trash").is_dir() || path.join("Backups").is_dir();
    if has_legacy_notes && has_legacy_recovery {
        VaultLayout::LegacyNotesDirectory
    } else {
        VaultLayout::Root
    }
}

fn default_vault_path(config_dir: &Path) -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| config_dir.join("Vault"))
        .join("LiloVault")
}
