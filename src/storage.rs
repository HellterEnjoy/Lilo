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

pub type StorageResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const SETTINGS_VERSION: u32 = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteSort {
    #[default]
    Updated,
    Title,
    Created,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    Light,
    #[default]
    Dark,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolbarPlacement {
    #[default]
    Auto,
    Top,
    Left,
    Right,
    Floating,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickCaptureTarget {
    #[default]
    DailyNote,
    Inbox,
    NewNote,
    CustomNote(String),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutSettings {
    pub new_note: String,
    pub search: String,
    pub graph: String,
    pub graph_overlay: String,
    pub save: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPreset {
    pub id: Uuid,
    pub name: String,
    pub query: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            new_note: "Ctrl+N".to_owned(),
            search: "Ctrl+P".to_owned(),
            graph: "Ctrl+G".to_owned(),
            graph_overlay: "Ctrl+Shift+G".to_owned(),
            save: "Ctrl+S".to_owned(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct NoteFrontmatter {
    id: Uuid,
    title: String,
    created_at: DateTime<Local>,
    updated_at: DateTime<Local>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
}

#[derive(Clone)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub file_path: PathBuf,
    pub search_text: String,
}

impl Note {
    pub fn new(notes_dir: &Path) -> Self {
        Self::new_named(notes_dir, "")
    }

    pub fn new_named(notes_dir: &Path, title: &str) -> Self {
        let id = Uuid::new_v4();
        let now = Local::now();
        let title = title.trim().to_owned();
        let file_title = if title.is_empty() {
            "Untitled"
        } else {
            title.as_str()
        };
        let file_path = notes_dir.join(note_file_name(file_title, id));

        let mut note = Self {
            id,
            title,
            content: String::new(),
            created_at: now,
            updated_at: now,
            aliases: Vec::new(),
            tags: Vec::new(),
            pinned: false,
            file_path,
            search_text: String::new(),
        };
        note.refresh_search_text();
        note
    }

    pub fn mark_as_updated(&mut self) {
        self.updated_at = Local::now();
        self.refresh_search_text();
    }

    pub fn refresh_search_text(&mut self) {
        self.search_text = format!("{}\n{}", self.title, self.content).to_lowercase();
    }
}

pub struct AppData {
    pub notes: Vec<Note>,
    pub selected_note_id: Option<Uuid>,
}

impl AppData {
    pub fn create_note(&mut self, notes_dir: &Path) -> Uuid {
        let note = Note::new(notes_dir);
        let id = note.id;
        self.notes.push(note);
        self.selected_note_id = Some(id);
        id
    }

    pub fn create_note_named(&mut self, notes_dir: &Path, title: &str) -> Uuid {
        let note = Note::new_named(notes_dir, title);
        let id = note.id;
        self.notes.push(note);
        self.selected_note_id = Some(id);
        id
    }

    pub fn remove_note(&mut self, id: Uuid) -> Option<Note> {
        let index = self.notes.iter().position(|note| note.id == id)?;
        let note = self.notes.remove(index);

        if self.selected_note_id == Some(id) {
            self.selected_note_id = self
                .notes
                .get(index)
                .or_else(|| self.notes.last())
                .map(|note| note.id);
        }

        Some(note)
    }

    pub fn selected_note(&self) -> Option<&Note> {
        let selected_id = self.selected_note_id?;
        self.notes.iter().find(|note| note.id == selected_id)
    }

    pub fn selected_note_mut(&mut self) -> Option<&mut Note> {
        let selected_id = self.selected_note_id?;
        self.notes.iter_mut().find(|note| note.id == selected_id)
    }

    fn normalize_selection(&mut self) {
        let selection_exists = self
            .selected_note_id
            .is_some_and(|id| self.notes.iter().any(|note| note.id == id));

        if !selection_exists {
            self.selected_note_id = self.notes.first().map(|note| note.id);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    pub version: u32,
    pub vault_path: PathBuf,
    pub selected_note_id: Option<Uuid>,
    pub legacy_migration_completed: bool,
    pub selected_folder: PathBuf,
    pub collapsed_folders: Vec<PathBuf>,
    pub note_sort: NoteSort,
    pub theme: ThemeChoice,
    pub font_size: f32,
    pub left_sidebar_open: bool,
    pub right_sidebar_open: bool,
    pub editor_max_width: f32,
    pub typewriter_mode: bool,
    pub show_status_bar: bool,
    pub editor_font_size: f32,
    pub ui_font_size: f32,
    pub sidebar_width: f32,
    pub zen_mode: bool,
    pub daily_notes_folder: PathBuf,
    pub daily_note_format: String,
    pub templates_folder: PathBuf,
    pub default_daily_template: String,
    pub attachments_folder: PathBuf,
    pub quick_capture_target: QuickCaptureTarget,
    pub quick_capture_custom_note: String,
    pub global_quick_capture_enabled: bool,
    pub global_quick_capture_shortcut: String,
    pub recent_commands: Vec<crate::commands::CommandAction>,
    pub recent_note_ids: Vec<Uuid>,
    pub sidebar_recent_open: bool,
    pub search_presets: Vec<SearchPreset>,
    pub tag_browser_expanded: bool,
    pub saved_searches_expanded: bool,
    pub accent_rgb: [u8; 3],
    pub always_on_top: bool,
    pub autostart: bool,
    pub shortcuts: ShortcutSettings,
    pub graph_node_offsets: Vec<GraphNodeOffset>,
    pub backups_enabled: bool,
    pub backup_limit: usize,
    pub toolbar_placement: ToolbarPlacement,
    pub toolbar_expanded: bool,
    pub floating_toolbar_vertical: bool,
    pub floating_toolbar_position: [f32; 2],
    pub analytics: crate::analytics::AnalyticsSettings,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphNodeOffset {
    pub scope: String,
    pub note_id: Uuid,
    pub x: f32,
    pub y: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            vault_path: PathBuf::new(),
            selected_note_id: None,
            legacy_migration_completed: true,
            selected_folder: PathBuf::new(),
            collapsed_folders: Vec::new(),
            note_sort: NoteSort::Updated,
            theme: ThemeChoice::Dark,
            font_size: 14.0,
            left_sidebar_open: true,
            right_sidebar_open: false,
            editor_max_width: 780.0,
            typewriter_mode: false,
            show_status_bar: true,
            editor_font_size: 14.0,
            ui_font_size: 13.0,
            sidebar_width: 260.0,
            zen_mode: false,
            daily_notes_folder: PathBuf::from("Daily"),
            daily_note_format: "%Y-%m-%d".to_owned(),
            templates_folder: PathBuf::from("Templates"),
            default_daily_template: "Daily".to_owned(),
            attachments_folder: PathBuf::from("Attachments"),
            quick_capture_target: QuickCaptureTarget::DailyNote,
            quick_capture_custom_note: "Quick Notes".to_owned(),
            global_quick_capture_enabled: true,
            global_quick_capture_shortcut: "Ctrl+Shift+C".to_owned(),
            recent_commands: Vec::new(),
            recent_note_ids: Vec::new(),
            sidebar_recent_open: true,
            search_presets: vec![
                SearchPreset {
                    id: Uuid::new_v4(),
                    name: "To Do Items".to_owned(),
                    query: "tag:todo".to_owned(),
                },
                SearchPreset {
                    id: Uuid::new_v4(),
                    name: "Daily Notes".to_owned(),
                    query: "path:Daily".to_owned(),
                },
            ],
            tag_browser_expanded: true,
            saved_searches_expanded: true,
            accent_rgb: [129, 140, 248],
            always_on_top: true,
            autostart: false,
            shortcuts: ShortcutSettings::default(),
            graph_node_offsets: Vec::new(),
            backups_enabled: true,
            backup_limit: 20,
            toolbar_placement: ToolbarPlacement::Auto,
            toolbar_expanded: false,
            floating_toolbar_vertical: false,
            floating_toolbar_position: [24.0, 72.0],
            analytics: crate::analytics::AnalyticsSettings::default(),
        }
    }
}

pub struct StoragePaths {
    pub settings_path: PathBuf,
    pub notes_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub backups_dir: PathBuf,
    #[allow(dead_code)]
    pub cache_dir: PathBuf,
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
    let config_dir = project_dirs.config_dir().to_path_buf();
    fs::create_dir_all(&config_dir)?;

    let cache_dir = project_dirs.cache_dir().to_path_buf();
    fs::create_dir_all(&cache_dir)?;

    let settings_path = config_dir.join("settings.json");
    let default_vault_path = default_vault_path(&config_dir);
    let mut settings = load_settings(&settings_path)?;
    if settings.vault_path.as_os_str().is_empty() {
        settings.vault_path = default_vault_path;
    }
    settings.version = SETTINGS_VERSION;

    if settings.editor_font_size == 0.0 {
        settings.editor_font_size = if settings.font_size > 0.0 {
            settings.font_size
        } else {
            14.0
        };
    }
    if settings.font_size == 0.0 {
        settings.font_size = settings.editor_font_size;
    }
    if settings.ui_font_size == 0.0 {
        settings.ui_font_size = 13.0;
    }
    if settings.sidebar_width == 0.0 {
        settings.sidebar_width = 220.0;
    }
    if settings.daily_note_format.trim().is_empty() {
        settings.daily_note_format = "%Y-%m-%d".to_owned();
    }

    let notes_dir = settings.vault_path.join("Notes");
    let trash_dir = settings.vault_path.join("Trash");
    let backups_dir = settings.vault_path.join("Backups");
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
    let (mut notes, warnings, folder_paths) =
        load_notes_excluding(&notes_dir, &excluded_directories)?;

    settings.legacy_migration_completed = true;
    if !is_safe_relative_path(&settings.selected_folder)
        || !folder_paths.contains(&settings.selected_folder)
    {
        settings.selected_folder = PathBuf::new();
    }
    settings
        .collapsed_folders
        .retain(|path| is_safe_relative_path(path) && folder_paths.contains(path));

    if notes.is_empty() {
        let note = Note::new(&notes_dir);
        save_note(&note)?;
        notes.push(note);
    }

    let mut data = AppData {
        notes,
        selected_note_id: settings.selected_note_id,
    };
    data.normalize_selection();
    settings.selected_note_id = data.selected_note_id;
    save_settings(&settings_path, &settings)?;

    Ok(LoadedStorage {
        data,
        settings,
        paths: StoragePaths {
            settings_path,
            notes_dir,
            trash_dir,
            backups_dir,
            cache_dir,
        },
        warnings,
        folder_paths,
    })
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> StorageResult<()> {
    let json = serde_json::to_string_pretty(settings)?;
    atomic_write(path, json.as_bytes())?;
    Ok(())
}

pub fn save_note(note: &Note) -> StorageResult<()> {
    if let Some(parent) = note.file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut aliases = note.aliases.clone();
    if !note.title.trim().is_empty()
        && !aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(note.title.trim()))
    {
        aliases.push(note.title.trim().to_owned());
    }

    let metadata = NoteFrontmatter {
        id: note.id,
        title: note.title.clone(),
        created_at: note.created_at,
        updated_at: note.updated_at,
        aliases,
        tags: note.tags.clone(),
        pinned: note.pinned,
    };
    let yaml = serde_saphyr::to_string(&metadata)?;
    let markdown = format!("---\n{}\n---\n\n{}", yaml.trim_end(), note.content);
    atomic_write(&note.file_path, markdown.as_bytes())?;
    Ok(())
}

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

pub fn move_note_to_folder(
    note: &mut Note,
    paths: &StoragePaths,
    target_relative: &Path,
) -> StorageResult<()> {
    let source_relative = note
        .file_path
        .strip_prefix(&paths.notes_dir)
        .map_err(|_| io::Error::other("Refusing to move a note outside the Notes directory"))?;
    if !is_safe_relative_path(source_relative) || source_relative.as_os_str().is_empty() {
        return Err(io::Error::other("Note has an unsafe relative path").into());
    }

    let target_directory = ensure_note_folder(&paths.notes_dir, target_relative)?;
    let file_name = note
        .file_path
        .file_name()
        .ok_or_else(|| io::Error::other("Note path has no file name"))?;
    let destination = target_directory.join(file_name);
    if destination == note.file_path {
        return Ok(());
    }
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note file with this name already exists in the target folder",
        )
        .into());
    }

    fs::rename(&note.file_path, &destination)?;
    note.file_path = destination;
    Ok(())
}

/// Creates a safe folder path below `Notes`.
pub fn ensure_note_folder(notes_dir: &Path, relative: &Path) -> StorageResult<PathBuf> {
    if !is_safe_relative_path(relative) {
        return Err(io::Error::other("Folder path must stay inside Notes").into());
    }

    fs::create_dir_all(notes_dir)?;
    let mut current = notes_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::other("Invalid folder path component").into());
        };
        validate_folder_name(name.to_string_lossy().as_ref())?;
        current.push(name);

        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::other("Links are not allowed inside Notes folders").into());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other("A folder path component is not a directory").into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(current)
}

