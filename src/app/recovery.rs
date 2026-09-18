use super::*;

impl WidgetApp {
    pub(super) fn show_trash(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn show_trash_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn show_backups_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn show_diagnostics_tab(&mut self, ui: &mut egui::Ui) {
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
}
