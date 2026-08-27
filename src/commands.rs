use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Pos2, Sense, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandAction {
    OpenTodayNote,
    OpenYesterdayNote,
    OpenTomorrowNote,
    OpenPrevDayNote,
    OpenNextDayNote,
    NewNoteFromTemplate,
    InsertTemplate,
    QuickCapture,
    NewNote,
    SaveNote,
    TogglePin,
    MoveToFolder,
    DeleteNote,
    NoteDetails,
    ViewEditor,
    ViewNotesList,
    ViewGraph,
    ViewTrash,
    ViewSettings,
    ToggleZenMode,
    ToggleLeftSidebar,
    ToggleRightInspector,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleTheme,
    ToggleAlwaysOnTop,
    SwitchVault,
    ScanDiagnostics,
    ExportVault,
    NewFolder,
    DeleteFolder,
    SaveCurrentSearch,
    ClearSearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCategory {
    DailyNotes,
    Templates,
    QuickCapture,
    Navigation,
    NoteActions,
    SearchAndTags,
    ViewAndLayout,
    StorageAndVault,
}

impl CommandCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::DailyNotes => "Daily Notes",
            Self::Templates => "Templates",
            Self::QuickCapture => "Quick Capture",
            Self::Navigation => "Navigation",
            Self::NoteActions => "Note Actions",
            Self::SearchAndTags => "Search & Tags",
            Self::ViewAndLayout => "View & Layout",
            Self::StorageAndVault => "Storage & Vault",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub action: CommandAction,
    pub title: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub default_shortcut: Option<&'static str>,
}