pub fn create_note_folder(
    notes_dir: &Path,
    parent_relative: &Path,
    name: &str,
) -> StorageResult<PathBuf> {
    validate_folder_name(name)?;
    let parent = ensure_note_folder(notes_dir, parent_relative)?;
    let relative = parent_relative.join(name.trim());
    let target = parent.join(name.trim());

    match fs::create_dir(&target) {
        Ok(()) => Ok(relative),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "Folder already exists").into())
        }
        Err(error) => Err(error.into()),
    }
}

pub fn rename_note_file(note: &mut Note) -> StorageResult<()> {
    let parent = note
        .file_path
        .parent()
        .ok_or_else(|| io::Error::other("Note path has no parent folder"))?;
    let title = if note.title.trim().is_empty() {
        "Untitled"
    } else {
        note.title.trim()
    };
    let destination = parent.join(note_file_name(title, note.id));
    if destination == note.file_path {
        return Ok(());
    }
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note with this file name already exists",
        )
        .into());
    }
    fs::rename(&note.file_path, &destination)?;
    note.file_path = destination;
    Ok(())
}

pub fn rename_folder(notes_dir: &Path, relative: &Path, new_name: &str) -> StorageResult<PathBuf> {
    if relative.as_os_str().is_empty() || !is_safe_relative_path(relative) {
        return Err(io::Error::other("The Notes root cannot be renamed").into());
    }
    validate_folder_name(new_name)?;
    let source = notes_dir.join(relative);
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new(""));
    let destination_relative = parent_relative.join(new_name.trim());
    let destination = notes_dir.join(&destination_relative);
    if !source.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Folder does not exist").into());
    }
    if destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Folder already exists").into());
    }
    fs::rename(source, destination)?;
    Ok(destination_relative)
}

