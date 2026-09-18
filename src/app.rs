mod actions;
mod editor;
mod explorer;
mod inspector;
mod navigation;
mod notes;
mod persistence;
mod recovery;
mod settings;
mod shell;
mod vault;
use crate::analytics::{
    self, AnalyticsClient, AnalyticsFeature, AnalyticsOperation, AnalyticsResult,
};
use crate::commands::{self, CommandAction, CommandPaletteResult, CommandPaletteState};
use crate::daily::LocalDateService;
use crate::folders;
use crate::global_hotkey::{GlobalHotkeyEvent, GlobalHotkeyManager};
use crate::graph;
use crate::links::{self, LinkIndex, LinkResolution};
use crate::markdown;
use crate::platform;
use crate::quick_capture::{self, QuickCaptureState, QuickCaptureSubmission};
use crate::search::SearchQuery;
use crate::storage::{
    self, AppData, AppSettings, Note, NoteSort, QuickCaptureTarget, SearchPreset, StoragePaths,
    ThemeChoice,
};
use crate::tags::{self, TagIndex};
use crate::templates::TemplateEngine;
use crate::ui_style::{self, Icon};
use chrono::TimeZone;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

const EXTERNAL_SYNC_INTERVAL: Duration = Duration::from_secs(2);
const INDEX_REFRESH_DEBOUNCE: Duration = Duration::from_millis(350);
const ANALYTICS_INITIAL_DELAY: Duration = Duration::from_secs(10);
const ANALYTICS_UPDATE_INTERVAL: Duration = Duration::from_secs(15 * 60);
const AUTOSAVE_INTERVAL_OPTIONS: [u64; 6] = [15, 30, 60, 120, 300, 600];

pub(crate) struct WidgetApp {
    data: AppData,
    settings: AppSettings,
    storage_paths: StoragePaths,
    pending_delete_id: Option<Uuid>,
    dirty_note_ids: HashSet<Uuid>,
    pending_title_rename_ids: HashSet<Uuid>,
    dirty_since: Option<Instant>,
    pending_index_note_ids: HashSet<Uuid>,
    last_index_change: Option<Instant>,
    storage_message: Option<String>,
    link_index: LinkIndex,
    tag_index: TagIndex,
    folder_paths: Vec<PathBuf>,
    graph_state: graph::GraphState,
    preview_cache: crate::note_preview::PreviewCache,
    navigation_cache: Option<(u64, std::sync::Arc<navigation::NavigationIndex>)>,
    viewport_width: f32,
    explorer_drawer_open: bool,
    applied_theme: Option<(bool, [u8; 3], u32, bool)>,
    close_pending: bool,
    discard_on_close: bool,

    view: AppView,
    settings_section: usize,
    inspector_tab: usize,
    failed_save_ids: HashSet<Uuid>,
    search_query: String,
    focus_search: bool,
    focus_editor: bool,
    show_new_folder_input: bool,
    new_folder_name: String,
    editing_folder: Option<PathBuf>,
    folder_name_buffer: String,
    graph_overlay_open: bool,
    vault_path_buffer: String,
    vault_snapshot: HashSet<(PathBuf, u128)>,
    snapshot_worker: crate::vault_watch::SnapshotWorker,
    snapshot_epoch: u64,
    last_external_sync: Instant,
    external_conflict: bool,
    window_settings_applied: bool,
    recovery_tab: RecoveryTab,
    selected_backup: Option<PathBuf>,
    backup_preview: String,
    diagnostics: Vec<String>,
    import_path_buffer: String,
    export_path_buffer: String,
    external_changed_paths: Vec<PathBuf>,
    new_tag: String,
    new_alias: String,
    note_details_open: bool,

    // Tags and saved search state.
    tag_rename_dialog_open: bool,
    tag_to_rename: String,
    tag_new_name_buffer: String,
    pending_link_rewrite: Option<PendingLinkRewrite>,
    show_new_preset_input: bool,
    new_preset_name_buffer: String,

    // Attachment inspection state.
    attachments_orphans: Vec<PathBuf>,
    attachments_inspected: bool,

    // Note navigation state.
    history_back: Vec<Uuid>,
    history_forward: Vec<Uuid>,
    is_navigating_history: bool,
    note_titles_snapshot: HashMap<Uuid, String>,

    // Daily workflow and global capture state.
    hotkey_manager: GlobalHotkeyManager,
    command_palette_state: CommandPaletteState,
    quick_capture_state: QuickCaptureState,
    template_selector_open: bool,
    template_cache: Option<(PathBuf, PathBuf, Vec<crate::templates::TemplateEntry>)>,
    graph_fullscreen: bool,
    template_selector_for_new_note: bool,
    pending_folder_delete: Option<PathBuf>,
    pending_folder_notes_count: usize,
    pending_cursor_char_index: Option<(Uuid, usize)>,

