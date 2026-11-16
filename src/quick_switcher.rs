//! Ctrl+O quick switcher: open any note by fuzzy-matching its title, aliases
//! or folder path. With an empty query it lists recent, pinned and recently
//! edited notes.

use crate::commands::fuzzy_score;
use crate::storage::Note;
use crate::ui_style;
use eframe::egui::{self, Align2, FontId, Key};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use uuid::Uuid;

const MAX_RESULTS: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitcherEntry {
    pub id: Uuid,
    pub title: String,
    pub folder: String,
    /// The alias that matched the query, when it scored better than the title.
    pub matched_alias: Option<String>,
}

#[derive(Default)]
pub struct QuickSwitcherState {
    pub is_open: bool,
    pub query: String,
    selected: usize,
    focus_input: bool,
    /// Results for the last query and vault state; ranking 10,000 notes every frame is too slow.
    cache: Option<(u64, Vec<SwitcherEntry>)>,
}

impl QuickSwitcherState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected = 0;
        self.focus_input = true;
        self.cache = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.selected = 0;
        self.cache = None;
    }

    fn results(&mut self, notes: &[Note], root: &Path, recent: &[Uuid]) -> &[SwitcherEntry] {
        let mut hasher = DefaultHasher::new();
        (&self.query, recent, notes.len(), root).hash(&mut hasher);
        // Any rename, alias edit or save bumps `updated_at`, so the newest timestamp
        // is a cheap change marker for the whole vault.
        notes
            .iter()
            .map(|note| note.updated_at)
            .max()
            .hash(&mut hasher);
        let key = hasher.finish();
        if self.cache.as_ref().is_none_or(|(cached, _)| *cached != key) {
            self.cache = Some((key, entries(&self.query, notes, root, recent)));
        }
        self.cache
            .as_ref()
            .map_or(&[], |(_, results)| results.as_slice())
    }
}

fn display_title(note: &Note) -> String {
    if note.title.trim().is_empty() {
        "Untitled".to_owned()
    } else {
        note.title.clone()
    }
}

fn folder_label(note: &Note, root: &Path) -> String {
    note.file_path
        .strip_prefix(root)
        .ok()
        .and_then(Path::parent)
        .map(|parent| parent.display().to_string().replace('\\', "/"))
        .unwrap_or_default()
}

/// Builds the ranked list of notes for `query`.
pub fn entries(query: &str, notes: &[Note], root: &Path, recent: &[Uuid]) -> Vec<SwitcherEntry> {
    let query = query.trim();
    if query.is_empty() {
        let mut seen = HashSet::new();
        let mut ordered: Vec<&Note> = recent
            .iter()
            .filter_map(|id| notes.iter().find(|note| note.id == *id))
            .filter(|note| seen.insert(note.id))
            .collect();
        let mut rest: Vec<&Note> = notes
            .iter()
            .filter(|note| !seen.contains(&note.id))
            .collect();
        rest.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        ordered.extend(rest);
        return ordered
            .into_iter()
            .take(MAX_RESULTS)
            .map(|note| SwitcherEntry {
                id: note.id,
                title: display_title(note),
                folder: folder_label(note, root),
                matched_alias: None,
            })
            .collect();
    }

    let mut scored: Vec<(i64, SwitcherEntry)> = notes
        .iter()
        .filter_map(|note| {
            let folder = folder_label(note, root);
            let title_score = fuzzy_score(query, &note.title).map(|score| score + 50);
            let alias_score = note
                .aliases
                .iter()
                .filter_map(|alias| fuzzy_score(query, alias).map(|score| (score, alias)))
                .max_by_key(|(score, _)| *score);
            let path_score =
                fuzzy_score(query, &format!("{folder}/{}", note.title)).map(|score| score / 2);

            let mut best = title_score.map(|score| (score, None));
            if let Some((score, alias)) = alias_score
                && best.as_ref().is_none_or(|(best, _)| score > *best)
            {
                best = Some((score, Some(alias.clone())));
            }
            if let Some(score) = path_score
                && best.as_ref().is_none_or(|(best, _)| score > *best)
            {
                best = Some((score, None));
            }
            let (mut score, matched_alias) = best?;
            if recent.contains(&note.id) {
                score += 15;
            }
            Some((
                score,
                SwitcherEntry {
                    id: note.id,
                    title: display_title(note),
                    folder,
                    matched_alias,
                },
            ))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, entry)| entry)
        .collect()
}