#[allow(dead_code)]
pub fn delete_empty_folder(notes_dir: &Path, relative: &Path) -> StorageResult<()> {
    if relative.as_os_str().is_empty() || !is_safe_relative_path(relative) {
        return Err(io::Error::other("The Notes root cannot be deleted").into());
    }
    let target = notes_dir.join(relative);
    if fs::read_dir(&target)?.next().transpose()?.is_some() {
        return Err(io::Error::other("Only empty folders can be deleted with this method").into());
    }
    fs::remove_dir(target)?;
    Ok(())
}

/// Safely deletes a folder by moving any notes inside it to Trash.
pub fn delete_folder_with_trash(
    notes_dir: &Path,
    trash_dir: &Path,
    relative: &Path,
    notes: &[Note],
) -> StorageResult<Vec<Uuid>> {
    if relative.as_os_str().is_empty() || !is_safe_relative_path(relative) {
        return Err(io::Error::other("The Notes root cannot be deleted").into());
    }
    let target = notes_dir.join(relative);
    if !target.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Folder does not exist").into());
    }

    let mut trashed_ids = Vec::new();
    let paths = StoragePaths {
        settings_path: PathBuf::new(),
        notes_dir: notes_dir.to_path_buf(),
        trash_dir: trash_dir.to_path_buf(),
        backups_dir: PathBuf::new(),
        cache_dir: PathBuf::new(),
    };

    for note in notes {
        if let Ok(rel) = note.file_path.strip_prefix(notes_dir)
            && rel.starts_with(relative)
        {
            move_note_to_trash(note, &paths)?;
            trashed_ids.push(note.id);
        }
    }

    if target.exists() {
        let _ = fs::remove_dir_all(&target);
    }

    Ok(trashed_ids)
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
    settings.vault_path = path;
    Ok(())
}

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
    let vault_root = paths
        .notes_dir
        .parent()
        .ok_or_else(|| io::Error::other("Vault has no root directory"))?
        .canonicalize()?;
    if destination_root.starts_with(&vault_root) {
        return Err(io::Error::other("Export destination must be outside the active vault").into());
    }

    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let destination = destination_root.join(format!("Lilo-Vault-{timestamp}"));
    if destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Export already exists").into());
    }
    fs::create_dir(&destination)?;
    copy_directory(&paths.notes_dir, &destination.join("Notes"))?;
    copy_directory(&paths.trash_dir, &destination.join("Trash"))?;
    fs::copy(&paths.settings_path, destination.join("settings.json"))?;
    Ok(destination)
}

