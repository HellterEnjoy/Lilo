use super::*;

impl WidgetApp {
    pub(super) fn activate_view(&mut self, view: AppView) {
        if view == AppView::Graph && self.view != AppView::Graph {
            self.record_analytics(AnalyticsFeature::GraphOpened);
        }
        self.view = view;
        self.pending_delete_id = None;
        self.focus_search = view == AppView::NotesList;
        self.focus_editor = view == AppView::Editor;
    }

    pub(super) fn show_toolbar_menu(&mut self, ui: &mut egui::Ui, include_hidden_views: bool) {
        let mut requested_action = None;
        ui.menu_button("...", |ui| {
            ui.label(format!(
                "Vault: {}",
                storage::vault_name(&self.settings.vault_path)
            ));
            if ui.button("Switch vault...").clicked() {
                requested_action = Some(CommandAction::SwitchVault);
                ui.close();
            }
            ui.separator();
            if let Some(note) = self.data.selected_note() {
                let pinned = note.pinned;
                if ui
                    .button(if pinned { "Unpin note" } else { "Pin note" })
                    .clicked()
                {
                    requested_action = Some(CommandAction::TogglePin);
                    ui.close();
                }
                if ui.button("Save note").clicked() {
                    requested_action = Some(CommandAction::SaveNote);
                    ui.close();
                }
                if ui.button("Move note to Trash…").clicked() {
                    requested_action = Some(CommandAction::DeleteNote);
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("New note").clicked() {
                requested_action = Some(CommandAction::NewNote);
                ui.close();
            }
            if ui.button("Sidebar (Ctrl+Shift+B)").clicked() {
                requested_action = Some(CommandAction::ToggleLeftSidebar);
                ui.close();
            }
            if ui.button("Back in note history").clicked() {
                requested_action = Some(CommandAction::HistoryBack);
                ui.close();
            }
            if ui.button("Forward in note history").clicked() {
                requested_action = Some(CommandAction::HistoryForward);
                ui.close();
            }
            if ui.button("All Notes").clicked() {
                requested_action = Some(CommandAction::ViewNotesList);
                ui.close();
            }
            if ui.button("Graph").clicked() {
                requested_action = Some(CommandAction::ViewGraph);
                ui.close();
            }
            if ui.button("Note context").clicked() {
                requested_action = Some(CommandAction::NoteDetails);
                ui.close();
            }
            if ui.button("New from template…").clicked() {
                requested_action = Some(CommandAction::NewNoteFromTemplate);
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
                    requested_action = Some(CommandAction::ViewTrash);
                    ui.close();
                }
                if ui.button("Settings").clicked() {
                    requested_action = Some(CommandAction::ViewSettings);
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("Search & Commands (Ctrl+K)").clicked() {
                self.command_palette_state.open();
                ui.close();
            }
            if ui.button("Quick Capture (Ctrl+Shift+C)").clicked() {
                requested_action = Some(CommandAction::QuickCapture);
                ui.close();
            }
            if ui.button("Today's Note (Alt+D)").clicked() {
                requested_action = Some(CommandAction::OpenTodayNote);
                ui.close();
            }
            ui.separator();
            let mut zen_mode = self.settings.zen_mode;
            if ui
                .checkbox(&mut zen_mode, "Zen / Writing mode (F11)")
                .changed()
            {
                requested_action = Some(CommandAction::ToggleZenMode);
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
        if let Some(action) = requested_action {
            self.handle_command_action(action);
        }
    }

    pub(super) fn handle_command_action(&mut self, action: CommandAction) {
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
                    self.pending_delete_id = Some(id);
                }
            }
            CommandAction::NoteDetails => self.note_details_open = true,
            CommandAction::ViewEditor => self.activate_view(AppView::Editor),
            CommandAction::ViewNotesList => self.activate_view(AppView::NotesList),
            CommandAction::ViewGraph => self.activate_view(AppView::Graph),
            CommandAction::ViewTrash => self.activate_view(AppView::Trash),
            CommandAction::ViewSettings => self.activate_view(AppView::Settings),
            CommandAction::HistoryBack => self.navigate_back(),
            CommandAction::HistoryForward => self.navigate_forward(),
            CommandAction::ToggleGraphOverlay => {
                self.graph_overlay_open = !self.graph_overlay_open;
                if self.graph_overlay_open {
                    self.record_analytics(AnalyticsFeature::GraphOpened);
                }
            }
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
                self.save_settings();
            }
            CommandAction::ZoomOut => {
                self.settings.editor_font_size = (self.settings.editor_font_size - 1.0).max(10.0);
                self.save_settings();
            }
            CommandAction::ZoomReset => {
                self.settings.editor_font_size = 16.0;
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
            CommandAction::SwitchVault => self.choose_vault_folder(),
            CommandAction::ExportVault => {
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
