mod editor;
mod navigation;
mod settings;
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

#[derive(Default)]
struct NotesListActions {
    selected_note_id: Option<Uuid>,
    requested_delete_id: Option<Uuid>,
    selected_folder: Option<PathBuf>,
    toggled_folder: Option<PathBuf>,
    toggled_pin_id: Option<Uuid>,
    rename_folder: Option<PathBuf>,
    delete_folder: Option<PathBuf>,
}

fn folder_has_visible_notes(
    folder: &folders::FolderNode,
    notes: &HashMap<Uuid, &Note>,
    query: &SearchQuery,
    outgoing_links_by_id: &HashMap<Uuid, Vec<String>>,
) -> bool {
    if query.is_empty() {
        return true;
    }
    folder.note_ids.iter().any(|id| {
        notes.get(id).is_some_and(|note| {
            let links = outgoing_links_by_id
                .get(id)
                .map_or(&[] as &[String], Vec::as_slice);
            query.matches_note(note, &folder.relative_path, links)
        })
    }) || folder
        .folders
        .iter()
        .any(|child| folder_has_visible_notes(child, notes, query, outgoing_links_by_id))
}

fn show_note_row(
    ui: &mut egui::Ui,
    note: &Note,
    selected_note_id: Option<Uuid>,
    actions: &mut NotesListActions,
) {
    let display_title = if note.title.trim().is_empty() {
        note.content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Untitled")
    } else {
        note.title.as_str()
    };
    let updated_text = note.updated_at.format("%d/%m %H:%M").to_string();

    let selected = selected_note_id == Some(note.id);
    let fill = if selected {
        ui.visuals().selection.bg_fill.gamma_multiply(0.65)
    } else {
        egui::Color32::TRANSPARENT
    };
    let row = egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let marker = if note.pinned { "•" } else { "" };
                ui.label(
                    egui::RichText::new(marker)
                        .color(ui.visuals().hyperlink_color)
                        .strong(),
                )
                .on_hover_text("Pinned");
                ui.label(egui::RichText::new(display_title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(updated_text)
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    let row = row.on_hover_text(note.file_path.display().to_string());
    if row.clicked() {
        actions.selected_note_id = Some(note.id);
    }
    row.context_menu(|ui| {
        if ui
            .button(if note.pinned { "Unpin" } else { "Pin" })
            .clicked()
        {
            actions.toggled_pin_id = Some(note.id);
            ui.close();
        }
        if ui.button("Move to Trash").clicked() {
            actions.requested_delete_id = Some(note.id);
            ui.close();
        }
    });
    ui.add_space(1.0);
}

#[allow(clippy::too_many_arguments)]
fn show_folder_node(
    ui: &mut egui::Ui,
    folder: &folders::FolderNode,
    notes: &HashMap<Uuid, &Note>,
    query: &SearchQuery,
    outgoing_links_by_id: &HashMap<Uuid, Vec<String>>,
    selected_note_id: Option<Uuid>,
    selected_folder: &Path,
    collapsed_folders: &[PathBuf],
    note_sort: NoteSort,
    actions: &mut NotesListActions,
) {
    if !folder_has_visible_notes(folder, notes, query, outgoing_links_by_id) {
        return;
    }

    let collapsed = collapsed_folders.contains(&folder.relative_path);
    ui.horizontal(|ui| {
        if ui.small_button(if collapsed { ">" } else { "v" }).clicked() {
            actions.toggled_folder = Some(folder.relative_path.clone());
        }
        let response = ui
            .selectable_label(selected_folder == folder.relative_path, &folder.name)
            .on_hover_text(folder.relative_path.display().to_string());
        if response.clicked() {
            actions.selected_folder = Some(folder.relative_path.clone());
        }
        if !folder.relative_path.as_os_str().is_empty() {
            response.context_menu(|ui| {
                if ui.button("Rename folder").clicked() {
                    actions.rename_folder = Some(folder.relative_path.clone());
                    ui.close();
                }
                if ui.button("Delete folder...").clicked() {
                    actions.delete_folder = Some(folder.relative_path.clone());
                    ui.close();
                }
            });
        }
    });

    if collapsed {
        return;
    }

    ui.indent(("folder", &folder.relative_path), |ui| {
        for child in &folder.folders {
            show_folder_node(
                ui,
                child,
                notes,
                query,
                outgoing_links_by_id,
                selected_note_id,
                selected_folder,
                collapsed_folders,
                note_sort,
                actions,
            );
        }

        let mut note_ids = folder.note_ids.clone();
        note_ids.sort_by(|left, right| {
            let left = notes.get(left).expect("folder note exists");
            let right = notes.get(right).expect("folder note exists");
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| match note_sort {
                    NoteSort::Updated => right.updated_at.cmp(&left.updated_at),
                    NoteSort::Created => right.created_at.cmp(&left.created_at),
                    NoteSort::Title => left.title.to_lowercase().cmp(&right.title.to_lowercase()),
                })
        });
        for note_id in note_ids {
            let Some(note) = notes.get(&note_id) else {
                continue;
            };
            let links = outgoing_links_by_id
                .get(&note.id)
                .map_or(&[] as &[String], Vec::as_slice);
            if !note.pinned && query.matches_note(note, &folder.relative_path, links) {
                show_note_row(ui, note, selected_note_id, actions);
            }
        }
    });
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

    fn save_settings(&mut self) -> bool {
        self.settings.selected_note_id = self.data.selected_note_id;
        if let Err(error) =
            storage::save_settings(&self.storage_paths.settings_path, &self.settings)
        {
            self.storage_message = Some(format!("Failed to save settings: {error}"));
            return false;
        }
        true
    }

    fn record_analytics(&mut self, feature: AnalyticsFeature) {
        if self.settings.analytics.record(feature) {
            self.analytics_dirty = true;
        }
    }

    fn set_analytics_enabled(&mut self, enabled: bool) {
        if enabled {
            self.settings.analytics.enable();
            self.analytics_dirty = true;
            self.analytics_next_send = Instant::now() + ANALYTICS_INITIAL_DELAY;
            self.analytics_status = Some("Analytics enabled".to_owned());
        } else {
            self.settings.analytics.disable_and_queue_deletion();
            self.analytics_dirty = false;
            self.analytics_next_delete_attempt = Instant::now();
            self.analytics_status = Some(
                "Analytics disabled; previously collected data is queued for deletion".to_owned(),
            );
        }
        self.save_settings();
    }

    fn process_analytics(&mut self, ctx: &egui::Context) {
        while let Some(result) = self.analytics_client.try_result() {
            match result {
                AnalyticsResult::Delivered(AnalyticsOperation::DailyReport, _) => {
                    self.analytics_report_in_flight = false;
                    self.analytics_status = Some("Latest daily counters delivered".to_owned());
                }
                AnalyticsResult::Delivered(
                    AnalyticsOperation::DeleteInstallation,
                    Some(installation_id),
                ) => {
                    self.analytics_deletion_in_flight = false;
                    self.settings.analytics.finish_deletion(installation_id);
                    self.analytics_status = Some("Collected analytics data deleted".to_owned());
                    self.save_settings();
                }
                AnalyticsResult::Delivered(AnalyticsOperation::DeleteInstallation, None) => {
                    self.analytics_deletion_in_flight = false;
                }
                AnalyticsResult::Failed(AnalyticsOperation::DailyReport) => {
                    self.analytics_report_in_flight = false;
                    self.analytics_dirty = true;
                    self.analytics_next_send = Instant::now() + ANALYTICS_UPDATE_INTERVAL;
                    self.analytics_status = Some(
                        "Analytics delivery failed; Lilo will retry without interrupting your work"
                            .to_owned(),
                    );
                }
                AnalyticsResult::Failed(AnalyticsOperation::DeleteInstallation) => {
                    self.analytics_deletion_in_flight = false;
                    self.analytics_next_delete_attempt = Instant::now() + ANALYTICS_UPDATE_INTERVAL;
                    self.analytics_status =
                        Some("Deletion is pending and will be retried automatically".to_owned());
                }
            }
        }

        let now = Instant::now();
        if let Some(installation_id) = self.settings.analytics.pending_deletion_id
            && !self.analytics_deletion_in_flight
            && now >= self.analytics_next_delete_attempt
            && self.analytics_client.delete_installation(installation_id)
        {
            self.analytics_deletion_in_flight = true;
        }

        if self.settings.analytics.enabled()
            && self.analytics_dirty
            && !self.analytics_report_in_flight
            && now >= self.analytics_next_send
            && let Some(payload) = self.settings.analytics.daily_payload()
            && self.analytics_client.send_daily(payload)
        {
            self.analytics_dirty = false;
            self.analytics_report_in_flight = true;
            self.analytics_next_send = now + ANALYTICS_UPDATE_INTERVAL;
            self.save_settings();
        }

        if self.analytics_report_in_flight || self.analytics_deletion_in_flight {
            ctx.request_repaint_after(Duration::from_secs(1));
        } else if self.settings.analytics.enabled() && self.analytics_dirty {
            ctx.request_repaint_after(self.analytics_next_send.saturating_duration_since(now));
        } else if self.settings.analytics.pending_deletion_id.is_some() {
            ctx.request_repaint_after(
                self.analytics_next_delete_attempt
                    .saturating_duration_since(now),
            );
        }
    }

    fn show_analytics_data_description(ui: &mut egui::Ui) {
        ui.label("Lilo sends only:");
        ui.label("• a random installation identifier");
        ui.label("• the local calendar date and Lilo version");
        ui.label("• daily counters from the fixed feature whitelist below");
        ui.add_space(6.0);
        ui.small(analytics::FEATURE_NAMES.join(", "));
        ui.add_space(6.0);
        ui.label("Lilo never sends note contents, titles, paths, tags, search queries, window names, device details or account information.");
        ui.label("Cloudflare necessarily handles the network connection, but Lilo does not store the request IP address in its database.");
    }

    fn show_analytics_consent(&mut self, ctx: &egui::Context) {
        if self.settings.analytics.consent.is_some() {
            return;
        }

        let mut choice = None;
        let screen_rect = ui_style::screen_rect(ctx);
        let center = screen_rect.center();
        let modal_width = (screen_rect.width() - 48.0).clamp(232.0, 440.0);
        let details_height = (screen_rect.height() - 220.0).clamp(90.0, 260.0);
        egui::Window::new("Help improve Lilo?")
            .id(egui::Id::new("analytics_consent"))
            .collapsible(false)
            .resizable(false)
            .constrain_to(screen_rect)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(center)
            .show(ctx, |ui| {
                ui.set_width(modal_width);
                ui.label("With your permission, Lilo can send privacy-preserving usage analytics to help prioritise improvements.");
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("analytics_consent_details")
                    .max_height(details_height)
                    .show(ui, Self::show_analytics_data_description);
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Allow analytics").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("No, thanks").clicked() {
                        choice = Some(false);
                    }
                });
                ui.small("Your choice can be changed at any time in Settings.");
            });

        if let Some(enabled) = choice {
            if enabled {
                self.set_analytics_enabled(true);
            } else {
                self.settings.analytics.decline();
                self.analytics_status = Some("Analytics disabled".to_owned());
                self.save_settings();
            }
        }
    }

    fn activate_view(&mut self, view: AppView) {
        if view == AppView::Graph && self.view != AppView::Graph {
            self.record_analytics(AnalyticsFeature::GraphOpened);
        }
        self.view = view;
        self.pending_delete_id = None;
        self.focus_search = view == AppView::NotesList;
        self.focus_editor = view == AppView::Editor;
    }

    fn show_toolbar_menu(&mut self, ui: &mut egui::Ui, include_hidden_views: bool) {
        let zen_mode_before = self.settings.zen_mode;
        let mut requested_view = None;
        ui.menu_button("...", |ui| {
            ui.label(format!(
                "Vault: {}",
                storage::vault_name(&self.settings.vault_path)
            ));
            if ui.button("Switch vault...").clicked() {
                self.choose_vault_folder();
                ui.close();
            }
            ui.separator();
            if let Some(note) = self.data.selected_note() {
                let id = note.id;
                let pinned = note.pinned;
                if ui
                    .button(if pinned { "Unpin note" } else { "Pin note" })
                    .clicked()
                {
                    self.toggle_pin(id);
                    ui.close();
                }
                if ui.button("Save note").clicked() {
                    self.flush_dirty_notes();
                    ui.close();
                }
                if ui.button("Move note to Trash…").clicked() {
                    self.pending_delete_id = Some(id);
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("New note").clicked() {
                self.create_note();
                ui.close();
            }
            if ui.button("Sidebar (Ctrl+Shift+B)").clicked() {
                self.toggle_explorer();
                ui.close();
            }
            if ui.button("Back in note history").clicked() {
                self.navigate_back();
                ui.close();
            }
            if ui.button("Forward in note history").clicked() {
                self.navigate_forward();
                ui.close();
            }
            if ui.button("All Notes").clicked() {
                requested_view = Some(AppView::NotesList);
                ui.close();
            }
            if ui.button("Graph").clicked() {
                requested_view = Some(AppView::Graph);
                ui.close();
            }
            if ui.button("Note context").clicked() {
                self.note_details_open = true;
                ui.close();
            }
            if ui.button("New from template…").clicked() {
                self.template_selector_open = true;
                self.template_selector_for_new_note = true;
                ui.close();
            }
            if ui.button("Compact widget").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(360.0, 520.0)));
                ui.close();
            }
            if ui.button("Minimize").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                ui.close();
            }
            if ui.button("Close Lilo").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                ui.close();
            }
            if include_hidden_views {
                if ui.button("Recovery").clicked() {
                    requested_view = Some(AppView::Trash);
                    ui.close();
                }
                if ui.button("Settings").clicked() {
                    requested_view = Some(AppView::Settings);
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("Search & Commands (Ctrl+K)").clicked() {
                self.command_palette_state.open();
                ui.close();
            }
            if ui.button("Quick Capture (Ctrl+Shift+C)").clicked() {
                self.quick_capture_state.open();
                ui.close();
            }
            if ui.button("Today's Note (Alt+D)").clicked() {
                self.open_or_create_daily_note(0);
                ui.close();
            }
            ui.separator();
            ui.checkbox(&mut self.settings.zen_mode, "Zen / Writing mode (F11)");
            ui.separator();
            if ui.button("Minimize window").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                ui.close();
            }
            if ui.button("Maximize / restore window").clicked() {
                let maximized = ui
                    .ctx()
                    .input(|input| input.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                ui.close();
            }
        });
        if zen_mode_before != self.settings.zen_mode {
            if !zen_mode_before && self.settings.zen_mode {
                self.record_analytics(AnalyticsFeature::ZenModeEnabled);
            }
            self.save_settings();
        }
        if let Some(view) = requested_view {
            self.activate_view(view);
        }
    }

    fn save_note_to_disk(&mut self, id: Uuid) -> bool {
        if !self.verify_disk_versions(&[id]) {
            return false;
        }
        let result = self
            .data
            .notes
            .iter()
            .find(|note| note.id == id)
            .map(|note| {
                if self.settings.backups_enabled {
                    storage::save_note_with_backup(
                        note,
                        &self.storage_paths.backups_dir,
                        self.settings.backup_limit,
                    )
                } else {
                    storage::save_note(note)
                }
            });

        match result {
            Some(Ok(())) => {
                self.failed_save_ids.remove(&id);
                true
            }
            Some(Err(error)) => {
                self.failed_save_ids.insert(id);
                self.mark_note_dirty(id);
                self.storage_message = Some(format!("Failed to save note: {error}"));
                false
            }
            None => false,
        }
    }

    fn save_note_now(&mut self, id: Uuid) -> bool {
        if self.external_conflict {
            self.mark_note_dirty(id);
            self.storage_message =
                Some("! Resolve the external conflict before saving.".to_owned());
            return false;
        }
        let saved = self.save_note_to_disk(id);
        if saved {
            self.record_saved_versions(&[id]);
        }
        saved
    }

    fn refresh_vault_snapshot(&mut self) {
        self.snapshot_epoch = self.snapshot_epoch.wrapping_add(1);
        self.vault_snapshot =
            storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
    }

    fn record_saved_versions(&mut self, ids: &[Uuid]) {
        self.snapshot_epoch = self.snapshot_epoch.wrapping_add(1);
        for note in self.data.notes.iter().filter(|note| ids.contains(&note.id)) {
            self.vault_snapshot
                .retain(|(path, _)| path != &note.file_path);
            if let Ok(meta) = std::fs::metadata(&note.file_path)
                && let Ok(modified) = meta.modified()
                && let Ok(stamp) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                self.vault_snapshot
                    .insert((note.file_path.clone(), stamp.as_nanos()));
            }
        }
    }

    fn mark_note_dirty(&mut self, id: Uuid) {
        self.dirty_note_ids.insert(id);
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    fn flush_dirty_notes(&mut self) {
        if self.external_conflict {
            return;
        }
        let ids = self.dirty_note_ids.clone();
        if ids.is_empty() {
            self.dirty_since = None;
            self.save_settings();
            return;
        }

        if !self.verify_disk_versions(&ids.iter().copied().collect::<Vec<_>>()) {
            return;
        }
        let report = storage::save_notes_with_report(
            &self.data.notes,
            &ids,
            &self.storage_paths.backups_dir,
            self.settings.backups_enabled,
            self.settings.backup_limit,
        );
        for failure in &report.failures {
            self.dirty_note_ids.insert(failure.note_id);
            self.failed_save_ids.insert(failure.note_id);
        }
        let mut completed = 0;
        let mut saved_ids = Vec::new();
        let mut success_details = Vec::new();
        let mut failure_details = report
            .failures
            .iter()
            .map(|failure| {
                format!(
                    "Could not save '{}' ({}): {}",
                    failure.title,
                    failure.path.display(),
                    failure.error
                )
            })
            .collect::<Vec<_>>();

        for id in report.saved_note_ids {
            let rename_result = self
                .pending_title_rename_ids
                .contains(&id)
                .then(|| {
                    self.data
                        .notes
                        .iter_mut()
                        .find(|note| note.id == id)
                        .map(storage::rename_note_file)
                })
                .flatten();
            if let Some(Err(error)) = rename_result {
                failure_details.push(format!(
                    "Saved note contents but could not rename its file: {error}"
                ));
                continue;
            }
            self.pending_title_rename_ids.remove(&id);
            self.dirty_note_ids.remove(&id);
            self.failed_save_ids.remove(&id);
            completed += 1;
            saved_ids.push(id);
            if let Some(note) = self.data.notes.iter().find(|note| note.id == id) {
                success_details.push(format!("Saved successfully: {}", note.file_path.display()));
            }
        }

        if completed > 0 {
            self.record_saved_versions(&saved_ids);
        }

        if !failure_details.is_empty() {
            self.diagnostics.extend(success_details);
            self.diagnostics.extend(failure_details);
            if self.diagnostics.len() > 200 {
                let excess = self.diagnostics.len() - 200;
                self.diagnostics.drain(..excess);
            }
            self.storage_message = Some(format!(
                "Saved {completed} of {} note(s); {} failed. Details are in Recovery > Diagnostics.",
                ids.len(),
                ids.len().saturating_sub(completed)
            ));
        } else if ids.len() > 1 {
            self.storage_message = Some(format!("Saved all {} changed notes", ids.len()));
        }

        self.dirty_since = (!self.dirty_note_ids.is_empty()).then(Instant::now);
        self.save_settings();
    }

    fn schedule_note_index_refresh(&mut self, id: Uuid) {
        self.pending_index_note_ids.insert(id);
        self.last_index_change = Some(Instant::now());
    }

    fn flush_pending_index_refresh(&mut self) {
        let note_ids = std::mem::take(&mut self.pending_index_note_ids);
        for id in note_ids {
            if let Some(note) = self.data.notes.iter().find(|note| note.id == id) {
                self.link_index.refresh_note_content(note);
            }
        }
        self.tag_index = TagIndex::build(&self.data.notes);
        self.last_index_change = None;
    }

    fn process_deferred_index_refresh(&mut self, ctx: &egui::Context) {
        let Some(last_change) = self.last_index_change else {
            return;
        };
        let elapsed = last_change.elapsed();
        if elapsed >= INDEX_REFRESH_DEBOUNCE {
            self.flush_pending_index_refresh();
        } else {
            ctx.request_repaint_after(INDEX_REFRESH_DEBOUNCE - elapsed);
        }
    }

    fn process_autosave(&mut self, ctx: &egui::Context) {
        if self.external_conflict || !self.settings.autosave_enabled {
            return;
        }
        let Some(dirty_since) = self.dirty_since else {
            return;
        };
        let interval = Duration::from_secs(self.settings.autosave_interval_seconds.clamp(
            storage::MIN_AUTOSAVE_INTERVAL_SECONDS,
            storage::MAX_AUTOSAVE_INTERVAL_SECONDS,
        ));
        let elapsed = dirty_since.elapsed();

        if elapsed >= interval {
            self.flush_dirty_notes();
        } else {
            ctx.request_repaint_after(interval - elapsed);
        }
    }

    fn create_note(&mut self) {
        let note_directory = match storage::ensure_note_folder(
            &self.storage_paths.notes_dir,
            &self.settings.selected_folder,
        ) {
            Ok(directory) => directory,
            Err(error) => {
                self.storage_message = Some(format!("Failed to open note folder: {error}"));
                return;
            }
        };
        let id = self.data.create_note(&note_directory);
        self.pending_delete_id = None;
        self.view = AppView::Editor;
        self.focus_search = false;
        self.focus_editor = true;
        self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
        if self.save_note_now(id) {
            self.record_analytics(AnalyticsFeature::NoteCreated);
        }
        self.save_settings();
    }

    // Daily Notes Management
    pub fn current_daily_note_date(&self) -> Option<chrono::NaiveDate> {
        let note = self.data.selected_note()?;
        let rel = note
            .file_path
            .strip_prefix(&self.storage_paths.notes_dir)
            .unwrap_or(&note.file_path);
        LocalDateService::parse_date_from_note(&note.title, rel)
    }

    pub fn open_or_create_daily_note(&mut self, offset_days: i64) {
        let target_date = LocalDateService::today() + chrono::Duration::days(offset_days);
        self.open_or_create_daily_note_for_date(target_date);
    }

    pub fn open_or_create_daily_note_for_date(&mut self, target_date: chrono::NaiveDate) {
        let (subfolder, note_title) = match LocalDateService::format_daily_path(
            &self.settings.daily_note_format,
            target_date,
        ) {
            Ok(res) => res,
            Err(err) => {
                self.storage_message = Some(format!("Daily note format error: {err}"));
                return;
            }
        };

        let target_folder_rel = self.settings.daily_notes_folder.join(&subfolder);
        let target_folder_abs =
            match storage::ensure_note_folder(&self.storage_paths.notes_dir, &target_folder_rel) {
                Ok(dir) => dir,
                Err(err) => {
                    self.storage_message = Some(format!("Failed to ensure daily folder: {err}"));
                    return;
                }
            };

        // Check if note already exists
        let existing_id = self
            .data
            .notes
            .iter()
            .find(|n| {
                if let Ok(rel) = n.file_path.strip_prefix(&self.storage_paths.notes_dir)
                    && let Some(parent) = rel.parent()
                {
                    return parent == target_folder_rel
                        && n.title.eq_ignore_ascii_case(&note_title);
                }
                n.title.eq_ignore_ascii_case(&note_title)
            })
            .map(|n| n.id);

        if let Some(id) = existing_id {
            self.open_note(id);
            self.record_analytics(AnalyticsFeature::DailyNoteOpened);
            return;
        }

        // Create new daily note from template
        let template_text = if !self.settings.default_daily_template.is_empty() {
            TemplateEngine::load_template(
                &self.storage_paths.notes_dir,
                &self.settings.templates_folder,
                &self.settings.default_daily_template,
            )
            .unwrap_or_default()
        } else {
            String::new()
        };

        let now_with_target_date = target_date
            .and_hms_opt(12, 0, 0)
            .and_then(|naive| chrono::Local.from_local_datetime(&naive).single())
            .unwrap_or_else(LocalDateService::now);

        let (expanded_content, cursor_pos) = if !template_text.is_empty() {
            TemplateEngine::expand(&template_text, &note_title, now_with_target_date)
        } else {
            (
                format!(
                    "---\ntags:\n  - daily\n---\n# {}\n\n## 🎯 Focus\n- [ ] \n\n## 📋 Tasks\n- [ ] \n\n## 📝 Notes & Log\n",
                    note_title
                ),
                None,
            )
        };

        let id = self.data.create_note_named(&target_folder_abs, &note_title);
        if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
            note.content = expanded_content;
            note.refresh_search_text();
        }

        if let Some(pos) = cursor_pos {
            self.pending_cursor_char_index = Some((id, pos));
        }

        let saved = self.save_note_now(id);
        self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
        self.open_note(id);
        if saved {
            self.record_analytics(AnalyticsFeature::DailyNoteOpened);
        }
        self.storage_message = Some(format!(
            "Opened daily note for {}",
            target_date.format("%Y-%m-%d")
        ));
    }

    // Markdown Templates Execution
    pub fn create_note_from_template(&mut self, template_name: &str) {
        let Some(template_text) = TemplateEngine::load_template(
            &self.storage_paths.notes_dir,
            &self.settings.templates_folder,
            template_name,
        ) else {
            self.storage_message = Some(format!("Template '{template_name}' not found"));
            return;
        };

        let note_directory = match storage::ensure_note_folder(
            &self.storage_paths.notes_dir,
            &self.settings.selected_folder,
        ) {
            Ok(directory) => directory,
            Err(error) => {
                self.storage_message = Some(format!("Failed to open note folder: {error}"));
                return;
            }
        };

        let (expanded, cursor_pos) =
            TemplateEngine::expand(&template_text, "Untitled", LocalDateService::now());
        let id = self.data.create_note(&note_directory);

        if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
            note.content = expanded;
            note.refresh_search_text();
        }

        if let Some(pos) = cursor_pos {
            self.pending_cursor_char_index = Some((id, pos));
        }

        let saved = self.save_note_now(id);
        self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
        self.open_note(id);
        if saved {
            self.record_analytics(AnalyticsFeature::TemplateNoteCreated);
        }
        self.storage_message = Some(format!("Created note from template '{template_name}'"));
    }

    pub fn insert_template_into_active_note(&mut self, template_name: &str) {
        let Some(template_text) = TemplateEngine::load_template(
            &self.storage_paths.notes_dir,
            &self.settings.templates_folder,
            template_name,
        ) else {
            self.storage_message = Some(format!("Template '{template_name}' not found"));
            return;
        };

        let Some(selected_id) = self.data.selected_note_id else {
            return;
        };

        let title = self
            .data
            .selected_note()
            .map(|n| n.title.clone())
            .unwrap_or_default();
        let (expanded, _) = TemplateEngine::expand(&template_text, &title, LocalDateService::now());

        let mut inserted = false;
        if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == selected_id) {
            if !note.content.is_empty() && !note.content.ends_with('\n') {
                note.content.push('\n');
            }
            note.content.push_str(&expanded);
            note.mark_as_updated();
            self.link_index.refresh_note_content(note);
            self.mark_note_dirty(selected_id);
            self.storage_message = Some(format!("Inserted template '{template_name}'"));
            inserted = true;
        }
        if inserted {
            self.record_analytics(AnalyticsFeature::TemplateInserted);
        }
    }

    // Quick Capture with Buffer Synchronization
    fn capture_candidate(&self, submission: &QuickCaptureSubmission) -> Result<Note, String> {
        if let Some(id) = submission.existing_note_id {
            return self
                .data
                .notes
                .iter()
                .find(|note| note.id == id)
                .cloned()
                .ok_or_else(|| {
                    "The selected note no longer exists. Choose another destination.".to_owned()
                });
        }
        let (folder, title) = match &submission.target {
            QuickCaptureTarget::DailyNote => {
                let (subfolder, title) = LocalDateService::format_daily_path(
                    &self.settings.daily_note_format,
                    LocalDateService::today(),
                )?;
                (self.settings.daily_notes_folder.join(subfolder), title)
            }
            QuickCaptureTarget::Inbox => (PathBuf::new(), "Inbox".to_owned()),
            QuickCaptureTarget::NewNote => (
                self.settings.selected_folder.clone(),
                format!("Thought {}", submission.timestamp.format("%Y-%m-%d %H%M%S")),
            ),
            QuickCaptureTarget::CustomNote(title) => {
                let title = title.trim();
                if title.is_empty() {
                    return Err("Enter a destination name or choose an existing note.".to_owned());
                }
                let matches: Vec<_> = self
                    .data
                    .notes
                    .iter()
                    .filter(|note| note.title.eq_ignore_ascii_case(title))
                    .collect();
                if matches.len() > 1 {
                    return Err(
                        "Several notes have this name. Use Choose note to select its folder."
                            .to_owned(),
                    );
                }
                if let Some(note) = matches.first() {
                    return Ok((*note).clone());
                }
                (self.settings.selected_folder.clone(), title.to_owned())
            }
        };
        let absolute = storage::ensure_note_folder(&self.storage_paths.notes_dir, &folder)
            .map_err(|error| error.to_string())?;
        if !matches!(submission.target, QuickCaptureTarget::NewNote)
            && let Some(note) = self.data.notes.iter().find(|note| {
                note.file_path.parent() == Some(absolute.as_path())
                    && note.title.eq_ignore_ascii_case(&title)
            })
        {
            return Ok(note.clone());
        }
        let mut note = Note::new_named(&absolute, &title);
        if matches!(submission.target, QuickCaptureTarget::DailyNote)
            && !self.settings.default_daily_template.is_empty()
            && let Some(template) = TemplateEngine::load_template(
                &self.storage_paths.notes_dir,
                &self.settings.templates_folder,
                &self.settings.default_daily_template,
            )
        {
            note.content = TemplateEngine::expand(&template, &title, submission.timestamp).0;
        }
        if note.content.is_empty() {
            note.content = format!("# {title}\n\n");
        }
        Ok(note)
    }

    pub fn apply_quick_capture(&mut self, submission: QuickCaptureSubmission) {
        let mut candidate = match self.capture_candidate(&submission) {
            Ok(note) => note,
            Err(error) => {
                self.quick_capture_state.error = Some(error);
                return;
            }
        };
        if self.external_conflict || !self.verify_disk_versions(&[candidate.id]) {
            self.quick_capture_state.error = Some("Resolve the external file conflict before saving this capture. Your draft is kept here.".to_owned());
            return;
        }
        if !candidate.content.is_empty() && !candidate.content.ends_with('\n') {
            candidate.content.push('\n');
        }
        candidate
            .content
            .push_str(&quick_capture::format_capture_entry(
                &submission.text,
                submission.timestamp,
            ));
        candidate.mark_as_updated();
        let saved = if self.settings.backups_enabled {
            storage::save_note_with_backup(
                &candidate,
                &self.storage_paths.backups_dir,
                self.settings.backup_limit,
            )
        } else {
            storage::save_note(&candidate)
        };
        if let Err(error) = saved {
            self.quick_capture_state.error = Some(format!(
                "Save failed: {error}. Your draft has not been discarded; retry when the destination is writable."
            ));
            return;
        }
        let id = candidate.id;
        let title = candidate.title.clone();
        if let Some(note) = self.data.notes.iter_mut().find(|note| note.id == id) {
            *note = candidate;
        } else {
            self.data.notes.push(candidate);
        }
        self.dirty_note_ids.remove(&id);
        self.failed_save_ids.remove(&id);
        self.settings.recent_note_ids.retain(|recent| *recent != id);
        self.settings.recent_note_ids.insert(0, id);
        self.settings.recent_note_ids.truncate(15);
        self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
        self.tag_index = TagIndex::build(&self.data.notes);
        self.record_saved_versions(&[id]);
        self.quick_capture_state.close();
        self.storage_message = Some(format!("Captured to {title}"));
        self.record_analytics(AnalyticsFeature::QuickCaptureSaved);
        self.save_settings();
    }

    /// Check the files being written immediately before a save, including deletions.
    /// The periodic watcher alone cannot protect the interval before its next poll.
    fn verify_disk_versions(&mut self, ids: &[Uuid]) -> bool {
        let mut conflicts = Vec::new();
        for note in self.data.notes.iter().filter(|note| ids.contains(&note.id)) {
            let expected = self
                .vault_snapshot
                .iter()
                .find(|(path, _)| path == &note.file_path)
                .map(|(_, stamp)| *stamp);
            let actual = match std::fs::metadata(&note.file_path) {
                Ok(meta) => meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| time.as_nanos()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    self.storage_message =
                        Some(format!("Cannot check file before saving: {error}"));
                    return false;
                }
            };
            if expected != actual {
                conflicts.push(note.file_path.clone());
            }
        }
        if !conflicts.is_empty() {
            self.external_conflict = true;
            self.external_changed_paths = conflicts;
            self.storage_message = Some(
                "! File changed or was removed outside Lilo. Resolve the conflict before saving."
                    .to_owned(),
            );
            return false;
        }
        true
    }

    fn create_folder_from_input(&mut self) {
        match storage::create_note_folder(
            &self.storage_paths.notes_dir,
            &self.settings.selected_folder,
            &self.new_folder_name,
        ) {
            Ok(relative_path) => {
                if !self.folder_paths.contains(&relative_path) {
                    self.folder_paths.push(relative_path.clone());
                    self.folder_paths.sort();
                }
                self.settings.selected_folder = relative_path;
                self.new_folder_name.clear();
                self.show_new_folder_input = false;
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.record_analytics(AnalyticsFeature::FolderCreated);
                self.save_settings();
            }
            Err(error) => {
                self.storage_message = Some(format!("Failed to create folder: {error}"));
            }
        }
    }

    fn move_selected_note_to_selected_folder(&mut self) {
        let Some(note_id) = self.data.selected_note_id else {
            return;
        };
        if !self.save_note_now(note_id) {
            return;
        }

        let Some(note) = self.data.notes.iter_mut().find(|note| note.id == note_id) else {
            return;
        };
        match storage::move_note_to_folder(
            note,
            &self.storage_paths,
            &self.settings.selected_folder,
        ) {
            Ok(()) => {
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.save_settings();
            }
            Err(error) => {
                self.storage_message = Some(format!("Failed to move note: {error}"));
            }
        }
    }

    fn toggle_pin(&mut self, id: Uuid) {
        let mut pinned = false;
        if let Some(note) = self.data.notes.iter_mut().find(|note| note.id == id) {
            note.pinned = !note.pinned;
            pinned = note.pinned;
            note.mark_as_updated();
            self.mark_note_dirty(id);
        }
        if pinned {
            self.record_analytics(AnalyticsFeature::NotePinned);
        }
    }

    fn rename_selected_folder(&mut self) {
        let Some(source) = self.editing_folder.clone() else {
            return;
        };
        match storage::rename_folder(
            &self.storage_paths.notes_dir,
            &source,
            &self.folder_name_buffer,
        ) {
            Ok(destination) => {
                for note in &mut self.data.notes {
                    if let Ok(relative) = note.file_path.strip_prefix(&self.storage_paths.notes_dir)
                        && relative.starts_with(&source)
                        && let Ok(suffix) = relative.strip_prefix(&source)
                    {
                        note.file_path =
                            self.storage_paths.notes_dir.join(&destination).join(suffix);
                    }
                }
                for folder in &mut self.folder_paths {
                    if folder.starts_with(&source)
                        && let Ok(suffix) = folder.strip_prefix(&source)
                    {
                        *folder = destination.join(suffix);
                    }
                }
                for folder in &mut self.settings.collapsed_folders {
                    if folder.starts_with(&source)
                        && let Ok(suffix) = folder.strip_prefix(&source)
                    {
                        *folder = destination.join(suffix);
                    }
                }
                self.folder_paths.sort();
                self.settings.selected_folder = destination;
                self.editing_folder = None;
                self.folder_name_buffer.clear();
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.save_settings();
            }
            Err(error) => self.storage_message = Some(format!("Failed to rename folder: {error}")),
        }
    }

    fn delete_folder(&mut self, path: &Path) {
        match storage::delete_folder_with_trash(
            &self.storage_paths.notes_dir,
            &self.storage_paths.trash_dir,
            path,
            &self.data.notes,
        ) {
            Ok(report) => {
                for id in &report.trashed_note_ids {
                    let id = *id;
                    self.data.remove_note(id);
                }
                self.folder_paths.retain(|folder| {
                    folder.as_os_str().is_empty()
                        || self.storage_paths.notes_dir.join(folder).is_dir()
                });
                self.settings
                    .collapsed_folders
                    .retain(|folder| self.storage_paths.notes_dir.join(folder).is_dir());
                if !self.settings.selected_folder.as_os_str().is_empty()
                    && !self
                        .storage_paths
                        .notes_dir
                        .join(&self.settings.selected_folder)
                        .is_dir()
                {
                    self.settings.selected_folder = PathBuf::new();
                }
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.save_settings();
                if report.failures.is_empty() && report.retained_files.is_empty() {
                    self.storage_message = Some(format!(
                        "Folder '{}' and {} note(s) moved to Trash",
                        path.display(),
                        report.trashed_note_ids.len()
                    ));
                } else {
                    self.diagnostics
                        .extend(report.failures.iter().map(|failure| {
                            format!(
                                "Could not move '{}' to Trash ({}): {}",
                                failure.title,
                                failure.path.display(),
                                failure.error
                            )
                        }));
                    self.diagnostics
                        .extend(report.retained_files.iter().map(|file| {
                            format!(
                                "Folder retained because it contains a non-note file: {}",
                                file.display()
                            )
                        }));
                    self.storage_message = Some(format!(
                        "Moved {} note(s) to Trash; {} item(s) were retained. See Recovery > Diagnostics.",
                        report.trashed_note_ids.len(),
                        report.failures.len() + report.retained_files.len()
                    ));
                }
            }
            Err(error) => self.storage_message = Some(format!("Failed to delete folder: {error}")),
        }
    }

    fn reload_vault(&mut self, reason: &str) {
        self.template_cache = None;
        self.preview_cache.clear();
        match storage::reload_notes(&self.storage_paths, &self.settings) {
            Ok((notes, warnings, folders)) => {
                let selected = self.data.selected_note_id;
                self.data.notes = notes;
                self.data.selected_note_id =
                    selected.filter(|id| self.data.notes.iter().any(|note| note.id == *id));
                if self.data.selected_note_id.is_none() {
                    self.data.selected_note_id = self.data.notes.first().map(|note| note.id);
                }
                self.folder_paths = folders;
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.tag_index = TagIndex::build(&self.data.notes);
                self.pending_index_note_ids.clear();
                self.last_index_change = None;
                self.note_titles_snapshot = self
                    .data
                    .notes
                    .iter()
                    .map(|note| (note.id, note.title.clone()))
                    .collect();
                self.pending_link_rewrite = None;
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.storage_message = warnings
                    .first()
                    .cloned()
                    .or_else(|| Some(reason.to_owned()));
                self.external_conflict = false;
                self.external_changed_paths.clear();
                self.diagnostics = warnings;
                self.save_settings();
            }
            Err(error) => self.storage_message = Some(format!("Failed to reload vault: {error}")),
        }
    }

    fn sync_external_changes(&mut self, ctx: &egui::Context) {
        if self.last_external_sync.elapsed() < EXTERNAL_SYNC_INTERVAL {
            ctx.request_repaint_after(EXTERNAL_SYNC_INTERVAL - self.last_external_sync.elapsed());
            return;
        }
        let Some(scanned) = self.snapshot_worker.poll() else {
            self.snapshot_worker
                .request(self.storage_paths.notes_dir.clone(), self.snapshot_epoch);
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        };
        if scanned.root != self.storage_paths.notes_dir || scanned.epoch != self.snapshot_epoch {
            ctx.request_repaint();
            return;
        }
        self.last_external_sync = Instant::now();
        let current = match scanned.result {
            Ok(current) => current,
            Err(error) => {
                self.storage_message = Some(format!(
                    "Could not check storage for external changes: {error}"
                ));
                ctx.request_repaint_after(EXTERNAL_SYNC_INTERVAL);
                return;
            }
        };
        if current != self.vault_snapshot {
            let changed_files: HashSet<PathBuf> = current
                .symmetric_difference(&self.vault_snapshot)
                .map(|(path, _)| path.clone())
                .collect();

            // Collect file paths of all notes currently dirty in memory
            let dirty_file_paths: HashSet<PathBuf> = self
                .data
                .notes
                .iter()
                .filter(|note| self.dirty_note_ids.contains(&note.id))
                .map(|note| note.file_path.clone())
                .collect();

            // True conflict only if an external change modified a note that is dirty in Lilo's memory
            let has_dirty_conflict = changed_files.iter().any(|p| dirty_file_paths.contains(p));

            if has_dirty_conflict {
                self.external_conflict = true;
                let mut changed_list: Vec<PathBuf> = changed_files.into_iter().collect();
                changed_list.sort();
                self.external_changed_paths = changed_list;
                self.storage_message = Some(
                    "Files changed outside Lilo conflict with unsaved local edits.".to_owned(),
                );
            } else if self.dirty_note_ids.is_empty() {
                self.reload_vault("Reloaded changes from disk");
            } else {
                // Merge clean disk notes while retaining every dirty in-memory buffer.
                match storage::reload_notes(&self.storage_paths, &self.settings) {
                    Ok((mut notes, warnings, folders)) => {
                        notes.retain(|note| !self.dirty_note_ids.contains(&note.id));
                        notes.extend(
                            self.data
                                .notes
                                .iter()
                                .filter(|note| self.dirty_note_ids.contains(&note.id))
                                .cloned(),
                        );
                        self.data.notes = notes;
                        self.folder_paths = folders;
                        self.diagnostics.extend(warnings);
                        self.link_index =
                            LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                        self.tag_index = TagIndex::build(&self.data.notes);
                        self.preview_cache.clear();
                        ctx.forget_all_images();
                        self.template_cache = None;
                        self.vault_snapshot = current;
                    }
                    Err(error) => {
                        self.storage_message =
                            Some(format!("Could not reload changed files: {error}"))
                    }
                }
            }
        }
        ctx.request_repaint_after(EXTERNAL_SYNC_INTERVAL);
    }

    fn show_trash(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(ui.available_width().min(900.0));
        ui.horizontal(|ui| {
            ui_style::screen_title(
                ui,
                match self.recovery_tab {
                    RecoveryTab::Trash => "Trash",
                    RecoveryTab::Backups => "Backups",
                    RecoveryTab::Diagnostics => "Diagnostics",
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(
                    &mut self.recovery_tab,
                    RecoveryTab::Diagnostics,
                    "Diagnostics",
                );
                ui.selectable_value(&mut self.recovery_tab, RecoveryTab::Backups, "Backups");
                ui.selectable_value(&mut self.recovery_tab, RecoveryTab::Trash, "Trash");
            });
        });
        ui.add_space(4.0);
        match self.recovery_tab {
            RecoveryTab::Trash => self.show_trash_tab(ui),
            RecoveryTab::Backups => self.show_backups_tab(ui),
            RecoveryTab::Diagnostics => self.show_diagnostics_tab(ui),
        }
    }

    fn show_trash_tab(&mut self, ui: &mut egui::Ui) {
        ui_style::muted(
            ui,
            "Trash receives notes removed by you, including notes from deleted folders. Restoring returns a file to its original relative path.",
        );
        ui.small("Trash is for deletion recovery; ordinary edits are recovered from Backups.");
        ui.add_space(8.0);
        match storage::list_trash(&self.storage_paths) {
            Ok(entries) if entries.is_empty() => {
                ui.add_space(20.0);
                ui.label("Trash is empty");
            }
            Ok(entries) => {
                let mut restore = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in entries {
                        ui_style::card_frame(ui).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&entry.display_name).strong());
                                    ui_style::muted(ui, entry.relative_path.display().to_string());
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui_style::compact_action(ui, Icon::Restore, "Restore")
                                            .clicked()
                                        {
                                            restore = Some(entry.relative_path.clone());
                                        }
                                    },
                                );
                            });
                        });
                        ui.add_space(5.0);
                    }
                });
                if let Some(relative) = restore {
                    match storage::restore_from_trash(&self.storage_paths, &relative) {
                        Ok(_) => {
                            self.record_analytics(AnalyticsFeature::TrashRestored);
                            self.reload_vault("Note restored");
                        }
                        Err(error) => {
                            self.storage_message = Some(format!("Restore failed: {error}"))
                        }
                    }
                }
            }
            Err(error) => {
                ui.label(format!("Could not read Trash: {error}"));
            }
        }
    }

    fn show_backups_tab(&mut self, ui: &mut egui::Ui) {
        ui.small("Backups are rotating snapshots created before Lilo overwrites an existing note.");
        ui.small("Restoring a backup replaces that note's contents, while preserving its current version as another backup. It does not use Trash.");
        let mut restore = None;
        match storage::list_backups(&self.storage_paths) {
            Ok(entries) if entries.is_empty() => {
                ui.add_space(20.0);
                ui.label("No backups yet");
            }
            Ok(entries) => {
                let list_height = (ui.available_height() * 0.48).max(90.0);
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .show(ui, |ui| {
                        for entry in entries {
                            let selected =
                                self.selected_backup.as_ref() == Some(&entry.relative_path);
                            let response = ui.selectable_label(
                                selected,
                                format!(
                                    "{}  ·  {}  ·  {} B",
                                    entry.title, entry.created_label, entry.size
                                ),
                            );
                            if response.clicked() {
                                self.selected_backup = Some(entry.relative_path.clone());
                                self.backup_preview = storage::backup_preview(
                                    &self.storage_paths,
                                    &entry.relative_path,
                                )
                                .unwrap_or_else(|error| format!("Preview failed: {error}"));
                            }
                            if selected
                                && ui_style::compact_action(
                                    ui,
                                    Icon::Restore,
                                    "Restore this version",
                                )
                                .clicked()
                            {
                                restore = Some((entry.note_id, entry.relative_path));
                            }
                        }
                    });
                if !self.backup_preview.is_empty() {
                    ui.separator();
                    ui.small("Backup preview");
                    egui::ScrollArea::vertical()
                        .max_height((ui.available_height() - 32.0).max(80.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.backup_preview)
                                    .interactive(false)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                }
            }
            Err(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Backup error: {error}"),
                );
            }
        }

        if let Some((note_id, relative)) = restore {
            let result = self
                .data
                .notes
                .iter_mut()
                .find(|note| note.id == note_id)
                .ok_or_else(|| "The original note is not present in this vault".to_owned())
                .and_then(|note| {
                    storage::restore_backup(
                        note,
                        &self.storage_paths,
                        &relative,
                        self.settings.backup_limit,
                    )
                    .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => {
                    self.data.selected_note_id = Some(note_id);
                    self.link_index =
                        LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                    self.vault_snapshot =
                        storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                    self.storage_message =
                        Some("Backup restored; the previous version was preserved".to_owned());
                    self.record_analytics(AnalyticsFeature::BackupRestored);
                }
                Err(error) => self.storage_message = Some(format!("Restore failed: {error}")),
            }
        }
    }

    fn show_diagnostics_tab(&mut self, ui: &mut egui::Ui) {
        let operating_system = platform::OperatingSystem::current();
        ui.small(format!("Platform: {}", operating_system.name()));
        ui.small(if operating_system.supports_autostart() {
            "Autostart integration: available"
        } else {
            "Autostart integration: unavailable"
        });
        ui.add_space(6.0);
        ui.small("Diagnostics only reads the vault. It reports malformed notes, missing or malformed attachment links, and unavailable managed folders.");
        if ui.button("Scan vault now").clicked() {
            self.diagnostics = storage::vault_diagnostics(&self.storage_paths, &self.settings)
                .unwrap_or_else(|error| vec![format!("Diagnostics failed: {error}")]);
        }
        if self.diagnostics.is_empty() {
            ui.add_space(20.0);
            ui.colored_label(ui.visuals().hyperlink_color, "No vault problems detected");
        } else {
            ui.small("Files are never rewritten merely by running diagnostics.");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for diagnostic in &self.diagnostics {
                    ui.colored_label(ui.visuals().warn_fg_color, diagnostic);
                    ui.add_space(4.0);
                }
            });
        }
    }

    fn switch_vault_from_buffer(&mut self) {
        self.flush_dirty_notes();
        if !self.dirty_note_ids.is_empty() {
            self.storage_message = Some(
                "Vault switch cancelled because one or more edited notes could not be saved"
                    .to_owned(),
            );
            return;
        }
        if self.quick_capture_state.is_open && !self.quick_capture_state.text.trim().is_empty() {
            self.storage_message =
                Some("Save or dismiss Quick Capture before switching vaults".to_owned());
            return;
        }
        let previous_settings = self.settings.clone();
        let mut next_settings = previous_settings.clone();
        next_settings.selected_note_id = self.data.selected_note_id;
        if let Err(error) = storage::set_vault_path(&mut next_settings, &self.vault_path_buffer) {
            self.storage_message = Some(format!("Invalid vault path: {error}"));
            return;
        }
        if next_settings.vault_path == previous_settings.vault_path {
            self.vault_path_buffer = previous_settings.vault_path.display().to_string();
            return;
        }
        if let Err(error) =
            storage::save_settings(&self.storage_paths.settings_path, &next_settings)
        {
            self.storage_message = Some(format!("Failed to save vault path: {error}"));
            return;
        }
        match storage::load_storage() {
            Ok(loaded) => {
                self.data = loaded.data;
                self.settings = loaded.settings;
                self.storage_paths = loaded.paths;
                self.folder_paths = loaded.folder_paths;
                self.diagnostics = loaded.warnings;
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.tag_index = TagIndex::build(&self.data.notes);
                self.graph_state = graph::GraphState::restore(&self.settings.graph_node_offsets);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.snapshot_epoch = self.snapshot_epoch.wrapping_add(1);
                self.vault_path_buffer = self.settings.vault_path.display().to_string();
                self.dirty_note_ids.clear();
                self.failed_save_ids.clear();
                self.pending_title_rename_ids.clear();
                self.pending_index_note_ids.clear();
                self.last_index_change = None;
                self.pending_link_rewrite = None;
                self.pending_delete_id = None;
                self.pending_folder_delete = None;
                self.preview_cache = Default::default();
                self.navigation_cache = None;
                self.template_cache = None;
                self.search_query.clear();
                self.graph_overlay_open = false;
                self.attachments_inspected = false;
                self.attachments_orphans.clear();
                self.note_titles_snapshot = self
                    .data
                    .notes
                    .iter()
                    .map(|note| (note.id, note.title.clone()))
                    .collect();
                self.history_back.clear();
                self.history_forward.clear();
                self.external_conflict = false;
                self.external_changed_paths.clear();
                self.last_external_sync = Instant::now();
                self.storage_message = Some("Vault switched successfully".to_owned());
                self.view = AppView::NotesList;
            }
            Err(error) => {
                let _ =
                    storage::save_settings(&self.storage_paths.settings_path, &previous_settings);
                self.storage_message = Some(format!("Could not switch vault: {error}"));
            }
        }
    }

    fn show_vault_switcher(&mut self, ui: &mut egui::Ui) {
        let active_path = self.settings.vault_path.clone();
        let vaults = self.settings.vaults.clone();
        let mut chosen = None;
        let mut choose_folder = false;
        let name = storage::vault_name(&active_path);
        let max_chars = if self.viewport_width < ui_style::COMPACT_WIDTH {
            14
        } else {
            22
        };
        let shortened: String = name.chars().take(max_chars).collect();
        let label = if name.chars().count() > max_chars {
            format!("⌄  {shortened}…")
        } else {
            format!("⌄  {name}")
        };
        ui.menu_button(label, |ui| {
            ui.set_min_width(220.0);
            ui.label(egui::RichText::new("VAULTS").small().weak());
            for vault in &vaults {
                let selected = vault.path == active_path;
                if ui
                    .selectable_label(selected, vault.name())
                    .on_hover_text(vault.path.display().to_string())
                    .clicked()
                {
                    chosen = Some(vault.path.clone());
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Open another folder...").clicked() {
                choose_folder = true;
                ui.close();
            }
        });
        if let Some(path) = chosen {
            self.vault_path_buffer = path.display().to_string();
            self.switch_vault_from_buffer();
        } else if choose_folder {
            self.choose_vault_folder();
        }
    }

    fn choose_vault_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose Lilo vault folder");
        if self.settings.vault_path.is_dir() {
            dialog = dialog.set_directory(&self.settings.vault_path);
        }
        if let Some(path) = dialog.pick_folder() {
            self.vault_path_buffer = path.display().to_string();
            self.switch_vault_from_buffer();
        }
    }

    fn import_markdown_from_buffer(&mut self) {
        let source = PathBuf::from(self.import_path_buffer.trim());
        match storage::import_markdown(&source, &self.storage_paths, &self.settings.selected_folder)
        {
            Ok(note) => {
                let id = note.id;
                self.data.notes.push(note);
                self.data.selected_note_id = Some(id);
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.import_path_buffer.clear();
                self.storage_message = Some("Markdown note imported".to_owned());
                self.view = AppView::Editor;
                self.record_analytics(AnalyticsFeature::MarkdownImported);
                self.save_settings();
            }
            Err(error) => self.storage_message = Some(format!("Import failed: {error}")),
        }
    }

    fn export_vault_from_buffer(&mut self) {
        self.flush_dirty_notes();
        let destination = PathBuf::from(self.export_path_buffer.trim());
        match storage::export_vault(&self.storage_paths, &destination) {
            Ok(path) => {
                self.storage_message = Some(format!("Vault exported to {}", path.display()));
                self.record_analytics(AnalyticsFeature::VaultExported);
            }
            Err(error) => self.storage_message = Some(format!("Export failed: {error}")),
        }
    }

    fn apply_notes_list_actions(&mut self, actions: NotesListActions) {
        if let Some(id) = actions.selected_note_id {
            self.open_note(id);
        }
        if let Some(path) = actions.toggled_folder {
            if let Some(index) = self
                .settings
                .collapsed_folders
                .iter()
                .position(|collapsed| collapsed == &path)
            {
                self.settings.collapsed_folders.remove(index);
            } else {
                self.settings.collapsed_folders.push(path);
            }
            self.save_settings();
        }

        if let Some(path) = actions.selected_folder {
            self.settings.selected_folder = path;
            self.pending_delete_id = None;
            self.save_settings();
        }
        if let Some(id) = actions.requested_delete_id {
            self.pending_delete_id = Some(id);
        }
        if let Some(id) = actions.toggled_pin_id {
            self.toggle_pin(id);
        }
        if let Some(path) = actions.rename_folder {
            self.folder_name_buffer = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            self.editing_folder = Some(path);
        }
        if let Some(path) = actions.delete_folder {
            let count = self
                .data
                .notes
                .iter()
                .filter(|n| {
                    n.file_path
                        .strip_prefix(&self.storage_paths.notes_dir)
                        .ok()
                        .is_some_and(|r| r.starts_with(&path))
                })
                .count();
            if count == 0 {
                self.delete_folder(&path);
            } else {
                self.pending_folder_delete = Some(path);
                self.pending_folder_notes_count = count;
            }
        }
    }

    fn show_notes_list(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(ui.available_width().min(1100.0));
        let mut create_note_clicked = false;
        let mut submit_new_folder = false;
        let mut move_current_note = false;

        ui.horizontal(|ui| {
            ui_style::screen_title(ui, &storage::vault_name(&self.settings.vault_path));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui_style::compact_action(ui, Icon::Folder, "New folder").clicked() {
                    self.show_new_folder_input = !self.show_new_folder_input;
                    self.new_folder_name.clear();
                }
                if ui_style::compact_action(ui, Icon::Add, "New note").clicked() {
                    create_note_clicked = true;
                }
            });
        });

        let vault_name = storage::vault_name(&self.settings.vault_path);
        let selected_folder_text = if self.settings.selected_folder.as_os_str().is_empty() {
            format!("{vault_name} (root)")
        } else {
            format!("{vault_name} / {}", self.settings.selected_folder.display())
        };
        ui_style::muted(ui, selected_folder_text);

        let current_note_is_elsewhere = self.data.selected_note().is_some_and(|note| {
            note.file_path
                .parent()
                .and_then(|parent| parent.strip_prefix(&self.storage_paths.notes_dir).ok())
                != Some(self.settings.selected_folder.as_path())
        });
        if current_note_is_elsewhere && ui.small_button("Move current note here").clicked() {
            move_current_note = true;
        }

        if self.show_new_folder_input {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 58.0).max(40.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.new_folder_name)
                        .desired_width(input_width)
                        .hint_text("Folder name..."),
                );
                let enter_pressed =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui.small_button("Create").clicked() || enter_pressed {
                    submit_new_folder = true;
                }
            });
        }

        ui.add_space(4.0);
        let search_response = ui.add(
            egui::TextEdit::singleline(&mut self.search_query)
                .desired_width(f32::INFINITY)
                .hint_text("Search notes or tag:rust path:\"Daily Notes\" link:Target..."),
        );
        if self.focus_search {
            search_response.request_focus();
            self.focus_search = false;
        }
        if search_response.lost_focus() && !self.search_query.trim().is_empty() {
            self.record_analytics(AnalyticsFeature::SearchUsed);
        }

        ui.horizontal(|ui| {
            let previous_sort = self.settings.note_sort;
            egui::ComboBox::from_id_salt("note_sort")
                .width((ui.available_width() - 2.0).max(100.0))
                .selected_text(match self.settings.note_sort {
                    NoteSort::Updated => "Recently updated",
                    NoteSort::Created => "Recently created",
                    NoteSort::Title => "Title",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.settings.note_sort,
                        NoteSort::Updated,
                        "Recently updated",
                    );
                    ui.selectable_value(
                        &mut self.settings.note_sort,
                        NoteSort::Created,
                        "Recently created",
                    );
                    ui.selectable_value(&mut self.settings.note_sort, NoteSort::Title, "Title");
                });
            if self.settings.note_sort != previous_sort {
                self.save_settings();
            }
        });

        ui.add_space(6.0);

        let parsed_query = SearchQuery::parse(&self.search_query);
        let navigation = self.navigation_index();
        let outgoing_links_by_id = &navigation.outgoing;

        let actions = {
            let tree = &navigation.tree;
            let notes: HashMap<Uuid, &Note> =
                self.data.notes.iter().map(|note| (note.id, note)).collect();
            let mut actions = NotesListActions::default();

            let list_height = (ui.available_height() - 64.0).max(80.0);
            egui::ScrollArea::vertical()
                .max_height(list_height)
                .show(ui, |ui| {
                    let mut pinned = notes
                        .values()
                        .filter(|note| note.pinned)
                        .copied()
                        .collect::<Vec<_>>();
                    pinned.sort_by_key(|note| std::cmp::Reverse(note.updated_at));
                    if !pinned.is_empty() {
                        ui.strong("Pinned");
                        for note in pinned {
                            let links = outgoing_links_by_id
                                .get(&note.id)
                                .map_or(&[] as &[String], Vec::as_slice);
                            let folder_rel = note
                                .file_path
                                .parent()
                                .and_then(|p| p.strip_prefix(&self.storage_paths.notes_dir).ok())
                                .unwrap_or(Path::new(""));
                            if parsed_query.matches_note(note, folder_rel, links) {
                                show_note_row(ui, note, self.data.selected_note_id, &mut actions);
                            }
                        }
                        ui.separator();
                        ui.strong("Folders and recent notes");
                    }
                    if !folder_has_visible_notes(
                        &tree.root,
                        &notes,
                        &parsed_query,
                        outgoing_links_by_id,
                    ) {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label("No notes found");
                        });
                    } else {
                        show_folder_node(
                            ui,
                            &tree.root,
                            &notes,
                            &parsed_query,
                            outgoing_links_by_id,
                            self.data.selected_note_id,
                            &self.settings.selected_folder,
                            &self.settings.collapsed_folders,
                            self.settings.note_sort,
                            &mut actions,
                        );
                    }
                });
            actions
        };

        self.apply_notes_list_actions(actions);

        if self.editing_folder.is_some() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("New folder name:");
                ui.text_edit_singleline(&mut self.folder_name_buffer);
                if ui.button("Rename").clicked() {
                    self.rename_selected_folder();
                }
                if ui.button("Cancel").clicked() {
                    self.editing_folder = None;
                }
            });
        }

        if submit_new_folder {
            self.create_folder_from_input();
        }
        if move_current_note {
            self.move_selected_note_to_selected_folder();
        }
        if create_note_clicked {
            self.create_note();
        }
    }

    fn open_note(&mut self, id: Uuid) {
        if !self.data.notes.iter().any(|note| note.id == id) {
            return;
        }

        if !self.is_navigating_history
            && let Some(cur_id) = self.data.selected_note_id
            && cur_id != id
        {
            self.history_back.push(cur_id);
            self.history_forward.clear();
            if self.history_back.len() > 50 {
                self.history_back.remove(0);
            }
        }
        self.data.selected_note_id = Some(id);
        self.pending_delete_id = None;
        self.view = AppView::Editor;
        self.focus_search = false;
        self.focus_editor = true;

        self.settings
            .recent_note_ids
            .retain(|&recent_id| recent_id != id);
        self.settings.recent_note_ids.insert(0, id);
        self.settings.recent_note_ids.truncate(15);

        self.save_settings();
    }

    fn navigate_back(&mut self) {
        while let Some(prev_id) = self.history_back.pop() {
            if self.data.notes.iter().any(|note| note.id == prev_id) {
                if let Some(cur_id) = self.data.selected_note_id {
                    self.history_forward.push(cur_id);
                }
                self.is_navigating_history = true;
                self.open_note(prev_id);
                self.is_navigating_history = false;
                self.activate_view(AppView::Editor);
                break;
            }
        }
    }

    fn navigate_forward(&mut self) {
        while let Some(next_id) = self.history_forward.pop() {
            if self.data.notes.iter().any(|note| note.id == next_id) {
                if let Some(cur_id) = self.data.selected_note_id {
                    self.history_back.push(cur_id);
                }
                self.is_navigating_history = true;
                self.open_note(next_id);
                self.is_navigating_history = false;
                self.activate_view(AppView::Editor);
                break;
            }
        }
    }

    fn navigate_note_list(&mut self, direction: isize) {
        let query = SearchQuery::parse(&self.search_query);
        let mut notes = self
            .data
            .notes
            .iter()
            .filter(|note| {
                let folder = note
                    .file_path
                    .parent()
                    .and_then(|p| p.strip_prefix(&self.storage_paths.notes_dir).ok())
                    .unwrap_or(Path::new(""));
                query.matches_note(note, folder, &[])
            })
            .collect::<Vec<_>>();
        notes.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| match self.settings.note_sort {
                    NoteSort::Updated => right.updated_at.cmp(&left.updated_at),
                    NoteSort::Created => right.created_at.cmp(&left.created_at),
                    NoteSort::Title => left.title.to_lowercase().cmp(&right.title.to_lowercase()),
                })
        });
        if notes.is_empty() {
            return;
        }
        let current = self
            .data
            .selected_note_id
            .and_then(|id| notes.iter().position(|note| note.id == id))
            .unwrap_or(0);
        let next = (current as isize + direction).clamp(0, notes.len() as isize - 1) as usize;
        self.data.selected_note_id = Some(notes[next].id);
        self.save_settings();
    }

    fn create_note_from_link(&mut self, title: &str) {
        let Some((explicit_folder, note_title)) = links::split_target_path(title) else {
            self.storage_message = Some(format!("Cannot create note from unsafe link [[{title}]]"));
            return;
        };

        let current_folder = self
            .data
            .selected_note()
            .and_then(|note| note.file_path.parent())
            .and_then(|parent| parent.strip_prefix(&self.storage_paths.notes_dir).ok())
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let target_folder = if title.contains(['/', '\\']) {
            explicit_folder
        } else {
            current_folder
        };
        let note_directory =
            match storage::ensure_note_folder(&self.storage_paths.notes_dir, &target_folder) {
                Ok(directory) => directory,
                Err(error) => {
                    self.storage_message = Some(format!("Failed to create linked note: {error}"));
                    return;
                }
            };

        // Update the tree without rescanning the vault.
        let mut ancestor = PathBuf::new();
        for component in target_folder.components() {
            ancestor.push(component.as_os_str());
            if !self.folder_paths.contains(&ancestor) {
                self.folder_paths.push(ancestor.clone());
            }
        }
        self.folder_paths.sort();
        self.settings.selected_folder = target_folder;

        let id = self.data.create_note_named(&note_directory, &note_title);
        self.pending_delete_id = None;
        self.view = AppView::Editor;
        self.focus_search = false;
        self.focus_editor = true;
        self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
        self.save_note_now(id);
        self.save_settings();
    }

    fn delete_note(&mut self, id: Uuid) {
        let move_result = self
            .data
            .notes
            .iter()
            .find(|note| note.id == id)
            .map(|note| storage::move_note_to_trash(note, &self.storage_paths));

        match move_result {
            Some(Ok(())) => {
                self.data.remove_note(id);
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.dirty_note_ids.remove(&id);
                self.failed_save_ids.remove(&id);
                self.settings
                    .recent_note_ids
                    .retain(|&recent_id| recent_id != id);
                self.pending_delete_id = None;
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.save_settings();
            }
            Some(Err(error)) => {
                self.storage_message = Some(format!("Failed to move note to Trash: {error}"));
            }
            None => {
                self.pending_delete_id = None;
            }
        }
    }

    fn handle_graph_output(&mut self, output: graph::GraphOutput) -> bool {
        if output.persist_layout {
            self.settings.graph_node_offsets = self.graph_state.persisted_offsets();
            self.save_settings();
        }
        if output.expand {
            self.graph_overlay_open = false;
            self.graph_fullscreen = !self.graph_fullscreen;
            self.activate_view(AppView::Graph);
        }
        let _graph_interacted = output.state_changed;
        if let Some(id) = output.opened_note_id {
            self.open_note(id);
            return true;
        }
        if let Some(target) = output.create_missing_target {
            self.create_note_from_link(&target);
            return true;
        }
        false
    }

    fn show_note_connections(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let Some(links) = self.link_index.links_for(note_id).cloned() else {
            return;
        };
        let mut open_note = None;
        let mut create_note = None;
        for (heading, ids, empty) in [
            (
                "Outgoing links",
                &links.outgoing,
                "No outgoing links. Use [[note title]] to connect a note.",
            ),
            (
                "Backlinks",
                &links.backlinks,
                "No backlinks yet. Links from other notes will appear here.",
            ),
        ] {
            ui.label(egui::RichText::new(format!("{heading} ({})", ids.len())).strong());
            if ids.is_empty() {
                ui_style::muted(ui, empty);
            }
            for id in ids {
                let title = self
                    .data
                    .notes
                    .iter()
                    .find(|note| note.id == *id)
                    .map_or("Untitled", |note| note.title.as_str());
                if ui
                    .add(egui::Button::new(title).frame(false).wrap())
                    .clicked()
                {
                    open_note = Some(*id);
                }
            }
            ui.add_space(12.0);
        }
        if !links.unresolved.is_empty() {
            ui.label(egui::RichText::new("Missing notes").strong());
            for target in &links.unresolved {
                if ui
                    .add(egui::Button::new(format!("+ {target}")).frame(false).wrap())
                    .on_hover_text("Create the linked note")
                    .clicked()
                {
                    create_note = Some(target.clone());
                }
            }
        }
        if let Some(id) = open_note {
            self.open_note(id);
        } else if let Some(target) = create_note {
            self.create_note_from_link(&target);
        }
    }

    fn show_note_properties(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let mut changed = false;
        egui::Frame::NONE.show(ui, |ui| {
            let Some(note) = self.data.notes.iter_mut().find(|note| note.id == note_id) else {
                return;
            };
            ui.horizontal_wrapped(|ui| {
                ui.small("Tags");
                let mut remove = None;
                for (index, tag) in note.tags.iter().enumerate() {
                    if ui.button(format!("#{tag} ×")).clicked() {
                        remove = Some(index);
                    }
                }
                if let Some(index) = remove {
                    note.tags.remove(index);
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                let tag_input = ui.add(
                    egui::TextEdit::singleline(&mut self.new_tag)
                        .hint_text("new tag")
                        .desired_width(120.0),
                );
                let submit_tag = ui.button("Add tag").clicked()
                    || (tag_input.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if submit_tag {
                    let tag = tags::clean_tag(&self.new_tag);
                    if !tag.is_empty()
                        && !note
                            .tags
                            .iter()
                            .any(|existing| tags::clean_tag(existing) == tag)
                    {
                        note.tags.push(tag);
                        self.new_tag.clear();
                        changed = true;
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.small("Aliases");
                let mut remove = None;
                for (index, alias) in note.aliases.iter().enumerate() {
                    if ui.button(format!("{alias} ×")).clicked() {
                        remove = Some(index);
                    }
                }
                if let Some(index) = remove {
                    note.aliases.remove(index);
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_alias)
                        .hint_text("new alias")
                        .desired_width(120.0),
                );
                if ui.button("Add alias").clicked() {
                    let alias = self.new_alias.trim();
                    if !alias.is_empty() && !note.aliases.iter().any(|existing| existing == alias) {
                        note.aliases.push(alias.to_owned());
                        self.new_alias.clear();
                        changed = true;
                    }
                }
            });
            if changed {
                note.mark_as_updated();
            }
        });
        if changed {
            self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
            self.tag_index = TagIndex::build(&self.data.notes);
            self.mark_note_dirty(note_id);
        }
    }

    fn is_daily_note(&self, note: &Note) -> bool {
        let rel = note
            .file_path
            .strip_prefix(&self.storage_paths.notes_dir)
            .unwrap_or(&note.file_path);
        LocalDateService::parse_date_from_note(&note.title, rel).is_some()
    }

    fn render_tag_node(
        ui: &mut egui::Ui,
        node: &crate::tags::TagTreeNode,
        current_query: &str,
        filter_tag: &mut Option<String>,
        rename_tag_target: &mut Option<String>,
    ) {
        let tag_query = format!("tag:{}", node.full_tag);
        let is_active = current_query.contains(&tag_query);
        let label_text = format!("#{} ({})", node.name, node.count);

        if node.children.is_empty() {
            let resp = ui
                .selectable_label(is_active, label_text)
                .on_hover_text(format!(
                    "Filter notes by #{}\nRight-click to rename",
                    node.full_tag
                ));
            if resp.clicked() {
                *filter_tag = Some(node.full_tag.clone());
            }
            resp.context_menu(|ui| {
                if ui.button("Filter Notes").clicked() {
                    *filter_tag = Some(node.full_tag.clone());
                    ui.close();
                }
                if ui.button("Rename Tag across Vault...").clicked() {
                    *rename_tag_target = Some(node.full_tag.clone());
                    ui.close();
                }
            });
        } else {
            let id = ui.make_persistent_id(format!("tag_tree_{}", node.full_tag));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .show_header(ui, |ui| {
                    let resp =
                        ui.selectable_label(is_active, format!("#{} ({})", node.name, node.count));
                    if resp.clicked() {
                        *filter_tag = Some(node.full_tag.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Filter Notes").clicked() {
                            *filter_tag = Some(node.full_tag.clone());
                            ui.close();
                        }
                        if ui.button("Rename Tag across Vault...").clicked() {
                            *rename_tag_target = Some(node.full_tag.clone());
                            ui.close();
                        }
                    });
                })
                .body(|ui| {
                    for child in &node.children {
                        Self::render_tag_node(
                            ui,
                            child,
                            current_query,
                            filter_tag,
                            rename_tag_target,
                        );
                    }
                });
        }
    }

    fn show_left_explorer(&mut self, ui: &mut egui::Ui) {
        let mut create_note_clicked = false;
        let mut submit_new_folder = false;

        ui.horizontal(|ui| {
            self.show_vault_switcher(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui_style::compact_action(ui, Icon::Folder, "New folder").clicked() {
                    self.show_new_folder_input = !self.show_new_folder_input;
                    self.new_folder_name.clear();
                }
                if ui_style::compact_action(ui, Icon::Add, "New note").clicked() {
                    create_note_clicked = true;
                }
            });
        });

        ui.add_space(4.0);

        if ui_style::navigation_button(ui, Icon::Search, false, "Search    Ctrl+K", true).clicked()
        {
            self.command_palette_state.open();
        }
        if ui_style::navigation_button(ui, Icon::Calendar, false, "Today    Alt+D", true).clicked()
        {
            self.open_or_create_daily_note(0);
            self.activate_view(AppView::Editor);
        }
        if ui_style::navigation_button(ui, Icon::Inbox, false, "Quick Capture", true)
            .on_hover_text("Quick Capture (Ctrl+Shift+C)")
            .clicked()
        {
            self.quick_capture_state.open();
        }
        for (view, icon, title) in [
            (AppView::NotesList, Icon::Notes, "All Notes"),
            (AppView::Graph, Icon::Graph, "Graph"),
        ] {
            if ui_style::navigation_button(ui, icon, self.view == view, title, true).clicked() {
                self.activate_view(view);
            }
        }
        ui.separator();
        // Pinned notes section
        let pinned_notes: Vec<(Uuid, String)> = self
            .data
            .notes
            .iter()
            .filter(|n| n.pinned)
            .map(|n| (n.id, n.title.clone()))
            .collect();
        let mut open_pinned_id = None;
        if !pinned_notes.is_empty() {
            ui.add_space(4.0);
            ui.collapsing(
                egui::RichText::new(format!("Pinned ({})", pinned_notes.len())).strong(),
                |ui| {
                    for (pinned_id, pinned_title) in pinned_notes {
                        let selected = self.data.selected_note_id == Some(pinned_id);
                        if ui
                            .selectable_label(selected, format!("• {pinned_title}"))
                            .clicked()
                        {
                            open_pinned_id = Some(pinned_id);
                        }
                    }
                },
            );
        }
        if let Some(id) = open_pinned_id {
            self.open_note(id);
            self.activate_view(AppView::Editor);
        }

        // Recent notes section
        let recent_notes_list: Vec<(Uuid, String, String)> = self
            .settings
            .recent_note_ids
            .iter()
            .filter_map(|&id| {
                self.data.notes.iter().find(|n| n.id == id).map(|n| {
                    let title = if n.title.trim().is_empty() {
                        "Untitled".to_owned()
                    } else {
                        n.title.clone()
                    };
                    let updated = n.updated_at.format("%d/%m %H:%M").to_string();
                    (n.id, title, updated)
                })
            })
            .take(6)
            .collect();

        let mut open_recent_id = None;
        if !recent_notes_list.is_empty() {
            ui.add_space(2.0);
            ui.collapsing(
                egui::RichText::new(format!("Recent ({})", recent_notes_list.len())).strong(),
                |ui| {
                    for (recent_id, recent_title, updated) in recent_notes_list {
                        let selected = self.data.selected_note_id == Some(recent_id);
                        if ui
                            .selectable_label(selected, format!("• {recent_title}"))
                            .on_hover_text(format!("Updated {updated}"))
                            .clicked()
                        {
                            open_recent_id = Some(recent_id);
                        }
                    }
                },
            );
        }
        if let Some(id) = open_recent_id {
            self.open_note(id);
            self.activate_view(AppView::Editor);
        }

        // Tags section
        let tag_tree = self.tag_index.build_tree();
        if !tag_tree.is_empty() {
            ui.add_space(2.0);
            let mut filter_tag = None;
            let mut rename_tag_target = None;
            ui.collapsing(
                egui::RichText::new(format!("Tags ({})", self.tag_index.all_tags().len())).strong(),
                |ui| {
                    for node in &tag_tree {
                        Self::render_tag_node(
                            ui,
                            node,
                            &self.search_query,
                            &mut filter_tag,
                            &mut rename_tag_target,
                        );
                    }
                },
            );
            if let Some(tag) = filter_tag {
                self.search_query = format!("tag:{tag}");
                self.focus_search = false;
                self.record_analytics(AnalyticsFeature::TagFilterUsed);
            }
            if let Some(tag) = rename_tag_target {
                self.tag_to_rename = tag.clone();
                self.tag_new_name_buffer = tag;
                self.tag_rename_dialog_open = true;
            }
        }

        // Saved Searches section
        let mut delete_preset_id = None;
        let mut apply_preset_query = None;
        if !self.settings.search_presets.is_empty() || !self.search_query.trim().is_empty() {
            ui.add_space(2.0);
            ui.collapsing(
                egui::RichText::new(format!(
                    "⭐ Saved Searches ({})",
                    self.settings.search_presets.len()
                ))
                .strong(),
                |ui| {
                    for preset in &self.settings.search_presets {
                        let is_active = self.search_query == preset.query;
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(is_active, format!("• {}", preset.name))
                                .on_hover_text(&preset.query)
                                .clicked()
                            {
                                apply_preset_query = Some(preset.query.clone());
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("×")
                                        .on_hover_text("Delete preset")
                                        .clicked()
                                    {
                                        delete_preset_id = Some(preset.id);
                                    }
                                },
                            );
                        });
                    }

                    if !self.search_query.trim().is_empty() {
                        ui.add_space(4.0);
                        if !self.show_new_preset_input {
                            if ui.button("+ Save active search...").clicked() {
                                self.show_new_preset_input = true;
                                self.new_preset_name_buffer =
                                    format!("Search: {}", self.search_query.trim());
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.new_preset_name_buffer);
                                if ui.small_button("Save").clicked()
                                    && !self.new_preset_name_buffer.trim().is_empty()
                                {
                                    self.settings.search_presets.push(SearchPreset {
                                        id: Uuid::new_v4(),
                                        name: self.new_preset_name_buffer.trim().to_owned(),
                                        query: self.search_query.trim().to_owned(),
                                    });
                                    self.record_analytics(AnalyticsFeature::SavedSearchCreated);
                                    self.save_settings();
                                    self.show_new_preset_input = false;
                                }
                                if ui.small_button("Cancel").clicked() {
                                    self.show_new_preset_input = false;
                                }
                            });
                        }
                    }
                },
            );
        }
        if let Some(id) = delete_preset_id {
            self.settings.search_presets.retain(|p| p.id != id);
            self.save_settings();
        }
        if let Some(query) = apply_preset_query {
            self.search_query = query;
            self.focus_search = false;
        }

        ui.add_space(4.0);
        ui.separator();

        // Folder and Note Tree with search input
        if self.show_new_folder_input {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 58.0).max(40.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.new_folder_name)
                        .desired_width(input_width)
                        .hint_text("Folder name..."),
                );
                let enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.small_button("Create").clicked() || enter_pressed {
                    submit_new_folder = true;
                }
            });
        }

        let search_response = ui.add(
            egui::TextEdit::singleline(&mut self.search_query)
                .desired_width(f32::INFINITY)
                .hint_text("Search notes or #tag..."),
        );
        if self.focus_search {
            search_response.request_focus();
            self.focus_search = false;
        }
        if search_response.lost_focus() && !self.search_query.trim().is_empty() {
            self.record_analytics(AnalyticsFeature::SearchUsed);
        }

        ui.add_space(4.0);

        let parsed_query = SearchQuery::parse(&self.search_query);
        let navigation = self.navigation_index();
        let outgoing_links_by_id = &navigation.outgoing;
        let folder_tree = &navigation.tree;
        let notes_by_id: HashMap<Uuid, &Note> = self.data.notes.iter().map(|n| (n.id, n)).collect();

        let mut actions = NotesListActions::default();

        let available_tree_height = (ui.available_height() - 36.0).max(100.0);
        egui::ScrollArea::vertical()
            .max_height(available_tree_height)
            .show(ui, |ui| {
                show_folder_node(
                    ui,
                    &folder_tree.root,
                    &notes_by_id,
                    &parsed_query,
                    outgoing_links_by_id,
                    self.data.selected_note_id,
                    &self.settings.selected_folder,
                    &self.settings.collapsed_folders,
                    self.settings.note_sort,
                    &mut actions,
                );
            });

        if create_note_clicked {
            self.create_note();
        }
        if submit_new_folder {
            self.create_folder_from_input();
        }
        self.apply_notes_list_actions(actions);
    }

    fn show_right_inspector(&mut self, ui: &mut egui::Ui, note: &Note) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (index, label) in ["Outline", "Links", "Properties"].iter().enumerate() {
                ui.selectable_value(&mut self.inspector_tab, index, *label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("inspector_content")
            .show(ui, |ui| match self.inspector_tab {
                0 => {
                    let outline = markdown::extract_outline(&note.content);
                    if outline.is_empty() {
                        ui_style::muted(ui, "Add a Markdown heading to build your outline.");
                    }
                    for item in outline {
                        ui.horizontal(|ui| {
                            ui.add_space(item.level.saturating_sub(1) as f32 * 12.0);
                            if ui.selectable_label(false, &item.title).clicked() {
                                self.pending_cursor_char_index = Some((note.id, item.char_index));
                                self.focus_editor = true;
                            }
                        });
                    }
                }
                1 => {
                    self.show_note_connections(ui, note.id);
                    ui.add_space(12.0);
                    if ui_style::navigation_button(
                        ui,
                        Icon::Graph,
                        false,
                        "Open contextual graph",
                        true,
                    )
                    .clicked()
                    {
                        self.graph_overlay_open = true;
                    }
                }
                _ => {
                    self.show_note_properties(ui, note.id);
                    ui.separator();
                    ui_style::muted(
                        ui,
                        format!("Created {}", note.created_at.format("%d %b %Y, %H:%M")),
                    );
                    ui_style::muted(
                        ui,
                        format!("Modified {}", note.updated_at.format("%d %b %Y, %H:%M")),
                    );
                    let (words, chars) = markdown::count_words_and_chars(&note.content);
                    ui_style::muted(ui, format!("{words} words · {chars} characters"));
                }
            });
    }

    fn show_compact_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let title = ui.add(
                egui::Label::new(egui::RichText::new("Lilo").strong().size(16.0))
                    .sense(egui::Sense::drag()),
            );
            if title.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            self.show_vault_switcher(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.show_toolbar_menu(ui, true);
                if ui_style::icon_button(ui, Icon::Maximize, false, "Expand to workspace").clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1280.0, 800.0,
                        )));
                }
                if ui_style::icon_button(
                    ui,
                    Icon::Pin,
                    self.settings.always_on_top,
                    "Always on top",
                )
                .clicked()
                {
                    self.handle_command_action(CommandAction::ToggleAlwaysOnTop);
                    self.window_settings_applied = false;
                }
                if ui_style::icon_button(ui, Icon::Add, false, "New note").clicked() {
                    self.create_note();
                }
                if ui_style::icon_button(ui, Icon::Search, false, "Search (Ctrl+K)").clicked() {
                    self.command_palette_state.open();
                }
            });
        });
    }

    fn show_bottom_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let total_width = ui.available_width();

            // Right-aligned elements: resize grip, save status, words/chars
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui_style::paint_resize_grip(ui);
                ui.add_space(4.0);

                if let Some(note) = self.data.selected_note() {
                    let saving = self.dirty_note_ids.contains(&note.id);
                    let updated = note.updated_at.format("%H:%M").to_string();
                    let save_status = if self.external_conflict { "! External conflict".to_owned() } else if self.failed_save_ids.contains(&note.id) { "! Save failed".to_owned() } else if saving {
                        if self.settings.autosave_enabled {
                            "• Autosave pending".to_owned()
                        } else {
                            "• Unsaved".to_owned()
                        }
                    } else {
                        format!("• Saved · {updated}")
                    };
                    ui_style::muted(ui, save_status);

                    if total_width >= 560.0 {
                        ui.add_space(8.0);
                        let (words, chars) = markdown::count_words_and_chars(&note.content);
                        ui_style::muted(ui, format!("Words: {words}  ·  Chars: {chars}"));
                    }
                }

                // Left-aligned in remaining area
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if let Some(msg) = &self.storage_message {
                        ui.add(egui::Label::new(egui::RichText::new(msg).small()).truncate()).on_hover_text(msg);
                    } else if let Some(note) = self.data.selected_note() {
                        let (outgoing, backlinks, missing) = self
                            .link_index
                            .links_for(note.id)
                            .map(|l| (l.outgoing.len(), l.backlinks.len(), l.unresolved.len()))
                            .unwrap_or_default();
                        ui_style::muted(
                            ui,
                            if total_width < 460.0 {
                                format!("🔗 {outgoing}  ·  ← {backlinks}")
                            } else {
                                format!("🔗 Links: {outgoing}  ·  Backlinks: {backlinks}  ·  Missing: {missing}")
                            },
                        );
                    } else {
                        ui_style::muted(ui, format!("{} notes in vault", self.data.notes.len()));
                    }
                });
            });
        });
    }

    fn handle_command_action(&mut self, action: CommandAction) {
        self.settings
            .recent_commands
            .retain(|&recent_action| recent_action != action);
        self.settings.recent_commands.insert(0, action);
        self.settings.recent_commands.truncate(8);
        self.save_settings();

        match action {
            CommandAction::OpenTodayNote => self.open_or_create_daily_note(0),
            CommandAction::OpenYesterdayNote => self.open_or_create_daily_note(-1),
            CommandAction::OpenTomorrowNote => self.open_or_create_daily_note(1),
            CommandAction::OpenPrevDayNote => {
                let cur = self
                    .current_daily_note_date()
                    .unwrap_or_else(LocalDateService::today);
                self.open_or_create_daily_note_for_date(LocalDateService::prev_day(cur));
            }
            CommandAction::OpenNextDayNote => {
                let cur = self
                    .current_daily_note_date()
                    .unwrap_or_else(LocalDateService::today);
                self.open_or_create_daily_note_for_date(LocalDateService::next_day(cur));
            }
            CommandAction::QuickCapture => self.quick_capture_state.open(),
            CommandAction::NewNoteFromTemplate => {
                self.template_selector_open = true;
                self.template_selector_for_new_note = true;
            }
            CommandAction::InsertTemplate => {
                self.template_selector_open = true;
                self.template_selector_for_new_note = false;
            }
            CommandAction::NewNote => self.create_note(),
            CommandAction::SaveNote => self.flush_dirty_notes(),
            CommandAction::TogglePin => {
                if let Some(id) = self.data.selected_note_id {
                    self.toggle_pin(id);
                }
            }
            CommandAction::MoveToFolder => self.move_selected_note_to_selected_folder(),
            CommandAction::DeleteNote => {
                if let Some(id) = self.data.selected_note_id {
                    self.delete_note(id);
                }
            }
            CommandAction::NoteDetails => self.note_details_open = true,
            CommandAction::ViewEditor => self.activate_view(AppView::Editor),
            CommandAction::ViewNotesList => self.activate_view(AppView::NotesList),
            CommandAction::ViewGraph => self.activate_view(AppView::Graph),
            CommandAction::ViewTrash => self.activate_view(AppView::Trash),
            CommandAction::ViewSettings => self.activate_view(AppView::Settings),
            CommandAction::ToggleZenMode => {
                self.settings.zen_mode = !self.settings.zen_mode;
                if self.settings.zen_mode {
                    self.record_analytics(AnalyticsFeature::ZenModeEnabled);
                }
                self.save_settings();
            }
            CommandAction::ToggleLeftSidebar => self.toggle_explorer(),
            CommandAction::ToggleRightInspector => self.toggle_inspector(),
            CommandAction::ZoomIn => {
                self.settings.editor_font_size = (self.settings.editor_font_size + 1.0).min(32.0);
                self.settings.font_size = self.settings.editor_font_size;
                self.save_settings();
            }
            CommandAction::ZoomOut => {
                self.settings.editor_font_size = (self.settings.editor_font_size - 1.0).max(10.0);
                self.settings.font_size = self.settings.editor_font_size;
                self.save_settings();
            }
            CommandAction::ZoomReset => {
                self.settings.editor_font_size = 16.0;
                self.settings.font_size = 16.0;
                self.save_settings();
            }
            CommandAction::ToggleTheme => {
                self.settings.theme = match self.settings.theme {
                    ThemeChoice::Dark => ThemeChoice::Light,
                    ThemeChoice::Light => ThemeChoice::Dark,
                    ThemeChoice::System => ThemeChoice::Dark,
                };
                self.save_settings();
            }
            CommandAction::ToggleAlwaysOnTop => {
                self.settings.always_on_top = !self.settings.always_on_top;
                if self.settings.always_on_top {
                    self.record_analytics(AnalyticsFeature::AlwaysOnTopEnabled);
                }
                self.window_settings_applied = false;
                self.save_settings();
            }
            CommandAction::SwitchVault | CommandAction::ExportVault => {
                self.settings_section = 3;
                self.activate_view(AppView::Settings);
            }
            CommandAction::ScanDiagnostics => {
                self.recovery_tab = RecoveryTab::Diagnostics;
                self.activate_view(AppView::Trash);
            }
            CommandAction::NewFolder => {
                self.activate_view(AppView::NotesList);
                self.show_new_folder_input = true;
            }
            CommandAction::DeleteFolder => {
                let folder = self.settings.selected_folder.clone();
                if !folder.as_os_str().is_empty() {
                    let count = self
                        .data
                        .notes
                        .iter()
                        .filter(|n| {
                            n.file_path
                                .strip_prefix(&self.storage_paths.notes_dir)
                                .ok()
                                .is_some_and(|r| r.starts_with(&folder))
                        })
                        .count();
                    if count == 0 {
                        self.delete_folder(&folder);
                    } else {
                        self.pending_folder_delete = Some(folder);
                        self.pending_folder_notes_count = count;
                    }
                }
            }
            CommandAction::SaveCurrentSearch => {
                if !self.search_query.trim().is_empty() {
                    let name = format!("Search: {}", self.search_query.trim());
                    self.settings.search_presets.push(SearchPreset {
                        id: Uuid::new_v4(),
                        name,
                        query: self.search_query.trim().to_owned(),
                    });
                    self.record_analytics(AnalyticsFeature::SavedSearchCreated);
                    self.save_settings();
                    self.storage_message = Some("Saved search preset".to_owned());
                }
            }
            CommandAction::ClearSearch => {
                self.search_query.clear();
                self.focus_search = false;
            }
        }
    }
}

