use crate::storage::QuickCaptureTarget;
use crate::{storage::Note, ui_style};
use chrono::{DateTime, Local};
use eframe::egui::{self, Align2, FontId, Key};
use std::path::Path;
use uuid::Uuid;

#[derive(Default)]
pub struct QuickCaptureState {
    pub is_open: bool,
    pub text: String,
    pub focus_input: bool,
    pub selected_target: Option<QuickCaptureTarget>,
    pub custom_note_name: String,
    pub error: Option<String>,
    pub chosen_note_id: Option<Uuid>,
}

impl QuickCaptureState {
    pub fn open(&mut self) {
        if self.is_open {
            self.focus_input = true;
            return;
        }
        self.is_open = true;
        self.error = None;
        self.text.clear();
        self.focus_input = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.text.clear();
        self.selected_target = None;
        self.chosen_note_id = None;
    }
}

pub struct QuickCaptureSubmission {
    pub text: String,
    pub timestamp: DateTime<Local>,
    pub target: QuickCaptureTarget,
    pub existing_note_id: Option<Uuid>,
}

/// Formats a quick capture entry with timestamp and bullet.
pub fn format_capture_entry(text: &str, timestamp: DateTime<Local>) -> String {
    let trimmed = text.trim();
    let time_prefix = timestamp.format("%H:%M").to_string();
    if trimmed.lines().count() <= 1 {
        format!("- {time_prefix} {trimmed}\n")
    } else {
        let mut result = format!("- {time_prefix}\n");
        for line in trimmed.lines() {
            result.push_str(&format!("  {line}\n"));
        }
        result
    }
}