    // Analytics is opt-in and all network work runs outside the UI thread.
    analytics_client: AnalyticsClient,
    analytics_dirty: bool,
    analytics_report_in_flight: bool,
    analytics_deletion_in_flight: bool,
    analytics_next_send: Instant,
    analytics_next_delete_attempt: Instant,
    analytics_status: Option<String>,
    analytics_details_open: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppView {
    Editor,
    NotesList,
    Graph,
    Trash,
    Settings,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RecoveryTab {
    #[default]
    Trash,
    Backups,
    Diagnostics,
}

#[derive(Clone)]
struct PendingLinkRewrite {
    old_title: String,
    new_title: String,
    affected_note_ids: Vec<Uuid>,
}

fn parse_local_shortcut(shortcut: &str) -> Option<egui::KeyboardShortcut> {
    let parts: Vec<_> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let key_name = *parts.last()?;
    let key = egui::Key::ALL
        .iter()
        .copied()
        .find(|key| key.name().eq_ignore_ascii_case(key_name))
        .or_else(|| egui::Key::from_name(key_name))?;
    let mut modifiers = egui::Modifiers::NONE;
    for part in &parts[..parts.len() - 1] {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            _ => return None,
        }
    }
    Some(egui::KeyboardShortcut::new(modifiers, key))
}

fn shortcut_pressed(ctx: &egui::Context, shortcut: &str) -> bool {
    parse_local_shortcut(shortcut).is_some_and(|shortcut| {
        ctx.input(|i| {
            i.modifiers.ctrl == shortcut.modifiers.ctrl
                && i.modifiers.shift == shortcut.modifiers.shift
                && i.modifiers.alt == shortcut.modifiers.alt
                && i.key_pressed(shortcut.logical_key)
        })
    })
}

fn shortcut_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(120.0));
    });
    if parse_local_shortcut(value).is_none() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "Invalid shortcut, for example Ctrl+Shift+N or Alt+F2.",
        );
    }
}

fn autosave_interval_label(seconds: u64) -> String {
    match seconds {
        60 => "1 minute".to_owned(),
        seconds if seconds % 60 == 0 => format!("{} minutes", seconds / 60),
        _ => format!("{seconds} seconds"),
    }
}

impl WidgetApp {
    pub(crate) fn new() -> Self {
        Self::from_loaded(storage::load_storage().expect("Failed to initialize Markdown storage"))
    }

    fn from_loaded(loaded: storage::LoadedStorage) -> Self {
        let link_index = LinkIndex::build(&loaded.data.notes, &loaded.paths.notes_dir);
        let tag_index = TagIndex::build(&loaded.data.notes);
        let note_titles_snapshot: HashMap<Uuid, String> = loaded
            .data
            .notes
            .iter()
            .map(|n| (n.id, n.title.clone()))
            .collect();

        for warning in &loaded.warnings {
            eprintln!("Storage warning: {warning}");
        }

        let vault_path_buffer = loaded.settings.vault_path.display().to_string();
        let graph_state = graph::GraphState::restore(&loaded.settings.graph_node_offsets);
        let vault_snapshot = storage::vault_snapshot(&loaded.paths.notes_dir).unwrap_or_default();
        let diagnostics = loaded.warnings.clone();
        let operating_system = platform::OperatingSystem::current();
        if !cfg!(test)
            && std::env::var_os("LILO_DATA_DIR").is_none()
            && operating_system.supports_autostart()
        {
            let _ = platform::set_autostart(loaded.settings.autostart);
        }
        let hotkey_manager = GlobalHotkeyManager::new(
            loaded.settings.global_quick_capture_enabled,
            &loaded.settings.global_quick_capture_shortcut,
        );
        let analytics_dirty = loaded.settings.analytics.enabled();
        let analytics_client = AnalyticsClient::new(analytics::configured_endpoint());
        let now = Instant::now();

        Self {
            data: loaded.data,
            settings: loaded.settings,
            storage_paths: loaded.paths,
            pending_delete_id: None,
            dirty_note_ids: HashSet::new(),
            pending_title_rename_ids: HashSet::new(),
            dirty_since: None,
            pending_index_note_ids: HashSet::new(),
            last_index_change: None,
            storage_message: None,
            link_index,
            tag_index,
            folder_paths: loaded.folder_paths,
            graph_state,
            preview_cache: Default::default(),
            navigation_cache: None,
            viewport_width: 400.0,
            explorer_drawer_open: false,
            applied_theme: None,
            close_pending: false,
            discard_on_close: false,
            view: AppView::Editor,
            settings_section: 0,
            inspector_tab: 0,
            failed_save_ids: HashSet::new(),
            search_query: String::new(),
            focus_search: false,
            focus_editor: false,
            show_new_folder_input: false,
            new_folder_name: String::new(),
            editing_folder: None,
            folder_name_buffer: String::new(),
            graph_overlay_open: false,
            vault_path_buffer,
            vault_snapshot,
            snapshot_worker: Default::default(),
            snapshot_epoch: 0,
            last_external_sync: Instant::now(),
            external_conflict: false,
            window_settings_applied: false,
            recovery_tab: RecoveryTab::Trash,
            selected_backup: None,
            backup_preview: String::new(),
            diagnostics,
            import_path_buffer: String::new(),
            export_path_buffer: String::new(),
            external_changed_paths: Vec::new(),
            new_tag: String::new(),
            new_alias: String::new(),
            note_details_open: false,

            tag_rename_dialog_open: false,
            tag_to_rename: String::new(),
            tag_new_name_buffer: String::new(),
            pending_link_rewrite: None,
            show_new_preset_input: false,
            new_preset_name_buffer: String::new(),

            attachments_orphans: Vec::new(),
            attachments_inspected: false,

            history_back: Vec::new(),
            history_forward: Vec::new(),
            is_navigating_history: false,
            note_titles_snapshot,

            hotkey_manager,
            command_palette_state: CommandPaletteState::default(),
            quick_capture_state: QuickCaptureState::default(),
            template_selector_open: false,
            template_cache: None,
            graph_fullscreen: false,
            template_selector_for_new_note: true,
            pending_folder_delete: None,
            pending_folder_notes_count: 0,
            pending_cursor_char_index: None,
            analytics_client,
            analytics_dirty,
            analytics_report_in_flight: false,
            analytics_deletion_in_flight: false,
            analytics_next_send: now + ANALYTICS_INITIAL_DELAY,
            analytics_next_delete_attempt: now,
            analytics_status: None,
            analytics_details_open: false,
        }
    }
}
impl eframe::App for WidgetApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show_ui(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn persist_egui_memory(&self) -> bool {
        // Layout and navigation state are stored explicitly in AppSettings. Persisting egui's
        // transient areas kept positions and visibility from the pre-redesign floating UI.
        false
    }

    fn on_exit(&mut self) {
        if !self.discard_on_close {
            self.flush_dirty_notes();
        }
        self.save_settings();
    }
}