impl WidgetApp {
    fn show_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.discard_on_close {
            let capture_draft = self.quick_capture_state.is_open
                && !self.quick_capture_state.text.trim().is_empty();
            if !self.dirty_note_ids.is_empty() {
                self.flush_dirty_notes();
            }
            if !self.dirty_note_ids.is_empty() || capture_draft {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_pending = true;
            }
        }
        let mut analytics_events = Vec::new();
        self.process_analytics(&ctx);

        while let Some(event) = self.hotkey_manager.try_recv() {
            match event {
                GlobalHotkeyEvent::QuickCapture => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    self.command_palette_state.close();
                    self.quick_capture_state.open();
                    ctx.request_repaint();
                }
            }
        }

        if !self.window_settings_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                if self.settings.always_on_top {
                    egui::viewport::WindowLevel::AlwaysOnTop
                } else {
                    egui::viewport::WindowLevel::Normal
                },
            ));
            self.window_settings_applied = true;
        }

        let accent = egui::Color32::from_rgb(
            self.settings.accent_rgb[0],
            self.settings.accent_rgb[1],
            self.settings.accent_rgb[2],
        );
        let dark_theme = match self.settings.theme {
            ThemeChoice::Light => false,
            ThemeChoice::Dark => true,
            ThemeChoice::System => !matches!(ctx.system_theme(), Some(egui::Theme::Light)),
        };
        let theme_key = (
            dark_theme,
            self.settings.accent_rgb,
            self.settings.ui_font_size.to_bits(),
            self.settings.compact_density,
        );
        if self.applied_theme != Some(theme_key) {
            ui_style::apply_theme(&ctx, dark_theme, accent, self.settings.ui_font_size);
            if self.settings.compact_density {
                ctx.style_mut_of(
                    if dark_theme {
                        egui::Theme::Dark
                    } else {
                        egui::Theme::Light
                    },
                    |style| {
                        style.spacing.item_spacing.y = 4.0;
                        style.spacing.button_padding.y = 4.0;
                    },
                );
            }

            self.applied_theme = Some(theme_key);
        }
        // Paint the complete native viewport before laying out the redesigned shell. This keeps
        // stale pixels or previously persisted legacy panels from showing through during resize.
        ui.painter().rect_filled(
            ui.max_rect(),
            egui::CornerRadius::ZERO,
            ui_style::layer0_color(dark_theme),
        );
        // Frameless window edge and corner resizing
        ui_style::show_window_resize_handles(&ctx);

        let window_width = ui.available_width();
        self.viewport_width = window_width;

        if !self.command_palette_state.is_open && !self.quick_capture_state.is_open {
            if shortcut_pressed(&ctx, "Ctrl+Shift+I") {
                self.toggle_inspector();
            }
            if shortcut_pressed(&ctx, "Ctrl+Shift+B") {
                self.toggle_explorer();
            }
            let modal_open = self.command_palette_state.is_open || self.quick_capture_state.is_open;
            if modal_open {
                ui.disable();
            }
            // Hotkeys handling
            let create_note_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.new_note);
            let open_search_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.search);
            let toggle_graph_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.graph);
            let toggle_overlay_shortcut =
                shortcut_pressed(&ctx, &self.settings.shortcuts.graph_overlay);
            let save_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.save);
            let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));

            // Additional QoL Hotkeys
            let command_palette_shortcut = ctx.input(|i| {
                (i.modifiers.ctrl && i.key_pressed(egui::Key::P))
                    || (i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::P))
                    || (i.modifiers.ctrl && i.key_pressed(egui::Key::K))
            });
            let quick_capture_shortcut = ctx.input(|i| {
                (i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::C))
                    || (i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(egui::Key::N))
            });
            let daily_note_shortcut = ctx.input(|i| {
                (i.modifiers.alt && i.key_pressed(egui::Key::D))
                    || (i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(egui::Key::D))
            });
            let zen_mode_shortcut = ctx.input(|i| i.key_pressed(egui::Key::F11));

            // Ctrl + Plus / Ctrl + Minus / Ctrl + 0 Font Zoom
            let zoom_in = ctx.input(|i| {
                i.modifiers.ctrl
                    && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
            });
            let zoom_out = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Minus));
            let zoom_reset = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Num0));

            // Ctrl + MouseWheel font zoom
            let wheel_delta = ctx.input(|i| {
                if i.modifiers.ctrl {
                    i.smooth_scroll_delta.y
                } else {
                    0.0
                }
            });
            if wheel_delta.abs() > f32::EPSILON {
                // Consume scroll delta to prevent scrolling simultaneously
                ctx.input_mut(|i| {
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                    i.raw
                        .events
                        .retain(|e| !matches!(e, egui::Event::MouseWheel { .. }));
                });
                if wheel_delta > 0.0 {
                    self.settings.editor_font_size =
                        (self.settings.editor_font_size + 0.5).min(32.0);
                } else {
                    self.settings.editor_font_size =
                        (self.settings.editor_font_size - 0.5).max(10.0);
                }
                self.settings.font_size = self.settings.editor_font_size;
                self.save_settings();
            }

            if zoom_in {
                self.settings.editor_font_size = (self.settings.editor_font_size + 1.0).min(32.0);
                self.settings.font_size = self.settings.editor_font_size;
                self.save_settings();
            }
            if zoom_out {
                self.settings.editor_font_size = (self.settings.editor_font_size - 1.0).max(10.0);
                self.settings.font_size = self.settings.editor_font_size;
                self.save_settings();
            }
            if zoom_reset {
                self.settings.editor_font_size = 16.0;
                self.settings.font_size = 16.0;
                self.save_settings();
            }

            if zen_mode_shortcut {
                self.settings.zen_mode = !self.settings.zen_mode;
                if self.settings.zen_mode {
                    analytics_events.push(AnalyticsFeature::ZenModeEnabled);
                }
                self.save_settings();
            }
            if command_palette_shortcut && !self.quick_capture_state.is_open {
                self.command_palette_state.open();
            }
            if quick_capture_shortcut && !self.command_palette_state.is_open {
                self.quick_capture_state.open();
            }
            if daily_note_shortcut && !modal_open {
                self.open_or_create_daily_note(0);
            }

            let direct_view = ctx.input(|input| {
                if !input.modifiers.ctrl || input.modifiers.alt || input.modifiers.shift {
                    None
                } else if input.key_pressed(egui::Key::Num1) {
                    Some(AppView::Editor)
                } else if input.key_pressed(egui::Key::Num2) {
                    Some(AppView::NotesList)
                } else if input.key_pressed(egui::Key::Num3) {
                    Some(AppView::Graph)
                } else if input.key_pressed(egui::Key::Num4) {
                    Some(AppView::Trash)
                } else if input.key_pressed(egui::Key::Num5) || input.key_pressed(egui::Key::Comma)
                {
                    Some(AppView::Settings)
                } else {
                    None
                }
            });

            if let Some(view) = direct_view.filter(|_| !modal_open) {
                self.activate_view(view);
            }

            if self.view == AppView::NotesList {
                let navigation = ctx.input(|input| {
                    if input.modifiers.ctrl && input.key_pressed(egui::Key::ArrowDown) {
                        1
                    } else if input.modifiers.ctrl && input.key_pressed(egui::Key::ArrowUp) {
                        -1
                    } else {
                        0
                    }
                });
                if navigation != 0 {
                    self.navigate_note_list(navigation);
                }
                if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter))
                    && let Some(id) = self.data.selected_note_id
                {
                    self.open_note(id);
                }
            }

            if create_note_shortcut && !modal_open {
                self.create_note();
            }
            if open_search_shortcut && !command_palette_shortcut {
                self.command_palette_state.open();
            }
            if toggle_graph_shortcut && !modal_open {
                let target = if self.view == AppView::Graph {
                    AppView::Editor
                } else {
                    AppView::Graph
                };
                self.activate_view(target);
            }

            if toggle_overlay_shortcut && !modal_open {
                self.graph_overlay_open = !self.graph_overlay_open;
                if self.graph_overlay_open {
                    analytics_events.push(AnalyticsFeature::GraphOpened);
                }
            }

            let navigate_back_shortcut =
                ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft));
            let navigate_forward_shortcut =
                ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight));
            if navigate_back_shortcut {
                if self.current_daily_note_date().is_some() {
                    self.handle_command_action(CommandAction::OpenPrevDayNote);
                } else {
                    self.navigate_back();
                }
            }
            if navigate_forward_shortcut {
                if self.current_daily_note_date().is_some() {
                    self.handle_command_action(CommandAction::OpenNextDayNote);
                } else {
                    self.navigate_forward();
                }
            }

            if save_shortcut && !self.external_conflict {
                self.flush_dirty_notes();
            }
            if escape_pressed {
                if self.command_palette_state.is_open {
                    self.command_palette_state.close();
                } else if self.quick_capture_state.is_open {
                    self.quick_capture_state.close();
                } else if self.note_details_open {
                    self.note_details_open = false;
                } else if self.explorer_drawer_open {
                    self.explorer_drawer_open = false;
                } else if self.template_selector_open {
                    self.template_selector_open = false;
                } else if self.pending_folder_delete.is_some() {
                    self.pending_folder_delete = None;
                } else if self.graph_overlay_open {
                    self.graph_overlay_open = false;
                } else if self.pending_delete_id.is_some() {
                    self.pending_delete_id = None;
                } else if self.view != AppView::Editor {
                    self.view = AppView::Editor;
                    self.focus_search = false;
                    self.focus_editor = true;
                }
            }

            if self.view != AppView::NotesList {
                self.focus_search = false;
            }
            if self.view != AppView::Editor {
                self.focus_editor = false;
            }
        }
        if self.command_palette_state.is_open || self.quick_capture_state.is_open {
            ui.disable();
        }
        let full_graph = self.view == AppView::Graph && self.graph_fullscreen;
        let effective_left_open = !full_graph
            && self.settings.left_sidebar_open
            && window_width >= ui_style::NAV_BREAKPOINT
            && !self.settings.zen_mode;
        let effective_right_open = !full_graph
            && self.settings.right_sidebar_open
            && window_width >= ui_style::WIDE_BREAKPOINT
            && !self.settings.zen_mode
            && self.view == AppView::Editor;
        let effective_status_bar = self.settings.show_status_bar && !self.settings.zen_mode;

        egui::Panel::top("lilo_titlebar")
            .exact_size(ui_style::TOP_BAR_HEIGHT)
            .show(ui, |ui| {
                if window_width < ui_style::COMPACT_WIDTH {
                    self.show_compact_header(ui);
                } else {
                    self.show_workspace_header(ui);
                }
            });
        // Left Panel (Navigator / Explorer - Layer 1)
        if effective_left_open {
            let previous_width = self.settings.sidebar_width;
            let sidebar = egui::Panel::left("left_explorer_panel")
                .default_size(self.settings.sidebar_width)
                .min_size(180.0)
                .max_size(360.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(ui_style::layer1_color(dark_theme))
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(ui, |ui| {
                    egui::Panel::bottom("sidebar_footer")
                        .exact_size(76.0)
                        .show(ui, |ui| {
                            for (view, icon, label) in [
                                (AppView::Settings, Icon::Settings, "Settings"),
                                (AppView::Trash, Icon::Trash, "Trash & Backups"),
                            ] {
                                if ui_style::navigation_button(
                                    ui,
                                    icon,
                                    self.view == view,
                                    label,
                                    true,
                                )
                                .clicked()
                                {
                                    self.activate_view(view);
                                }
                            }
                        });
                    egui::ScrollArea::vertical()
                        .id_salt("explorer_scroll")
                        .show(ui, |ui| {
                            self.show_left_explorer(ui);
                        });
                });
            self.settings.sidebar_width = sidebar.response.rect.width().clamp(180.0, 360.0);
            if (self.settings.sidebar_width - previous_width).abs() > 0.5
                && ctx.input(|i| i.pointer.any_released())
            {
                self.save_settings();
            }
        }

        // Right Panel (Context Inspector - Layer 1)
        if effective_right_open {
            egui::Panel::right("right_inspector_panel")
                .default_size(ui_style::INSPECTOR_PANEL_WIDTH)
                .min_size(200.0)
                .max_size(380.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(ui_style::layer1_color(dark_theme))
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(ui, |ui| {
                    if let Some(note) = self.data.selected_note() {
                        self.show_right_inspector(ui, &note.clone());
                    }
                });
        }

        // Bottom Status Bar (Layer 1)
        if effective_status_bar {
            egui::Panel::bottom("bottom_status_bar")
                .exact_size(ui_style::BOTTOM_BAR_HEIGHT)
                .frame(
                    egui::Frame::new()
                        .fill(ui_style::layer1_color(dark_theme))
                        .inner_margin(egui::Margin::symmetric(12, 4)),
                )
                .show(ui, |ui| {
                    self.show_bottom_status_bar(ui);
                });
        }

        if self.external_conflict {
            egui::Panel::top("external_conflict").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let changed = self
                        .external_changed_paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "External changes conflict with local edits",
                    )
                    .on_hover_text(changed);
                    if ui.button("Reload disk").clicked() {
                        self.dirty_note_ids.clear();
                        self.pending_title_rename_ids.clear();
                        self.reload_vault("Reloaded disk version");
                    }
                    if ui.button("Keep mine").clicked() {
                        self.refresh_vault_snapshot();
                        self.external_conflict = false;
                        self.flush_dirty_notes();
                    }
                });
            });
        }

        // Central Panel (Layer 0 Background with Elevated Note Card)
        let canvas_fill = ui_style::layer0_color(dark_theme);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(canvas_fill)
                    .inner_margin(egui::Margin::symmetric(
                        if window_width < 650.0 { 8 } else { 16 },
                        if window_width < 650.0 { 8 } else { 12 },
                    )),
            )
            .show(ui, |ui| match self.view {
                AppView::Editor => {
                    self.show_editor_workspace(ui, window_width, canvas_fill, &mut analytics_events)
                }
                AppView::NotesList => self.show_notes_list(ui),
                AppView::Graph => {
                    let output = graph::show(
                        ui,
                        &mut self.graph_state,
                        &self.data.notes,
                        &self.link_index,
                        self.data.selected_note_id,
                        &self.storage_paths.notes_dir,
                        &self.settings.selected_folder,
                    );
                    self.handle_graph_output(output);
                }
                AppView::Trash => self.show_trash(ui),
                AppView::Settings => self.show_settings(ui, &ctx),
            });

        if self.explorer_drawer_open && window_width < ui_style::NAV_BREAKPOINT {
            let mut open = true;
            egui::Window::new("Your notes")
                .open(&mut open)
                .collapsible(false)
                .default_width((window_width - 48.0).max(180.0))
                .max_height(ui_style::screen_rect(&ctx).height() - 80.0)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.show_left_explorer(ui));
                });
            self.explorer_drawer_open = open;
        }
        if self.note_details_open {
            let mut open = true;
            egui::Window::new("Note details")
                .id(egui::Id::new("note_details"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(300.0, 260.0))
                .show(&ctx, |ui| {
                    if let Some(note) = self.data.selected_note().cloned() {
                        self.show_right_inspector(ui, &note);
                    }
                });
            self.note_details_open = open;
        }

        if self.graph_overlay_open {
            let mut open = true;
            let mut graph_output = None;
            egui::Window::new("Knowledge graph")
                .id(egui::Id::new("graph_overlay"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(320.0, 360.0))
                .show(&ctx, |ui| {
                    let output = graph::show(
                        ui,
                        &mut self.graph_state,
                        &self.data.notes,
                        &self.link_index,
                        self.data.selected_note_id,
                        &self.storage_paths.notes_dir,
                        &self.settings.selected_folder,
                    );
                    graph_output = Some(output);
                });
            self.graph_overlay_open = open;
            if graph_output.is_some_and(|output| self.handle_graph_output(output)) {
                self.graph_overlay_open = false;
            }
        }

        // Global note deletion confirmation dialog
        if let Some(id) = self.pending_delete_id {
            let note_title = self
                .data
                .notes
                .iter()
                .find(|n| n.id == id)
                .map(|n| {
                    if n.title.trim().is_empty() {
                        "Untitled".to_string()
                    } else {
                        n.title.clone()
                    }
                })
                .unwrap_or_else(|| "Note".to_string());
            let center_pos = ui_style::screen_rect(&ctx).center();
            egui::Window::new("Move to Trash")
                .id(egui::Id::new("confirm_delete_note_modal"))
                .collapsible(false)
                .resizable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center_pos)
                .show(&ctx, |ui| {
                    ui.label(format!("Move note \"{note_title}\" to Trash?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Move to Trash").clicked() {
                            self.delete_note(id);
                            self.pending_delete_id = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_delete_id = None;
                        }
                    });
                });
        }

        // Folder deletion confirmation dialog
        if let Some(folder) = self.pending_folder_delete.clone() {
            let center_pos = ui_style::screen_rect(&ctx).center();
            egui::Window::new("Delete folder")
                .id(egui::Id::new("confirm_delete_folder"))
                .collapsible(false)
                .resizable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center_pos)
                .show(&ctx, |ui| {
                    ui.label(format!(
                        "Folder '{}' contains {} note(s).",
                        folder.display(),
                        self.pending_folder_notes_count
                    ));
                    ui.label("Move this folder and all its notes to Trash?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Move to Trash").clicked() {
                            self.delete_folder(&folder);
                            self.pending_folder_delete = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_folder_delete = None;
                        }
                    });
                });
        }

        // Template Selection Dialog
        if self.template_selector_open {
            let mut close = false;
            let mut selected_template = None;
            let templates = self.cached_templates();

            let center_pos = ui_style::screen_rect(&ctx).center();
            egui::Window::new(if self.template_selector_for_new_note {
                "Select Template for New Note"
            } else {
                "Select Template to Insert"
            })
            .id(egui::Id::new("template_selector_modal"))
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(320.0, 240.0))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(center_pos)
            .show(&ctx, |ui| {
                if templates.is_empty() {
                    ui.label("No templates found in Templates folder.");
                    ui.small("Create .md files in your Templates directory to use them here.");
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for entry in templates {
                                if ui.button(&entry.name).clicked() {
                                    selected_template = Some(entry.name);
                                    close = true;
                                }
                            }
                        });
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                }
            });

            if let Some(t_name) = selected_template {
                if self.template_selector_for_new_note {
                    self.create_note_from_template(&t_name);
                } else {
                    self.insert_template_into_active_note(&t_name);
                }
            }
            if close {
                self.template_selector_open = false;
            }
        }

        if let Some(preview) = self.pending_link_rewrite.clone() {
            let center_pos = ui_style::screen_rect(&ctx).center();
            egui::Window::new("Review vault-wide link update")
                .id(egui::Id::new("link_rewrite_preview_modal"))
                .collapsible(false)
                .resizable(true)
                .default_width(420.0)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center_pos)
                .show(&ctx, |ui| {
                    ui.label(format!(
                        "The note was renamed from '{}' to '{}'.",
                        preview.old_title, preview.new_title
                    ));
                    ui.label(format!(
                        "The following {} note(s) contain matching wiki-links:",
                        preview.affected_note_ids.len()
                    ));
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for id in &preview.affected_note_ids {
                            if let Some(note) = self.data.notes.iter().find(|note| note.id == *id) {
                                ui.label(format!("• {}", note.title));
                            }
                        }
                    });
                    ui.small("No files are changed until you confirm. Every affected note is backed up and saved independently.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Update links").clicked() {
                            let modified = links::rename_note_references(
                                &mut self.data.notes,
                                &preview.old_title,
                                &preview.new_title,
                            );
                            self.dirty_note_ids.extend(modified);
                            self.flush_dirty_notes();
                            self.link_index =
                                LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                            self.pending_link_rewrite = None;
                        }
                        if ui.button("Keep existing links").clicked() {
                            self.pending_link_rewrite = None;
                            self.storage_message = Some(
                                "Note renamed; existing wiki-links were left unchanged".to_owned(),
                            );
                        }
                    });
                });
        }

        // Tag Rename Dialog
        if self.tag_rename_dialog_open {
            let affected_note_ids = tags::preview_tag_rename(
                &self.data.notes,
                &self.tag_to_rename,
                &self.tag_new_name_buffer,
            );
            let center_pos = ui_style::screen_rect(&ctx).center();
            egui::Window::new("Rename Tag across Vault")
                .id(egui::Id::new("tag_rename_modal"))
                .collapsible(false)
                .resizable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(center_pos)
                .show(&ctx, |ui| {
                    ui.label(format!("Old tag: #{}", self.tag_to_rename));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("New tag:");
                        ui.text_edit_singleline(&mut self.tag_new_name_buffer);
                    });
                    ui.add_space(6.0);
                    if self.tag_new_name_buffer.trim().is_empty() {
                        ui.small("Enter the replacement tag to calculate the preview.");
                    } else if affected_note_ids.is_empty() {
                        ui.small("No notes would be changed.");
                    } else {
                        ui.label(format!(
                            "Review: {} note(s) will be updated:",
                            affected_note_ids.len()
                        ));
                        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                            for id in &affected_note_ids {
                                if let Some(note) = self.data.notes.iter().find(|note| note.id == *id) {
                                    ui.label(format!("• {}", note.title));
                                }
                            }
                        });
                        ui.small("Each changed file will receive a backup and will be saved independently.");
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !affected_note_ids.is_empty(),
                                egui::Button::new("Apply reviewed changes"),
                            )
                            .clicked()
                        {
                            let modified_note_ids = tags::rename_tag_in_vault(
                                &mut self.data.notes,
                                &self.tag_to_rename,
                                &self.tag_new_name_buffer,
                            );
                            let count = modified_note_ids.len();
                            self.dirty_note_ids.extend(modified_note_ids);
                            self.tag_index = TagIndex::build(&self.data.notes);
                            self.link_index =
                                LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                            self.storage_message = Some(format!("Renamed tag in {count} note(s)"));
                            self.flush_dirty_notes();
                            self.tag_rename_dialog_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.tag_rename_dialog_open = false;
                        }
                    });
                });
        }

        // Modal Command Palette
        if let Some(result) = commands::show_command_palette(
            &ctx,
            &mut self.command_palette_state,
            &self.data.notes,
            &self.storage_paths.notes_dir,
            &self.link_index,
            &self.tag_index,
            &self.settings,
        ) {
            analytics_events.push(AnalyticsFeature::CommandPaletteUsed);
            match result {
                CommandPaletteResult::Action(action) => self.handle_command_action(action),
                CommandPaletteResult::OpenNote(id) => {
                    self.open_note(id);
                    self.activate_view(AppView::Editor);
                }
            }
        }

        // Modal Quick Capture
        if let Some(submission) = quick_capture::show_quick_capture(
            &ctx,
            &mut self.quick_capture_state,
            &self.settings.quick_capture_target,
            &self.settings.quick_capture_custom_note,
            &self.data.notes,
            &self.storage_paths.notes_dir,
        ) {
            self.apply_quick_capture(submission);
        }

        for feature in analytics_events {
            self.record_analytics(feature);
        }

        self.show_analytics_consent(&ctx);
        if self.analytics_details_open {
            let mut open = true;
            let screen_rect = ui_style::screen_rect(&ctx);
            let details_size = egui::vec2(
                (screen_rect.width() - 32.0).clamp(248.0, 520.0),
                (screen_rect.height() - 48.0).clamp(152.0, 320.0),
            );
            egui::Window::new("Analytics data")
                .id(egui::Id::new("analytics_data_details"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .constrain_to(screen_rect)
                .default_size(details_size)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("analytics_data_details_scroll")
                        .show(ui, Self::show_analytics_data_description);
                });
            self.analytics_details_open = open;
        }

        if self.close_pending {
            egui::Modal::new(egui::Id::new("unsaved_close")).show(&ctx, |ui| {
                ui.set_max_width(360.0);
                ui.heading("Keep your unsaved work");
                ui.label("Some changes or a Quick Capture draft have not been saved. Continue editing to resolve the save error, or explicitly discard them.");
                if ui_style::primary_button(ui, "Continue editing").clicked() { self.close_pending = false; }
                if ui.button("Discard unsaved changes and close").clicked() {
                    self.discard_on_close = true;
                    self.close_pending = false;
                    self.dirty_note_ids.clear();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
        self.process_deferred_index_refresh(&ctx);
        self.process_autosave(&ctx);
        self.sync_external_changes(&ctx);
        self.process_analytics(&ctx);
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
                cache_dir: root.join(".lilo/cache"),
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
}