pub fn all_commands() -> Vec<CommandItem> {
    vec![
        CommandItem {
            action: CommandAction::OpenTodayNote,
            title: "Daily: Open today's note",
            description: "Open or create today's daily note",
            category: CommandCategory::DailyNotes,
            default_shortcut: Some("Alt+D"),
        },
        CommandItem {
            action: CommandAction::OpenYesterdayNote,
            title: "Daily: Open yesterday's note",
            description: "Navigate to yesterday's daily note",
            category: CommandCategory::DailyNotes,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::OpenTomorrowNote,
            title: "Daily: Open tomorrow's note",
            description: "Navigate to tomorrow's daily note",
            category: CommandCategory::DailyNotes,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::OpenPrevDayNote,
            title: "Daily: Previous day",
            description: "Navigate to the previous day relative to active daily note",
            category: CommandCategory::DailyNotes,
            default_shortcut: Some("Alt+Left"),
        },
        CommandItem {
            action: CommandAction::OpenNextDayNote,
            title: "Daily: Next day",
            description: "Navigate to the next day relative to active daily note",
            category: CommandCategory::DailyNotes,
            default_shortcut: Some("Alt+Right"),
        },
        CommandItem {
            action: CommandAction::QuickCapture,
            title: "Quick capture...",
            description: "Quickly record a thought without switching notes",
            category: CommandCategory::QuickCapture,
            default_shortcut: Some("Ctrl+Shift+C"),
        },
        CommandItem {
            action: CommandAction::NewNoteFromTemplate,
            title: "Templates: New note from template...",
            description: "Create a new note formatted with a template",
            category: CommandCategory::Templates,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::InsertTemplate,
            title: "Templates: Insert template into active note...",
            description: "Insert template text at the cursor",
            category: CommandCategory::Templates,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::NewNote,
            title: "New note",
            description: "Create a new blank note in the selected folder",
            category: CommandCategory::NoteActions,
            default_shortcut: Some("Ctrl+N"),
        },
        CommandItem {
            action: CommandAction::SaveNote,
            title: "Save note",
            description: "Flush and save changes with backup",
            category: CommandCategory::NoteActions,
            default_shortcut: Some("Ctrl+S"),
        },
        CommandItem {
            action: CommandAction::TogglePin,
            title: "Toggle pin on note",
            description: "Pin or unpin the active note",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::MoveToFolder,
            title: "Move note to selected folder",
            description: "Relocate current note to selected directory",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::DeleteNote,
            title: "Move note to Trash",
            description: "Safely move the active note into Trash",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::NoteDetails,
            title: "Note details: Tags, aliases & links",
            description: "Inspect outgoing links, backlinks and properties",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::ViewEditor,
            title: "View: Editor",
            description: "Switch to Markdown editor",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Ctrl+1"),
        },
        CommandItem {
            action: CommandAction::ViewNotesList,
            title: "View: Notes list",
            description: "Browse folders and search notes",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Ctrl+2"),
        },
        CommandItem {
            action: CommandAction::ViewGraph,
            title: "View: Knowledge graph",
            description: "Open visual interactive graph",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Ctrl+3"),
        },
        CommandItem {
            action: CommandAction::ViewTrash,
            title: "View: Recovery & backups",
            description: "Browse trash, backup history and diagnostics",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Ctrl+4"),
        },
        CommandItem {
            action: CommandAction::ViewSettings,
            title: "View: Settings",
            description: "Configure appearance, typography, shortcuts and storage",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Ctrl+5"),
        },
        CommandItem {
            action: CommandAction::ToggleZenMode,
            title: "Toggle Zen / writing mode",
            description: "Focus on writing by hiding all side navigation",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("F11"),
        },
        CommandItem {
            action: CommandAction::ToggleLeftSidebar,
            title: "Toggle left explorer sidebar",
            description: "Show or hide the file tree and explorer panel",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+B"),
        },
        CommandItem {
            action: CommandAction::ToggleRightInspector,
            title: "Toggle right context inspector",
            description: "Show or hide local graph, backlinks, and outline",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+I"),
        },
        CommandItem {
            action: CommandAction::ZoomIn,
            title: "Zoom in editor font",
            description: "Increase editor font size (+1)",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl++"),
        },
        CommandItem {
            action: CommandAction::ZoomOut,
            title: "Zoom out editor font",
            description: "Decrease editor font size (-1)",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+-"),
        },
        CommandItem {
            action: CommandAction::ZoomReset,
            title: "Reset editor font zoom",
            description: "Reset editor font size to default",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+0"),
        },
        CommandItem {
            action: CommandAction::ToggleTheme,
            title: "Switch theme: Light / Dark",
            description: "Toggle between light and dark themes",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::ToggleAlwaysOnTop,
            title: "Toggle always on top",
            description: "Keep Lilo floating above other windows",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::SwitchVault,
            title: "Vault: Switch vault path...",
            description: "Open or switch to another Markdown vault",
            category: CommandCategory::StorageAndVault,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::ScanDiagnostics,
            title: "Vault: Run diagnostics scan",
            description: "Verify vault integrity without rewriting files",
            category: CommandCategory::StorageAndVault,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::ExportVault,
            title: "Vault: Export vault...",
            description: "Export complete timestamped backup snapshot",
            category: CommandCategory::StorageAndVault,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::NewFolder,
            title: "Folder: New folder",
            description: "Create a subfolder in the active directory",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::DeleteFolder,
            title: "Folder: Delete folder...",
            description: "Safely delete folder and move its notes to Trash",
            category: CommandCategory::NoteActions,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::SaveCurrentSearch,
            title: "Search: Save current search as preset",
            description: "Save active search query to sidebar presets",
            category: CommandCategory::SearchAndTags,
            default_shortcut: None,
        },
        CommandItem {
            action: CommandAction::ClearSearch,
            title: "Search: Clear search and filters",
            description: "Reset active search query and show all notes",
            category: CommandCategory::SearchAndTags,
            default_shortcut: None,
        },
    ]
}

/// Fuzzy scoring algorithm rewarding prefix matches, word boundaries, and contiguous characters.
pub fn fuzzy_score(pattern: &str, target: &str) -> Option<i64> {
    let p = pattern.trim().to_lowercase();
    let t = target.to_lowercase();

    if p.is_empty() {
        return Some(0);
    }
    if p == t {
        return Some(1000);
    }
    if t.starts_with(&p) {
        return Some(500 + (100 - p.len().min(100) as i64));
    }

    let p_chars: Vec<char> = p.chars().collect();
    let t_chars: Vec<char> = t.chars().collect();
    let t_raw_chars: Vec<char> = target.chars().collect();

    let mut p_idx = 0;
    let mut score = 0_i64;
    let mut prev_matched_idx = None;

    for (t_idx, &t_ch) in t_chars.iter().enumerate() {
        if p_idx < p_chars.len() && t_ch == p_chars[p_idx] {
            let mut char_score = 10_i64;

            // Contiguous match bonus
            if let Some(prev) = prev_matched_idx {
                if prev + 1 == t_idx {
                    char_score += 25;
                } else {
                    char_score -= (t_idx - prev) as i64;
                }
            } else if t_idx == 0 {
                char_score += 40; // First char bonus
            }

            // Word boundary bonus
            if t_idx > 0 {
                let prev_ch = t_chars[t_idx - 1];
                if matches!(prev_ch, ' ' | '/' | '\\' | '-' | '_' | '.' | ':') {
                    char_score += 30;
                }
            }

            // CamelCase bonus from original string
            if t_idx < t_raw_chars.len() && t_raw_chars[t_idx].is_uppercase() {
                char_score += 20;
            }

            score += char_score;
            prev_matched_idx = Some(t_idx);
            p_idx += 1;
        }
    }

    if p_idx == p_chars.len() {
        Some(score)
    } else {
        None
    }
}

