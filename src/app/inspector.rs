use super::*;

impl WidgetApp {
    pub(super) fn handle_graph_output(&mut self, output: graph::GraphOutput) -> bool {
        if output.persist_layout {
            self.settings.graph_node_offsets = self.graph_state.persisted_offsets();
            self.save_settings();
        }
        if output.expand {
            self.graph_overlay_open = false;
            self.graph_fullscreen = !self.graph_fullscreen;
            self.activate_view(AppView::Graph);
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

    pub(super) fn show_note_connections(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let Some(links) = self.link_index.links_for(note_id).cloned() else {
            return;
        };
        let mut open_note = None;
        let mut create_note = None;
        for (heading, ids, empty) in [
            (
                "Outgoing links",
                &links.outgoing,
                "No outgoing links. Use [[note title]] to connect a note.",
            ),
            (
                "Backlinks",
                &links.backlinks,
                "No backlinks yet. Links from other notes will appear here.",
            ),
        ] {
            ui.label(egui::RichText::new(format!("{heading} ({})", ids.len())).strong());
            if ids.is_empty() {
                ui_style::muted(ui, empty);
            }
            for id in ids {
                let title = self
                    .data
                    .notes
                    .iter()
                    .find(|note| note.id == *id)
                    .map_or("Untitled", |note| note.title.as_str());
                if ui
                    .add(egui::Button::new(title).frame(false).wrap())
                    .clicked()
                {
                    open_note = Some(*id);
                }
            }
            ui.add_space(12.0);
        }
        if !links.unresolved.is_empty() {
            ui.label(egui::RichText::new("Missing notes").strong());
            for target in &links.unresolved {
                if ui
                    .add(egui::Button::new(format!("+ {target}")).frame(false).wrap())
                    .on_hover_text("Create the linked note")
                    .clicked()
                {
                    create_note = Some(target.clone());
                }
            }
        }
        if let Some(id) = open_note {
            self.open_note(id);
        } else if let Some(target) = create_note {
            self.create_note_from_link(&target);
        }
    }

    pub(super) fn show_note_properties(&mut self, ui: &mut egui::Ui, note_id: Uuid) {
        let mut changed = false;
        egui::Frame::NONE.show(ui, |ui| {
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
                let tag_input = ui.add(
                    egui::TextEdit::singleline(&mut self.new_tag)
                        .hint_text("new tag")
                        .desired_width(120.0),
                );
                let submit_tag = ui.button("Add tag").clicked()
                    || (tag_input.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if submit_tag {
                    let tag = tags::clean_tag(&self.new_tag);
                    if !tag.is_empty()
                        && !note
                            .tags
                            .iter()
                            .any(|existing| tags::clean_tag(existing) == tag)
                    {
                        note.tags.push(tag);
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
            self.tag_index = TagIndex::build(&self.data.notes);
            self.mark_note_dirty(note_id);
        }
    }

    pub(super) fn show_right_inspector(&mut self, ui: &mut egui::Ui, note: &Note) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (index, label) in ["Outline", "Links", "Properties"].iter().enumerate() {
                ui.selectable_value(&mut self.inspector_tab, index, *label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("inspector_content")
            .show(ui, |ui| match self.inspector_tab {
                0 => {
                    let outline = markdown::extract_outline(&note.content);
                    if outline.is_empty() {
                        ui_style::muted(ui, "Add a Markdown heading to build your outline.");
                    }
                    for item in outline {
                        ui.horizontal(|ui| {
                            ui.add_space(item.level.saturating_sub(1) as f32 * 12.0);
                            if ui.selectable_label(false, &item.title).clicked() {
                                self.pending_cursor_char_index = Some((note.id, item.char_index));
                                self.focus_editor = true;
                            }
                        });
                    }
                }
                1 => {
                    self.show_note_connections(ui, note.id);
                    ui.add_space(12.0);
                    if ui_style::navigation_button(
                        ui,
                        Icon::Graph,
                        false,
                        "Open contextual graph",
                        true,
                    )
                    .clicked()
                    {
                        self.graph_overlay_open = true;
                    }
                }
                _ => {
                    self.show_note_properties(ui, note.id);
                    ui.separator();
                    ui_style::muted(
                        ui,
                        format!("Created {}", note.created_at.format("%d %b %Y, %H:%M")),
                    );
                    ui_style::muted(
                        ui,
                        format!("Modified {}", note.updated_at.format("%d %b %Y, %H:%M")),
                    );
                    let (words, chars) = markdown::count_words_and_chars(&note.content);
                    ui_style::muted(ui, format!("{words} words · {chars} characters"));
                }
            });
    }
}
