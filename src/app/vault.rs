use super::*;

impl WidgetApp {
    pub(super) fn switch_vault_from_buffer(&mut self) {
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

    pub(super) fn show_vault_switcher(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn choose_vault_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose Lilo vault folder");
        if self.settings.vault_path.is_dir() {
            dialog = dialog.set_directory(&self.settings.vault_path);
        }
        if let Some(path) = dialog.pick_folder() {
            self.vault_path_buffer = path.display().to_string();
            self.switch_vault_from_buffer();
        }
    }

    pub(super) fn import_markdown_from_buffer(&mut self) {
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

    pub(super) fn export_vault_from_buffer(&mut self) {
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
}