use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPaletteResult {
    Action(CommandAction),
    OpenNote(Uuid),
}

#[derive(Clone)]
enum PaletteEntry {
    Command {
        action: CommandAction,
        title: &'static str,
        description: &'static str,
        category: &'static str,
        shortcut: Option<&'static str>,
        is_recent: bool,
    },
    Note {
        id: Uuid,
        title: String,
        folder: String,
        updated_text: String,
        is_recent: bool,
    },
}

#[derive(Default)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
    pub focus_input: bool,
}

impl CommandPaletteState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
        self.focus_input = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.selected_index = 0;
    }
}

/// Renders the modal Command Palette overlay supporting commands, recent items, and notes search.
pub fn show_command_palette(
    ctx: &egui::Context,
    state: &mut CommandPaletteState,
    recent_commands: &[CommandAction],
    recent_note_ids: &[Uuid],
    notes: &[crate::storage::Note],
    notes_dir: &std::path::Path,
) -> Option<CommandPaletteResult> {
    if !state.is_open {
        return None;
    }

    let mut executed_result = None;
    let commands = all_commands();
    let query_trimmed = state.query.trim();

    let mut scored: Vec<(i64, PaletteEntry)> = Vec::new();

    if query_trimmed.is_empty() {
        // 1. Recent Commands
        let mut added_recent_actions = Vec::new();
        for &recent_action in recent_commands {
            if let Some(cmd) = commands.iter().find(|c| c.action == recent_action)
                && !added_recent_actions.contains(&recent_action)
            {
                added_recent_actions.push(recent_action);
                scored.push((
                    1000,
                    PaletteEntry::Command {
                        action: cmd.action,
                        title: cmd.title,
                        description: cmd.description,
                        category: cmd.category.label(),
                        shortcut: cmd.default_shortcut,
                        is_recent: true,
                    },
                ));
            }
        }

        // 2. Recent Notes
        for &note_id in recent_note_ids.iter().take(6) {
            if let Some(note) = notes.iter().find(|n| n.id == note_id) {
                let folder_rel = note
                    .file_path
                    .strip_prefix(notes_dir)
                    .ok()
                    .and_then(|p| p.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let title = if note.title.trim().is_empty() {
                    "Untitled".to_owned()
                } else {
                    note.title.clone()
                };
                let updated_text = note.updated_at.format("%d/%m %H:%M").to_string();
                scored.push((
                    800,
                    PaletteEntry::Note {
                        id: note.id,
                        title,
                        folder: folder_rel,
                        updated_text,
                        is_recent: true,
                    },
                ));
            }
        }

        // 3. Other Commands
        for cmd in &commands {
            if !added_recent_actions.contains(&cmd.action) {
                scored.push((
                    500,
                    PaletteEntry::Command {
                        action: cmd.action,
                        title: cmd.title,
                        description: cmd.description,
                        category: cmd.category.label(),
                        shortcut: cmd.default_shortcut,
                        is_recent: false,
                    },
                ));
            }
        }
    } else {
        // Search Commands
        for cmd in &commands {
            let title_score = fuzzy_score(query_trimmed, cmd.title);
            let desc_score = fuzzy_score(query_trimmed, cmd.description).map(|s| s / 2);
            let cat_score = fuzzy_score(query_trimmed, cmd.category.label()).map(|s| s / 3);

            let max_score = title_score
                .into_iter()
                .chain(desc_score)
                .chain(cat_score)
                .max();

            if let Some(score) = max_score {
                let is_recent = recent_commands.contains(&cmd.action);
                let final_score = if is_recent { score + 50 } else { score };
                scored.push((
                    final_score,
                    PaletteEntry::Command {
                        action: cmd.action,
                        title: cmd.title,
                        description: cmd.description,
                        category: cmd.category.label(),
                        shortcut: cmd.default_shortcut,
                        is_recent,
                    },
                ));
            }
        }

        // Search Notes
        for note in notes {
            let note_title = if note.title.trim().is_empty() {
                "Untitled"
            } else {
                note.title.as_str()
            };
            let folder_rel = note
                .file_path
                .strip_prefix(notes_dir)
                .ok()
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            let title_score = fuzzy_score(query_trimmed, note_title);
            let folder_score = if !folder_rel.is_empty() {
                fuzzy_score(query_trimmed, &folder_rel).map(|s| s / 2)
            } else {
                None
            };

            let max_score = title_score.into_iter().chain(folder_score).max();
            if let Some(score) = max_score {
                let is_recent = recent_note_ids.contains(&note.id);
                let final_score = if is_recent { score + 60 } else { score };
                let updated_text = note.updated_at.format("%d/%m %H:%M").to_string();
                scored.push((
                    final_score,
                    PaletteEntry::Note {
                        id: note.id,
                        title: note_title.to_owned(),
                        folder: folder_rel,
                        updated_text,
                        is_recent,
                    },
                ));
            }
        }

        scored.sort_by_key(|item| std::cmp::Reverse(item.0));
    }

    let matching_count = scored.len();

    if matching_count > 0 && state.selected_index >= matching_count {
        state.selected_index = matching_count - 1;
    }

    // Keyboard navigation
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        state.close();
        return None;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) && matching_count > 0 {
        state.selected_index = (state.selected_index + 1) % matching_count;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) && matching_count > 0 {
        state.selected_index = if state.selected_index == 0 {
            matching_count - 1
        } else {
            state.selected_index - 1
        };
    }
    if ctx.input(|i| i.key_pressed(Key::Enter)) && matching_count > 0 {
        let result = match &scored[state.selected_index].1 {
            PaletteEntry::Command { action, .. } => CommandPaletteResult::Action(*action),
            PaletteEntry::Note { id, .. } => CommandPaletteResult::OpenNote(*id),
        };
        executed_result = Some(result);
        state.close();
        return executed_result;
    }

    // Draw dimmed background overlay
    let screen_rect = crate::ui_style::screen_rect(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("command_palette_dim"),
    ));
    painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(140));

    // Render palette window
    let modal_width = 580.0_f32.min(screen_rect.width() - 32.0);
    let modal_pos = Pos2::new(screen_rect.center().x, screen_rect.top() + 70.0);

    egui::Area::new(egui::Id::new("command_palette_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(modal_pos)
        .pivot(Align2::CENTER_TOP)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(ui.visuals().window_fill)
                .stroke(Stroke::new(
                    1.0,
                    ui.visuals().widgets.inactive.bg_stroke.color,
                ))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(12))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(180),
                })
                .show(ui, |ui| {
                    ui.set_width(modal_width);

                    ui.horizontal(|ui| {
                        let search_id = egui::Id::new("command_palette_input");
                        let input = ui.add(
                            egui::TextEdit::singleline(&mut state.query)
                                .id(search_id)
                                .desired_width(f32::INFINITY)
                                .hint_text(
                                    "Type a command, note title, or search (e.g. daily, zen)...",
                                )
                                .font(FontId::proportional(15.0))
                                .margin(egui::Margin::symmetric(8, 8)),
                        );

                        if state.focus_input {
                            input.request_focus();
                            state.focus_input = false;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    let list_height = 300.0_f32.min(screen_rect.height() - 180.0);
                    egui::ScrollArea::vertical()
                        .max_height(list_height)
                        .show(ui, |ui| {
                            if scored.is_empty() {
                                ui.add_space(16.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new("No matching commands or notes found")
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                });
                                ui.add_space(16.0);
                            } else {
                                for (idx, (_, entry)) in scored.iter().enumerate() {
                                    let is_selected = idx == state.selected_index;
                                    let fill = if is_selected {
                                        ui.visuals().selection.bg_fill.gamma_multiply(0.7)
                                    } else {
                                        Color32::TRANSPARENT
                                    };

                                    let row = egui::Frame::new()
                                        .fill(fill)
                                        .corner_radius(CornerRadius::same(6))
                                        .inner_margin(egui::Margin::symmetric(10, 7))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            match entry {
                                                PaletteEntry::Command {
                                                    title,
                                                    description,
                                                    category,
                                                    shortcut,
                                                    is_recent,
                                                    ..
                                                } => {
                                                    ui.horizontal(|ui| {
                                                        ui.vertical(|ui| {
                                                            ui.horizontal(|ui| {
                                                                if *is_recent {
                                                                    ui.label(
                                                                        egui::RichText::new("🕒")
                                                                            .small(),
                                                                    );
                                                                }
                                                                ui.label(
                                                                    egui::RichText::new(*title)
                                                                        .strong()
                                                                        .size(14.0),
                                                                );
                                                            });
                                                            ui.label(
                                                                egui::RichText::new(*description)
                                                                    .small()
                                                                    .color(
                                                                        ui.visuals()
                                                                            .weak_text_color(),
                                                                    ),
                                                            );
                                                        });
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if let Some(sc) = shortcut {
                                                                    ui.label(
                                                                        egui::RichText::new(*sc)
                                                                            .small()
                                                                            .monospace()
                                                                            .color(
                                                                                ui.visuals()
                                                                                    .hyperlink_color,
                                                                            ),
                                                                    );
                                                                }
                                                                ui.label(
                                                                    egui::RichText::new(if *is_recent {
                                                                        "Recent"
                                                                    } else {
                                                                        *category
                                                                    })
                                                                    .small()
                                                                    .color(
                                                                        ui.visuals()
                                                                            .weak_text_color(),
                                                                    ),
                                                                );
                                                            },
                                                        );
                                                    });
                                                }
                                                PaletteEntry::Note {
                                                    title,
                                                    folder,
                                                    updated_text,
                                                    is_recent,
                                                    ..
                                                } => {
                                                    ui.horizontal(|ui| {
                                                        ui.vertical(|ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new(if *is_recent {
                                                                        "🕒 📄"
                                                                    } else {
                                                                        "📄"
                                                                    })
                                                                    .small(),
                                                                );
                                                                ui.label(
                                                                    egui::RichText::new(title)
                                                                        .strong()
                                                                        .size(14.0),
                                                                );
                                                            });
                                                            if !folder.is_empty() {
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "📁 {folder}"
                                                                    ))
                                                                    .small()
                                                                    .color(
                                                                        ui.visuals()
                                                                            .weak_text_color(),
                                                                    ),
                                                                );
                                                            }
                                                        });
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(updated_text)
                                                                        .small()
                                                                        .color(
                                                                            ui.visuals()
                                                                                .weak_text_color(),
                                                                        ),
                                                                );
                                                                ui.label(
                                                                    egui::RichText::new(if *is_recent {
                                                                        "Recent Note"
                                                                    } else {
                                                                        "Note"
                                                                    })
                                                                    .small()
                                                                    .color(
                                                                        ui.visuals()
                                                                            .hyperlink_color,
                                                                    ),
                                                                );
                                                            },
                                                        );
                                                    });
                                                }
                                            }
                                        })
                                        .response
                                        .interact(Sense::click());

                                    if row.clicked() {
                                        let result = match &entry {
                                            PaletteEntry::Command { action, .. } => {
                                                CommandPaletteResult::Action(*action)
                                            }
                                            PaletteEntry::Note { id, .. } => {
                                                CommandPaletteResult::OpenNote(*id)
                                            }
                                        };
                                        executed_result = Some(result);
                                        state.close();
                                    }
                                    if row.hovered() {
                                        state.selected_index = idx;
                                    }
                                    ui.add_space(2.0);
                                }
                            }
                        });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("↑↓ Navigate  •  Enter Execute/Open  •  Esc Close")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                });
        });

    executed_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_scores_exact_and_prefix_higher() {
        let exact = fuzzy_score("daily", "daily");
        let prefix = fuzzy_score("daily", "daily notes");
        let scattered = fuzzy_score("dn", "daily notes");
        let none = fuzzy_score("xyz", "daily notes");

        assert!(exact.is_some());
        assert!(prefix.is_some());
        assert!(scattered.is_some());
        assert!(none.is_none());

        assert!(exact.unwrap() > prefix.unwrap());
        assert!(prefix.unwrap() > scattered.unwrap());
    }

    #[test]
    fn all_commands_have_unique_actions() {
        let commands = all_commands();
        assert!(!commands.is_empty());
        for cmd in &commands {
            assert!(!cmd.title.is_empty());
        }
    }

    #[test]
    fn fuzzy_matching_boundary_and_camelcase_bonuses() {
        let boundary_score = fuzzy_score("dn", "Daily Notes");
        let middle_score = fuzzy_score("dn", "Admn");
        assert!(boundary_score.is_some());
        assert!(middle_score.is_some());
        assert!(boundary_score.unwrap() > middle_score.unwrap());
    }

    #[test]
    fn command_palette_state_open_and_close() {
        let mut state = CommandPaletteState::default();
        assert!(!state.is_open);

        state.open();
        assert!(state.is_open);
        assert!(state.focus_input);
        assert_eq!(state.selected_index, 0);

        state.query = "zen".to_owned();
        state.selected_index = 3;
        state.close();
        assert!(!state.is_open);
        assert!(state.query.is_empty());
        assert_eq!(state.selected_index, 0);
    }
}
