use super::*;

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

impl WidgetApp {
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

    pub(super) fn show_notes_list(&mut self, ui: &mut egui::Ui) {
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
            self.handle_command_action(CommandAction::MoveToFolder);
        }
        if create_note_clicked {
            self.handle_command_action(CommandAction::NewNote);
        }
    }

    pub(super) fn is_daily_note(&self, note: &Note) -> bool {
        let rel = note
            .file_path
            .strip_prefix(&self.storage_paths.notes_dir)
            .unwrap_or(&note.file_path);
        LocalDateService::parse_date_from_note(&note.title, rel).is_some()
    }

    pub(super) fn render_tag_node(
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

    pub(super) fn show_left_explorer(&mut self, ui: &mut egui::Ui) {
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
            self.handle_command_action(CommandAction::OpenTodayNote);
        }
        if ui_style::navigation_button(ui, Icon::Inbox, false, "Quick Capture", true)
            .on_hover_text("Quick Capture (Ctrl+Shift+C)")
            .clicked()
        {
            self.handle_command_action(CommandAction::QuickCapture);
        }
        for (view, action, icon, title) in [
            (
                AppView::NotesList,
                CommandAction::ViewNotesList,
                Icon::Notes,
                "All Notes",
            ),
            (
                AppView::Graph,
                CommandAction::ViewGraph,
                Icon::Graph,
                "Graph",
            ),
        ] {
            if ui_style::navigation_button(ui, icon, self.view == view, title, true).clicked() {
                self.handle_command_action(action);
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
            self.handle_command_action(CommandAction::NewNote);
        }
        if submit_new_folder {
            self.create_folder_from_input();
        }
        self.apply_notes_list_actions(actions);
    }
}