/// Shows the switcher and returns the note chosen by the user.
pub fn show_quick_switcher(
    ctx: &egui::Context,
    state: &mut QuickSwitcherState,
    notes: &[Note],
    root: &Path,
    recent: &[Uuid],
) -> Option<Uuid> {
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

    let mut chosen = None;
    let screen = ui_style::screen_rect(ctx);
    let modal = egui::Modal::new(egui::Id::new("quick_switcher"))
        .frame(ui_style::modal_frame(ctx))
        .area(
            egui::Modal::default_area(egui::Id::new("quick_switcher"))
                .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).clamp(180.0, 560.0));
            let input = ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .id(egui::Id::new("quick_switcher_input"))
                    .hint_text("Open note by title, alias or folder…")
                    .desired_width(f32::INFINITY)
                    .font(FontId::proportional(16.0)),
            );
            if state.focus_input {
                input.request_focus();
                state.focus_input = false;
            }
            if input.changed() {
                state.selected = 0;
            }

            let results = state.results(notes, root, recent).to_vec();
            let count = results.len();
            state.selected = state.selected.min(count.saturating_sub(1));
            if count > 0 {
                if down {
                    state.selected = (state.selected + 1) % count;
                }
                if up {
                    state.selected = (state.selected + count - 1) % count;
                }
                if enter {
                    chosen = Some(results[state.selected].id);
                }
            }

            ui.separator();
            if results.is_empty() {
                ui_style::muted(ui, "No matching notes");
                return;
            }
            if state.query.trim().is_empty() {
                ui_style::muted(ui, "Recent and pinned notes");
            }
            egui::ScrollArea::vertical()
                .max_height((screen.height() * 0.6).clamp(120.0, 420.0))
                .show(ui, |ui| {
                    for (index, entry) in results.iter().enumerate() {
                        let selected = index == state.selected;
                        let mut label = entry.title.clone();
                        if let Some(alias) = &entry.matched_alias {
                            label.push_str(&format!("  ← {alias}"));
                        }
                        let response = ui
                            .horizontal(|ui| {
                                let response = ui.selectable_label(selected, label);
                                if !entry.folder.is_empty() {
                                    ui_style::muted(ui, &entry.folder);
                                }
                                response
                            })
                            .inner;
                        if selected && (up || down) {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            chosen = Some(entry.id);
                        }
                    }
                });
            ui.add_space(4.0);
            ui_style::muted(ui, "↑↓ to move · Enter to open · Esc to close");
        });
    if modal.should_close() {
        state.close();
    }
    if chosen.is_some() {
        state.close();
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(root: &Path, folder: &str, title: &str) -> Note {
        let mut note = Note::new_named(&root.join(folder), title);
        note.title = title.to_owned();
        note
    }

    #[test]
    fn empty_query_puts_recent_notes_first() {
        let root = Path::new("vault");
        let a = note(root, "", "Alpha");
        let b = note(root, "Work", "Beta");
        let entries = entries("", &[a.clone(), b.clone()], root, &[b.id]);
        assert_eq!(entries[0].id, b.id);
        assert_eq!(entries[0].folder, "Work");
        assert_eq!(entries[1].id, a.id);
    }

    #[test]
    fn query_matches_titles_aliases_and_folders() {
        let root = Path::new("vault");
        let project = note(root, "Work", "Project plan");
        let mut meeting = note(root, "", "Weekly sync");
        meeting.aliases.push("Standup".to_owned());
        let notes = [project.clone(), meeting.clone()];

        assert_eq!(entries("proj", &notes, root, &[])[0].id, project.id);
        let alias = &entries("stand", &notes, root, &[])[0];
        assert_eq!(alias.id, meeting.id);
        assert_eq!(alias.matched_alias.as_deref(), Some("Standup"));
        assert_eq!(entries("work/pl", &notes, root, &[])[0].id, project.id);
        assert!(entries("zzz", &notes, root, &[]).is_empty());
    }
}
