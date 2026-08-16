use crate::folders;
use crate::graph;
use crate::links::{self, LinkIndex, LinkResolution};
use crate::markdown;
use crate::platform;
use crate::storage::{
    self, AppData, AppSettings, Note, NoteSort, StoragePaths, ThemeChoice, ToolbarPlacement,
};
use crate::ui_style::{self, Icon};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const EXTERNAL_SYNC_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) struct WidgetApp {
    data: AppData,
    settings: AppSettings,
    storage_paths: StoragePaths,
    pending_delete_id: Option<Uuid>,
    dirty_note_ids: HashSet<Uuid>,
    pending_title_rename_ids: HashSet<Uuid>,
    last_edit_at: Option<Instant>,
    storage_message: Option<String>,
    link_index: LinkIndex,
    folder_paths: Vec<PathBuf>,
    graph_state: graph::GraphState,

    view: AppView,
    search_query: String,
    normalized_search_query: String,
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
    normalized_query: &str,
) -> bool {
    normalized_query.is_empty()
        || folder.note_ids.iter().any(|id| {
            notes
                .get(id)
                .is_some_and(|note| note.search_text.contains(normalized_query))
        })
        || folder
            .folders
            .iter()
            .any(|child| folder_has_visible_notes(child, notes, normalized_query))
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
    normalized_query: &str,
    selected_note_id: Option<Uuid>,
    selected_folder: &Path,
    collapsed_folders: &[PathBuf],
    note_sort: NoteSort,
    actions: &mut NotesListActions,
) {
    if !folder_has_visible_notes(folder, notes, normalized_query) {
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
                if ui.button("Delete empty folder").clicked() {
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
                normalized_query,
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
            if !note.pinned
                && (normalized_query.is_empty() || note.search_text.contains(normalized_query))
            {
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

impl WidgetApp {
    pub(crate) fn new() -> Self {
        let loaded = storage::load_storage().expect("Failed to initialize Markdown storage");
        let link_index = LinkIndex::build(&loaded.data.notes, &loaded.paths.notes_dir);

        for warning in &loaded.warnings {
            eprintln!("Storage warning: {warning}");
        }

        let storage_message = if loaded.migrated_notes > 0 {
            Some(format!(
                "Migrated {} note(s) to Markdown",
                loaded.migrated_notes
            ))
        } else {
            loaded.warnings.first().cloned()
        };

        let vault_path_buffer = loaded.settings.vault_path.display().to_string();
        let graph_state = graph::GraphState::restore(&loaded.settings.graph_node_offsets);
        let vault_snapshot = storage::vault_snapshot(&loaded.paths.notes_dir).unwrap_or_default();
        let diagnostics = loaded.warnings.clone();
        let operating_system = platform::OperatingSystem::current();
        if operating_system.supports_autostart() {
            let _ = platform::set_autostart(loaded.settings.autostart);
        }
        Self {
            data: loaded.data,
            settings: loaded.settings,
            storage_paths: loaded.paths,
            pending_delete_id: None,
            dirty_note_ids: HashSet::new(),
            pending_title_rename_ids: HashSet::new(),
            last_edit_at: None,
            storage_message,
            link_index,
            folder_paths: loaded.folder_paths,
            graph_state,
            view: AppView::Editor,
            search_query: String::new(),
            normalized_search_query: String::new(),
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

    fn effective_toolbar_placement(&self, width: f32) -> EffectiveToolbarPlacement {
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
        self.view = view;
        self.pending_delete_id = None;
        self.focus_search = view == AppView::NotesList;
        self.focus_editor = view == AppView::Editor;
    }

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

    fn show_toolbar_menu(&mut self, ui: &mut egui::Ui, include_hidden_views: bool) {
        let before = (
            self.settings.toolbar_placement,
            self.settings.toolbar_expanded,
            self.settings.floating_toolbar_vertical,
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
        );
        if before != after {
            self.save_settings();
        }
        if let Some(view) = requested_view {
            self.activate_view(view);
        }
    }

    fn save_note_now(&mut self, id: Uuid) -> bool {
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
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                true
            }
            Some(Err(error)) => {
                self.storage_message = Some(format!("Failed to save note: {error}"));
                false
            }
            None => false,
        }
    }

    fn mark_note_dirty(&mut self, id: Uuid) {
        self.dirty_note_ids.insert(id);
        self.last_edit_at = Some(Instant::now());
    }

    fn flush_dirty_notes(&mut self) {
        let ids: Vec<Uuid> = self.dirty_note_ids.iter().copied().collect();
        for id in ids {
            if self.save_note_now(id) {
                if self.pending_title_rename_ids.remove(&id) {
                    let rename_result = self
                        .data
                        .notes
                        .iter_mut()
                        .find(|note| note.id == id)
                        .map(storage::rename_note_file);
                    if let Some(Err(error)) = rename_result {
                        self.storage_message = Some(format!("Failed to rename note file: {error}"));
                    } else {
                        self.vault_snapshot =
                            storage::vault_snapshot(&self.storage_paths.notes_dir)
                                .unwrap_or_default();
                    }
                }
                self.dirty_note_ids.remove(&id);
            }
        }

        if self.dirty_note_ids.is_empty() {
            self.last_edit_at = None;
        }
        self.save_settings();
    }

    fn save_after_debounce(&mut self, ctx: &egui::Context) {
        if self.external_conflict {
            return;
        }
        let Some(last_edit_at) = self.last_edit_at else {
            return;
        };
        let elapsed = last_edit_at.elapsed();

        if elapsed >= SAVE_DEBOUNCE {
            self.flush_dirty_notes();
        } else {
            ctx.request_repaint_after(SAVE_DEBOUNCE - elapsed);
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
        self.save_note_now(id);
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
        if let Some(note) = self.data.notes.iter_mut().find(|note| note.id == id) {
            note.pinned = !note.pinned;
            note.mark_as_updated();
            self.mark_note_dirty(id);
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
        match storage::delete_empty_folder(&self.storage_paths.notes_dir, path) {
            Ok(()) => {
                self.folder_paths.retain(|folder| folder != path);
                self.settings
                    .collapsed_folders
                    .retain(|folder| folder != path);
                if self.settings.selected_folder == path {
                    self.settings.selected_folder = PathBuf::new();
                }
                self.vault_snapshot =
                    storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
                self.save_settings();
            }
            Err(error) => self.storage_message = Some(format!("Failed to delete folder: {error}")),
        }
    }

    fn reload_vault(&mut self, reason: &str) {
        match storage::reload_notes(&self.storage_paths) {
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
            self.external_changed_paths = current
                .symmetric_difference(&self.vault_snapshot)
                .map(|(path, _)| path.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            self.external_changed_paths.sort();
            if self.dirty_note_ids.is_empty() {
                self.reload_vault("Reloaded changes from disk");
            } else {
                self.external_conflict = true;
                self.storage_message = Some(
                    "Files changed outside Lilo. Save or discard local edits, then reload."
                        .to_owned(),
                );
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
                        Ok(_) => self.reload_vault("Note restored"),
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
            self.diagnostics = storage::vault_diagnostics(&self.storage_paths)
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
                self.storage_message = Some(format!("Vault exported to {}", path.display()))
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
                egui::CollapsingHeader::new(egui::RichText::new("Appearance").strong())
                    .id_salt("settings_appearance")
                    .show(ui, |ui| {
                        ui_style::muted(ui, "Theme, font size, accent colour and navigation");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.settings.theme,
                                ThemeChoice::Dark,
                                "Dark",
                            );
                            ui.selectable_value(
                                &mut self.settings.theme,
                                ThemeChoice::Light,
                                "Light",
                            );
                            ui.selectable_value(
                                &mut self.settings.theme,
                                ThemeChoice::System,
                                "System",
                            );
                        });
                        ui.add(
                            egui::Slider::new(&mut self.settings.font_size, 12.0..=22.0)
                                .text("Editor font"),
                        );
                        ui.horizontal(|ui| {
                            ui.label("Accent");
                            ui.color_edit_button_srgb(&mut self.settings.accent_rgb);
                        });
                        if ui
                            .checkbox(&mut self.settings.always_on_top, "Always on top")
                            .changed()
                        {
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
                            self.storage_message =
                                Some(format!("Autostart update failed: {error}"));
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
                            ui_style::muted(
                                ui,
                                "Drag the grip to move it. Drop near an edge to dock.",
                            );
                        }
                    });
            });

            ui.add_space(7.0);
            ui_style::card_frame(ui).show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("Storage").strong())
                    .id_salt("settings_storage")
                    .show(ui, |ui| {
                        ui_style::muted(ui, "Vault path, backups, import and export");
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
                .hint_text("Search notes..."),
        );
        if self.focus_search {
            search_response.request_focus();
            self.focus_search = false;
        }
        if search_response.changed() {
            self.normalized_search_query = self.search_query.trim().to_lowercase();
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
                            if self.normalized_search_query.is_empty()
                                || note.search_text.contains(&self.normalized_search_query)
                            {
                                show_note_row(ui, note, self.data.selected_note_id, &mut actions);
                            }
                        }
                        ui.separator();
                        ui.strong("Folders and recent notes");
                    }
                    if !folder_has_visible_notes(&tree.root, &notes, &self.normalized_search_query)
                    {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label("No notes found");
                        });
                    } else {
                        show_folder_node(
                            ui,
                            &tree.root,
                            &notes,
                            &self.normalized_search_query,
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
            self.delete_folder(&path);
        }

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

        if let Some(id) = self.pending_delete_id {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Move this note to Trash?");
                if ui.button("Yes").clicked() {
                    self.delete_note(id);
                }
                if ui.button("No").clicked() {
                    self.pending_delete_id = None;
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
        if let Some(id) = actions.selected_note_id {
            self.open_note(id);
        }
    }

    fn open_note(&mut self, id: Uuid) {
        if !self.data.notes.iter().any(|note| note.id == id) {
            return;
        }

        self.data.selected_note_id = Some(id);
        self.pending_delete_id = None;
        self.view = AppView::Editor;
        self.focus_search = false;
        self.focus_editor = true;
        self.save_settings();
    }

    fn navigate_note_list(&mut self, direction: isize) {
        let mut notes = self
            .data
            .notes
            .iter()
            .filter(|note| {
                self.normalized_search_query.is_empty()
                    || note.search_text.contains(&self.normalized_search_query)
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

    fn show_editor_footer(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let (outgoing, backlinks, missing) = self
            .link_index
            .links_for(note_id)
            .map(|links| {
                (
                    links.outgoing.len(),
                    links.backlinks.len(),
                    links.unresolved.len(),
                )
            })
            .unwrap_or_default();
        let updated = self
            .data
            .notes
            .iter()
            .find(|note| note.id == note_id)
            .map(|note| note.updated_at.format("%H:%M").to_string())
            .unwrap_or_default();
        let saving = self.dirty_note_ids.contains(&note_id);

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui_style::muted(
                ui,
                format!("Links: {outgoing}  ·  Backlinks: {backlinks}  ·  Missing: {missing}"),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Details").clicked() {
                    self.note_details_open = true;
                }
                ui_style::muted(
                    ui,
                    if saving {
                        "Saving...".to_owned()
                    } else {
                        format!("Saved · {updated}")
                    },
                );
            });
        });
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
}

impl eframe::App for WidgetApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

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
        ui_style::apply_theme(&ctx, dark_theme, accent);

        let create_note_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.new_note);
        let open_search_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.search);
        let toggle_graph_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.graph);
        let toggle_overlay_shortcut =
            shortcut_pressed(&ctx, &self.settings.shortcuts.graph_overlay);
        let save_shortcut = shortcut_pressed(&ctx, &self.settings.shortcuts.save);
        let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
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
            self.view = view;
            self.focus_search = view == AppView::NotesList;
            self.focus_editor = view == AppView::Editor;
            self.pending_delete_id = None;
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
        if open_search_shortcut {
            self.view = AppView::NotesList;
            self.focus_search = true;
            self.focus_editor = false;
            self.pending_delete_id = None;
        }
        if toggle_graph_shortcut {
            self.view = if self.view == AppView::Graph {
                AppView::Editor
            } else {
                AppView::Graph
            };
            self.pending_delete_id = None;
            self.focus_search = false;
            self.focus_editor = self.view == AppView::Editor;
        }
        if toggle_overlay_shortcut {
            self.graph_overlay_open = !self.graph_overlay_open;
        }
        if save_shortcut && !self.external_conflict {
            self.flush_dirty_notes();
        }
        if escape_pressed {
            if self.graph_overlay_open {
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

        let current_title = self
            .data
            .selected_note()
            .map(|note| {
                if note.title.trim().is_empty() {
                    "Untitled"
                } else {
                    note.title.as_str()
                }
            })
            .unwrap_or("NOTES")
            .to_owned();

        let window_width = ui.available_width();
        let toolbar_placement = self.effective_toolbar_placement(window_width);

        egui::Panel::top("title_bar")
            .exact_size(ui_style::TOP_BAR_HEIGHT)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Lilo").strong().size(14.0));

                    if toolbar_placement == EffectiveToolbarPlacement::Top {
                        let very_narrow = window_width < 320.0;
                        for (view, icon, label) in [
                            (AppView::Editor, Icon::Editor, "Editor"),
                            (AppView::NotesList, Icon::Notes, "Notes"),
                            (AppView::Graph, Icon::Graph, "Knowledge graph"),
                            (AppView::Trash, Icon::Trash, "Recovery"),
                            (AppView::Settings, Icon::Settings, "Settings"),
                        ] {
                            if very_narrow && matches!(view, AppView::Trash | AppView::Settings) {
                                continue;
                            }
                            if ui_style::navigation_button(
                                ui,
                                icon,
                                self.view == view,
                                label,
                                false,
                            )
                            .clicked()
                            {
                                self.activate_view(view);
                            }
                        }
                        self.show_toolbar_menu(ui, very_narrow);
                    }

                    // Prevent egui's negative-size panic in narrow windows.
                    let drag_width = (ui.available_width() - 32.0).max(0.0);
                    let drag_area = ui.allocate_response(
                        egui::vec2(drag_width, 30.0),
                        egui::Sense::click_and_drag(),
                    );
                    let react = drag_area.rect;
                    if drag_width > 48.0 {
                        let (position, align) =
                            if toolbar_placement == EffectiveToolbarPlacement::Top {
                                (react.left_center(), egui::Align2::LEFT_CENTER)
                            } else {
                                (react.center(), egui::Align2::CENTER_CENTER)
                            };
                        ui.painter().text(
                            position,
                            align,
                            current_title,
                            egui::FontId::proportional(14.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                    if drag_area.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag_area.double_clicked() {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if window_width >= 430.0
                        && ui_style::icon_button(ui, Icon::Minimize, false, "Minimize").clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if window_width >= 430.0
                        && ui_style::icon_button(ui, Icon::Maximize, false, "Maximize / restore")
                            .clicked()
                    {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if ui_style::icon_button(ui, Icon::Close, false, "Close Lilo").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        let expanded_navigation =
            self.settings.toolbar_expanded && window_width >= ui_style::EXPANDED_NAV_BREAKPOINT;
        let navigation_width = if expanded_navigation {
            ui_style::NAV_PANEL_WIDTH
        } else {
            ui_style::NAV_RAIL_WIDTH
        };
        match toolbar_placement {
            EffectiveToolbarPlacement::Left => {
                egui::Panel::left("navigation_rail_left")
                    .exact_size(navigation_width)
                    .resizable(false)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            self.show_navigation_buttons(ui, expanded_navigation);
                            ui.add_space(8.0);
                            self.show_toolbar_menu(ui, false);
                        });
                    });
            }
            EffectiveToolbarPlacement::Right => {
                egui::Panel::right("navigation_rail_right")
                    .exact_size(navigation_width)
                    .resizable(false)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            self.show_navigation_buttons(ui, expanded_navigation);
                            ui.add_space(8.0);
                            self.show_toolbar_menu(ui, false);
                        });
                    });
            }
            EffectiveToolbarPlacement::Floating => {
                let position = egui::pos2(
                    self.settings.floating_toolbar_position[0],
                    self.settings.floating_toolbar_position[1],
                );
                let vertical = self.settings.floating_toolbar_vertical;
                let response = egui::Area::new(egui::Id::new("floating_navigation"))
                    .order(egui::Order::Foreground)
                    .movable(true)
                    .constrain(true)
                    .default_pos(position)
                    .show(&ctx, |ui| {
                        egui::Frame::window(ui.style()).show(ui, |ui| {
                            if vertical {
                                ui.vertical_centered(|ui| {
                                    ui.small("::::").on_hover_text("Drag toolbar");
                                    self.show_navigation_buttons(
                                        ui,
                                        self.settings.toolbar_expanded,
                                    );
                                    self.show_toolbar_menu(ui, false);
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.small("::::").on_hover_text("Drag toolbar");
                                    self.show_navigation_buttons(ui, false);
                                    self.show_toolbar_menu(ui, false);
                                });
                            }
                        });
                    });
                let toolbar_rect = response.response.rect;
                self.settings.floating_toolbar_position = [toolbar_rect.min.x, toolbar_rect.min.y];
                if response.response.drag_stopped() {
                    let content = ctx.content_rect();
                    if toolbar_rect.left() <= content.left() + 16.0 {
                        self.settings.toolbar_placement = ToolbarPlacement::Left;
                    } else if toolbar_rect.right() >= content.right() - 16.0 {
                        self.settings.toolbar_placement = ToolbarPlacement::Right;
                    } else if toolbar_rect.top() <= content.top() + ui_style::TOP_BAR_HEIGHT + 12.0
                    {
                        self.settings.toolbar_placement = ToolbarPlacement::Top;
                    }
                    self.save_settings();
                }
            }
            EffectiveToolbarPlacement::Top => {}
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

        if let Some(message) = self.storage_message.clone() {
            let is_error = ["failed", "cannot", "could not", "invalid", "conflict"]
                .iter()
                .any(|word| message.to_lowercase().contains(word));
            egui::Panel::bottom("status_message").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui_style::status_color(ui.visuals(), is_error), message);
                    if ui_style::icon_button(ui, Icon::Close, false, "Dismiss message").clicked() {
                        self.storage_message = None;
                    }
                });
            });
        }

        let content_margin = if window_width >= ui_style::NAV_BREAKPOINT {
            24
        } else {
            ui_style::PANEL_MARGIN
        };
        let content_fill = if dark_theme {
            egui::Color32::from_rgb(23, 26, 33)
        } else {
            egui::Color32::WHITE
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(content_fill)
                    .inner_margin(egui::Margin::same(content_margin)),
            )
            .show(ui, |ui| {
                match self.view {
                    AppView::Editor => {
                        ui.set_max_width(ui.available_width().min(920.0));
                        ui.add_space(4.0);
                        let mut changed_note_id = None;
                        let mut note_name_changed = false;
                        let mut note_content_changed = false;
                        let mut activated_link_target = None;

                        if let Some(note) = self.data.selected_note_mut() {
                            let title_response = ui.add(
                                egui::TextEdit::singleline(&mut note.title)
                                    .font(egui::FontId::proportional(22.0))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Note title..."),
                            );

                            ui.add_space(6.0);

                            // UUID preserves cursor and undo state between frames.
                            let editor_id = ui.make_persistent_id(("markdown_editor", note.id));
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
                                } else if ui.input(|input| {
                                    input.modifiers.command && input.key_pressed(egui::Key::K)
                                }) {
                                    markdown_command = Some(markdown::MarkdownCommand::WikiLink);
                                }
                            }

                            let mut command_changed = markdown_command.is_some_and(|command| {
                                markdown::apply_command(
                                    ui.ctx(),
                                    editor_id,
                                    &mut note.content,
                                    command,
                                )
                            });
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

                            let editor_output = markdown::show_editor(
                                ui,
                                &mut note.content,
                                editor_id,
                                self.settings.font_size,
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
                                        "Ctrl+Click to open [[{}]]",
                                        wiki_link.target
                                    ));

                                let command_clicked = editor_output.response.clicked()
                                    && ui.input(|input| input.modifiers.command);
                                if command_clicked {
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
                        } else {
                            ui.label("No notes yet.");
                            if ui.button("Create note").clicked() {
                                self.create_note();
                            }
                        }

                        if let Some(id) = changed_note_id {
                            if note_name_changed {
                                self.pending_title_rename_ids.insert(id);
                                // Renaming can change link resolution across the vault.
                                self.link_index = LinkIndex::build(
                                    &self.data.notes,
                                    &self.storage_paths.notes_dir,
                                );
                            } else if note_content_changed
                                && let Some(note) =
                                    self.data.notes.iter().find(|note| note.id == id)
                            {
                                // Content edits only require reparsing this note.
                                self.link_index.refresh_note_content(note);
                            }
                            self.mark_note_dirty(id);
                        }

                        if let Some(target) = activated_link_target {
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

                        if let Some(note_id) = self.data.selected_note_id {
                            self.show_editor_footer(ui, note_id);
                        }
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

        self.save_after_debounce(&ctx);
        self.sync_external_changes(&ctx);
    }

    fn on_exit(&mut self) {
        self.flush_dirty_notes();
    }
}
