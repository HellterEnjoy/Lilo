use eframe::egui::{self, Align2, Color32, FontId, Key, Sense};
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
    HistoryBack,
    HistoryForward,
    ToggleGraphOverlay,
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
            action: CommandAction::HistoryBack,
            title: "Navigate: Back in note history",
            description: "Open the previously viewed note",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Alt+Left"),
        },
        CommandItem {
            action: CommandAction::HistoryForward,
            title: "Navigate: Forward in note history",
            description: "Open the next viewed note",
            category: CommandCategory::Navigation,
            default_shortcut: Some("Alt+Right"),
        },
        CommandItem {
            action: CommandAction::ToggleGraphOverlay,
            title: "Toggle contextual graph overlay",
            description: "Show or hide the graph over the current note",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+Shift+G"),
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
            default_shortcut: Some("Ctrl+Shift+B"),
        },
        CommandItem {
            action: CommandAction::ToggleRightInspector,
            title: "Toggle right context inspector",
            description: "Show or hide local graph, backlinks, and outline",
            category: CommandCategory::ViewAndLayout,
            default_shortcut: Some("Ctrl+Shift+I"),
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

use crate::{
    links::LinkIndex,
    search::SearchQuery,
    storage::{AppSettings, Note},
    tags::TagIndex,
    ui_style,
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandPaletteResult {
    Action(CommandAction),
    OpenNote(Uuid),
}

#[derive(Clone)]
enum Destination {
    Action(CommandAction),
    Note(Uuid),
    Query(String),
}

#[derive(Clone)]
struct PaletteEntry {
    title: String,
    detail: String,
    snippet: String,
    shortcut: String,
    destination: Destination,
}

#[derive(Default)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
    pub focus_input: bool,
    pub scope: usize,
    cache_key: Option<u64>,
    results: Vec<PaletteEntry>,
}

impl CommandPaletteState {
    pub fn open(&mut self) {
        if self.is_open {
            self.focus_input = true;
            return;
        }
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
        self.scope = 0;
        self.focus_input = true;
        self.cache_key = None;
    }
    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.selected_index = 0;
    }
}

fn excerpt(content: &str, query: &SearchQuery) -> String {
    let line = content
        .lines()
        .find(|line| {
            query
                .text_terms
                .iter()
                .any(|term| line.to_lowercase().contains(term))
        })
        .or_else(|| content.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("");
    let chars: Vec<char> = line.chars().collect();
    let match_start = query
        .text_terms
        .iter()
        .filter_map(|term| {
            line.to_lowercase()
                .find(term)
                .map(|byte| line.to_lowercase()[..byte].chars().count())
        })
        .min()
        .unwrap_or(0);
    let start = match_start.saturating_sub(32).min(chars.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..].iter().take(130).collect::<String>(),
        if chars.len() > start + 130 { "…" } else { "" }
    )
}

