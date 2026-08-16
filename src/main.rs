#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod folders;
mod graph;
mod links;
mod markdown;
mod platform;
mod storage;
mod ui_style;

use eframe::egui;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 450.0])
            .with_min_inner_size([250.0, 180.0])
            .with_decorations(false)
            .with_always_on_top(),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "Lilo",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(app::WidgetApp::new()))),
    )
}