pub fn show_quick_capture(
    ctx: &egui::Context,
    state: &mut QuickCaptureState,
    default_target: &QuickCaptureTarget,
    default_custom_note: &str,
    notes: &[Note],
    root: &Path,
) -> Option<QuickCaptureSubmission> {
    if !state.is_open {
        return None;
    }
    if state.selected_target.is_none() {
        state.selected_target = Some(default_target.clone());
    }
    if state.custom_note_name.is_empty() {
        state.custom_note_name = default_custom_note.to_owned();
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        state.close();
        return None;
    }
    let mut submission = None;
    let screen = ui_style::screen_rect(ctx);
    egui::Modal::new(egui::Id::new("quick_capture_area"))
        .frame(ui_style::modal_frame(ctx))
        .area(
            egui::Modal::default_area(egui::Id::new("quick_capture_area"))
                .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).clamp(180.0, 520.0));
            ui.heading("Quick Capture");
            egui::ScrollArea::vertical()
                .id_salt("capture_body")
                .max_height((screen.height() - 155.0).max(60.0))
                .show(ui, |ui| {
                    let input = ui.add(
                        egui::TextEdit::multiline(&mut state.text)
                            .id(egui::Id::new("quick_capture_input"))
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .font(FontId::proportional(16.0))
                            .hint_text("A thought, a task, something to remember…"),
                    );
                    if state.focus_input {
                        input.request_focus();
                        state.focus_input = false;
                    }
                    ui.label("Save to");
                    ui.horizontal_wrapped(|ui| {
                        for (target, label) in [
                            (QuickCaptureTarget::DailyNote, "Today"),
                            (QuickCaptureTarget::Inbox, "Inbox"),
                            (QuickCaptureTarget::NewNote, "New note"),
                        ] {
                            if ui
                                .selectable_label(
                                    state.chosen_note_id.is_none()
                                        && state.selected_target.as_ref() == Some(&target),
                                    label,
                                )
                                .clicked()
                            {
                                state.selected_target = Some(target);
                                state.chosen_note_id = None;
                            }
                        }
                    });
                    let chosen = state
                        .chosen_note_id
                        .and_then(|id| notes.iter().find(|note| note.id == id));
                    egui::ComboBox::from_id_salt("capture_note_picker")
                        .width(ui.available_width())
                        .selected_text(chosen.map_or("Choose note…", |note| note.title.as_str()))
                        .show_ui(ui, |ui| {
                            for note in notes {
                                let path =
                                    note.file_path.strip_prefix(root).unwrap_or(&note.file_path);
                                if ui
                                    .selectable_label(
                                        state.chosen_note_id == Some(note.id),
                                        format!(
                                            "{} · {}",
                                            if note.title.is_empty() {
                                                "Untitled"
                                            } else {
                                                &note.title
                                            },
                                            path.display()
                                        ),
                                    )
                                    .clicked()
                                {
                                    state.chosen_note_id = Some(note.id);
                                }
                            }
                            if notes.is_empty() {
                                ui_style::muted(
                                    ui,
                                    "No existing notes. Choose Today, Inbox or New note.",
                                );
                            }
                        });
                    if state.chosen_note_id.is_none()
                        && matches!(
                            state.selected_target,
                            Some(QuickCaptureTarget::CustomNote(_))
                        )
                    {
                        ui_style::muted(ui, "Configured named destination (created if missing)");
                        if ui
                            .text_edit_singleline(&mut state.custom_note_name)
                            .changed()
                        {
                            state.selected_target = Some(QuickCaptureTarget::CustomNote(
                                state.custom_note_name.clone(),
                            ));
                        }
                    }
                    if let Some(error) = &state.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                });
            ui_style::muted(ui, "Ctrl+Enter: save · Esc: cancel");
            let shortcut = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Enter));
            ui.horizontal(|ui| {
                let ready = !state.text.trim().is_empty();
                let save = ui
                    .add_enabled_ui(ready, |ui| ui_style::primary_button(ui, "Save"))
                    .inner
                    .clicked();
                if ready && (save || shortcut) {
                    submission = Some(QuickCaptureSubmission {
                        text: state.text.trim().to_owned(),
                        timestamp: Local::now(),
                        target: state
                            .selected_target
                            .clone()
                            .unwrap_or_else(|| default_target.clone()),
                        existing_note_id: state.chosen_note_id,
                    });
                }
                if ui.button("Cancel").clicked() {
                    state.close();
                }
            });
        });
    submission
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn capture_requires_ctrl_enter_and_keeps_plain_enter_for_multiline_input() {
        let ctx = egui::Context::default();
        let mut state = QuickCaptureState::default();
        state.open();
        state.text = "Первая строка".to_owned();
        for ctrl in [false, true] {
            let modifiers = egui::Modifiers {
                ctrl,
                ..Default::default()
            };
            let mut submitted = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360.0, 520.0),
                    )),
                    events: vec![
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Key {
                            key: Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                    ],
                    ..Default::default()
                },
                |_| {
                    submitted = show_quick_capture(
                        &ctx,
                        &mut state,
                        &QuickCaptureTarget::Inbox,
                        "",
                        &[],
                        Path::new(""),
                    );
                },
            );
            output.textures_delta.clear();
            assert_eq!(submitted.is_some(), ctrl);
            assert!(
                state.is_open,
                "Only a successful storage write closes capture"
            );
        }
    }

    #[test]
    fn formats_single_line_capture() {
        let time = Local.with_ymd_and_hms(2026, 8, 18, 9, 15, 0).unwrap();
        let entry = format_capture_entry("Review PR #42", time);
        assert_eq!(entry, "- 09:15 Review PR #42\n");
    }

    #[test]
    fn formats_multi_line_capture() {
        let time = Local.with_ymd_and_hms(2026, 8, 18, 9, 15, 0).unwrap();
        let entry = format_capture_entry("Task item\nDetails line 2", time);
        assert_eq!(entry, "- 09:15\n  Task item\n  Details line 2\n");
    }

    #[test]
    fn quick_capture_state_open_and_close() {
        let mut state = QuickCaptureState::default();
        assert!(!state.is_open);

        state.open();
        assert!(state.is_open);
        assert!(state.focus_input);

        state.text = "Temporary note".to_owned();
        state.close();
        assert!(!state.is_open);
        assert!(state.text.is_empty());
    }
}
