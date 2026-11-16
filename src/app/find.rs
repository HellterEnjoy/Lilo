//! In-note find and replace (Ctrl+F / Ctrl+H).

use super::state::FindState;
use super::*;
use crate::editor_tools;
use eframe::egui::text::CCursor;
use eframe::egui::text_edit::TextEditOutput;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

/// Matches are painted only up to this count to keep huge notes responsive.
const MAX_PAINTED_MATCHES: usize = 2_000;

/// What the find bar produced for the current frame.
#[derive(Default)]
pub(super) struct FindFrame {
    pub(super) matches: Vec<Range<usize>>,
    pub(super) current: Option<usize>,
    pub(super) text_changed: bool,
    pub(super) replacements: usize,
}

impl FindState {
    pub(super) fn open_bar(&mut self, with_replace: bool, prefill: Option<String>) {
        self.open = true;
        self.show_replace |= with_replace;
        if let Some(prefill) = prefill.filter(|text| !text.is_empty() && !text.contains('\n')) {
            self.query = prefill;
            self.current = 0;
        }
        self.focus_query = true;
        self.scroll_to_current = true;
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.cache = None;
    }

    fn matches(&mut self, text: &str) -> Vec<Range<usize>> {
        let mut hasher = DefaultHasher::new();
        (text, &self.query, self.case_sensitive).hash(&mut hasher);
        let key = hasher.finish();
        match &self.cache {
            Some((cached, matches)) if *cached == key => matches.clone(),
            _ => {
                let matches = editor_tools::find_matches(text, &self.query, self.case_sensitive);
                self.cache = Some((key, matches.clone()));
                matches
            }
        }
    }

    fn step(&mut self, forward: bool, count: usize) {
        if count == 0 {
            return;
        }
        self.current = if forward {
            (self.current + 1) % count
        } else {
            (self.current + count - 1) % count
        };
        self.scroll_to_current = true;
    }
}

/// Shows the find bar above the editor and applies replacements to `text`.
pub(super) fn show_find_bar(
    ui: &mut egui::Ui,
    state: &mut FindState,
    text: &mut String,
) -> FindFrame {
    let mut frame = FindFrame::default();
    if !state.open {
        return frame;
    }
    let mut matches = state.matches(text);
    state.current = state.current.min(matches.len().saturating_sub(1));
    let mut go_next = false;
    let mut go_previous = false;
    let mut replace_one = false;
    let mut replace_every = false;

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let query = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .id(egui::Id::new("find_in_note_query"))
                        .hint_text("Find in note")
                        .desired_width(180.0),
                );
                if state.focus_query {
                    query.request_focus();
                    state.focus_query = false;
                }
                if query.changed() {
                    state.current = 0;
                    state.scroll_to_current = true;
                }
                if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    if ui.input(|input| input.modifiers.shift) {
                        go_previous = true;
                    } else {
                        go_next = true;
                    }
                    query.request_focus();
                }

                let counter = if state.query.is_empty() {
                    String::new()
                } else if matches.is_empty() {
                    "No results".to_owned()
                } else {
                    format!("{} of {}", state.current + 1, matches.len())
                };
                ui_style::muted(ui, counter);

                if ui
                    .small_button("↑")
                    .on_hover_text("Previous match (Shift+Enter, Shift+F3)")
                    .clicked()
                {
                    go_previous = true;
                }
                if ui
                    .small_button("↓")
                    .on_hover_text("Next match (Enter, F3)")
                    .clicked()
                {
                    go_next = true;
                }
                if ui
                    .selectable_label(state.case_sensitive, "Aa")
                    .on_hover_text("Match case")
                    .clicked()
                {
                    state.case_sensitive = !state.case_sensitive;
                    state.current = 0;
                }
                if ui
                    .selectable_label(state.show_replace, "Replace")
                    .on_hover_text("Show replace (Ctrl+H)")
                    .clicked()
                {
                    state.show_replace = !state.show_replace;
                }
                if ui.small_button("✕").on_hover_text("Close (Esc)").clicked() {
                    state.close();
                }
            });

            if state.open && state.show_replace {
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.replacement)
                            .id(egui::Id::new("find_in_note_replacement"))
                            .hint_text("Replace with")
                            .desired_width(180.0),
                    );
                    let has_matches = !matches.is_empty();
                    replace_one = ui
                        .add_enabled(has_matches, egui::Button::new("Replace"))
                        .clicked();
                    replace_every = ui
                        .add_enabled(has_matches, egui::Button::new("Replace all"))
                        .clicked();
                });
            }
        });

    if ui.input(|input| input.key_pressed(egui::Key::F3)) {
        if ui.input(|input| input.modifiers.shift) {
            go_previous = true;
        } else {
            go_next = true;
        }
    }
    if go_next || go_previous {
        state.step(go_next, matches.len());
    }

    if replace_every {
        frame.replacements =
            editor_tools::replace_all(text, &state.query, &state.replacement, state.case_sensitive);
        frame.text_changed = frame.replacements > 0;
        state.current = 0;
    } else if replace_one && let Some(range) = matches.get(state.current).cloned() {
        editor_tools::replace_range(text, range, &state.replacement);
        frame.text_changed = true;
        frame.replacements = 1;
        state.scroll_to_current = true;
    }
    if frame.text_changed {
        matches = state.matches(text);
        state.current = state.current.min(matches.len().saturating_sub(1));
    }

    frame.current = (!matches.is_empty()).then_some(state.current);
    frame.matches = matches;
    frame
}

/// Highlights matches over the laid-out editor and scrolls to the current one.
pub(super) fn paint_matches(
    ui: &mut egui::Ui,
    output: &TextEditOutput,
    frame: &FindFrame,
    scroll_to_current: &mut bool,
) {
    if frame.matches.is_empty() {
        return;
    }
    let galley_rect = output.galley.rect.translate(output.galley_pos.to_vec2());
    let clip = ui.clip_rect();
    let accent = ui.visuals().selection.bg_fill;
    let other_fill = accent.gamma_multiply(0.45);
    let current_fill = ui.visuals().warn_fg_color.gamma_multiply(0.45);
    let rect_of = |index: usize| {
        output
            .galley
            .pos_from_cursor(CCursor::new(index))
            .translate(output.galley_pos.to_vec2())
    };

    for (index, range) in frame.matches.iter().enumerate().take(MAX_PAINTED_MATCHES) {
        let start = rect_of(range.start);
        let end = rect_of(range.end);
        let is_current = frame.current == Some(index);
        if is_current && std::mem::take(scroll_to_current) {
            ui.scroll_to_rect(start.union(end), Some(egui::Align::Center));
        }
        let rects = if (start.min.y - end.min.y).abs() < 1.0 {
            vec![egui::Rect::from_min_max(
                start.min,
                egui::pos2(end.min.x, start.max.y),
            )]
        } else {
            vec![
                egui::Rect::from_min_max(start.min, egui::pos2(galley_rect.right(), start.max.y)),
                egui::Rect::from_min_max(egui::pos2(galley_rect.left(), end.min.y), end.max),
            ]
        };
        let fill = if is_current { current_fill } else { other_fill };
        for rect in rects {
            if rect.intersects(clip) {
                ui.painter()
                    .rect_filled(rect.expand2(egui::vec2(1.0, 0.0)), 2.0, fill);
            }
        }
    }
}
