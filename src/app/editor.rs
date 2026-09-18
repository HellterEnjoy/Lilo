use super::*;
impl WidgetApp {
    pub(super) fn show_editor_workspace(
        &mut self,
        ui: &mut egui::Ui,
        window_width: f32,
        canvas_fill: egui::Color32,
        analytics_events: &mut Vec<AnalyticsFeature>,
    ) {
        egui::ScrollArea::vertical().id_salt(("note_scroll", self.data.selected_note_id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let total_available_w = ui.available_width();
                                let total_available_h = ui.available_height();
                                let is_wide = total_available_w > 820.0;
                                let sheet_width = if is_wide {
                                    self.settings.editor_max_width.min(total_available_w)
                                } else {
                                    total_available_w
                                };

                                ui.vertical_centered(|ui| {
                                    ui.set_max_width(sheet_width);
                                    ui.set_min_width(sheet_width);

                                    let card_fill = canvas_fill;
                                    let card_stroke = egui::Stroke::NONE;
                                    egui::Frame::new()
                                        .fill(card_fill)
                                        .stroke(card_stroke)
                                        .corner_radius(egui::CornerRadius::same(10))
                                        .inner_margin(egui::Margin::symmetric(
                                            if is_wide { 24 } else { 14 },
                                            16,
                                        ))
                                        .show(ui, |ui| {
                                            ui.set_min_height((total_available_h - 16.0).max(300.0));
                                            ui.with_layout(
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    let mut changed_note_id = None;
                                                    let mut note_name_changed = false;
                                                    let mut note_title_edit_finished = false;
                                                    let mut note_content_changed = false;
                                                    let mut activated_link_target = None;

                                                    let is_daily = self
                                                        .data
                                                        .selected_note()
                                                        .is_some_and(|n| self.is_daily_note(n));
                                                    let mut daily_nav_target = None;

                                                    if let Some(note) = self.data.selected_note_mut() {
                                                        // Daily Notes Navigation Banner
                                                        if is_daily {
                                                            let rel = note
                                                                .file_path
                                                                .strip_prefix(&self.storage_paths.notes_dir)
                                                                .unwrap_or(&note.file_path);
                                                            let current_date = LocalDateService::parse_date_from_note(&note.title, rel)
                                                                .unwrap_or_else(LocalDateService::today);
                                                            let prev_date = LocalDateService::prev_day(current_date);
                                                            let next_date = LocalDateService::next_day(current_date);
                                                            let is_today = LocalDateService::is_today(current_date);
                                                            let display_date_str = LocalDateService::format_daily_display(current_date);

                                                            let compact_navigation = ui.available_width() < 440.0;
                                                            let date_label = |ui: &mut egui::Ui| {
                                                                ui.label(
                                                                    egui::RichText::new(display_date_str.clone())
                                                                        .strong()
                                                                        .color(ui.visuals().hyperlink_color),
                                                                );
                                                                if is_today {
                                                                    ui.label(
                                                                        egui::RichText::new("[Today]")
                                                                            .small()
                                                                            .strong()
                                                                            .color(ui.visuals().hyperlink_color),
                                                                    );
                                                                }
                                                            };

                                                            if compact_navigation {
                                                                ui.horizontal_wrapped(date_label);
                                                                ui.horizontal_wrapped(|ui| {
                                                                    if ui
                                                                        .button("Previous")
                                                                        .on_hover_text(format!("Open daily note for {}", prev_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(prev_date);
                                                                    }
                                                                    if !is_today
                                                                        && ui
                                                                            .button("Today")
                                                                            .on_hover_text("Jump to today's daily note")
                                                                            .clicked()
                                                                    {
                                                                        daily_nav_target = Some(LocalDateService::today());
                                                                    }
                                                                    if ui
                                                                        .button("Next")
                                                                        .on_hover_text(format!("Open daily note for {}", next_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(next_date);
                                                                    }
                                                                });
                                                            } else {
                                                                ui.horizontal(|ui| {
                                                                    if ui
                                                                        .button("Previous day")
                                                                        .on_hover_text(format!("Open daily note for {}", prev_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(prev_date);
                                                                    }
                                                                    date_label(ui);
                                                                    if !is_today
                                                                        && ui
                                                                            .button("Today")
                                                                            .on_hover_text("Jump to today's daily note")
                                                                            .clicked()
                                                                    {
                                                                        daily_nav_target = Some(LocalDateService::today());
                                                                    }
                                                                    if ui
                                                                        .button("Next day")
                                                                        .on_hover_text(format!("Open daily note for {}", next_date.format("%Y-%m-%d")))
                                                                        .clicked()
                                                                    {
                                                                        daily_nav_target = Some(next_date);
                                                                    }
                                                                });
                                                            }
                                                            ui.add_space(6.0);
                                                        }

                                                        let breadcrumb = note.file_path.strip_prefix(&self.storage_paths.notes_dir).unwrap_or(&note.file_path);
                                                        let folder = breadcrumb.parent().unwrap_or_else(|| Path::new(""));
                                                        let breadcrumb_label = if folder.as_os_str().is_empty() { "All Notes".to_owned() } else { folder.display().to_string().replace('\\', " / ") };
                                                        ui_style::muted(ui, breadcrumb_label);
                                                        // Title Box (clean, frameless, natural)
                                                        let title_response = ui.add(
                                                            egui::TextEdit::singleline(&mut note.title)
                                                                .font(egui::FontId::proportional(if window_width < 600.0 { 24.0 } else { 28.0 }))
                                                                .frame(egui::Frame::NONE)
                                                                .desired_width(f32::INFINITY)
                                                                .hint_text("Note title..."),
                                                        );

                                                        if !note.tags.is_empty() {
                                                            ui.horizontal_wrapped(|ui| {
                                                                for tag in &note.tags { ui.label(egui::RichText::new(format!("#{tag}")).small().color(ui.visuals().hyperlink_color)); }
                                                            });
                                                        }
                                                        ui.add_space(12.0);
                                                        let editor_id = egui::Id::new(("markdown_editor", note.id));

                                                        let jump_to_cursor = self.pending_cursor_char_index.is_some_and(|(id, _)| id == note.id);
                                                        if let Some((target_note_id, char_idx)) = self.pending_cursor_char_index
                                                            && target_note_id == note.id
                                                        {
                                                            markdown::set_cursor_char_index(ui.ctx(), editor_id, char_idx);
                                                            self.pending_cursor_char_index = None;
                                                        }

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
                                                            }
                                                        }

                                                        let mut command_changed = false;

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

                                                        // Process dropped files after layout so their link can use the drop position.
                                                        let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());

                                                        // Clipboard Paste (Ctrl+V / Shift+Insert / Paste event) Screenshot & Image Ingestion
                                                        let mut trigger_paste_image = ui.ctx().input(|input| {
                                                            let ctrl_or_cmd = input.modifiers.ctrl || input.modifiers.command;
                                                            (ctrl_or_cmd && (
                                                                input.key_pressed(egui::Key::V)
                                                                || input.events.iter().any(|e| match e {
                                                                    egui::Event::Key { key, physical_key, pressed: true, .. } => {
                                                                        *key == egui::Key::V || *physical_key == Some(egui::Key::V)
                                                                    }
                                                                    egui::Event::Paste(_) => true,
                                                                    egui::Event::Text(t) => t.contains('\x16') || t.contains('v') || t.contains('V') || t.contains('м') || t.contains('М'),
                                                                    _ => false,
                                                                })
                                                            )) || (input.modifiers.shift && input.key_pressed(egui::Key::Insert))
                                                            || input.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                                                        });

                                                        // Formatting & Quick Tools toolbar
                                                        let compact_tools = ui.available_width() < 440.0;
                                                        ui.horizontal_wrapped(|ui| {
                                                            if ui
                                                                .small_button("Paste image")
                                                                .on_hover_text("Paste screenshot or image from clipboard (Ctrl+V)")
                                                                .clicked()
                                                            {
                                                                trigger_paste_image = true;
                                                            }
                                                            if ui.small_button("B").on_hover_text("Bold (Ctrl+B)").clicked() {
                                                                markdown_command = Some(markdown::MarkdownCommand::Bold);
                                                            }
                                                            if ui.small_button("I").on_hover_text("Italic (Ctrl+I)").clicked() {
                                                                markdown_command = Some(markdown::MarkdownCommand::Italic);
                                                            }
                                                            if compact_tools {
                                                                ui.menu_button("More…", |ui| {
                                                                    if ui.button("`code`  Inline code").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::InlineCode);
                                                                        ui.close();
                                                                    }
                                                                    if ui.button("[[link]]  Wiki-link").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::WikiLink);
                                                                        ui.close();
                                                                    }
                                                                    if ui.button("☑  Task checkbox").clicked() {
                                                                        markdown_command = Some(markdown::MarkdownCommand::Task);
                                                                        ui.close();
                                                                    }
                                                                });
                                                            } else {
                                                                if ui.small_button("`code`").on_hover_text("Inline Code").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::InlineCode);
                                                                }
                                                                if ui.small_button("[[link]]").on_hover_text("Wiki-Link").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::WikiLink);
                                                                }
                                                                if ui.small_button("☑ Task").on_hover_text("Task checkbox").clicked() {
                                                                    markdown_command = Some(markdown::MarkdownCommand::Task);
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(4.0);

                                                        if markdown_command.is_some_and(|command| {
                                                            markdown::apply_command(
                                                                ui.ctx(),
                                                                editor_id,
                                                                &mut note.content,
                                                                command,
                                                            )
                                                        }) {
                                                            command_changed = true;
                                                            analytics_events.push(
                                                                AnalyticsFeature::MarkdownFormattingUsed,
                                                            );
                                                        }

                                                        if trigger_paste_image {
                                                            match crate::attachments::AttachmentManager::try_save_clipboard_image(
                                                                &self.storage_paths.notes_dir,
                                                                &self.settings.attachments_folder,
                                                            ) {
                                                                Ok(Some(rel_link)) => {
                                                                    ui.ctx().input_mut(|i| {
                                                                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::V);
                                                                        i.consume_key(egui::Modifiers::CTRL, egui::Key::V);
                                                                        i.consume_key(egui::Modifiers::SHIFT, egui::Key::Insert);
                                                                        i.events.retain(|event| !matches!(event, egui::Event::Paste(_)));
                                                                    });
                                                                    markdown::insert_attachment_link(
                                                                        ui.ctx(),
                                                                        editor_id,
                                                                        &mut note.content,
                                                                        "Pasted Image",
                                                                        &rel_link,
                                                                        true,
                                                                    );
                                                                    command_changed = true;
                                                                    analytics_events.push(
                                                                        AnalyticsFeature::AttachmentAdded,
                                                                    );
                                                                    self.storage_message = Some(format!("Pasted image saved: {rel_link}"));
                                                                }
                                                                Ok(None) => {
                                                                    // Normal text in clipboard - let show_editor TextEdit handle it
                                                                }
                                                                Err(err) => {
                                                                    self.storage_message = Some(format!("Clipboard: {err}"));
                                                                }
                                                            }
                                                        }

                                                        let inline_images = self.preview_cache.blocks(note, &self.storage_paths.notes_dir).to_vec();
                                                        let editor_output = markdown::show_editor(
                                                             ui,
                                                             &mut note.content,
                                                             editor_id,
                                                             self.settings.editor_font_size,
                                                             &inline_images,
                                                         );

                                                        if !dropped_files.is_empty() {
                                                            if let Some(position) = ui.ctx().input(|input| input.pointer.latest_pos())
                                                                && editor_output.response.rect.contains(position)
                                                            {
                                                                let cursor = editor_output.galley.cursor_from_pos(position - editor_output.galley_pos);
                                                                markdown::set_cursor_char_index(ui.ctx(), editor_id, usize::from(cursor.index));
                                                            }
                                                            for dropped in dropped_files {
                                                                let path = dropped.path();
                                                                if !path.as_os_str().is_empty() {
                                                                    match crate::attachments::AttachmentManager::import_file(
                                                                        path,
                                                                        &self.storage_paths.notes_dir,
                                                                        &self.settings.attachments_folder,
                                                                    ) {
                                                                        Ok(rel_link) => {
                                                                            let is_img = matches!(
                                                                                path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(),
                                                                                Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
                                                                            );
                                                                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                                                                            markdown::insert_attachment_link(
                                                                                ui.ctx(),
                                                                                editor_id,
                                                                                &mut note.content,
                                                                                &name,
                                                                                &rel_link,
                                                                                is_img,
                                                                            );
                                                                            command_changed = true;
                                                                            analytics_events.push(AnalyticsFeature::AttachmentAdded);
                                                                            self.storage_message = Some(format!("Imported attachment: {rel_link}"));
                                                                        }
                                                                        Err(error) => self.storage_message = Some(format!("Attachment import failed: {error}")),
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        if jump_to_cursor && let Some(range) = editor_output.cursor_range {
                                                                let rect = editor_output.galley.pos_from_cursor(range.primary).translate(editor_output.galley_pos.to_vec2());
                                                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                                                        }
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
                                                                    "Double-click or Ctrl+Click to open [[{}]]",
                                                                    wiki_link.target
                                                                ));

                                                            let double_clicked = editor_output.response.double_clicked();
                                                            let command_clicked = editor_output.response.clicked()
                                                                && ui.input(|input| input.modifiers.command);
                                                            if double_clicked || command_clicked {
                                                                activated_link_target = Some(wiki_link.target);
                                                            }
                                                        }

                                                        let content_response = editor_output.response;

                                                        if self.focus_editor {
                                                            content_response.request_focus();
                                                            self.focus_editor = false;
                                                        }

                                                        note_name_changed = title_response.changed();
                                                        note_title_edit_finished = title_response.lost_focus();
                                                        note_content_changed =
                                                            content_response.changed() || checkbox_toggled || command_changed;
                                                        if note_name_changed || note_content_changed {
                                                            note.mark_as_updated();
                                                            changed_note_id = Some(note.id);
                                                        }

                                                    } else {
                                                        ui.add_space(32.0);
                                                        ui.vertical_centered(|ui| {
                                                            ui.label(
                                                                egui::RichText::new("📝 Lilo")
                                                                    .size(26.0)
                                                                    .strong()
                                                                    .color(ui.visuals().hyperlink_color),
                                                            );
                                                            ui.add_space(4.0);
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    "Quick thoughts. Deeper connections.",
                                                                )
                                                                .small()
                                                                .color(ui.visuals().weak_text_color()),
                                                            );
                                                            ui.add_space(16.0);

                                                            ui.horizontal_wrapped(|ui| {
                                                                if ui.button("Open storage folder…").clicked() { self.handle_command_action(CommandAction::SwitchVault); }
                                                                if ui.button("Create your first note").clicked() {
                                                                    self.handle_command_action(CommandAction::NewNote);
                                                                }
                                                                if ui.button("📅 Today's Note (Alt+D)").clicked() {
                                                                    self.handle_command_action(CommandAction::OpenTodayNote);
                                                                }
                                                                if ui.button("⚡ Quick Capture (Ctrl+Shift+C)").clicked() {
                                                                    self.handle_command_action(CommandAction::QuickCapture);
                                                                }
                                                                if ui.button("📝 Templates...").clicked() {
                                                                    self.handle_command_action(CommandAction::NewNoteFromTemplate);
                                                                }
                                                                if ui.button("🔍 Commands (Ctrl+P)").clicked() {
                                                                    self.command_palette_state.open();
                                                                }
                                                            });

                                                            let recent_preview: Vec<(Uuid, String)> = self
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
                                                                        (n.id, title)
                                                                    })
                                                                })
                                                                .take(5)
                                                                .collect();

                                                            if !recent_preview.is_empty() {
                                                                ui.add_space(20.0);
                                                                ui.separator();
                                                                ui.add_space(8.0);
                                                                ui.label(
                                                                    egui::RichText::new("Recent Notes")
                                                                        .strong()
                                                                        .color(ui.visuals().weak_text_color()),
                                                                );
                                                                ui.add_space(6.0);
                                                                for (r_id, r_title) in recent_preview {
                                                                    if ui.button(format!("📄 {r_title}")).clicked() {
                                                                        self.open_note(r_id);
                                                                    }
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(32.0);
                                                    }

                                                    if let Some(id) = changed_note_id {
                                                        if note_name_changed {
                                                            self.pending_title_rename_ids.insert(id);
                                                            self.link_index = LinkIndex::build(
                                                                &self.data.notes,
                                                                &self.storage_paths.notes_dir,
                                                            );
                                                            self.tag_index = TagIndex::build(&self.data.notes);
                                                            self.pending_index_note_ids.clear();
                                                            self.last_index_change = None;
                                                        } else if note_content_changed {
                                                            self.schedule_note_index_refresh(id);
                                                        }
                                                        self.mark_note_dirty(id);
                                                    }

                                                    if note_title_edit_finished
                                                        && let Some(note) = self.data.selected_note()
                                                    {
                                                        let id = note.id;
                                                        let new_title = note.title.clone();
                                                        let old_title = self
                                                            .note_titles_snapshot
                                                            .get(&id)
                                                            .cloned()
                                                            .unwrap_or_default();
                                                        if !old_title.is_empty() && old_title != new_title {
                                                            let affected_note_ids =
                                                                links::preview_note_reference_rename(
                                                                    &self.data.notes,
                                                                    &old_title,
                                                                    &new_title,
                                                                );
                                                            if !affected_note_ids.is_empty() {
                                                                self.pending_link_rewrite =
                                                                    Some(PendingLinkRewrite {
                                                                        old_title,
                                                                        new_title: new_title.clone(),
                                                                        affected_note_ids,
                                                                    });
                                                            }
                                                            self.note_titles_snapshot.insert(id, new_title);
                                                        }
                                                    }

                                                    if let Some(target) = activated_link_target {
                                                        analytics_events.push(
                                                            AnalyticsFeature::WikiLinkOpened,
                                                        );
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

                                                    if let Some(target_date) = daily_nav_target {
                                                        self.open_or_create_daily_note_for_date(target_date);
                                                    }
                                                },
                                            );
                                        });
                                });
                            });
    }
}
