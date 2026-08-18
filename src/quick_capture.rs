//! Quick Capture modal and buffer synchronization helpers.

use chrono::{DateTime, Local};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Pos2, Stroke, Vec2};

#[derive(Default)]
pub struct QuickCaptureState {
    pub is_open: bool,
    pub text: String,
    pub focus_input: bool,
}

impl QuickCaptureState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.text.clear();
        self.focus_input = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.text.clear();
    }
}

pub struct QuickCaptureSubmission {
    pub text: String,
    pub timestamp: DateTime<Local>,
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

/// Renders the Quick Capture popup modal.
pub fn show_quick_capture(
    ctx: &egui::Context,
    state: &mut QuickCaptureState,
    target_label: &str,
) -> Option<QuickCaptureSubmission> {
    if !state.is_open {
        return None;
    }

    let mut submission = None;

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        state.close();
        return None;
    }

    let submit_shortcut = ctx.input(|i| {
        (i.modifiers.ctrl && i.key_pressed(Key::Enter))
            || (!i.modifiers.shift
                && !i.modifiers.ctrl
                && !i.modifiers.alt
                && i.key_pressed(Key::Enter))
    });

    let screen_rect = crate::ui_style::screen_rect(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("quick_capture_dim"),
    ));
    painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(140));

    let modal_width = 540.0_f32.min(screen_rect.width() - 32.0);
    let modal_pos = Pos2::new(screen_rect.center().x, screen_rect.top() + 90.0);

    egui::Area::new(egui::Id::new("quick_capture_area"))
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
                .inner_margin(egui::Margin::same(14))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(180),
                })
                .show(ui, |ui| {
                    ui.set_width(modal_width);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Quick Capture").strong().size(16.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("Target: {target_label}"))
                                    .small()
                                    .color(ui.visuals().hyperlink_color),
                            );
                        });
                    });

                    ui.add_space(8.0);

                    let input_id = egui::Id::new("quick_capture_input");
                    let input = ui.add(
                        egui::TextEdit::multiline(&mut state.text)
                            .id(input_id)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4)
                            .hint_text(
                                "Capture a thought, task or note... (Enter or Ctrl+Enter to save)",
                            )
                            .font(FontId::proportional(14.0))
                            .margin(egui::Margin::symmetric(10, 8)),
                    );

                    if state.focus_input {
                        input.request_focus();
                        state.focus_input = false;
                    }

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Enter: Save  •  Esc: Cancel")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let can_submit = !state.text.trim().is_empty();
                            if ui
                                .add_enabled(
                                    can_submit,
                                    egui::Button::new("Save Capture")
                                        .fill(ui.visuals().selection.bg_fill)
                                        .min_size(Vec2::new(100.0, 26.0)),
                                )
                                .clicked()
                                || (can_submit && submit_shortcut)
                            {
                                submission = Some(QuickCaptureSubmission {
                                    text: state.text.trim().to_string(),
                                    timestamp: Local::now(),
                                });
                                state.close();
                            }

                            if ui.button("Cancel").clicked() {
                                state.close();
                            }
                        });
                    });
                });
        });

    submission
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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