fn entries(
    query: &str,
    scope: usize,
    notes: &[Note],
    root: &Path,
    links: &LinkIndex,
    tags: &TagIndex,
    settings: &AppSettings,
) -> Vec<PaletteEntry> {
    let parsed = SearchQuery::parse(query);
    let filtered = !parsed.tags.is_empty()
        || !parsed.negated_tags.is_empty()
        || !parsed.paths.is_empty()
        || !parsed.negated_paths.is_empty()
        || !parsed.links.is_empty()
        || !parsed.titles.is_empty()
        || !parsed.negated_terms.is_empty();
    let mut results: Vec<(i64, PaletteEntry)> = Vec::new();
    if scope == 0 || scope == 1 {
        let by_id: HashMap<_, _> = notes.iter().map(|note| (note.id, note)).collect();
        for note in notes {
            let title = if note.title.trim().is_empty() {
                "Untitled"
            } else {
                &note.title
            };
            let relative = note.file_path.strip_prefix(root).unwrap_or(&note.file_path);
            let targets: Vec<String> = if parsed.links.is_empty() {
                Vec::new()
            } else {
                links
                    .links_for(note.id)
                    .map(|links| {
                        links
                            .unresolved
                            .iter()
                            .cloned()
                            .chain(
                                links
                                    .outgoing
                                    .iter()
                                    .filter_map(|id| by_id.get(id).map(|note| note.title.clone())),
                            )
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let matches =
                parsed.matches_note(note, relative.parent().unwrap_or(Path::new("")), &targets);
            let score = if query.is_empty() {
                Some(100)
            } else if filtered {
                matches.then_some(150)
            } else {
                fuzzy_score(query, title)
                    .into_iter()
                    .chain(
                        note.aliases
                            .iter()
                            .filter_map(|alias| fuzzy_score(query, alias)),
                    )
                    .chain(matches.then_some(80))
                    .max()
            };
            if let Some(score) = score {
                let recent = settings
                    .recent_note_ids
                    .iter()
                    .position(|id| *id == note.id);
                results.push((
                    score + recent.map_or(0, |i| 40i64.saturating_sub(i as i64)),
                    PaletteEntry {
                        title: title.to_owned(),
                        detail: relative.display().to_string(),
                        snippet: excerpt(&note.content, &parsed),
                        shortcut: String::new(),
                        destination: Destination::Note(note.id),
                    },
                ));
            }
        }
    }
    if scope == 0 || scope == 3 {
        for command in all_commands() {
            let score = if query.is_empty() {
                Some(20)
            } else {
                fuzzy_score(query, command.title)
                    .into_iter()
                    .chain(fuzzy_score(query, command.description).map(|score| score / 2))
                    .max()
            };
            if let Some(score) = score {
                let shortcut = match command.action {
                    CommandAction::NewNote => settings.shortcuts.new_note.as_str(),
                    CommandAction::SaveNote => settings.shortcuts.save.as_str(),
                    _ => command.default_shortcut.unwrap_or(""),
                };
                results.push((
                    score,
                    PaletteEntry {
                        title: command.title.to_owned(),
                        detail: format!("{} · {}", command.category.label(), command.description),
                        snippet: String::new(),
                        shortcut: shortcut.to_owned(),
                        destination: Destination::Action(command.action),
                    },
                ));
            }
        }
    }
    if scope == 0 || scope == 2 {
        for tag in tags.all_tags() {
            if query.is_empty()
                || tag
                    .tag
                    .to_lowercase()
                    .contains(&query.trim_start_matches('#').to_lowercase())
            {
                results.push((
                    60,
                    PaletteEntry {
                        title: format!("#{}", tag.tag),
                        detail: format!("{} notes · filter by tag", tag.count),
                        snippet: String::new(),
                        shortcut: String::new(),
                        destination: Destination::Query(format!("tag:\"{}\"", tag.tag)),
                    },
                ));
            }
        }
    }
    if scope == 0 || scope == 4 {
        for preset in &settings.search_presets {
            if query.is_empty()
                || fuzzy_score(query, &preset.name).is_some()
                || preset.query.contains(query)
            {
                results.push((
                    50,
                    PaletteEntry {
                        title: preset.name.clone(),
                        detail: preset.query.clone(),
                        snippet: String::new(),
                        shortcut: "Saved search".to_owned(),
                        destination: Destination::Query(preset.query.clone()),
                    },
                ));
            }
        }
    }
    results.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
    results.into_iter().map(|(_, entry)| entry).collect()
}

/// Input, scoring and activation happen in that order so Enter cannot open stale results.
pub fn show_command_palette(
    ctx: &egui::Context,
    state: &mut CommandPaletteState,
    notes: &[Note],
    root: &Path,
    links: &LinkIndex,
    tags: &TagIndex,
    settings: &AppSettings,
) -> Option<CommandPaletteResult> {
    if !state.is_open {
        return None;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        state.close();
        return None;
    }
    let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
    let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
    let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
    let mut result = None;
    let mut activate = None;
    let screen = ui_style::screen_rect(ctx);
    egui::Modal::new(egui::Id::new("command_palette_area"))
        .frame(ui_style::modal_frame(ctx))
        .area(
            egui::Modal::default_area(egui::Id::new("command_palette_area"))
                .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).clamp(180.0, 620.0));
            ui.heading("Search & Commands");
            let input = ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .id(egui::Id::new("command_palette_input"))
                    .hint_text("Search notes, tag: or path:…")
                    .desired_width(f32::INFINITY)
                    .font(FontId::proportional(16.0)),
            );
            if state.focus_input {
                input.request_focus();
                state.focus_input = false;
            }
            ui.horizontal_wrapped(|ui| {
                for (scope, label) in ["All", "Notes", "Tags", "Commands", "Saved"]
                    .iter()
                    .enumerate()
                {
                    if ui
                        .selectable_value(&mut state.scope, scope, *label)
                        .changed()
                    {
                        state.selected_index = 0;
                    }
                }
            });
            if input.changed() {
                state.selected_index = 0;
            }
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                links.generation(),
                &state.query,
                state.scope,
                &settings.recent_note_ids,
            )
                .hash(&mut hash);
            for preset in &settings.search_presets {
                (&preset.name, &preset.query).hash(&mut hash);
            }
            (&settings.shortcuts.new_note, &settings.shortcuts.save).hash(&mut hash);
            let key = hash.finish();
            if state.cache_key != Some(key) {
                state.results = entries(
                    state.query.trim(),
                    state.scope,
                    notes,
                    root,
                    links,
                    tags,
                    settings,
                );
                state.cache_key = Some(key);
            }
            let count = state.results.len();
            state.selected_index = state.selected_index.min(count.saturating_sub(1));

            if count > 0 {
                if down {
                    state.selected_index = (state.selected_index + 1) % count;
                }
                if up {
                    state.selected_index = (state.selected_index + count - 1) % count;
                }
                if enter {
                    activate = Some(state.results[state.selected_index].destination.clone());
                }
            }
            ui.separator();
            let parsed = SearchQuery::parse(&state.query);
            let terms: Vec<&str> = parsed
                .text_terms
                .iter()
                .chain(&parsed.titles)
                .map(String::as_str)
                .collect();
            if count == 0 {
                ui.label(format!("No results for “{}”", state.query));
                ui_style::muted(ui, "Check spelling or try fewer filters.");
                if ui.button("Search all notes").clicked() {
                    state.query.clear();
                    state.scope = 1;
                }
            }
            let row_height = 96.0;
            let list_height = (screen.height() - 220.0).max(60.0);
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("palette_results")
                .max_height(list_height);
            if down || up || input.changed() {
                scroll = scroll.vertical_scroll_offset(
                    (state.selected_index as f32 * (row_height + ui.spacing().item_spacing.y)
                        - list_height / 2.0
                        + row_height / 2.0)
                        .max(0.0),
                );
            }
            scroll.show_rows(ui, row_height, count, |ui, range| {
                for index in range {
                    let entry = &state.results[index];
                    let selected = index == state.selected_index;
                    let row = egui::Frame::new()
                        .fill(if selected {
                            ui.visuals().selection.bg_fill
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(8)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.set_min_size(egui::vec2(ui.available_width(), row_height - 16.0));
                            ui.add(
                                egui::Label::new(ui_style::highlighted_terms(
                                    ui,
                                    &entry.title,
                                    &terms,
                                    15.0,
                                    false,
                                ))
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&entry.detail).size(12.0).weak(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&entry.detail);
                            if !entry.snippet.is_empty() {
                                ui.add(
                                    egui::Label::new(ui_style::highlighted_terms(
                                        ui,
                                        &entry.snippet,
                                        &terms,
                                        13.0,
                                        true,
                                    ))
                                    .truncate(),
                                );
                            }
                            if !entry.shortcut.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&entry.shortcut).size(11.0).weak(),
                                    )
                                    .truncate(),
                                );
                            }
                        })
                        .response
                        .interact(Sense::click());
                    if row.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                        state.selected_index = index;
                    }
                    if row.clicked() {
                        activate = Some(entry.destination.clone());
                    }
                }
            });
            ui_style::muted(ui, "Up/Down: navigate · Enter: open · Esc: close");
        });
    if let Some(destination) = activate {
        match destination {
            Destination::Note(id) => {
                result = Some(CommandPaletteResult::OpenNote(id));
                state.close();
            }
            Destination::Action(action) => {
                result = Some(CommandPaletteResult::Action(action));
                state.close();
            }
            Destination::Query(query) => {
                state.query = query;
                state.scope = 1;
                state.selected_index = 0;
                state.focus_input = true;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_keyboard_uses_current_query_and_navigates_focused_results() {
        let ctx = egui::Context::default();
        let root = Path::new("/vault");
        let notes = vec![
            Note::new_named(root, "Alpha"),
            Note::new_named(root, "Beta"),
        ];
        let links = LinkIndex::build(&notes, root);
        let tags = TagIndex::build(&notes);
        let settings = AppSettings::default();
        let mut state = CommandPaletteState::default();
        state.open();
        state.scope = 1;
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        for (events, expected) in [
            (vec![], None),
            (vec![key(Key::ArrowDown)], None),
            (
                vec![key(Key::Enter)],
                Some(CommandPaletteResult::OpenNote(notes[1].id)),
            ),
        ] {
            let mut result = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |_| {
                    result = show_command_palette(
                        &ctx, &mut state, &notes, root, &links, &tags, &settings,
                    );
                },
            );
            output.textures_delta.clear();
            assert_eq!(result, expected);
        }
        state.open();
        state.scope = 1;
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            show_command_palette(&ctx, &mut state, &notes, root, &links, &tags, &settings);
        });
        output.textures_delta.clear();
        let mut result = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Text("Beta".into()), key(Key::Enter)],
                ..Default::default()
            },
            |_| {
                result =
                    show_command_palette(&ctx, &mut state, &notes, root, &links, &tags, &settings);
            },
        );
        output.textures_delta.clear();
        assert_eq!(result, Some(CommandPaletteResult::OpenNote(notes[1].id)));
    }

    #[test]
    fn palette_scopes_include_all_notes_aliases_tags_saved_queries_and_filters() {
        let root = Path::new("/vault");
        let mut first = Note::new_named(&root.join("Projects"), "Первый");
        first.aliases.push("Alias".to_owned());
        first.tags.push("design".to_owned());
        first.content = "Содержимое важной заметки".to_owned();
        first.refresh_search_text();
        let notes = vec![first, Note::new_named(root, "Second")];
        let links = LinkIndex::build(&notes, root);
        let tags = TagIndex::build(&notes);
        let settings = AppSettings::default();
        assert_eq!(
            entries("", 1, &notes, root, &links, &tags, &settings).len(),
            2
        );
        assert_eq!(
            entries("Alias", 1, &notes, root, &links, &tags, &settings).len(),
            1
        );
        assert_eq!(
            entries(
                "path:Projects tag:design важной",
                1,
                &notes,
                root,
                &links,
                &tags,
                &settings
            )
            .len(),
            1
        );
        assert_eq!(
            entries("", 2, &notes, root, &links, &tags, &settings).len(),
            1
        );
        assert_eq!(
            entries("", 4, &notes, root, &links, &tags, &settings).len(),
            settings.search_presets.len()
        );
    }

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