#[cfg(test)]
mod redesign_tests {
    use super::*;

    #[test]
    fn capture_targets_exact_note_and_failure_preserves_draft_without_duplicate_append() {
        let (_temp, mut app) = fixture();
        let original_selection = app.data.selected_note_id;
        let mut second = Note::new_named(&app.storage_paths.notes_dir.join("Other"), "Проверка");
        second.content = "Original".to_owned();
        storage::save_note(&second).unwrap();
        let id = second.id;
        let path = second.file_path.clone();
        app.data.notes.push(second);
        app.refresh_vault_snapshot();
        app.quick_capture_state.open();
        app.quick_capture_state.text = "Captured once".to_owned();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        app.apply_quick_capture(QuickCaptureSubmission {
            text: "Captured once".to_owned(),
            timestamp: chrono::Local::now(),
            target: QuickCaptureTarget::Inbox,
            existing_note_id: Some(id),
        });
        assert!(app.quick_capture_state.is_open);
        assert!(app.quick_capture_state.error.is_some());
        assert_eq!(
            app.data
                .notes
                .iter()
                .find(|note| note.id == id)
                .unwrap()
                .content,
            "Original"
        );
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        app.apply_quick_capture(QuickCaptureSubmission {
            text: "Captured once".to_owned(),
            timestamp: chrono::Local::now(),
            target: QuickCaptureTarget::Inbox,
            existing_note_id: Some(id),
        });
        assert!(!app.quick_capture_state.is_open);
        assert_eq!(app.data.selected_note_id, original_selection);
        assert_eq!(
            std::fs::read_to_string(path)
                .unwrap()
                .matches("Captured once")
                .count(),
            1
        );
    }

