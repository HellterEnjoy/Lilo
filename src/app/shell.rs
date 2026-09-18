use super::*;

impl WidgetApp {
    pub(super) fn show_compact_header(&mut self, ui: &mut egui::Ui) {
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
                    self.handle_command_action(CommandAction::NewNote);
                }
                if ui_style::icon_button(ui, Icon::Search, false, "Search (Ctrl+K)").clicked() {
                    self.command_palette_state.open();
                }
            });
        });
    }

    pub(super) fn show_bottom_status_bar(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn show_ui(&mut self, ui: &mut egui::Ui) {
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
                    self.handle_command_action(CommandAction::QuickCapture);
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
                self.handle_command_action(CommandAction::ToggleRightInspector);
            }
            if shortcut_pressed(&ctx, "Ctrl+Shift+B") {
                self.handle_command_action(CommandAction::ToggleLeftSidebar);
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
                self.save_settings();
            }

            if zoom_in {
                self.handle_command_action(CommandAction::ZoomIn);
            }
            if zoom_out {
                self.handle_command_action(CommandAction::ZoomOut);
            }
            if zoom_reset {
                self.handle_command_action(CommandAction::ZoomReset);
            }

            if zen_mode_shortcut {
                self.handle_command_action(CommandAction::ToggleZenMode);
            }
            if command_palette_shortcut && !self.quick_capture_state.is_open {
                self.command_palette_state.open();
            }
            if quick_capture_shortcut && !self.command_palette_state.is_open {
                self.handle_command_action(CommandAction::QuickCapture);
            }
            if daily_note_shortcut {
                self.handle_command_action(CommandAction::OpenTodayNote);
            }

            let direct_view = ctx.input(|input| {
                if !input.modifiers.ctrl || input.modifiers.alt || input.modifiers.shift {
                    None
                } else if input.key_pressed(egui::Key::Num1) {
                    Some(CommandAction::ViewEditor)
                } else if input.key_pressed(egui::Key::Num2) {
                    Some(CommandAction::ViewNotesList)
                } else if input.key_pressed(egui::Key::Num3) {
                    Some(CommandAction::ViewGraph)
                } else if input.key_pressed(egui::Key::Num4) {
                    Some(CommandAction::ViewTrash)
                } else if input.key_pressed(egui::Key::Num5) || input.key_pressed(egui::Key::Comma)
                {
                    Some(CommandAction::ViewSettings)
                } else {
                    None
                }
            });

            if let Some(action) = direct_view {
                self.handle_command_action(action);
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
                self.handle_command_action(CommandAction::NewNote);
            }
            if open_search_shortcut && !command_palette_shortcut {
                self.command_palette_state.open();
            }
            if toggle_graph_shortcut {
                let action = if self.view == AppView::Graph {
                    CommandAction::ViewEditor
                } else {
                    CommandAction::ViewGraph
                };
                self.handle_command_action(action);
            }

            if toggle_overlay_shortcut {
                self.handle_command_action(CommandAction::ToggleGraphOverlay);
            }

            let navigate_back_shortcut =
                ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft));
            let navigate_forward_shortcut =
                ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight));
            if navigate_back_shortcut {
                if self.current_daily_note_date().is_some() {
                    self.handle_command_action(CommandAction::OpenPrevDayNote);
                } else {
                    self.handle_command_action(CommandAction::HistoryBack);
                }
            }
            if navigate_forward_shortcut {
                if self.current_daily_note_date().is_some() {
                    self.handle_command_action(CommandAction::OpenNextDayNote);
                } else {
                    self.handle_command_action(CommandAction::HistoryForward);
                }
            }

            if save_shortcut && !self.external_conflict {
                self.handle_command_action(CommandAction::SaveNote);
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
                    self.handle_command_action(CommandAction::ViewEditor);
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
                            for (view, action, icon, label) in [
                                (
                                    AppView::Settings,
                                    CommandAction::ViewSettings,
                                    Icon::Settings,
                                    "Settings",
                                ),
                                (
                                    AppView::Trash,
                                    CommandAction::ViewTrash,
                                    Icon::Trash,
                                    "Trash & Backups",
                                ),
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
                                    self.handle_command_action(action);
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