pub fn vault_diagnostics(
    paths: &StoragePaths,
    settings: &AppSettings,
) -> StorageResult<Vec<String>> {
    let excluded_directories = managed_note_exclusions(&paths.notes_dir, settings);
    let (_, warnings, _) = load_notes_excluding(&paths.notes_dir, &excluded_directories)?;
    let mut diagnostics = warnings;
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

fn safe_managed_file(root: &Path, relative: &Path) -> StorageResult<PathBuf> {
    if !is_safe_relative_path(relative) || relative.as_os_str().is_empty() {
        return Err(io::Error::other("Managed file path is unsafe").into());
    }
    let path = root.join(relative);
    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Managed file does not exist").into());
    }
    Ok(path)
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

fn load_settings(path: &Path) -> StorageResult<AppSettings> {
    match fs::read_to_string(path) {
        Ok(json) => Ok(serde_json::from_str(&json)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(error) => Err(error.into()),
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> StorageResult<()> {
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

fn default_vault_path(config_dir: &Path) -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| config_dir.join("Vault"))
        .join("LiloVault")
}

#[cfg(test)]
fn load_notes(notes_dir: &Path) -> StorageResult<(Vec<Note>, Vec<String>, Vec<PathBuf>)> {
    load_notes_excluding(notes_dir, &[])
}

fn managed_note_exclusions(notes_dir: &Path, settings: &AppSettings) -> Vec<PathBuf> {
    let templates = if settings.templates_folder.as_os_str().is_empty() {
        Path::new("Templates")
    } else {
        &settings.templates_folder
    };
    let attachments = if settings.attachments_folder.as_os_str().is_empty() {
        Path::new("Attachments")
    } else {
        &settings.attachments_folder
    };

    [templates, attachments]
        .into_iter()
        .filter(|path| is_safe_relative_path(path))
        .map(|path| notes_dir.join(path))
        .collect()
}

fn load_notes_excluding(
    notes_dir: &Path,
    excluded_directories: &[PathBuf],
) -> StorageResult<(Vec<Note>, Vec<String>, Vec<PathBuf>)> {
    let mut paths = Vec::new();
    let mut folder_paths = vec![PathBuf::new()];
    let mut pending_directories = vec![notes_dir.to_path_buf()];

    while let Some(directory) = pending_directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            // Never traverse links outside the vault.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if excluded_directories.contains(&path) {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str.eq_ignore_ascii_case("cache") {
                    continue;
                }

                let relative = path
                    .strip_prefix(notes_dir)
                    .map_err(|_| io::Error::other("Folder escaped Notes root"))?
                    .to_path_buf();
                if is_safe_relative_path(&relative) {
                    folder_paths.push(relative);
                    pending_directories.push(path);
                }
                continue;
            }

            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
            if file_type.is_file() && is_markdown {
                paths.push(path);
            }
        }
    }
    paths.sort();
    folder_paths.sort();
    folder_paths.dedup();

    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut ids = HashSet::new();
    for path in paths {
        match load_note(&path) {
            Ok(note) if ids.insert(note.id) => notes.push(note),
            Ok(note) => warnings.push(format!(
                "Skipped duplicate note UUID {} in {}",
                note.id,
                path.display()
            )),
            Err(error) => match recover_corrupt_note(&path) {
                Ok(note) if ids.insert(note.id) => {
                    warnings.push(format!(
                        "Recovered {} without valid frontmatter: {}",
                        path.display(),
                        error
                    ));
                    notes.push(note);
                }
                Ok(_) => warnings.push(format!(
                    "Skipped duplicate recovered note in {}",
                    path.display()
                )),
                Err(recovery_error) => warnings.push(format!(
                    "Failed to load {}: {}; recovery failed: {}",
                    path.display(),
                    error,
                    recovery_error
                )),
            },
        }
    }
    Ok((notes, warnings, folder_paths))
}

