use super::*;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub(super) struct NavigationIndex {
    pub tree: folders::FolderTree,
    pub outgoing: HashMap<Uuid, Vec<String>>,
}

impl WidgetApp {
    pub(super) fn navigation_index(&mut self) -> Arc<NavigationIndex> {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            self.link_index.generation(),
            &self.storage_paths.notes_dir,
            &self.folder_paths,
        )
            .hash(&mut hash);
        let key = hash.finish();
        if self
            .navigation_cache
            .as_ref()
            .is_none_or(|(old, _)| *old != key)
        {
            let titles: HashMap<_, _> = self
                .data
                .notes
                .iter()
                .map(|note| (note.id, &note.title))
                .collect();
            let outgoing =
                self.data
                    .notes
                    .iter()
                    .map(|note| {
                        let targets =
                            self.link_index
                                .links_for(note.id)
                                .map_or_else(Vec::new, |links| {
                                    let mut targets = links.unresolved.clone();
                                    targets.extend(links.outgoing.iter().filter_map(|id| {
                                        titles.get(id).map(|title| (*title).clone())
                                    }));
                                    targets
                                });
                        (note.id, targets)
                    })
                    .collect();
            let mut tree = folders::FolderTree::build(
                &self.data.notes,
                &self.storage_paths.notes_dir,
                &self.folder_paths,
            );
            tree.root.name = storage::vault_name(&self.settings.vault_path);
            self.navigation_cache = Some((key, Arc::new(NavigationIndex { tree, outgoing })));
        }
        Arc::clone(&self.navigation_cache.as_ref().unwrap().1)
    }

    pub(super) fn toggle_explorer(&mut self) {
        if self.viewport_width < ui_style::NAV_BREAKPOINT {
            self.explorer_drawer_open = !self.explorer_drawer_open;
        } else {
            self.settings.left_sidebar_open = !self.settings.left_sidebar_open;
            self.save_settings();
        }
    }

    pub(super) fn toggle_inspector(&mut self) {
        if self.viewport_width < ui_style::WIDE_BREAKPOINT {
            self.note_details_open = !self.note_details_open;
        } else {
            self.settings.right_sidebar_open = !self.settings.right_sidebar_open;
            self.save_settings();
        }
    }

    pub(super) fn show_workspace_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui_style::icon_button(
                ui,
                Icon::SidebarLeft,
                self.settings.left_sidebar_open,
                "Toggle sidebar (Ctrl+Shift+B)",
            )
            .clicked()
            {
                self.toggle_explorer();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui_style::icon_button(ui, Icon::Close, false, "Close Lilo").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui_style::icon_button(ui, Icon::Maximize, false, "Maximize / restore").clicked()
                {
                    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                if ui_style::icon_button(ui, Icon::Minimize, false, "Minimize").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                self.show_toolbar_menu(ui, true);
                if self.view == AppView::Editor
                    && ui_style::icon_button(
                        ui,
                        Icon::SidebarRight,
                        self.settings.right_sidebar_open,
                        "Note context (Ctrl+Shift+I)",
                    )
                    .clicked()
                {
                    self.toggle_inspector();
                }
                if !self.settings.left_sidebar_open
                    && ui_style::icon_button(ui, Icon::Search, false, "Search (Ctrl+K)").clicked()
                {
                    self.command_palette_state.open();
                }
                let title = ui.allocate_response(
                    egui::vec2(ui.available_width(), 28.0),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().text(
                    title.rect.left_center() + egui::vec2(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "Lilo",
                    egui::FontId::proportional(16.0),
                    ui.visuals().text_color(),
                );
                if title.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if title.double_clicked() {
                    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
            });
        });
    }
}
