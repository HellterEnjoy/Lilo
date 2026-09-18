use super::*;

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

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    pub version: u32,
    pub vault_path: PathBuf,
    pub vault_layout: VaultLayout,
    pub vaults: Vec<VaultEntry>,
    pub selected_note_id: Option<Uuid>,
    pub selected_folder: PathBuf,
    pub collapsed_folders: Vec<PathBuf>,
    pub note_sort: NoteSort,
    pub theme: ThemeChoice,
    pub left_sidebar_open: bool,
    pub right_sidebar_open: bool,
    pub editor_max_width: f32,
    pub typewriter_mode: bool,
    pub show_status_bar: bool,
    pub editor_font_size: f32,
    pub ui_font_size: f32,
    pub compact_density: bool,
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
    pub autosave_enabled: bool,
    pub autosave_interval_seconds: u64,
    pub backups_enabled: bool,
    pub backup_limit: usize,
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
            vault_layout: VaultLayout::Root,
            vaults: Vec::new(),
            selected_note_id: None,
            selected_folder: PathBuf::new(),
            collapsed_folders: Vec::new(),
            note_sort: NoteSort::Updated,
            theme: ThemeChoice::Dark,
            left_sidebar_open: true,
            right_sidebar_open: false,
            editor_max_width: 780.0,
            typewriter_mode: false,
            show_status_bar: true,
            editor_font_size: 16.0,
            ui_font_size: 14.0,
            compact_density: false,
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
            accent_rgb: [155, 124, 255],
            always_on_top: true,
            autostart: false,
            shortcuts: ShortcutSettings::default(),
            graph_node_offsets: Vec::new(),
            autosave_enabled: true,
            autosave_interval_seconds: DEFAULT_AUTOSAVE_INTERVAL_SECONDS,
            backups_enabled: true,
            backup_limit: 20,
            analytics: crate::analytics::AnalyticsSettings::default(),
        }
    }
}

impl AppSettings {
    pub fn remember_active_vault(&mut self) {
        if self.vault_path.as_os_str().is_empty() {
            return;
        }
        let entry = VaultEntry {
            path: self.vault_path.clone(),
            layout: self.vault_layout,
            selected_note_id: self.selected_note_id,
            selected_folder: self.selected_folder.clone(),
            collapsed_folders: self.collapsed_folders.clone(),
            recent_note_ids: self.recent_note_ids.clone(),
        };
        if let Some(existing) = self
            .vaults
            .iter_mut()
            .find(|vault| vault.path == entry.path)
        {
            *existing = entry;
        } else {
            self.vaults.insert(0, entry);
        }
    }

    pub(super) fn activate_vault(&mut self, path: PathBuf, layout: VaultLayout) {
        self.remember_active_vault();
        let saved = self
            .vaults
            .iter()
            .position(|vault| vault.path == path)
            .map(|index| self.vaults.remove(index));
        self.vault_path = path;
        self.vault_layout = saved.as_ref().map_or(layout, |vault| vault.layout);
        self.selected_note_id = saved.as_ref().and_then(|vault| vault.selected_note_id);
        self.selected_folder = saved
            .as_ref()
            .map_or_else(PathBuf::new, |vault| vault.selected_folder.clone());
        self.collapsed_folders = saved
            .as_ref()
            .map_or_else(Vec::new, |vault| vault.collapsed_folders.clone());
        self.recent_note_ids = saved
            .as_ref()
            .map_or_else(Vec::new, |vault| vault.recent_note_ids.clone());
        self.vaults.insert(
            0,
            VaultEntry {
                path: self.vault_path.clone(),
                layout: self.vault_layout,
                selected_note_id: self.selected_note_id,
                selected_folder: self.selected_folder.clone(),
                collapsed_folders: self.collapsed_folders.clone(),
                recent_note_ids: self.recent_note_ids.clone(),
            },
        );
    }
}

pub(super) fn migrate_settings(settings: &mut AppSettings, default_vault_path: &Path) {
    let loaded_settings_version = settings.version;
    if settings.vault_path.as_os_str().is_empty() {
        settings.vault_path = default_vault_path.to_path_buf();
    }
    if (MIN_SUPPORTED_SETTINGS_VERSION..ROOT_VAULT_LAYOUT_VERSION)
        .contains(&loaded_settings_version)
    {
        settings.vault_layout = VaultLayout::LegacyNotesDirectory;
    }
    settings.version = SETTINGS_VERSION;
    settings.remember_active_vault();

    if settings.editor_font_size == 0.0 {
        settings.editor_font_size = 16.0;
    }
    if settings.ui_font_size == 0.0 {
        settings.ui_font_size = 13.0;
    }
    if settings.sidebar_width == 0.0 {
        settings.sidebar_width = 220.0;
    }
    settings.autosave_interval_seconds = settings
        .autosave_interval_seconds
        .clamp(MIN_AUTOSAVE_INTERVAL_SECONDS, MAX_AUTOSAVE_INTERVAL_SECONDS);
    if settings.daily_note_format.trim().is_empty() {
        settings.daily_note_format = "%Y-%m-%d".to_owned();
    }
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> StorageResult<()> {
    let mut saved = settings.clone();
    saved.remember_active_vault();
    let json = serde_json::to_string_pretty(&saved)?;
    atomic_write(path, json.as_bytes())?;
    Ok(())
}

pub(super) fn load_settings(path: &Path) -> StorageResult<AppSettings> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(AppSettings::default()),
        Err(error) => {
            report_invalid_settings(path, &error.to_string());
            return Ok(AppSettings::default());
        }
    };
    let json = match std::str::from_utf8(&bytes) {
        Ok(json) => json,
        Err(error) => {
            backup_invalid_settings(path, &bytes, &error.to_string());
            return Ok(AppSettings::default());
        }
    };
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(value) => value,
        Err(error) => {
            backup_invalid_settings(path, &bytes, &error.to_string());
            return Ok(AppSettings::default());
        }
    };
    if value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|version| version < u64::from(MIN_SUPPORTED_SETTINGS_VERSION))
    {
        backup_invalid_settings(path, &bytes, "settings format predates Lilo 0.2.0");
        return Ok(AppSettings::default());
    }
    match serde_json::from_value(value) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            backup_invalid_settings(path, &bytes, &error.to_string());
            Ok(AppSettings::default())
        }
    }
}

fn backup_invalid_settings(path: &Path, bytes: &[u8], reason: &str) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let file_name = path.file_name().map_or_else(
        || "settings.json".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let backup = path.with_file_name(format!("{file_name}.invalid-{suffix}.bak"));
    match fs::write(&backup, bytes) {
        Ok(()) => eprintln!(
            "Invalid Lilo settings ({reason}); preserved {} and restored defaults",
            backup.display()
        ),
        Err(error) => eprintln!(
            "Invalid Lilo settings ({reason}); could not create backup {}: {error}; restored defaults",
            backup.display()
        ),
    }
}

fn report_invalid_settings(path: &Path, reason: &str) {
    eprintln!(
        "Could not read Lilo settings {} ({reason}); restored defaults",
        path.display()
    );
}