fn recover_corrupt_note(path: &Path) -> StorageResult<Note> {
    let raw = fs::read_to_string(path)?;
    let normalized = raw.replace("\r\n", "\n");
    let content = normalized.strip_prefix("---\n").map_or_else(
        || normalized.clone(),
        |after_opening| {
            after_opening
                .split_once("\n---\n")
                .map_or(normalized.clone(), |(_, body)| {
                    body.trim_start_matches('\n').to_owned()
                })
        },
    );
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified().unwrap_or(SystemTime::now());
    let created_at = DateTime::<Local>::from(modified);
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let title = stem
        .rsplit_once("--")
        .map_or(stem.as_str(), |(title, _)| title)
        .replace('-', " ");
    let mut stable_id = 0xcbf29ce484222325_u128;
    for byte in path.to_string_lossy().bytes() {
        stable_id ^= u128::from(byte);
        stable_id = stable_id.wrapping_mul(0x100000001b3);
    }
    let id = Uuid::from_u128(stable_id);
    let mut note = Note {
        id,
        title,
        content,
        created_at,
        updated_at: created_at,
        aliases: Vec::new(),
        tags: Vec::new(),
        pinned: false,
        file_path: path.to_path_buf(),
        search_text: String::new(),
    };
    note.refresh_search_text();
    Ok(note)
}

