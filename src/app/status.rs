//! Writing statistics shown in the bottom status bar.

use super::*;
use crate::editor_tools::{self, TextStats};

/// The egui id that owns the live editor's cursor and undo state for a note.
pub(super) fn editor_id(note_id: Uuid) -> egui::Id {
    egui::Id::new(("markdown_editor", note_id))
}

/// The current non-empty selection of a note editor, as character indices.
pub(super) fn editor_selection(
    ctx: &egui::Context,
    note_id: Uuid,
) -> Option<std::ops::Range<usize>> {
    let state = egui::text_edit::TextEditState::load(ctx, editor_id(note_id))?;
    let range = state.cursor.char_range()?.as_sorted_char_range();
    let range = usize::from(range.start)..usize::from(range.end);
    (!range.is_empty()).then_some(range)
}

impl LiloApp {
    pub(super) fn show_note_statistics(&self, ui: &mut egui::Ui, note: &Note, detailed: bool) {
        let stats = TextStats::of(&note.content);
        let selection = editor_selection(ui.ctx(), note.id)
            .map(|range| TextStats::of(editor_tools::char_slice(&note.content, range)));

        let summary = match (selection, detailed) {
            (Some(selected), _) => format!(
                "Selected {} of {} words · {} chars",
                selected.words, stats.words, selected.chars
            ),
            (None, true) => format!(
                "{} words · {} chars · {} min read",
                stats.words,
                stats.chars,
                stats.reading_minutes()
            ),
            (None, false) => format!("{} words", stats.words),
        };
        ui_style::muted(ui, summary).on_hover_text(format!(
            "Words: {}\nCharacters: {}\nCharacters without spaces: {}\nLines: {}\nReading time: about {} min",
            stats.words,
            stats.chars,
            stats.chars_without_spaces,
            stats.lines,
            stats.reading_minutes()
        ));
    }
}
