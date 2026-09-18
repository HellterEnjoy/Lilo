use super::*;
impl WidgetApp {
    pub(super) fn cached_templates(&mut self) -> Vec<crate::templates::TemplateEntry> {
        if self
            .template_cache
            .as_ref()
            .is_none_or(|(root, folder, _)| {
                root != &self.storage_paths.notes_dir || folder != &self.settings.templates_folder
            })
        {
            self.template_cache = Some((
                self.storage_paths.notes_dir.clone(),
                self.settings.templates_folder.clone(),
                TemplateEngine::list_templates(
                    &self.storage_paths.notes_dir,
                    &self.settings.templates_folder,
                ),
            ));
        }
        self.template_cache
            .as_ref()
            .map(|(_, _, entries)| entries.clone())
            .unwrap_or_default()
    }
    pub(super) fn show_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let operating_system = platform::OperatingSystem::current();

        ui_style::screen_title(ui, "Settings");
        ui.add_space(8.0);
        let categories = [
            (0, "Appearance"),
            (6, "Editor"),
            (1, "Daily Notes"),
            (8, "Templates"),
            (3, "Files & Storage"),
            (2, "Attachments"),
            (4, "Hotkeys"),
            (5, "Privacy"),
            (7, "About"),
        ];
        ui.spacing_mut().text_edit_width = ui.available_width().min(220.0);
        if ui.available_width() >= 640.0 {
            egui::Panel::left("settings_categories")
                .exact_size(164.0)
                .show(ui, |ui| {
                    for &(index, label) in &categories {
                        ui.selectable_value(&mut self.settings_section, index, label);
                    }
                });
        } else {
            egui::ComboBox::from_id_salt("settings_category")
                .selected_text(
                    categories
                        .iter()
                        .find(|(id, _)| *id == self.settings_section)
                        .map_or("Appearance", |(_, label)| *label),
                )
                .show_ui(ui, |ui| {
                    for &(index, label) in &categories {
                        ui.selectable_value(&mut self.settings_section, index, label);
                    }
                });
        }
        egui::ScrollArea::vertical().id_salt(("settings_content", self.settings_section)).show(ui, |ui| {
            ui.set_max_width(720.0);
            if self.settings_section == 6 {
                ui.heading("Editor");
                ui.add(egui::Slider::new(&mut self.settings.editor_font_size, 12.0..=32.0).text("Font size"));
                ui.add(egui::Slider::new(&mut self.settings.editor_max_width, 600.0..=1600.0).text("Maximum line width"));
                ui_style::muted(ui, "The editor uses the available space up to this width. Ctrl + / − adjusts text size.");
                ui.checkbox(&mut self.settings.show_status_bar, "Show save status and word count");
            }
            if self.settings_section == 8 {
                ui.heading("Templates");
                ui.label("Templates folder");
                let mut folder = self.settings.templates_folder.display().to_string();
                if ui.text_edit_singleline(&mut folder).changed() { self.settings.templates_folder = PathBuf::from(folder); }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open folder").clicked() {
                        match storage::ensure_note_folder(&self.storage_paths.notes_dir, &self.settings.templates_folder) {
                            Ok(path) => { if let Err(error) = platform::open_folder(&path) { self.storage_message = Some(error.to_string()); } }
                            Err(error) => self.storage_message = Some(error.to_string()),
                        }
                    }
                    if ui.button("Refresh templates").clicked() { self.template_cache = None; }
                });
                ui_style::muted(ui, "Templates are Markdown files. Supported variables: {{title}}, {{date}}, {{time}}, {{datetime}}, {{yesterday}}, {{tomorrow}}, {{cursor}}.");
                let templates = self.cached_templates();
                if templates.is_empty() { ui.label("No templates yet. Add a Markdown file to the templates folder."); }
                for template in templates {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(&template.name);
                        if ui.button("New note").clicked() { self.create_note_from_template(&template.name); }
                        if ui.add_enabled(self.data.selected_note_id.is_some(), egui::Button::new("Insert")).clicked() { self.insert_template_into_active_note(&template.name); }
                    });
                }
            }
            if self.settings_section == 7 {
                ui.heading("Lilo");
                ui.label("Quick thoughts. Deeper connections.");
                ui_style::muted(ui, concat!("Version ", env!("CARGO_PKG_VERSION")));
                ui.label("Your notes are Markdown files stored in your chosen folder.");
                ui.label("Built with Rust and egui for Windows and Linux.");
            }

            if self.settings_section == 0 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Appearance");
                ui.scope(|ui| {
                    ui_style::muted(ui, "Theme, typography, accent colour and layout");
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.settings.theme, ThemeChoice::Dark, "Dark");
                        ui.selectable_value(&mut self.settings.theme, ThemeChoice::Light, "Light");
                        ui.selectable_value(
                            &mut self.settings.theme,
                            ThemeChoice::System,
                            "System",
                        );
                    });
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
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Density");
                        ui.selectable_value(&mut self.settings.compact_density, false, "Comfortable");
                        ui.selectable_value(&mut self.settings.compact_density, true, "Compact");
                    });
                    ui.checkbox(&mut self.settings.left_sidebar_open, "Show sidebar");
                    ui.checkbox(&mut self.settings.right_sidebar_open, "Show context inspector on wide windows");
                    ui.horizontal_wrapped(|ui| {
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
                            operating_system.supports_autostart() && std::env::var_os("LILO_DATA_DIR").is_none(),
                            egui::Checkbox::new(
                                &mut self.settings.autostart,
                                operating_system.autostart_label(),
                            ),
                        )
                        .on_disabled_hover_text(format!(
                            "Autostart is unavailable in portable mode or on {}",
                            operating_system.name()
                        ));
                    if autostart_response.changed()
                        && let Err(error) = platform::set_autostart(self.settings.autostart)
                    {
                        self.settings.autostart = autostart_before;
                        self.storage_message = Some(format!("Autostart update failed: {error}"));
                    }
                });
            });
            }

            ui.add_space(7.0);
            if self.settings_section == 1 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Daily Notes");
                ui.scope(|ui| {
                    ui_style::muted(
                        ui,
                        "Configuration for daily workflow, templates and quick capture",
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Daily notes folder:");
                        let mut folder_str = self.settings.daily_notes_folder.display().to_string();
                        if ui.text_edit_singleline(&mut folder_str).changed() {
                            self.settings.daily_notes_folder = PathBuf::from(folder_str);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Daily note date format:");
                        ui.text_edit_singleline(&mut self.settings.daily_note_format);
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Default daily template:");
                        let available_templates = self.cached_templates();
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
                    ui.horizontal_wrapped(|ui| {
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
                        ui.horizontal_wrapped(|ui| {
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
            }

            ui.add_space(7.0);
            if self.settings_section == 2 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Attachments");
                ui.scope(|ui| {
                        ui_style::muted(
                            ui,
                            "Manage vault attachments, paste screenshots and cleanup orphaned files",
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Attachments folder:");
                            let mut folder_str =
                                self.settings.attachments_folder.display().to_string();
                            if ui.text_edit_singleline(&mut folder_str).changed() {
                                self.settings.attachments_folder = PathBuf::from(folder_str);
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
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
                                            ui.horizontal_wrapped(|ui| {
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
            }

            ui.add_space(7.0);
            if self.settings_section == 3 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Files & Storage");
                ui.scope(|ui| {
                        ui_style::muted(
                            ui,
                            "Choose the master folder whose Markdown files and subfolders form this vault",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.vault_path_buffer)
                                .desired_width(f32::INFINITY),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Choose folder...").clicked() {
                                self.choose_vault_folder();
                            }
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
                        ui_style::muted(
                            ui,
                            match self.settings.vault_layout {
                                storage::VaultLayout::Root => {
                                    "Current layout: notes live directly in this master folder; recovery data stays in .lilo."
                                }
                                storage::VaultLayout::LegacyNotesDirectory => {
                                    "Compatibility layout: this older vault keeps notes in its existing Notes folder. No files are moved automatically."
                                }
                            },
                        );
                        let autosave_before = (
                            self.settings.autosave_enabled,
                            self.settings.autosave_interval_seconds,
                        );
                        ui.checkbox(
                            &mut self.settings.autosave_enabled,
                            "Automatically save edited notes",
                        );
                        ui.add_enabled_ui(self.settings.autosave_enabled, |ui| {
                            ui.horizontal_wrapped(|ui| {
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
                        ui.horizontal_wrapped(|ui| {
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
                        ui.horizontal_wrapped(|ui| {
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
            }

            ui.add_space(7.0);
            if self.settings_section == 4 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Hotkeys");
                ui.scope(|ui| {
                        ui_style::muted(ui, "Editable bindings use Ctrl, Shift, Alt and a letter, digit or function key. Save settings to persist changes.");
                        egui::Grid::new("fixed_shortcuts").striped(true).show(ui, |ui| {
                            for (action, binding) in [("Search & Commands", "Ctrl+K"), ("Quick Capture", "Ctrl+Shift+C"), ("Today", "Alt+D"), ("Previous / next day or history", "Alt+Left / Right"), ("Sidebar", "Ctrl+Shift+B"), ("Inspector", "Ctrl+Shift+I"), ("Zen mode", "F11"), ("Bold / Italic", "Ctrl+B / I"), ("Capture save / cancel", "Ctrl+Enter / Esc")] {
                                ui.label(action); ui.monospace(binding); ui.end_row();
                            }
                        });
                        ui.separator();
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

                        ui.add_enabled(cfg!(target_os = "windows"), egui::Checkbox::new(&mut self.settings.global_quick_capture_enabled, "System-wide Quick Capture (Windows)"));
                        if !cfg!(target_os = "windows") { ui_style::muted(ui, "On Linux, use Ctrl+Shift+C while Lilo is focused."); }
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
            }

            ui.add_space(7.0);
            if self.settings_section == 5 {
            egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.heading("Privacy");
                ui.scope(|ui| {
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
            }

            ui.add_space(10.0);
            if ui_style::primary_button(ui, "Save settings").clicked() && self.save_settings() {
                self.storage_message = Some("Settings saved".to_owned());
            }
        });
    }
}
