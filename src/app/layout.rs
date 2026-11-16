//! Responsive layout decisions shared by the header, panels and floating overlays.
//!
//! The visible layout depends on window width, Zen mode and the active view.
//! It is computed once per frame so header buttons reflect what is actually on
//! screen, and at most one floating overlay is open at a time so overlays never
//! stack on top of each other or duplicate a docked panel.

use super::*;

/// Floating windows that stand in for panels or views on narrow layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Overlay {
    /// The explorer drawer used below the navigation breakpoint.
    Explorer,
    /// The note inspector used when there is no room for the right panel.
    NoteDetails,
    /// The small contextual knowledge graph.
    Graph,
}

/// What is actually visible this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct EffectiveLayout {
    pub(super) compact_header: bool,
    pub(super) left_panel: bool,
    pub(super) right_panel: bool,
    pub(super) status_bar: bool,
}

impl LiloApp {
    pub(super) fn compute_layout(&self, window_width: f32) -> EffectiveLayout {
        let full_graph = self.view == AppView::Graph && self.graph_fullscreen;
        let zen = self.settings.zen_mode;
        EffectiveLayout {
            compact_header: window_width < ui_style::COMPACT_WIDTH,
            left_panel: !full_graph
                && !zen
                && self.settings.left_sidebar_open
                && window_width >= ui_style::NAV_BREAKPOINT,
            right_panel: !full_graph
                && !zen
                && self.settings.right_sidebar_open
                && window_width >= ui_style::WIDE_BREAKPOINT
                && self.view == AppView::Editor,
            status_bar: self.settings.show_status_bar && !zen,
        }
    }

    pub(super) fn overlay_is(&self, overlay: Overlay) -> bool {
        self.overlay == Some(overlay)
    }

    /// Opens `overlay`, closing any other floating overlay.
    pub(super) fn open_overlay(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    pub(super) fn close_overlay(&mut self, overlay: Overlay) {
        if self.overlay_is(overlay) {
            self.overlay = None;
        }
    }

    pub(super) fn toggle_overlay(&mut self, overlay: Overlay) {
        if self.overlay_is(overlay) {
            self.overlay = None;
        } else {
            self.open_overlay(overlay);
        }
    }

    /// Closes overlays that became redundant after a resize, mode or view change.
    pub(super) fn reconcile_overlays(&mut self, window_width: f32) {
        let redundant = match self.overlay {
            Some(Overlay::Explorer) => window_width >= ui_style::NAV_BREAKPOINT,
            Some(Overlay::NoteDetails) => {
                self.layout.right_panel
                    || self.view != AppView::Editor
                    || self.data.selected_note().is_none()
            }
            Some(Overlay::Graph) => self.view == AppView::Graph,
            None => false,
        };
        if redundant {
            self.overlay = None;
        }
    }

    pub(super) fn toggle_explorer(&mut self) {
        if self.viewport_width < ui_style::NAV_BREAKPOINT {
            self.toggle_overlay(Overlay::Explorer);
        } else if self.settings.zen_mode || !self.layout.left_panel {
            // A hidden panel is shown, leaving Zen mode if that is what hides it.
            self.settings.zen_mode = false;
            self.settings.left_sidebar_open = true;
            self.graph_fullscreen = false;
            self.save_settings();
        } else {
            self.settings.left_sidebar_open = false;
            self.save_settings();
        }
    }

    pub(super) fn toggle_inspector(&mut self) {
        if self.view != AppView::Editor {
            self.activate_view(AppView::Editor);
        }
        if self.settings.zen_mode {
            self.settings.zen_mode = false;
            self.settings.right_sidebar_open = true;
            self.save_settings();
        } else if self.viewport_width < ui_style::WIDE_BREAKPOINT {
            self.toggle_overlay(Overlay::NoteDetails);
        } else {
            self.settings.right_sidebar_open = !self.layout.right_panel;
            self.save_settings();
        }
    }
}

impl LiloApp {
    /// Draws the open overlay, sized to fit the current window.
    pub(super) fn show_overlay(&mut self, ctx: &egui::Context, window_width: f32) {
        let Some(overlay) = self.overlay else {
            return;
        };
        let screen = ui_style::screen_rect(ctx).shrink(8.0);
        let fit = |width: f32, height: f32| {
            egui::vec2(
                width.min(screen.width()).max(160.0),
                height
                    .min(screen.height() - ui_style::TOP_BAR_HEIGHT)
                    .max(120.0),
            )
        };
        let mut open = true;
        let mut graph_output = None;
        match overlay {
            Overlay::Explorer => {
                let selected_before = self.data.selected_note_id;
                egui::Window::new("Your notes")
                    .id(egui::Id::new("explorer_drawer"))
                    .open(&mut open)
                    .collapsible(false)
                    .constrain_to(screen)
                    .max_width(screen.width())
                    .max_height(screen.height())
                    .default_pos(screen.left_top() + egui::vec2(0.0, ui_style::TOP_BAR_HEIGHT))
                    .default_size(fit(window_width - 48.0, screen.height()))
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| self.show_left_explorer(ui));
                    });
                // Picking a note in the drawer reveals it instead of leaving the drawer on top.
                if self.data.selected_note_id != selected_before {
                    open = false;
                }
            }
            Overlay::NoteDetails => {
                egui::Window::new("Note details")
                    .id(egui::Id::new("note_details"))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .constrain_to(screen)
                    .max_width(screen.width())
                    .max_height(screen.height())
                    .default_pos(egui::pos2(
                        screen.right() - fit(300.0, 0.0).x,
                        screen.top() + ui_style::TOP_BAR_HEIGHT,
                    ))
                    .default_size(fit(300.0, 360.0))
                    .show(ctx, |ui| {
                        if let Some(note) = self.data.selected_note().cloned() {
                            self.show_right_inspector(ui, &note);
                        }
                    });
            }
            Overlay::Graph => {
                egui::Window::new("Knowledge graph")
                    .id(egui::Id::new("graph_overlay"))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .constrain_to(screen)
                    .max_width(screen.width())
                    .max_height(screen.height())
                    .default_size(fit(320.0, 360.0))
                    .show(ctx, |ui| {
                        graph_output = Some(graph::show(
                            ui,
                            &mut self.graph_state,
                            &self.data.notes,
                            &self.link_index,
                            self.data.selected_note_id,
                            &self.storage_paths.notes_dir,
                            &self.settings.selected_folder,
                        ));
                    });
            }
        }
        // The window content may have switched to another overlay; only close ours.
        if !open {
            self.close_overlay(overlay);
        }
        if graph_output.is_some_and(|output| self.handle_graph_output(output)) {
            self.close_overlay(Overlay::Graph);
        }
    }
}