fn is_safe_relative_path(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        || path.as_os_str().is_empty()
}

fn validate_folder_name(name: &str) -> StorageResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err(io::Error::other("Folder name cannot be empty").into());
    }
    if trimmed.ends_with(['.', ' '])
        || trimmed.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err(io::Error::other("Folder name contains characters invalid on Windows").into());
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| trimmed.eq_ignore_ascii_case(reserved_name))
    {
        return Err(io::Error::other("Folder name is reserved on Windows").into());
    }
    Ok(())
}

fn load_note(path: &Path) -> StorageResult<Note> {
    let raw = fs::read_to_string(path)?;
    let normalized = raw.replace("\r\n", "\n");
    let after_opening = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| io::Error::other("Markdown file has no YAML frontmatter"))?;
    let closing_position = after_opening
        .find("\n---\n")
        .ok_or_else(|| io::Error::other("YAML frontmatter is not closed"))?;
    let yaml = &after_opening[..closing_position];
    let content = after_opening[closing_position + "\n---\n".len()..]
        .strip_prefix('\n')
        .unwrap_or(&after_opening[closing_position + "\n---\n".len()..])
        .to_owned();
    let metadata: NoteFrontmatter = serde_saphyr::from_str(yaml)?;
    let mut note = Note {
        id: metadata.id,
        title: metadata.title,
        content,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        aliases: metadata.aliases,
        tags: metadata.tags,
        pinned: metadata.pinned,
        file_path: path.to_path_buf(),
        search_text: String::new(),
    };
    note.refresh_search_text();
    Ok(note)
}