    #[test]
    fn external_write_before_watcher_poll_is_not_overwritten() {
        let (_temp, mut app) = fixture();
        let note = app.data.selected_note_mut().unwrap();
        let path = note.file_path.clone();
        let id = note.id;
        note.content = "Local unsaved content".to_owned();
        app.mark_note_dirty(id);
        std::fs::write(&path, "Externally changed content").unwrap();
        app.flush_dirty_notes();
        assert!(app.external_conflict);
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "Externally changed content"
        );
        assert!(app.dirty_note_ids.contains(&id));
    }

    fn fixture() -> (tempfile::TempDir, WidgetApp) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let mut note = Note::new_named(&root, "Проверка");
        note.content = "# План\n\n- [ ] Проверить редактор\n\n[[Связь]] #design".to_owned();
        note.refresh_search_text();
        storage::save_note(&note).unwrap();
        let selected_note_id = Some(note.id);
        let mut settings = AppSettings {
            vault_path: root.clone(),
            global_quick_capture_enabled: false,
            right_sidebar_open: true,
            selected_note_id,
            ..Default::default()
        };
        settings.analytics.consent = Some(false);
        let app = WidgetApp::from_loaded(storage::LoadedStorage {
            data: AppData {
                notes: vec![note],
                selected_note_id,
            },
            settings,
            paths: StoragePaths {
                settings_path: root.join(".lilo/settings.json"),
                vault_root: root.clone(),
                notes_dir: root.clone(),
                trash_dir: root.join(".lilo/Trash"),
                backups_dir: root.join(".lilo/Backups"),
            },
            warnings: Vec::new(),
            folder_paths: vec![PathBuf::new()],
        });
        (temp, app)
    }

    fn render(app: &mut WidgetApp, ctx: &egui::Context, width: f32) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, if width <= 360.0 { 520.0 } else { 800.0 }),
                )),
                ..Default::default()
            },
            |ui| app.show_ui(ui),
        );
        output.textures_delta.clear();
    }

    #[test]
    fn workspace_widget_dialogs_and_settings_render_without_losing_note_state() {
        let (_temp, mut app) = fixture();
        let ctx = egui::Context::default();
        let original_id = app.data.selected_note_id;
        let original_content = app.data.selected_note().unwrap().content.clone();
        for theme in [ThemeChoice::Dark, ThemeChoice::Light] {
            app.settings.theme = theme;
            for width in [360.0, 760.0, 1440.0] {
                for view in [
                    AppView::Editor,
                    AppView::NotesList,
                    AppView::Graph,
                    AppView::Settings,
                ] {
                    app.view = view;
                    if view == AppView::Settings {
                        for section in 0..9 {
                            app.settings_section = section;
                            render(&mut app, &ctx, width);
                        }
                    } else {
                        render(&mut app, &ctx, width);
                    }
                }
                app.quick_capture_state.open();
                render(&mut app, &ctx, width);
                app.quick_capture_state.close();
                app.command_palette_state.open();
                render(&mut app, &ctx, width);
                app.command_palette_state.close();
            }
        }
        assert_eq!(app.data.selected_note_id, original_id);
        assert_eq!(app.data.selected_note().unwrap().content, original_content);
    }

    #[test]
    fn capture_saves_real_content_and_conflicts_block_writes() {
        let (_temp, mut app) = fixture();
        app.apply_quick_capture(QuickCaptureSubmission {
            text: "Записать мысль".to_owned(),
            timestamp: chrono::Local::now(),
            target: QuickCaptureTarget::Inbox,
            existing_note_id: None,
        });
        let inbox = app
            .data
            .notes
            .iter()
            .find(|note| note.title == "Inbox")
            .unwrap();
        assert!(
            std::fs::read_to_string(&inbox.file_path)
                .unwrap()
                .contains("Записать мысль")
        );
        let id = inbox.id;
        let path = inbox.file_path.clone();
        let before = std::fs::read(&path).unwrap();
        app.external_conflict = true;
        app.data
            .notes
            .iter_mut()
            .find(|note| note.id == id)
            .unwrap()
            .content
            .push_str("Local change");
        assert!(!app.save_note_now(id));
        app.flush_dirty_notes();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(app.dirty_note_ids.contains(&id));
        app.external_conflict = false;
        app.flush_dirty_notes();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("Local change")
        );
        assert!(!app.dirty_note_ids.contains(&id));
    }

    #[test]
    fn daily_folder_alone_does_not_make_a_note_a_daily_note() {
        let (_temp, app) = fixture();
        let note = Note::new_named(&app.storage_paths.notes_dir.join("Daily"), "Ideas");
        assert!(!app.is_daily_note(&note));
        let dated = Note::new_named(&app.storage_paths.notes_dir.join("Daily"), "2026-09-11");
        assert!(app.is_daily_note(&dated));
    }

    #[test]
    fn command_actions_route_views_capture_and_graph_overlay() {
        let (_temp, mut app) = fixture();

        for (action, expected) in [
            (CommandAction::ViewNotesList, AppView::NotesList),
            (CommandAction::ViewGraph, AppView::Graph),
            (CommandAction::ViewTrash, AppView::Trash),
            (CommandAction::ViewSettings, AppView::Settings),
            (CommandAction::ViewEditor, AppView::Editor),
        ] {
            app.handle_command_action(action);
            assert!(app.view == expected);
        }

        app.handle_command_action(CommandAction::QuickCapture);
        assert!(app.quick_capture_state.is_open);
        app.handle_command_action(CommandAction::ToggleGraphOverlay);
        assert!(app.graph_overlay_open);
        assert!(!eframe::App::persist_egui_memory(&app));
    }
}
