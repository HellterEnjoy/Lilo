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
    ThemeChoice, ToolbarPlacement,
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

    view: AppView,
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

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum EffectiveToolbarPlacement {
    Top,
    Left,
    Right,
    Floating,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RecoveryTab {
    #[default]
    Trash,
    Backups,
    Diagnostics,
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

fn shortcut_pressed(ctx: &egui::Context, shortcut: &str) -> bool {
    let parts = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(key_name) = parts.last() else {
        return false;
    };
    let key = match key_name.to_ascii_uppercase().as_str() {
        "A" => egui::Key::A,
        "B" => egui::Key::B,
        "C" => egui::Key::C,
        "D" => egui::Key::D,
        "E" => egui::Key::E,
        "F" => egui::Key::F,
        "G" => egui::Key::G,
        "H" => egui::Key::H,
        "I" => egui::Key::I,
        "J" => egui::Key::J,
        "K" => egui::Key::K,
        "L" => egui::Key::L,
        "M" => egui::Key::M,
        "N" => egui::Key::N,
        "O" => egui::Key::O,
        "P" => egui::Key::P,
        "Q" => egui::Key::Q,
        "R" => egui::Key::R,
        "S" => egui::Key::S,
        "T" => egui::Key::T,
        "U" => egui::Key::U,
        "V" => egui::Key::V,
        "W" => egui::Key::W,
        "X" => egui::Key::X,
        "Y" => egui::Key::Y,
        "Z" => egui::Key::Z,
        "0" => egui::Key::Num0,
        "1" => egui::Key::Num1,
        "2" => egui::Key::Num2,
        "3" => egui::Key::Num3,
        "4" => egui::Key::Num4,
        "5" => egui::Key::Num5,
        "F11" => egui::Key::F11,
        "=" | "+" => egui::Key::Plus,
        "-" => egui::Key::Minus,
        _ => return false,
    };
    ctx.input(|input| {
        let wants_ctrl = parts.iter().any(|part| part.eq_ignore_ascii_case("ctrl"));
        let wants_shift = parts.iter().any(|part| part.eq_ignore_ascii_case("shift"));
        let wants_alt = parts.iter().any(|part| part.eq_ignore_ascii_case("alt"));
        input.modifiers.ctrl == wants_ctrl
            && input.modifiers.shift == wants_shift
            && input.modifiers.alt == wants_alt
            && input.key_pressed(key)
    })
}

fn shortcut_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(120.0));
    });
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
        let loaded = storage::load_storage().expect("Failed to initialize Markdown storage");
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
        if operating_system.supports_autostart() {
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
            view: AppView::Editor,
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

    fn save_settings(&mut self) {
        self.settings.selected_note_id = self.data.selected_note_id;
        if let Err(error) =
            storage::save_settings(&self.storage_paths.settings_path, &self.settings)
        {
            self.storage_message = Some(format!("Failed to save settings: {error}"));
        }
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

    #[allow(dead_code)]
    fn effective_toolbar_placement(&self, width: f32) -> EffectiveToolbarPlacement {
        if self.settings.zen_mode {
            return EffectiveToolbarPlacement::Top;
        }
        if width < 340.0 {
            return EffectiveToolbarPlacement::Top;
        }
        match self.settings.toolbar_placement {
            ToolbarPlacement::Auto if width < ui_style::NAV_BREAKPOINT => {
                EffectiveToolbarPlacement::Top
            }
            ToolbarPlacement::Auto | ToolbarPlacement::Left => EffectiveToolbarPlacement::Left,
            ToolbarPlacement::Top => EffectiveToolbarPlacement::Top,
            ToolbarPlacement::Right => EffectiveToolbarPlacement::Right,
            ToolbarPlacement::Floating => EffectiveToolbarPlacement::Floating,
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

    #[allow(dead_code)]
    fn show_navigation_buttons(&mut self, ui: &mut egui::Ui, expanded: bool) {
        for (view, icon, label) in [
            (AppView::Editor, Icon::Editor, "Editor"),
            (AppView::NotesList, Icon::Notes, "Notes"),
            (AppView::Graph, Icon::Graph, "Knowledge graph"),
            (AppView::Trash, Icon::Trash, "Recovery"),
            (AppView::Settings, Icon::Settings, "Settings"),
        ] {
            if ui_style::navigation_button(ui, icon, self.view == view, label, expanded).clicked() {
                self.activate_view(view);
            }
        }
    }

    #[allow(dead_code)]
    fn show_toolbar_menu(&mut self, ui: &mut egui::Ui, include_hidden_views: bool) {
        let before = (
            self.settings.toolbar_placement,
            self.settings.toolbar_expanded,
            self.settings.floating_toolbar_vertical,
            self.settings.zen_mode,
        );
        let mut requested_view = None;
        ui.menu_button("...", |ui| {
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
            if ui.button("Command Palette (Ctrl+P)").clicked() {
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
            ui.label("Toolbar position");
            ui.selectable_value(
                &mut self.settings.toolbar_placement,
                ToolbarPlacement::Auto,
                "Auto",
            );
            ui.selectable_value(
                &mut self.settings.toolbar_placement,
                ToolbarPlacement::Top,
                "Top",
            );
            ui.selectable_value(
                &mut self.settings.toolbar_placement,
                ToolbarPlacement::Left,
                "Left",
            );
            ui.selectable_value(
                &mut self.settings.toolbar_placement,
                ToolbarPlacement::Right,
                "Right",
            );
            ui.selectable_value(
                &mut self.settings.toolbar_placement,
                ToolbarPlacement::Floating,
                "Floating",
            );
            ui.separator();
            ui.checkbox(&mut self.settings.toolbar_expanded, "Show labels");
            if self.settings.toolbar_placement == ToolbarPlacement::Floating {
                ui.checkbox(
                    &mut self.settings.floating_toolbar_vertical,
                    "Vertical floating toolbar",
                );
            }
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
        let after = (
            self.settings.toolbar_placement,
            self.settings.toolbar_expanded,
            self.settings.floating_toolbar_vertical,
            self.settings.zen_mode,
        );
        if before != after {
            if !before.3 && after.3 {
                self.record_analytics(AnalyticsFeature::ZenModeEnabled);
            }
            self.save_settings();
        }
        if let Some(view) = requested_view {
            self.activate_view(view);
        }
    }

    fn save_note_to_disk(&mut self, id: Uuid) -> bool {
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
            Some(Ok(())) => true,
            Some(Err(error)) => {
                self.storage_message = Some(format!("Failed to save note: {error}"));
                false
            }
            None => false,
        }
    }

    fn save_note_now(&mut self, id: Uuid) -> bool {
        let saved = self.save_note_to_disk(id);
        if saved {
            self.refresh_vault_snapshot();
        }
        saved
    }

    fn refresh_vault_snapshot(&mut self) {
        self.vault_snapshot =
            storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
    }

    fn mark_note_dirty(&mut self, id: Uuid) {
        self.dirty_note_ids.insert(id);
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    fn flush_dirty_notes(&mut self) {
        let ids: Vec<Uuid> = self.dirty_note_ids.iter().copied().collect();
        let mut disk_changed = false;
        for id in ids {
            if self.save_note_to_disk(id) {
                disk_changed = true;
                if self.pending_title_rename_ids.remove(&id) {
                    let rename_result = self
                        .data
                        .notes
                        .iter_mut()
                        .find(|note| note.id == id)
                        .map(storage::rename_note_file);
                    if let Some(Err(error)) = rename_result {
                        self.storage_message = Some(format!("Failed to rename note file: {error}"));
                    }
                }
                self.dirty_note_ids.remove(&id);
            }
        }

        if disk_changed {
            self.refresh_vault_snapshot();
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
    pub fn apply_quick_capture(&mut self, submission: QuickCaptureSubmission) {
        let entry = quick_capture::format_capture_entry(&submission.text, submission.timestamp);

        let (captured_note_id, captured) = match submission.target {
            QuickCaptureTarget::DailyNote => {
                let target_date = LocalDateService::today();
                let (subfolder, note_title) = match LocalDateService::format_daily_path(
                    &self.settings.daily_note_format,
                    target_date,
                ) {
                    Ok(res) => res,
                    Err(_) => (PathBuf::new(), target_date.format("%Y-%m-%d").to_string()),
                };

                let target_folder_rel = self.settings.daily_notes_folder.join(&subfolder);
                let target_folder_abs =
                    storage::ensure_note_folder(&self.storage_paths.notes_dir, &target_folder_rel)
                        .unwrap_or_else(|_| self.storage_paths.notes_dir.clone());

                let existing_id = self
                    .data
                    .notes
                    .iter()
                    .find(|n| n.title.eq_ignore_ascii_case(&note_title))
                    .map(|n| n.id);

                let note_id = if let Some(id) = existing_id {
                    id
                } else {
                    let id = self.data.create_note_named(&target_folder_abs, &note_title);
                    if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
                        note.content = format!("# {}\n\n", note_title);
                    }
                    id
                };

                let target_title =
                    if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == note_id) {
                        if !note.content.is_empty() && !note.content.ends_with('\n') {
                            note.content.push('\n');
                        }
                        note.content.push_str(&entry);
                        note.mark_as_updated();
                        self.link_index.refresh_note_content(note);
                        note.title.clone()
                    } else {
                        String::new()
                    };

                let saved = self.save_note_now(note_id);
                self.storage_message = Some(format!("Captured to daily note ({target_title})"));
                (note_id, saved)
            }
            QuickCaptureTarget::Inbox => {
                let inbox_title = "Inbox";
                let existing_id = self
                    .data
                    .notes
                    .iter()
                    .find(|n| n.title.eq_ignore_ascii_case(inbox_title))
                    .map(|n| n.id);

                let note_id = if let Some(id) = existing_id {
                    id
                } else {
                    let id = self
                        .data
                        .create_note_named(&self.storage_paths.notes_dir, inbox_title);
                    if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
                        note.content = format!("# {}\n\n", inbox_title);
                    }
                    id
                };

                if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == note_id) {
                    if !note.content.is_empty() && !note.content.ends_with('\n') {
                        note.content.push('\n');
                    }
                    note.content.push_str(&entry);
                    note.mark_as_updated();
                    self.link_index.refresh_note_content(note);
                }

                let saved = self.save_note_now(note_id);
                self.storage_message = Some("Captured to Inbox".to_owned());
                (note_id, saved)
            }
            QuickCaptureTarget::NewNote => {
                let note_directory = storage::ensure_note_folder(
                    &self.storage_paths.notes_dir,
                    &self.settings.selected_folder,
                )
                .unwrap_or_else(|_| self.storage_paths.notes_dir.clone());

                let title = format!("Thought {}", submission.timestamp.format("%Y-%m-%d %H%M%S"));
                let id = self.data.create_note_named(&note_directory, &title);
                if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
                    note.content = format!("# {}\n\n{}", title, entry);
                    note.refresh_search_text();
                }
                let saved = self.save_note_now(id);
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.storage_message = Some(format!("Created capture note '{title}'"));
                (id, saved)
            }
            QuickCaptureTarget::CustomNote(target_title) => {
                let clean_title = if target_title.trim().is_empty() {
                    "Quick Notes"
                } else {
                    target_title.trim()
                };

                let existing_id = self
                    .data
                    .notes
                    .iter()
                    .find(|n| n.title.eq_ignore_ascii_case(clean_title))
                    .map(|n| n.id);

                let note_id = if let Some(id) = existing_id {
                    id
                } else {
                    let note_directory = storage::ensure_note_folder(
                        &self.storage_paths.notes_dir,
                        &self.settings.selected_folder,
                    )
                    .unwrap_or_else(|_| self.storage_paths.notes_dir.clone());
                    let id = self.data.create_note_named(&note_directory, clean_title);
                    if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == id) {
                        note.content = format!("# {}\n\n", clean_title);
                    }
                    id
                };

                if let Some(note) = self.data.notes.iter_mut().find(|n| n.id == note_id) {
                    if !note.content.is_empty() && !note.content.ends_with('\n') {
                        note.content.push('\n');
                    }
                    note.content.push_str(&entry);
                    note.mark_as_updated();
                    self.link_index.refresh_note_content(note);
                }

                let saved = self.save_note_now(note_id);
                self.storage_message = Some(format!("Captured to '{clean_title}'"));
                (note_id, saved)
            }
        };

        // Record in recent notes
        self.settings
            .recent_note_ids
            .retain(|&recent_id| recent_id != captured_note_id);
        self.settings.recent_note_ids.insert(0, captured_note_id);
        self.settings.recent_note_ids.truncate(15);
        if captured {
            self.record_analytics(AnalyticsFeature::QuickCaptureSaved);
        }
        self.save_settings();
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
            Ok(trashed_ids) => {
                for id in trashed_ids {
                    self.data.remove_note(id);
                }
                self.folder_paths.retain(|folder| !folder.starts_with(path));
                self.settings
                    .collapsed_folders
                    .retain(|folder| !folder.starts_with(path));
                if self.settings.selected_folder.starts_with(path) {
                    self.settings.selected_folder = PathBuf::new();
                }
                self.link_index = LinkIndex::build(&self.data.notes, &self.storage_paths.notes_dir);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.save_settings();
                self.storage_message = Some(format!("Folder '{}' moved to Trash", path.display()));
            }
            Err(error) => self.storage_message = Some(format!("Failed to delete folder: {error}")),
        }
    }

    fn reload_vault(&mut self, reason: &str) {
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
        self.last_external_sync = Instant::now();
        let current = storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
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
                // If other files changed on disk but not the one being typed into, update snapshot safely
                self.vault_snapshot = current;
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
            "Notes in trash can be restored to their original folder.",
        );
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
        ui.small("Lilo creates rotating snapshots before overwriting note files.");
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
        let previous_path = self.settings.vault_path.clone();
        if let Err(error) = storage::set_vault_path(&mut self.settings, &self.vault_path_buffer) {
            self.storage_message = Some(format!("Invalid vault path: {error}"));
            return;
        }
        if let Err(error) =
            storage::save_settings(&self.storage_paths.settings_path, &self.settings)
        {
            self.settings.vault_path = previous_path;
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
                self.graph_state = graph::GraphState::restore(&self.settings.graph_node_offsets);
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.vault_path_buffer = self.settings.vault_path.display().to_string();
                self.dirty_note_ids.clear();
                self.pending_title_rename_ids.clear();
                self.external_conflict = false;
                self.external_changed_paths.clear();
                self.storage_message = Some("Vault switched successfully".to_owned());
                self.view = AppView::NotesList;
            }
            Err(error) => {
                self.settings.vault_path = previous_path;
                let _ = storage::save_settings(&self.storage_paths.settings_path, &self.settings);
                self.storage_message = Some(format!("Could not switch vault: {error}"));
            }
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

    fn show_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let operating_system = platform::OperatingSystem::current();
        ui.set_max_width(ui.available_width().min(760.0));
        ui_style::screen_title(ui, "Settings");
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("Appearance & Typography").strong(),
                )
                .id_salt("settings_appearance")
                .show(ui, |ui| {
                    ui_style::muted(ui, "Theme, typography, accent colour and layout");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.settings.theme, ThemeChoice::Dark, "Dark");
                        ui.selectable_value(&mut self.settings.theme, ThemeChoice::Light, "Light");
                        ui.selectable_value(
                            &mut self.settings.theme,
                            ThemeChoice::System,
                            "System",
                        );
                    });
                    ui.add(
                        egui::Slider::new(&mut self.settings.editor_font_size, 10.0..=32.0)
                            .text("Editor font size (Ctrl +/-)"),
                    );
                    self.settings.font_size = self.settings.editor_font_size;
                    ui.add(
                        egui::Slider::new(&mut self.settings.ui_font_size, 11.0..=20.0)
                            .text("UI interface font size"),
                    );
                    if ui
                        .checkbox(&mut self.settings.zen_mode, "Zen / Writing mode (F11)")
                        .changed()
                        && self.settings.zen_mode
                    {
                        self.record_analytics(AnalyticsFeature::ZenModeEnabled);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Accent");
                        ui.color_edit_button_srgb(&mut self.settings.accent_rgb);
                    });
                    if ui
                        .checkbox(&mut self.settings.always_on_top, "Always on top")
                        .changed()
                    {
                        if self.settings.always_on_top {
                            self.record_analytics(AnalyticsFeature::AlwaysOnTopEnabled);
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            if self.settings.always_on_top {
                                egui::viewport::WindowLevel::AlwaysOnTop
                            } else {
                                egui::viewport::WindowLevel::Normal
                            },
                        ));
                    }
                    let autostart_before = self.settings.autostart;
                    let autostart_response = ui
                        .add_enabled(
                            operating_system.supports_autostart(),
                            egui::Checkbox::new(
                                &mut self.settings.autostart,
                                operating_system.autostart_label(),
                            ),
                        )
                        .on_disabled_hover_text(format!(
                            "Autostart integration is not implemented for {}",
                            operating_system.name()
                        ));
                    if autostart_response.changed()
                        && let Err(error) = platform::set_autostart(self.settings.autostart)
                    {
                        self.settings.autostart = autostart_before;
                        self.storage_message = Some(format!("Autostart update failed: {error}"));
                    }
                    ui.label("Navigation toolbar");
                    egui::ComboBox::from_id_salt("toolbar_placement")
                        .selected_text(match self.settings.toolbar_placement {
                            ToolbarPlacement::Auto => "Auto",
                            ToolbarPlacement::Top => "Top",
                            ToolbarPlacement::Left => "Left",
                            ToolbarPlacement::Right => "Right",
                            ToolbarPlacement::Floating => "Floating",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.settings.toolbar_placement,
                                ToolbarPlacement::Auto,
                                "Auto — top when compact, left when wide",
                            );
                            ui.selectable_value(
                                &mut self.settings.toolbar_placement,
                                ToolbarPlacement::Top,
                                "Top",
                            );
                            ui.selectable_value(
                                &mut self.settings.toolbar_placement,
                                ToolbarPlacement::Left,
                                "Left",
                            );
                            ui.selectable_value(
                                &mut self.settings.toolbar_placement,
                                ToolbarPlacement::Right,
                                "Right",
                            );
                            ui.selectable_value(
                                &mut self.settings.toolbar_placement,
                                ToolbarPlacement::Floating,
                                "Floating",
                            );
                        });
                    ui.checkbox(
                        &mut self.settings.toolbar_expanded,
                        "Show labels when there is enough space",
                    );
                    if self.settings.toolbar_placement == ToolbarPlacement::Floating {
                        ui.checkbox(
                            &mut self.settings.floating_toolbar_vertical,
                            "Vertical floating toolbar",
                        );
                        ui_style::muted(ui, "Drag the grip to move it. Drop near an edge to dock.");
                    }
                });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("Daily Notes & Templates").strong(),
                )
                .id_salt("settings_daily_templates")
                .show(ui, |ui| {
                    ui_style::muted(
                        ui,
                        "Configuration for daily workflow, templates and quick capture",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Daily notes folder:");
                        let mut folder_str = self.settings.daily_notes_folder.display().to_string();
                        if ui.text_edit_singleline(&mut folder_str).changed() {
                            self.settings.daily_notes_folder = PathBuf::from(folder_str);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Daily note date format:");
                        ui.text_edit_singleline(&mut self.settings.daily_note_format);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Templates folder:");
                        let mut t_str = self.settings.templates_folder.display().to_string();
                        if ui.text_edit_singleline(&mut t_str).changed() {
                            self.settings.templates_folder = PathBuf::from(t_str);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Default daily template:");
                        let available_templates = TemplateEngine::list_templates(
                            &self.storage_paths.notes_dir,
                            &self.settings.templates_folder,
                        );
                        egui::ComboBox::from_id_salt("default_daily_template_combo")
                            .selected_text(if self.settings.default_daily_template.is_empty() {
                                "(None / Default Format)".to_owned()
                            } else {
                                self.settings.default_daily_template.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.settings.default_daily_template,
                                    String::new(),
                                    "(None / Default Format)",
                                );
                                for t in available_templates {
                                    ui.selectable_value(
                                        &mut self.settings.default_daily_template,
                                        t.name.clone(),
                                        &t.name,
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Quick capture target:");
                        egui::ComboBox::from_id_salt("quick_capture_target_combo")
                            .selected_text(match &self.settings.quick_capture_target {
                                QuickCaptureTarget::DailyNote => "Today's Daily Note",
                                QuickCaptureTarget::Inbox => "Inbox.md",
                                QuickCaptureTarget::NewNote => "Create New Timestamped Note",
                                QuickCaptureTarget::CustomNote(_) => "Specific Custom Note",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.settings.quick_capture_target,
                                    QuickCaptureTarget::DailyNote,
                                    "Today's Daily Note",
                                );
                                ui.selectable_value(
                                    &mut self.settings.quick_capture_target,
                                    QuickCaptureTarget::Inbox,
                                    "Inbox.md",
                                );
                                ui.selectable_value(
                                    &mut self.settings.quick_capture_target,
                                    QuickCaptureTarget::NewNote,
                                    "Create New Timestamped Note",
                                );
                                ui.selectable_value(
                                    &mut self.settings.quick_capture_target,
                                    QuickCaptureTarget::CustomNote(
                                        self.settings.quick_capture_custom_note.clone(),
                                    ),
                                    "Specific Custom Note",
                                );
                            });
                    });
                    if matches!(
                        self.settings.quick_capture_target,
                        QuickCaptureTarget::CustomNote(_)
                    ) {
                        ui.horizontal(|ui| {
                            ui.label("Custom note name:");
                            if ui
                                .text_edit_singleline(&mut self.settings.quick_capture_custom_note)
                                .changed()
                            {
                                self.settings.quick_capture_target = QuickCaptureTarget::CustomNote(
                                    self.settings.quick_capture_custom_note.clone(),
                                );
                            }
                        });
                    }
                });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Attachments").strong())
                    .id_salt("settings_attachments")
                    .show(ui, |ui| {
                        ui_style::muted(
                            ui,
                            "Manage vault attachments, paste screenshots and cleanup orphaned files",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Attachments folder:");
                            let mut folder_str =
                                self.settings.attachments_folder.display().to_string();
                            if ui.text_edit_singleline(&mut folder_str).changed() {
                                self.settings.attachments_folder = PathBuf::from(folder_str);
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Inspect Orphaned Attachments").clicked() {
                                match crate::attachments::AttachmentManager::find_orphaned_attachments(
                                    &self.data.notes,
                                    &self.storage_paths.notes_dir,
                                    &self.settings.attachments_folder,
                                ) {
                                    Ok(orphans) => {
                                        self.attachments_orphans = orphans;
                                        self.attachments_inspected = true;
                                    }
                                    Err(error) => {
                                        self.attachments_orphans.clear();
                                        self.attachments_inspected = false;
                                        self.storage_message = Some(error);
                                    }
                                }
                            }
                        });

                        if self.attachments_inspected {
                            if self.attachments_orphans.is_empty() {
                                ui_style::muted(ui, "✓ No orphaned attachment files found.");
                            } else {
                                ui.label(format!(
                                    "Found {} unreferenced attachment(s):",
                                    self.attachments_orphans.len()
                                ));
                                let mut delete_orphan_path = None;
                                egui::ScrollArea::vertical()
                                    .max_height(140.0)
                                    .show(ui, |ui| {
                                        for orphan in &self.attachments_orphans {
                                            let file_name = orphan
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy();
                                            ui.horizontal(|ui| {
                                                ui.label(format!("• {file_name}"));
                                                if ui.small_button("Delete").clicked() {
                                                    delete_orphan_path = Some(orphan.clone());
                                                }
                                            });
                                        }
                                    });

                                if let Some(path_to_del) = delete_orphan_path {
                                    match std::fs::remove_file(&path_to_del) {
                                        Ok(()) => {
                                            self.attachments_orphans.retain(|p| p != &path_to_del);
                                            self.storage_message =
                                                Some("Deleted orphaned attachment".to_owned());
                                        }
                                        Err(error) => {
                                            self.storage_message = Some(format!(
                                                "Failed to delete orphaned attachment: {error}"
                                            ));
                                        }
                                    }
                                }

                                if ui.button("Clean Up All Orphans").clicked() {
                                    let mut count = 0;
                                    for orphan in &self.attachments_orphans {
                                        if std::fs::remove_file(orphan).is_ok() {
                                            count += 1;
                                        }
                                    }
                                    self.attachments_orphans.retain(|path| path.exists());
                                    self.storage_message =
                                        Some(format!("Deleted {count} orphaned file(s)"));
                                }
                            }
                        }
                    });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Storage & Cache").strong())
                    .id_salt("settings_storage")
                    .show(ui, |ui| {
                        ui_style::muted(ui, "Vault path, backups, cache directory and export");
                        ui.text_edit_singleline(&mut self.vault_path_buffer);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Switch vault now").clicked() {
                                self.switch_vault_from_buffer();
                            }
                            if ui.button("Open vault folder").clicked()
                                && let Err(error) = platform::open_folder(&self.settings.vault_path)
                            {
                                self.storage_message =
                                    Some(format!("Could not open vault: {error}"));
                            }
                        });
                        let autosave_before = (
                            self.settings.autosave_enabled,
                            self.settings.autosave_interval_seconds,
                        );
                        ui.checkbox(
                            &mut self.settings.autosave_enabled,
                            "Automatically save edited notes",
                        );
                        ui.add_enabled_ui(self.settings.autosave_enabled, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Autosave interval:");
                                egui::ComboBox::from_id_salt("autosave_interval")
                                    .selected_text(autosave_interval_label(
                                        self.settings.autosave_interval_seconds,
                                    ))
                                    .show_ui(ui, |ui| {
                                        for seconds in AUTOSAVE_INTERVAL_OPTIONS {
                                            ui.selectable_value(
                                                &mut self.settings.autosave_interval_seconds,
                                                seconds,
                                                autosave_interval_label(seconds),
                                            );
                                        }
                                    });
                            });
                        });
                        ui_style::muted(
                            ui,
                            "Ctrl+S and saving on application exit remain available when autosave is disabled.",
                        );
                        if autosave_before
                            != (
                                self.settings.autosave_enabled,
                                self.settings.autosave_interval_seconds,
                            )
                        {
                            if self.settings.autosave_enabled && !self.dirty_note_ids.is_empty() {
                                self.dirty_since = Some(Instant::now());
                            }
                            self.save_settings();
                        }
                        ui.separator();
                        ui.checkbox(
                            &mut self.settings.backups_enabled,
                            "Create backups before overwriting notes",
                        );
                        ui.add(
                            egui::Slider::new(&mut self.settings.backup_limit, 1..=100)
                                .text("Backups per note"),
                        );
                        ui.label("Import one Markdown file into the selected folder");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.import_path_buffer)
                                    .hint_text(operating_system.markdown_path_hint())
                                    .desired_width((ui.available_width() - 72.0).max(80.0)),
                            );
                            if ui.button("Import").clicked() {
                                self.import_markdown_from_buffer();
                            }
                        });
                        ui.label("Export the vault to a timestamped folder");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.export_path_buffer)
                                    .hint_text(operating_system.export_path_hint())
                                    .desired_width((ui.available_width() - 72.0).max(80.0)),
                            );
                            if ui.button("Export").clicked() {
                                self.export_vault_from_buffer();
                            }
                        });
                    });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Shortcuts").strong())
                    .id_salt("settings_shortcuts")
                    .show(ui, |ui| {
                        ui_style::muted(ui, "Keyboard shortcuts");
                        shortcut_field(ui, "New note", &mut self.settings.shortcuts.new_note);
                        shortcut_field(ui, "Search", &mut self.settings.shortcuts.search);
                        shortcut_field(ui, "Graph", &mut self.settings.shortcuts.graph);
                        shortcut_field(
                            ui,
                            "Graph overlay",
                            &mut self.settings.shortcuts.graph_overlay,
                        );
                        shortcut_field(ui, "Save", &mut self.settings.shortcuts.save);

                        ui.add_space(6.0);
                        let hotkey_enabled_before = self.settings.global_quick_capture_enabled;
                        let hotkey_str_before = self.settings.global_quick_capture_shortcut.clone();

                        ui.checkbox(
                            &mut self.settings.global_quick_capture_enabled,
                            "System-wide Quick Capture hotkey (works when minimized)",
                        );
                        shortcut_field(
                            ui,
                            "Global Quick Capture",
                            &mut self.settings.global_quick_capture_shortcut,
                        );

                        if hotkey_enabled_before != self.settings.global_quick_capture_enabled
                            || hotkey_str_before != self.settings.global_quick_capture_shortcut
                        {
                            self.hotkey_manager.update_shortcut(
                                self.settings.global_quick_capture_enabled,
                                &self.settings.global_quick_capture_shortcut,
                            );
                        }
                    });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("Privacy & Analytics").strong(),
                )
                .id_salt("settings_privacy_analytics")
                .show(ui, |ui| {
                    ui_style::muted(
                        ui,
                        "Optional usage counters with no note contents or personal profile",
                    );
                    let mut enabled = self.settings.analytics.enabled();
                    if ui
                        .checkbox(&mut enabled, "Share privacy-preserving usage analytics")
                        .changed()
                    {
                        self.set_analytics_enabled(enabled);
                    }

                    ui.horizontal_wrapped(|ui| {
                        if ui.button("View exactly what is sent").clicked() {
                            self.analytics_details_open = true;
                        }
                        if self.settings.analytics.enabled()
                            && ui.button("Disable and delete my analytics data").clicked()
                        {
                            self.set_analytics_enabled(false);
                        }
                    });

                    if self.settings.analytics.pending_deletion_id.is_some() {
                        ui_style::muted(
                            ui,
                            "Deletion is pending and will retry automatically when online.",
                        );
                    }
                    if let Some(status) = &self.analytics_status {
                        ui_style::muted(ui, status);
                    }
                });
            });

            ui.add_space(10.0);
            if ui
                .add(
                    egui::Button::new("Save settings")
                        .fill(ui.visuals().selection.bg_fill)
                        .min_size(egui::vec2(130.0, 34.0)),
                )
                .clicked()
            {
                self.save_settings();
                self.storage_message = Some("Settings saved".to_owned());
            }
        });
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
            ui_style::screen_title(ui, "Notes");
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

        let selected_folder_text = if self.settings.selected_folder.as_os_str().is_empty() {
            "Notes (root)".to_owned()
        } else {
            format!("Notes / {}", self.settings.selected_folder.display())
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
        let mut outgoing_links_by_id = HashMap::new();
        for note in &self.data.notes {
            let links = self
                .link_index
                .links_for(note.id)
                .map(|l| {
                    let mut targets = l.unresolved.clone();
                    for &target_id in &l.outgoing {
                        if let Some(target_note) =
                            self.data.notes.iter().find(|n| n.id == target_id)
                        {
                            targets.push(target_note.title.clone());
                        }
                    }
                    targets
                })
                .unwrap_or_default();
            outgoing_links_by_id.insert(note.id, links);
        }

        let actions = {
            let tree = folders::FolderTree::build(
                &self.data.notes,
                &self.storage_paths.notes_dir,
                &self.folder_paths,
            );
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
                        &outgoing_links_by_id,
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
                            &outgoing_links_by_id,
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
        let title_by_id = self
            .data
            .notes
            .iter()
            .map(|note| (note.id, note.title.clone()))
            .collect::<HashMap<_, _>>();
        let total = links.outgoing.len() + links.backlinks.len() + links.unresolved.len();
        if total == 0 {
            return;
        }

        let mut open_note = None;
        let mut create_note = None;
        ui.collapsing(
            format!(
                "Connections  ·  {} links  ·  {} backlinks  ·  {} missing",
                links.outgoing.len(),
                links.backlinks.len(),
                links.unresolved.len()
            ),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for id in &links.outgoing {
                        let title = title_by_id.get(id).map_or("Untitled", String::as_str);
                        if ui.button(format!("→ {title}")).clicked() {
                            open_note = Some(*id);
                        }
                    }
                    for id in &links.backlinks {
                        let title = title_by_id.get(id).map_or("Untitled", String::as_str);
                        if ui.button(format!("← {title}")).clicked() {
                            open_note = Some(*id);
                        }
                    }
                    for target in &links.unresolved {
                        if ui.button(format!("+ {target}")).clicked() {
                            create_note = Some(target.clone());
                        }
                    }
                });
            },
        );
        if let Some(id) = open_note {
            self.open_note(id);
        } else if let Some(target) = create_note {
            self.create_note_from_link(&target);
        }
    }

    fn show_note_properties(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let mut changed = false;
        ui.collapsing("Properties: tags and aliases", |ui| {
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_tag)
                        .hint_text("new tag")
                        .desired_width(120.0),
                );
                if ui.button("Add tag").clicked() {
                    let tag = self.new_tag.trim();
                    if !tag.is_empty() && !note.tags.iter().any(|existing| existing == tag) {
                        note.tags.push(tag.to_owned());
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
            self.mark_note_dirty(note_id);
        }
    }

    fn is_daily_note(&self, note: &Note) -> bool {
        let rel = note
            .file_path
            .strip_prefix(&self.storage_paths.notes_dir)
            .unwrap_or(&note.file_path);
        let in_daily_folder = rel.starts_with(&self.settings.daily_notes_folder);
        let title_is_date =
            chrono::NaiveDate::parse_from_str(note.title.trim(), "%Y-%m-%d").is_ok();
        in_daily_folder || title_is_date
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
            let width = ui.available_width();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui_style::compact_action(ui, Icon::Folder, "New folder").clicked() {
                    self.show_new_folder_input = !self.show_new_folder_input;
                    self.new_folder_name.clear();
                }
                if ui_style::compact_action(ui, Icon::Add, "New note").clicked() {
                    create_note_clicked = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if width >= 120.0 {
                        ui.label(
                            egui::RichText::new("EXPLORER")
                                .strong()
                                .size(12.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
            });
        });

        ui.add_space(4.0);

        // Quick Access Rows
        let today_date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
        if ui
            .button(format!("📅 Today ({today_date_str})"))
            .on_hover_text("Open or create today's daily note (Alt+D)")
            .clicked()
        {
            self.open_or_create_daily_note(0);
            self.activate_view(AppView::Editor);
        }
        if ui
            .button("⚡ Quick Capture")
            .on_hover_text("Open Quick Capture overlay (Ctrl+Shift+C)")
            .clicked()
        {
            self.quick_capture_state.open();
        }
        if ui
            .button("📝 Templates...")
            .on_hover_text("Select template for a new note")
            .clicked()
        {
            self.template_selector_open = true;
            self.template_selector_for_new_note = true;
        }

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
                egui::RichText::new(format!("⭐ Pinned ({})", pinned_notes.len())).strong(),
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
                egui::RichText::new(format!("🕒 Recent ({})", recent_notes_list.len())).strong(),
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
                egui::RichText::new(format!("🏷️ Tags ({})", self.tag_index.all_tags().len()))
                    .strong(),
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
        let mut outgoing_links_by_id = HashMap::new();
        for note in &self.data.notes {
            let links = self.link_index.links_for(note.id).map_or(Vec::new(), |l| {
                l.outgoing
                    .iter()
                    .filter_map(|&id| {
                        self.data
                            .notes
                            .iter()
                            .find(|n| n.id == id)
                            .map(|n| n.title.clone())
                    })
                    .collect()
            });
            outgoing_links_by_id.insert(note.id, links);
        }
        let notes_by_id: HashMap<Uuid, &Note> = self.data.notes.iter().map(|n| (n.id, n)).collect();
        let folder_tree = folders::FolderTree::build(
            &self.data.notes,
            &self.storage_paths.notes_dir,
            &self.folder_paths,
        );

        let mut actions = NotesListActions::default();

        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        for note in &self.data.notes {
            for tag in &note.tags {
                *tag_counts.entry(tag.clone()).or_default() += 1;
            }
        }
        let mut sorted_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut clicked_tag_toggle = None;

        let available_tree_height = (ui.available_height() - 36.0).max(100.0);
        egui::ScrollArea::vertical()
            .max_height(available_tree_height)
            .show(ui, |ui| {
                show_folder_node(
                    ui,
                    &folder_tree.root,
                    &notes_by_id,
                    &parsed_query,
                    &outgoing_links_by_id,
                    self.data.selected_note_id,
                    &self.settings.selected_folder,
                    &self.settings.collapsed_folders,
                    self.settings.note_sort,
                    &mut actions,
                );

                if !sorted_tags.is_empty() {
                    ui.add_space(8.0);
                    ui.collapsing(
                        egui::RichText::new(format!("Tags ({})", sorted_tags.len())).strong(),
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (tag, count) in &sorted_tags {
                                    let is_selected =
                                        self.search_query.contains(&format!("#{tag}"));
                                    let pill =
                                        ui_style::pill_frame(ui, is_selected).show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("#{tag} ({count})"))
                                                    .small()
                                                    .color(if is_selected {
                                                        ui.visuals().hyperlink_color
                                                    } else {
                                                        ui.visuals().text_color()
                                                    }),
                                            )
                                        });
                                    if pill.response.interact(egui::Sense::click()).clicked() {
                                        clicked_tag_toggle = Some((tag.clone(), is_selected));
                                    }
                                }
                            });
                        },
                    );
                }
            });

        if let Some((tag, is_selected)) = clicked_tag_toggle {
            if is_selected {
                self.search_query.clear();
            } else {
                self.search_query = format!("#{tag}");
            }
        }

        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("🗑️ Trash")
                .on_hover_text("Open Trash & Backups")
                .clicked()
            {
                self.activate_view(AppView::Trash);
            }
            if ui
                .button("⚙ Settings")
                .on_hover_text("Open Settings (Ctrl+,)")
                .clicked()
            {
                self.activate_view(AppView::Settings);
            }
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
        let editor_id = ui.make_persistent_id(("markdown_editor", note.id));
        let mut navigate_to_note = None;
        let mut jump_cursor_idx = None;
        let mut create_unresolved_target = None;

        let outgoing_links: Vec<(Uuid, String)> = self
            .link_index
            .links_for(note.id)
            .map(|l| {
                l.outgoing
                    .iter()
                    .filter_map(|&id| {
                        self.data
                            .notes
                            .iter()
                            .find(|n| n.id == id)
                            .map(|n| (id, n.title.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let backlinks: Vec<(Uuid, String)> = self
            .link_index
            .links_for(note.id)
            .map(|l| {
                l.backlinks
                    .iter()
                    .filter_map(|&id| {
                        self.data
                            .notes
                            .iter()
                            .find(|n| n.id == id)
                            .map(|n| (id, n.title.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("INSPECTOR")
                        .strong()
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
            ui.separator();

            // 1. Outline (TOC)
            let outline = markdown::extract_outline(&note.content);
            ui.collapsing(
                egui::RichText::new(format!("Outline ({})", outline.len())).strong(),
                |ui| {
                    if outline.is_empty() {
                        ui.label(
                            egui::RichText::new("No headings found")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else {
                        for item in outline {
                            let indent = (item.level.saturating_sub(1) as f32) * 10.0;
                            ui.horizontal(|ui| {
                                if indent > 0.0 {
                                    ui.add_space(indent);
                                }
                                let text = if item.level == 1 {
                                    egui::RichText::new(&item.title).strong()
                                } else {
                                    egui::RichText::new(&item.title)
                                };
                                if ui.link(text).clicked() {
                                    jump_cursor_idx = Some(item.char_index);
                                }
                            });
                        }
                    }
                },
            );
            ui.add_space(6.0);

            // 2. Connected Notes (Outgoing + Backlinks)
            ui.collapsing(
                egui::RichText::new(format!(
                    "Connected Notes ({})",
                    outgoing_links.len() + backlinks.len()
                ))
                .strong(),
                |ui| {
                    if !outgoing_links.is_empty() {
                        ui.label(
                            egui::RichText::new("Outgoing:")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        for (out_id, out_title) in &outgoing_links {
                            if ui.link(format!("→ {out_title}")).clicked() {
                                navigate_to_note = Some(*out_id);
                            }
                        }
                    }
                    if !backlinks.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Backlinks:")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        for (back_id, back_title) in &backlinks {
                            if ui.link(format!("← {back_title}")).clicked() {
                                navigate_to_note = Some(*back_id);
                            }
                        }
                    }
                },
            );
            ui.add_space(6.0);

            // 3. Properties: Tags and Aliases
            self.show_note_properties(ui, note.id);

            // 4. Unresolved Links in this note & vault
            let unresolved_here: Vec<String> = self
                .link_index
                .links_for(note.id)
                .map(|l| l.unresolved.clone())
                .unwrap_or_default();

            if !unresolved_here.is_empty() {
                ui.add_space(6.0);
                ui.collapsing(
                    egui::RichText::new(format!("Unresolved Links ({})", unresolved_here.len()))
                        .strong(),
                    |ui| {
                        for target in &unresolved_here {
                            ui.horizontal(|ui| {
                                ui.label(format!("• {target}"));
                                if ui.small_button("+ Create").clicked() {
                                    create_unresolved_target = Some(target.clone());
                                }
                            });
                        }
                    },
                );
            }
        });

        if let Some(idx) = jump_cursor_idx {
            markdown::set_cursor_char_index(ui.ctx(), editor_id, idx);
        }
        if let Some(id) = navigate_to_note {
            self.open_note(id);
        }
        if let Some(target) = create_unresolved_target {
            self.create_note_from_link(&target);
        }
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
                    let save_status = if saving {
                        if self.settings.autosave_enabled {
                            "Autosave pending".to_owned()
                        } else {
                            "Unsaved".to_owned()
                        }
                    } else {
                        format!("Saved · {updated}")
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
                        ui.label(
                            egui::RichText::new(msg)
                                .small()
                                .color(ui.visuals().hyperlink_color),
                        );
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
            CommandAction::ToggleLeftSidebar => {
                self.settings.left_sidebar_open = !self.settings.left_sidebar_open;
                self.save_settings();
            }
            CommandAction::ToggleRightInspector => {
                self.settings.right_sidebar_open = !self.settings.right_sidebar_open;
                self.save_settings();
            }
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
                self.settings.editor_font_size = 14.0;
                self.settings.font_size = 14.0;
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
            CommandAction::SwitchVault => self.activate_view(AppView::Settings),
            CommandAction::ScanDiagnostics => {
                self.recovery_tab = RecoveryTab::Diagnostics;
                self.activate_view(AppView::Trash);
            }
            CommandAction::ExportVault => self.activate_view(AppView::Settings),
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

impl eframe::App for WidgetApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut analytics_events = Vec::new();
        self.process_analytics(&ctx);

        while let Some(event) = self.hotkey_manager.try_recv() {
            match event {
                GlobalHotkeyEvent::QuickCapture => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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
        ui_style::apply_theme(&ctx, dark_theme, accent, self.settings.ui_font_size);

        // Frameless window edge and corner resizing
        ui_style::show_window_resize_handles(&ctx);

        let window_width = ui.available_width();

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
            i.modifiers.ctrl && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
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
                self.settings.editor_font_size = (self.settings.editor_font_size + 0.5).min(32.0);
            } else {
                self.settings.editor_font_size = (self.settings.editor_font_size - 0.5).max(10.0);
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
            self.settings.editor_font_size = 14.0;
            self.settings.font_size = 14.0;
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
        if daily_note_shortcut {
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
            } else if input.key_pressed(egui::Key::Num5) || input.key_pressed(egui::Key::Comma) {
                Some(AppView::Settings)
            } else {
                None
            }
        });

        if let Some(view) = direct_view {
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

        if create_note_shortcut {
            self.create_note();
        }
        if open_search_shortcut && !command_palette_shortcut {
            self.view = AppView::NotesList;
            self.focus_search = true;
            self.focus_editor = false;
            self.pending_delete_id = None;
        }
        if toggle_graph_shortcut {
            let target = if self.view == AppView::Graph {
                AppView::Editor
            } else {
                AppView::Graph
            };
            self.activate_view(target);
        }

        // QoL Fix: Disable Ctrl+Shift+G when in compact mode!
        if toggle_overlay_shortcut {
            if window_width >= ui_style::COMPACT_WIDTH {
                self.graph_overlay_open = !self.graph_overlay_open;
                if self.graph_overlay_open {
                    analytics_events.push(AnalyticsFeature::GraphOpened);
                }
            } else {
                self.storage_message = Some("Graph overlay is disabled in compact mode".to_owned());
            }
        }

        let navigate_back_shortcut =
            ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft));
        let navigate_forward_shortcut =
            ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight));
        if navigate_back_shortcut {
            self.navigate_back();
        }
        if navigate_forward_shortcut {
            self.navigate_forward();
        }

        if save_shortcut && !self.external_conflict {
            self.flush_dirty_notes();
        }
        if escape_pressed {
            if self.command_palette_state.is_open {
                self.command_palette_state.close();
            } else if self.quick_capture_state.is_open {
                self.quick_capture_state.close();
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

        let effective_left_open = self.settings.left_sidebar_open
            && window_width >= ui_style::NAV_BREAKPOINT
            && !self.settings.zen_mode;
        let effective_right_open = self.settings.right_sidebar_open
            && window_width >= ui_style::WIDE_BREAKPOINT
            && !self.settings.zen_mode
            && self.view == AppView::Editor;
        let effective_status_bar = self.settings.show_status_bar && !self.settings.zen_mode;

        // Top Navigation & Control Bar (Layer 1)
        egui::Panel::top("top_panel")
            .exact_size(ui_style::TOP_BAR_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(ui_style::layer1_color(dark_theme))
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut close_clicked = false;
                    let mut max_clicked = false;
                    let mut min_clicked = false;
                    let mut settings_clicked = false;
                    let mut zen_clicked = false;
                    let mut capture_clicked = false;
                    let mut right_inspector_clicked = false;
                    let mut search_clicked = false;

                    // 1. Right-side window controls & tools allocated FIRST:
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui_style::icon_button(ui, Icon::Close, false, "Close Lilo").clicked() {
                            close_clicked = true;
                        }
                        if window_width >= 360.0
                            && ui_style::icon_button(
                                ui,
                                Icon::Maximize,
                                false,
                                "Maximize / restore",
                            )
                            .clicked()
                        {
                            max_clicked = true;
                        }
                        if window_width >= 360.0
                            && ui_style::icon_button(ui, Icon::Minimize, false, "Minimize")
                                .clicked()
                        {
                            min_clicked = true;
                        }

                        // Always accessible Settings icon button!
                        if ui_style::icon_button(
                            ui,
                            Icon::Settings,
                            self.view == AppView::Settings,
                            "Settings (Ctrl+,)",
                        )
                        .clicked()
                        {
                            settings_clicked = true;
                        }

                        // Zen mode button (wider windows)
                        if window_width >= 540.0
                            && ui_style::icon_button(
                                ui,
                                Icon::Editor,
                                self.settings.zen_mode,
                                "Zen / Writing Mode (F11)",
                            )
                            .clicked()
                        {
                            zen_clicked = true;
                        }

                        // Quick capture button (wider windows)
                        if window_width >= 420.0
                            && ui_style::icon_button(
                                ui,
                                Icon::Daily,
                                false,
                                "Quick capture (Ctrl+Shift+C)",
                            )
                            .clicked()
                        {
                            capture_clicked = true;
                        }

                        // Right Context Inspector toggle
                        if self.view == AppView::Editor
                            && window_width >= ui_style::WIDE_BREAKPOINT
                            && ui_style::icon_button(
                                ui,
                                Icon::SidebarRight,
                                effective_right_open,
                                "Toggle context inspector (Ctrl+I)",
                            )
                            .clicked()
                        {
                            right_inspector_clicked = true;
                        }

                        // Search pill button on wider screens
                        if window_width >= 620.0 {
                            let search_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new("🔍 Search (Ctrl+P)")
                                        .small()
                                        .color(ui.visuals().weak_text_color()),
                                )
                                .fill(ui.visuals().widgets.inactive.bg_fill)
                                .corner_radius(egui::CornerRadius::same(6)),
                            );
                            if search_btn.clicked() {
                                search_clicked = true;
                            }
                        }

                        // 2. Left side & Drag Area in the remaining space (Never overlaps!):
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            // Left Explorer toggle
                            if (effective_left_open || window_width >= ui_style::NAV_BREAKPOINT)
                                && ui_style::icon_button(
                                    ui,
                                    Icon::SidebarLeft,
                                    effective_left_open,
                                    "Toggle explorer (Ctrl+B)",
                                )
                                .clicked()
                            {
                                self.settings.left_sidebar_open = !self.settings.left_sidebar_open;
                                self.save_settings();
                            }

                            if ui.available_width() >= 40.0 {
                                ui.label(egui::RichText::new("Lilo").strong().size(14.0));
                            }

                            // History Back / Forward navigation buttons
                            let back_enabled = !self.history_back.is_empty();
                            let forward_enabled = !self.history_forward.is_empty();
                            let mut nav_back = false;
                            let mut nav_forward = false;

                            ui.add_enabled_ui(back_enabled, |ui| {
                                if ui
                                    .small_button("◀")
                                    .on_hover_text("Navigate Back (Alt+Left)")
                                    .clicked()
                                {
                                    nav_back = true;
                                }
                            });
                            ui.add_enabled_ui(forward_enabled, |ui| {
                                if ui
                                    .small_button("▶")
                                    .on_hover_text("Navigate Forward (Alt+Right)")
                                    .clicked()
                                {
                                    nav_forward = true;
                                }
                            });

                            if nav_back {
                                self.navigate_back();
                            }
                            if nav_forward {
                                self.navigate_forward();
                            }

                            // Quick navigation icons if left sidebar is hidden
                            if !effective_left_open {
                                let rem_w = ui.available_width();
                                if rem_w >= 260.0 {
                                    for (v, icon, label) in [
                                        (AppView::Editor, Icon::Editor, "Editor"),
                                        (AppView::NotesList, Icon::Notes, "Notes"),
                                        (AppView::Graph, Icon::Graph, "Graph"),
                                        (AppView::Trash, Icon::Trash, "Trash"),
                                    ] {
                                        if ui_style::navigation_button(
                                            ui,
                                            icon,
                                            self.view == v,
                                            label,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.activate_view(v);
                                        }
                                    }
                                } else if rem_w >= 140.0 {
                                    for (v, icon, label) in [
                                        (AppView::Editor, Icon::Editor, "Editor"),
                                        (AppView::NotesList, Icon::Notes, "Notes"),
                                        (AppView::Graph, Icon::Graph, "Graph"),
                                        (AppView::Trash, Icon::Trash, "Trash"),
                                    ] {
                                        if ui_style::icon_button(ui, icon, self.view == v, label)
                                            .clicked()
                                        {
                                            self.activate_view(v);
                                        }
                                    }
                                } else if rem_w >= 60.0 {
                                    for (v, icon, label) in [
                                        (AppView::Editor, Icon::Editor, "Editor"),
                                        (AppView::NotesList, Icon::Notes, "Notes"),
                                    ] {
                                        if ui_style::icon_button(ui, icon, self.view == v, label)
                                            .clicked()
                                        {
                                            self.activate_view(v);
                                        }
                                    }
                                }
                            }

                            // Remaining space becomes Draggable Title Region (Handles Window Dragging & Double Click)
                            let drag_width = ui.available_width().max(0.0);
                            if drag_width > 8.0 {
                                let drag_area = ui.allocate_response(
                                    egui::vec2(drag_width, 28.0),
                                    egui::Sense::click_and_drag(),
                                );
                                let rect = drag_area.rect;
                                if drag_width > 80.0 {
                                    let note_title =
                                        self.data.selected_note().map_or("Lilo", |n| {
                                            if n.title.trim().is_empty() {
                                                "Untitled"
                                            } else {
                                                n.title.as_str()
                                            }
                                        });
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        note_title,
                                        egui::FontId::proportional(13.0),
                                        ui.visuals().weak_text_color(),
                                    );
                                }
                                if drag_area.drag_started() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                }
                                if drag_area.double_clicked() {
                                    let maximized = ctx
                                        .input(|input| input.viewport().maximized.unwrap_or(false));
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                        !maximized,
                                    ));
                                }
                            }
                        });
                    });

                    if close_clicked {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if max_clicked {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if min_clicked {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if settings_clicked {
                        self.activate_view(AppView::Settings);
                    }
                    if zen_clicked {
                        self.settings.zen_mode = !self.settings.zen_mode;
                        if self.settings.zen_mode {
                            analytics_events.push(AnalyticsFeature::ZenModeEnabled);
                        }
                        self.save_settings();
                    }
                    if capture_clicked {
                        self.quick_capture_state.open();
                    }
                    if right_inspector_clicked {
                        self.settings.right_sidebar_open = !self.settings.right_sidebar_open;
                        self.save_settings();
                    }
                    if search_clicked {
                        self.command_palette_state.open();
                    }
                });
            });

        // Left Panel (Navigator / Explorer - Layer 1)
        if effective_left_open {
            egui::Panel::left("left_explorer_panel")
                .default_size(self.settings.sidebar_width)
                .min_size(200.0)
                .max_size(420.0)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(ui_style::layer1_color(dark_theme))
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(ui, |ui| {
                    self.show_left_explorer(ui);
                });
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
                        self.external_conflict = false;
                        self.flush_dirty_notes();
                    }
                });
            });
        }

        // Central Panel (Layer 0 Background with Elevated Note Card)
        let canvas_fill = if dark_theme {
            egui::Color32::from_rgb(15, 17, 24)
        } else {
            egui::Color32::from_rgb(242, 244, 248)
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(canvas_fill)
                    .inner_margin(egui::Margin::symmetric(
                        if window_width < 650.0 { 8 } else { 16 },
                        if window_width < 650.0 { 8 } else { 12 },
                    )),
            )
            .show(ui, |ui| {
                match self.view {
                    AppView::Editor => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let total_available_w = ui.available_width();
                                let total_available_h = ui.available_height();
                                let is_wide = total_available_w > 820.0;
                                let sheet_width = if is_wide {
                                    self.settings.editor_max_width.min(total_available_w - 32.0)
                                } else {
                                    total_available_w
                                };

                                ui.vertical_centered(|ui| {
                                    ui.set_max_width(sheet_width);
                                    ui.set_min_width(sheet_width);

                                    let card_fill = if dark_theme {
                                        egui::Color32::from_rgb(22, 25, 35)
                                    } else {
                                        egui::Color32::WHITE
                                    };
                                    let card_stroke = egui::Stroke::new(
                                        1.0,
                                        if dark_theme {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                                        } else {
                                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 18)
                                        },
                                    );

                                    egui::Frame::new()
                                        .fill(card_fill)
                                        .stroke(card_stroke)
                                        .corner_radius(egui::CornerRadius::same(10))
                                        .inner_margin(egui::Margin::symmetric(
                                            if is_wide { 24 } else { 14 },
                                            16,
                                        ))
                                        .show(ui, |ui| {
                                            ui.set_min_height((total_available_h - 16.0).max(300.0));
                                            ui.with_layout(
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    let mut changed_note_id = None;
                                                    let mut note_name_changed = false;
                                                    let mut note_content_changed = false;
                                                    let mut activated_link_target = None;

                                                    let is_daily = self
                                                        .data
                                                        .selected_note()
                                                        .is_some_and(|n| self.is_daily_note(n));
                                                    let mut daily_nav_target = None;

                                                    if let Some(note) = self.data.selected_note_mut() {
                                                        // Daily Notes Navigation Banner
                                                        if is_daily {
                                                            let rel = note
                                                                .file_path
                                                                .strip_prefix(&self.storage_paths.notes_dir)
                                                                .unwrap_or(&note.file_path);
                                                            let current_date = LocalDateService::parse_date_from_note(&note.title, rel)
                                                                .unwrap_or_else(LocalDateService::today);
                                                            let prev_date = LocalDateService::prev_day(current_date);
                                                            let next_date = LocalDateService::next_day(current_date);
                                                            let is_today = LocalDateService::is_today(current_date);
                                                            let display_date_str = LocalDateService::format_daily_display(current_date);

                                                            let compact_navigation = ui.available_width() < 440.0;
                                                            let date_label = |ui: &mut egui::Ui| {
                                                                ui.label(
                                                                    egui::RichText::new(format!("📅 {display_date_str}"))
                                                                        .strong()
                                                                        .color(ui.visuals().hyperlink_color),
                                                                );
                                                                if is_today {
                                                                    ui.label(
                                                                        egui::RichText::new("[Today]")
                                                                            .small()
                                                                            .strong()
                                                                            .color(ui.visuals().selection.bg_fill),
                                                                    );
                                                                }
                                                            };

                                                            if compact_navigation {
                                                                ui.horizontal_wrapped(date_label);
                                                                ui.horizontal_wrapped(|ui| {
                                                                    if ui
                                                                        .button("← Prev")
                                                                        .on_hover_text(format!("Open daily note for {}", prev_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(prev_date);
                                                                    }
                                                                    if !is_today
                                                                        && ui
                                                                            .button("Today")
                                                                            .on_hover_text("Jump to today's daily note")
                                                                            .clicked()
                                                                    {
                                                                        daily_nav_target = Some(LocalDateService::today());
                                                                    }
                                                                    if ui
                                                                        .button("Next →")
                                                                        .on_hover_text(format!("Open daily note for {}", next_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(next_date);
                                                                    }
                                                                });
                                                            } else {
                                                                ui.horizontal(|ui| {
                                                                    if ui
                                                                        .button("← Prev Day")
                                                                        .on_hover_text(format!("Open daily note for {}", prev_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(prev_date);
                                                                    }
                                                                    date_label(ui);
                                                                    if !is_today
                                                                        && ui
                                                                            .button("Today")
                                                                            .on_hover_text("Jump to today's daily note")
                                                                            .clicked()
                                                                    {
                                                                        daily_nav_target = Some(LocalDateService::today());
                                                                    }
                                                                    if ui
                                                                        .button("Next Day →")
                                                                        .on_hover_text(format!("Open daily note for {}", next_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(next_date);
                                                                    }
                                                                });
                                                            }
                                                            ui.add_space(6.0);
                                                        }

                                                        // Title Box (clean, frameless, natural)
                                                        let title_response = ui.add(
                                                            egui::TextEdit::singleline(&mut note.title)
                                                                .font(egui::FontId::proportional(22.0))
                                                                .frame(egui::Frame::NONE)
                                                                .desired_width(f32::INFINITY)
                                                                .hint_text("Note title..."),
                                                        );

                                                        ui.add_space(6.0);
                                                        ui.separator();
                                                        ui.add_space(6.0);

                                                        let editor_id = ui.make_persistent_id(("markdown_editor", note.id));

                                                        if let Some((target_note_id, char_idx)) = self.pending_cursor_char_index
                                                            && target_note_id == note.id
                                                        {
                                                            markdown::set_cursor_char_index(ui.ctx(), editor_id, char_idx);
                                                            self.pending_cursor_char_index = None;
                                                        }

                                                        let mut markdown_command = None;
                                                        let editor_focused = ui.memory(|memory| memory.has_focus(editor_id));
                                                        if editor_focused {
                                                            if ui.input(|input| {
                                                                input.modifiers.command && input.key_pressed(egui::Key::B)
                                                            }) {
                                                                markdown_command = Some(markdown::MarkdownCommand::Bold);
                                                            } else if ui.input(|input| {
                                                                input.modifiers.command && input.key_pressed(egui::Key::I)
                                                            }) {
                                                                markdown_command = Some(markdown::MarkdownCommand::Italic);
                                                            }
                                                        }

                                                        let mut command_changed = false;

                                                        if editor_focused
                                                            && ui.input(|input| {
                                                                input.modifiers.is_none() && input.key_pressed(egui::Key::Enter)
                                                            })
                                                            && markdown::continue_list_at_cursor(
                                                                ui.ctx(),
                                                                editor_id,
                                                                &mut note.content,
                                                            )
                                                        {
                                                            ui.input_mut(|input| {
                                                                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                                                            });
                                                            command_changed = true;
                                                        }

                                                        // Drag & Drop Attachment Files
                                                        let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());
                                                        if !dropped_files.is_empty() {
                                                            for dropped in dropped_files {
                                                                let path = dropped.path();
                                                                if !path.as_os_str().is_empty()
                                                                    && let Ok(rel_link) = crate::attachments::AttachmentManager::import_file(
                                                                        path,
                                                                        &self.storage_paths.notes_dir,
                                                                        &self.settings.attachments_folder,
                                                                    )
                                                                {
                                                                    let is_img = matches!(
                                                                        path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(),
                                                                        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
                                                                    );
                                                                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                                                                    markdown::insert_attachment_link(
                                                                        ui.ctx(),
                                                                        editor_id,
                                                                        &mut note.content,
                                                                        &name,
                                                                        &rel_link,
                                                                        is_img,
                                                                    );
                                                                    command_changed = true;
                                                                    analytics_events.push(
                                                                        AnalyticsFeature::AttachmentAdded,
                                                                    );
                                                                    self.storage_message = Some(format!("Imported attachment: {rel_link}"));
                                                                }
                                                            }
                                                        }

                                                        // Clipboard Paste (Ctrl+V / Shift+Insert / Paste event) Screenshot & Image Ingestion
                                                        let mut trigger_paste_image = ui.ctx().input(|input| {
                                                            let ctrl_or_cmd = input.modifiers.ctrl || input.modifiers.command;
                                                            (ctrl_or_cmd && (
                                                                input.key_pressed(egui::Key::V)
                                                                || input.events.iter().any(|e| match e {
                                                                    egui::Event::Key { key, physical_key, pressed: true, .. } => {
                                                                        *key == egui::Key::V || *physical_key == Some(egui::Key::V)
                                                                    }
                                                                    egui::Event::Paste(_) => true,
                                                                    egui::Event::Text(t) => t.contains('\x16') || t.contains('v') || t.contains('V') || t.contains('м') || t.contains('М'),
                                                                    _ => false,
                                                                })
                                                            )) || (input.modifiers.shift && input.key_pressed(egui::Key::Insert))
                                                            || input.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                                                        });

                                                        // Formatting & Quick Tools toolbar
                                                        let compact_tools = ui.available_width() < 440.0;
                                                        ui.horizontal_wrapped(|ui| {
                                                            if ui
                                                                .small_button(if compact_tools {
                                                                    "📷 Paste"
                                                                } else {
                                                                    "📷 Paste Image"
                                                                })
                                                                .on_hover_text("Paste screenshot or image from clipboard (Ctrl+V)")
                                                                .clicked()
                                                            {
                                                                trigger_paste_image = true;
                                                            }
                                                            if ui.small_button("B").on_hover_text("Bold (Ctrl+B)").clicked() {
                                                                markdown_command = Some(markdown::MarkdownCommand::Bold);
                                                            }
                                                            if ui.small_button("I").on_hover_text("Italic (Ctrl+I)").clicked() {
                                                                markdown_command = Some(markdown::MarkdownCommand::Italic);
                                                            }
                                                            if compact_tools {
                                                                ui.menu_button("More ⋯", |ui| {
                                                                    if ui.button("`code`  Inline code").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::InlineCode);
                                                                        ui.close();
                                                                    }
                                                                    if ui.button("[[link]]  Wiki-link").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::WikiLink);
                                                                        ui.close();
                                                                    }
                                                                    if ui.button("☑  Task checkbox").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::Task);
                                                                        ui.close();
                                                                    }
                                                                });
                                                            } else {
                                                                if ui.small_button("`code`").on_hover_text("Inline Code").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::InlineCode);
                                                                }
                                                                if ui.small_button("[[link]]").on_hover_text("Wiki-Link").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::WikiLink);
                                                                }
                                                                if ui.small_button("☑ Task").on_hover_text("Task checkbox").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::Task);
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(4.0);

                                                        if markdown_command.is_some_and(|command| {
                                                            markdown::apply_command(
                                                                ui.ctx(),
                                                                editor_id,
                                                                &mut note.content,
                                                                command,
                                                            )
                                                        }) {
                                                            command_changed = true;
                                                            analytics_events.push(
                                                                AnalyticsFeature::MarkdownFormattingUsed,
                                                            );
                                                        }

                                                        if trigger_paste_image {
                                                            match crate::attachments::AttachmentManager::try_save_clipboard_image(
                                                                &self.storage_paths.notes_dir,
                                                                &self.settings.attachments_folder,
                                                            ) {
                                                                Ok(Some(rel_link)) => {
                                                                    ui.ctx().input_mut(|i| {
                                                                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::V);
                                                                        i.consume_key(egui::Modifiers::CTRL, egui::Key::V);
                                                                        i.consume_key(egui::Modifiers::SHIFT, egui::Key::Insert);
                                                                    });
                                                                    markdown::insert_attachment_link(
                                                                        ui.ctx(),
                                                                        editor_id,
                                                                        &mut note.content,
                                                                        "Pasted Image",
                                                                        &rel_link,
                                                                        true,
                                                                    );
                                                                    command_changed = true;
                                                                    analytics_events.push(
                                                                        AnalyticsFeature::AttachmentAdded,
                                                                    );
                                                                    self.storage_message = Some(format!("Pasted image saved: {rel_link}"));
                                                                }
                                                                Ok(None) => {
                                                                    // Normal text in clipboard - let show_editor TextEdit handle it
                                                                }
                                                                Err(err) => {
                                                                    self.storage_message = Some(format!("Clipboard: {err}"));
                                                                }
                                                            }
                                                        }

                                                        let editor_output = markdown::show_editor(
                                                             ui,
                                                             &mut note.content,
                                                             editor_id,
                                                             self.settings.editor_font_size,
                                                         );

                                                        let hovered_character = markdown::hovered_character(ui, &editor_output);
                                                        let checkbox_toggled =
                                                            hovered_character.is_some_and(|character_index| {
                                                                editor_output.response.clicked()
                                                                    && !ui.input(|input| input.modifiers.command)
                                                                    && markdown::toggle_checkbox_at_character(
                                                                        &mut note.content,
                                                                        character_index,
                                                                    )
                                                            });

                                                        if let Some(character_index) = hovered_character
                                                            && let Some(wiki_link) =
                                                                links::wiki_link_at_character(&note.content, character_index)
                                                        {
                                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                                            editor_output
                                                                .response
                                                                .response
                                                                .clone()
                                                                .on_hover_text(format!(
                                                                    "Double-click or Ctrl+Click to open [[{}]]",
                                                                    wiki_link.target
                                                                ));

                                                            let double_clicked = editor_output.response.double_clicked();
                                                            let command_clicked = editor_output.response.clicked()
                                                                && ui.input(|input| input.modifiers.command);
                                                            if double_clicked || command_clicked {
                                                                activated_link_target = Some(wiki_link.target);
                                                            }
                                                        }

                                                        let content_response = editor_output.response;

                                                        if self.focus_editor {
                                                            content_response.request_focus();
                                                            self.focus_editor = false;
                                                        }

                                                        note_name_changed = title_response.changed();
                                                        note_content_changed =
                                                            content_response.changed() || checkbox_toggled || command_changed;
                                                        if note_name_changed || note_content_changed {
                                                            note.mark_as_updated();
                                                            changed_note_id = Some(note.id);
                                                        }

                                                        // Render Embedded Attached Images right in note view
                                                        let embedded_attachments = crate::attachments::extract_attachments_from_markdown(&note.content);
                                                        if !embedded_attachments.is_empty() {
                                                            let valid_images: Vec<(PathBuf, String)> = embedded_attachments
                                                                .iter()
                                                                .filter_map(|att| {
                                                                    let full = self.storage_paths.notes_dir.join(att);
                                                                    if full.exists() {
                                                                        let name = full.file_name().unwrap_or_default().to_string_lossy().to_string();
                                                                        Some((full, name))
                                                                    } else {
                                                                        None
                                                                    }
                                                                })
                                                                .collect();

                                                            if !valid_images.is_empty() {
                                                                ui.add_space(16.0);
                                                                ui.separator();
                                                                ui.add_space(8.0);
                                                                ui.label(
                                                                    egui::RichText::new("📷 Attached Images")
                                                                        .strong()
                                                                        .color(ui.visuals().weak_text_color()),
                                                                );
                                                                ui.add_space(8.0);

                                                                for (full_path, name) in valid_images {
                                                                    if let Ok(bytes) = std::fs::read(&full_path) {
                                                                        ui.group(|ui| {
                                                                            ui.label(egui::RichText::new(format!("🖼 {name}")).small());
                                                                            ui.add_space(4.0);
                                                                            let max_w = (ui.available_width() - 20.0).max(100.0);
                                                                            let uri_key = format!("bytes://{}", name);
                                                                            let img = egui::Image::from_bytes(uri_key, bytes)
                                                                                .max_width(max_w)
                                                                                .corner_radius(egui::CornerRadius::same(6));
                                                                            ui.add(img);
                                                                        });
                                                                        ui.add_space(8.0);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        ui.add_space(32.0);
                                                        ui.vertical_centered(|ui| {
                                                            ui.label(
                                                                egui::RichText::new("📝 Lilo")
                                                                    .size(26.0)
                                                                    .strong()
                                                                    .color(ui.visuals().hyperlink_color),
                                                            );
                                                            ui.add_space(4.0);
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    "Local-first Markdown notes & daily workflow",
                                                                )
                                                                .small()
                                                                .color(ui.visuals().weak_text_color()),
                                                            );
                                                            ui.add_space(16.0);

                                                            ui.horizontal_wrapped(|ui| {
                                                                if ui.button("✨ New Note (Ctrl+N)").clicked() {
                                                                    self.create_note();
                                                                }
                                                                if ui.button("📅 Today's Note (Alt+D)").clicked() {
                                                                    self.open_or_create_daily_note(0);
                                                                }
                                                                if ui.button("⚡ Quick Capture (Ctrl+Shift+C)").clicked() {
                                                                    self.quick_capture_state.open();
                                                                }
                                                                if ui.button("📝 Templates...").clicked() {
                                                                    self.template_selector_open = true;
                                                                    self.template_selector_for_new_note = true;
                                                                }
                                                                if ui.button("🔍 Commands (Ctrl+P)").clicked() {
                                                                    self.command_palette_state.open();
                                                                }
                                                            });

                                                            let recent_preview: Vec<(Uuid, String)> = self
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
                                                                        (n.id, title)
                                                                    })
                                                                })
                                                                .take(5)
                                                                .collect();

                                                            if !recent_preview.is_empty() {
                                                                ui.add_space(20.0);
                                                                ui.separator();
                                                                ui.add_space(8.0);
                                                                ui.label(
                                                                    egui::RichText::new("Recent Notes")
                                                                        .strong()
                                                                        .color(ui.visuals().weak_text_color()),
                                                                );
                                                                ui.add_space(6.0);
                                                                for (r_id, r_title) in recent_preview {
                                                                    if ui.button(format!("📄 {r_title}")).clicked() {
                                                                        self.open_note(r_id);
                                                                    }
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(32.0);
                                                    }

                                                    if let Some(id) = changed_note_id {
                                                        if note_name_changed {
                                                            if let Some(note) = self.data.notes.iter().find(|note| note.id == id) {
                                                                let new_title = note.title.clone();
                                                                let old_title = self.note_titles_snapshot.get(&id).cloned().unwrap_or_default();
                                                                if !old_title.is_empty() && old_title != new_title {
                                                                    let modified_note_ids = links::rename_note_references(&mut self.data.notes, &old_title, &new_title);
                                                                    let ref_count = modified_note_ids.len();
                                                                    self.dirty_note_ids.extend(modified_note_ids);
                                                                    if ref_count > 0 {
                                                                        self.storage_message = Some(format!("Updated {ref_count} note reference(s) across vault"));
                                                                    }
                                                                    self.note_titles_snapshot.insert(id, new_title);
                                                                }
                                                            }
                                                            self.pending_title_rename_ids.insert(id);
                                                            self.link_index = LinkIndex::build(
                                                                &self.data.notes,
                                                                &self.storage_paths.notes_dir,
                                                            );
                                                            self.tag_index = TagIndex::build(&self.data.notes);
                                                            self.pending_index_note_ids.clear();
                                                            self.last_index_change = None;
                                                        } else if note_content_changed {
                                                            self.schedule_note_index_refresh(id);
                                                        }
                                                        self.mark_note_dirty(id);
                                                    }

                                                    if let Some(target) = activated_link_target {
                                                        analytics_events.push(
                                                            AnalyticsFeature::WikiLinkOpened,
                                                        );
                                                        match self.link_index.resolve_target(&target) {
                                                            LinkResolution::Resolved(id) => self.open_note(id),
                                                            LinkResolution::Missing => self.create_note_from_link(&target),
                                                            LinkResolution::Ambiguous => {
                                                                self.storage_message = Some(format!(
                                                                    "Cannot open [[{target}]]: more than one note has this name"
                                                                ));
                                                            }
                                                        }
                                                    }

                                                    if let Some(target_date) = daily_nav_target {
                                                        self.open_or_create_daily_note_for_date(target_date);
                                                    }
                                                },
                                            );
                                        });
                                });
                            });
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
                }
            });

        if self.note_details_open {
            let mut open = true;
            egui::Window::new("Note details")
                .id(egui::Id::new("note_details"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(360.0, 260.0))
                .show(&ctx, |ui| {
                    if let Some(note_id) = self.data.selected_note_id {
                        self.show_note_connections(ui, note_id);
                        self.show_note_properties(ui, note_id);
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
            let templates = TemplateEngine::list_templates(
                &self.storage_paths.notes_dir,
                &self.settings.templates_folder,
            );

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

        // Tag Rename Dialog
        if self.tag_rename_dialog_open {
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
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Rename Tag").clicked() {
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
            &self.settings.recent_commands,
            &self.settings.recent_note_ids,
            &self.data.notes,
            &self.storage_paths.notes_dir,
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

        self.process_deferred_index_refresh(&ctx);
        self.process_autosave(&ctx);
        self.sync_external_changes(&ctx);
        self.process_analytics(&ctx);
    }

    fn on_exit(&mut self) {
        self.flush_dirty_notes();
        self.save_settings();
    }
}
