use super::*;

impl WidgetApp {
    pub(super) fn save_settings(&mut self) -> bool {
        self.settings.selected_note_id = self.data.selected_note_id;
        if let Err(error) =
            storage::save_settings(&self.storage_paths.settings_path, &self.settings)
        {
            self.storage_message = Some(format!("Failed to save settings: {error}"));
            return false;
        }
        true
    }

    pub(super) fn record_analytics(&mut self, feature: AnalyticsFeature) {
        if self.settings.analytics.record(feature) {
            self.analytics_dirty = true;
        }
    }

    pub(super) fn set_analytics_enabled(&mut self, enabled: bool) {
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

    pub(super) fn process_analytics(&mut self, ctx: &egui::Context) {
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

    pub(super) fn show_analytics_data_description(ui: &mut egui::Ui) {
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

    pub(super) fn show_analytics_consent(&mut self, ctx: &egui::Context) {
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

    pub(super) fn save_note_to_disk(&mut self, id: Uuid) -> bool {
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

    pub(super) fn save_note_now(&mut self, id: Uuid) -> bool {
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

    pub(super) fn refresh_vault_snapshot(&mut self) {
        self.snapshot_epoch = self.snapshot_epoch.wrapping_add(1);
        self.vault_snapshot =
            storage::vault_snapshot(&self.storage_paths.notes_dir).unwrap_or_default();
    }

    pub(super) fn record_saved_versions(&mut self, ids: &[Uuid]) {
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

    pub(super) fn mark_note_dirty(&mut self, id: Uuid) {
        self.dirty_note_ids.insert(id);
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    pub(super) fn flush_dirty_notes(&mut self) {
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

    pub(super) fn schedule_note_index_refresh(&mut self, id: Uuid) {
        self.pending_index_note_ids.insert(id);
        self.last_index_change = Some(Instant::now());
    }

    pub(super) fn flush_pending_index_refresh(&mut self) {
        let note_ids = std::mem::take(&mut self.pending_index_note_ids);
        for id in note_ids {
            if let Some(note) = self.data.notes.iter().find(|note| note.id == id) {
                self.link_index.refresh_note_content(note);
            }
        }
        self.tag_index = TagIndex::build(&self.data.notes);
        self.last_index_change = None;
    }

    pub(super) fn process_deferred_index_refresh(&mut self, ctx: &egui::Context) {
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

    pub(super) fn process_autosave(&mut self, ctx: &egui::Context) {
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

    pub(super) fn reload_vault(&mut self, reason: &str) {
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

    pub(super) fn sync_external_changes(&mut self, ctx: &egui::Context) {
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
}
