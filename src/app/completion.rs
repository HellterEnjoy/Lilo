//! `[[wiki link]]` and `#tag` autocompletion in the live editor.
//!
//! The flow is split across one frame so the editor keeps exclusive access to
//! the note while it is laid out:
//! 1. `handle_completion_keys` consumes navigation keys before the editor sees them;
//! 2. `update_completion_context` reads the caret after layout;
//! 3. `finish_completion_frame` ranks candidates, draws the popup and applies a choice.

use super::state::CompletionState;
use super::*;
use crate::editor_tools::{self, CompletionKind};
use eframe::egui::text_edit::TextEditOutput;

const MAX_SUGGESTIONS: usize = 8;

/// Consumes Up/Down/Enter/Tab while suggestions are visible for `note_id`.
pub(super) fn handle_completion_keys(
    ctx: &egui::Context,
    state: &mut CompletionState,
    note_id: Uuid,
) {
    if !state.is_visible() || state.note_id != Some(note_id) {
        return;
    }
    let count = state.items.len();
    ctx.input_mut(|input| {
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            state.selected = (state.selected + 1) % count;
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            state.selected = (state.selected + count - 1) % count;
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
        {
            state.accept = Some(state.selected);
        }
    });
}

/// Recomputes the completion context from the caret after the editor is laid out.
pub(super) fn update_completion_context(
    ui: &egui::Ui,
    state: &mut CompletionState,
    note: &Note,
    editor_id: egui::Id,
    output: &TextEditOutput,
) {
    let focused = ui.memory(|memory| memory.has_focus(editor_id));
    let caret = output.cursor_range.and_then(|range| range.single());
    let context = caret
        .filter(|_| focused)
        .and_then(|caret| editor_tools::completion_context(&note.content, caret.index.into()))
        .filter(|context| state.dismissed != Some((note.id, context.start)));

    let Some(context) = context else {
        if state.accept.is_none() {
            state.clear();
        }
        return;
    };
    if let Some(caret) = caret {
        let caret_rect = output
            .galley
            .pos_from_cursor(caret)
            .translate(output.galley_pos.to_vec2());
        state.anchor = Some(caret_rect.left_bottom() + egui::vec2(0.0, 4.0));
    }
    if state.note_id != Some(note.id) || state.context.as_ref() != Some(&context) {
        state.note_id = Some(note.id);
        state.context = Some(context);
        state.selected = 0;
        state.stale = true;
    }
}

impl LiloApp {
    pub(super) fn finish_completion_frame(&mut self, ctx: &egui::Context) {
        if self.completion.stale {
            self.refresh_completion_items();
        }
        if self.completion.accept.is_none() && self.completion.is_visible() {
            self.show_completion_popup(ctx);
        }
        if let Some(index) = self.completion.accept.take() {
            self.apply_completion(ctx, index);
        }
    }

    fn refresh_completion_items(&mut self) {
        self.completion.stale = false;
        let Some(context) = &self.completion.context else {
            self.completion.items.clear();
            return;
        };
        let current = self.completion.note_id;
        self.completion.items = match context.kind {
            CompletionKind::WikiLink => {
                let candidates = self
                    .data
                    .notes
                    .iter()
                    .filter(|note| Some(note.id) != current)
                    .flat_map(|note| {
                        std::iter::once(note.title.as_str())
                            .chain(note.aliases.iter().map(String::as_str))
                    });
                editor_tools::rank(&context.query, candidates, MAX_SUGGESTIONS)
            }
            CompletionKind::Tag => {
                let candidates = self
                    .tag_index
                    .all_tags()
                    .iter()
                    .map(|info| info.tag.as_str());
                editor_tools::rank(&context.query, candidates, MAX_SUGGESTIONS)
            }
        }
        .into_iter()
        // A suggestion identical to what is already typed adds nothing.
        .filter(|item| *item != context.query)
        .map(str::to_owned)
        .collect();
        self.completion.selected = self
            .completion
            .selected
            .min(self.completion.items.len().saturating_sub(1));
    }

    fn show_completion_popup(&mut self, ctx: &egui::Context) {
        let Some(anchor) = self.completion.anchor else {
            return;
        };
        let kind = self.completion.context.as_ref().map(|context| context.kind);
        let mut clicked = None;
        egui::Area::new(egui::Id::new("editor_completion_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.set_max_width(320.0);
                    for (index, item) in self.completion.items.iter().enumerate() {
                        let label = match kind {
                            Some(CompletionKind::Tag) => format!("#{item}"),
                            _ => item.clone(),
                        };
                        let selected = index == self.completion.selected;
                        if ui.selectable_label(selected, label).clicked() {
                            clicked = Some(index);
                        }
                    }
                    ui_style::muted(ui, "Enter/Tab to insert · Esc to dismiss");
                });
            });
        if clicked.is_some() {
            self.completion.accept = clicked;
        }
    }

    fn apply_completion(&mut self, ctx: &egui::Context, index: usize) {
        let (Some(note_id), Some(context), Some(value)) = (
            self.completion.note_id,
            self.completion.context.clone(),
            self.completion.items.get(index).cloned(),
        ) else {
            self.completion.clear();
            return;
        };
        self.completion.clear();
        // Do not reopen suggestions for the token that was just completed.
        self.completion.dismissed = Some((note_id, context.start));
        let Some(note) = self.data.notes.iter_mut().find(|note| note.id == note_id) else {
            return;
        };
        let caret = editor_tools::apply_completion(&mut note.content, &context, &value);
        note.mark_as_updated();
        markdown::set_cursor_char_index(ctx, status::editor_id(note_id), caret);
        self.schedule_note_index_refresh(note_id);
        self.mark_note_dirty(note_id);
        self.record_analytics(AnalyticsFeature::CompletionAccepted);
    }
}