fn note_file_name(title: &str, id: Uuid) -> String {
    let sanitized = sanitize_file_stem(title);
    let id_text = id.simple().to_string();
    format!("{}--{}.md", sanitized, &id_text[..8])
}

fn sanitize_file_stem(title: &str) -> String {
    let mut stem: String = title
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            other => other,
        })
        .take(80)
        .collect();
    stem = stem.trim().trim_end_matches(['.', ' ']).to_owned();

    if stem.is_empty() {
        stem = "Untitled".to_owned();
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| stem.eq_ignore_ascii_case(reserved_name))
    {
        stem.insert(0, '_');
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &Path) -> StoragePaths {
        let paths = StoragePaths {
            settings_path: root.join("settings.json"),
            notes_dir: root.join("Notes"),
            trash_dir: root.join("Trash"),
            backups_dir: root.join("Backups"),
            cache_dir: root.join("Cache"),
        };
        for directory in [
            &paths.notes_dir,
            &paths.trash_dir,
            &paths.backups_dir,
            &paths.cache_dir,
        ] {
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
            notes_dir,
            trash_dir: trash_dir.clone(),
            backups_dir: temp.path().join("Backups"),
            cache_dir: temp.path().join("Cache"),
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
            notes_dir: notes_dir.clone(),
            trash_dir: temp.path().join("Trash"),
            backups_dir: temp.path().join("Backups"),
            cache_dir: temp.path().join("Cache"),
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
        let trashed = delete_folder_with_trash(&notes_dir, &trash_dir, &renamed, &notes)
            .expect("delete with trash");
        assert_eq!(trashed.len(), 1);
        assert!(!notes_dir.join(&renamed).exists());
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
            notes_dir: notes_dir.clone(),
            trash_dir,
            backups_dir,
            cache_dir: temp.path().join("Cache"),
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
    fn older_settings_receive_new_defaults() {
        let json = r#"{
            "version": 2,
            "vault_path": "C:/Vault",
            "selected_note_id": null,
            "legacy_migration_completed": true,
            "selected_folder": "",
            "collapsed_folders": []
        }"#;

        let settings: AppSettings = serde_json::from_str(json).expect("load old settings");

        assert_eq!(settings.note_sort, NoteSort::Updated);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert!(settings.backups_enabled);
        assert_eq!(settings.shortcuts.graph_overlay, "Ctrl+Shift+G");
        assert_eq!(settings.toolbar_placement, ToolbarPlacement::Auto);
        assert_eq!(settings.floating_toolbar_position, [24.0, 72.0]);
        assert_eq!(settings.editor_font_size, 14.0);
        assert_eq!(settings.ui_font_size, 13.0);
        assert_eq!(settings.sidebar_width, 260.0);
        assert_eq!(settings.daily_notes_folder, PathBuf::from("Daily"));
        assert_eq!(settings.daily_note_format, "%Y-%m-%d");
        assert_eq!(settings.templates_folder, PathBuf::from("Templates"));
        assert_eq!(settings.quick_capture_target, QuickCaptureTarget::DailyNote);
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
