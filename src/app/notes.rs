use super::*;

impl WidgetApp {
    pub(super) fn create_note(&mut self) {
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
    pub(super) fn capture_candidate(
        &self,
        submission: &QuickCaptureSubmission,
    ) -> Result<Note, String> {
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
    pub(super) fn verify_disk_versions(&mut self, ids: &[Uuid]) -> bool {
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

    pub(super) fn create_folder_from_input(&mut self) {
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

    pub(super) fn move_selected_note_to_selected_folder(&mut self) {
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

    pub(super) fn toggle_pin(&mut self, id: Uuid) {
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

    pub(super) fn rename_selected_folder(&mut self) {
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

    pub(super) fn delete_folder(&mut self, path: &Path) {
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

    pub(super) fn open_note(&mut self, id: Uuid) {
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

    pub(super) fn navigate_back(&mut self) {
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

    pub(super) fn navigate_forward(&mut self) {
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

    pub(super) fn navigate_note_list(&mut self, direction: isize) {
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

    pub(super) fn create_note_from_link(&mut self, title: &str) {
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

    pub(super) fn delete_note(&mut self, id: Uuid) {
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
}
