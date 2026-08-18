#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod daily;
mod folders;
mod graph;
mod links;
mod markdown;
mod platform;
mod quick_capture;
mod search;
mod storage;
mod templates;
mod ui_style;

use eframe::egui;

fn load_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png")).ok()
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([400.0, 520.0])
        .with_min_inner_size([280.0, 200.0])
        .with_decorations(false)
        .with_resizable(true)
        .with_always_on_top();

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "Lilo",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(app::WidgetApp::new()))),
    )
}
