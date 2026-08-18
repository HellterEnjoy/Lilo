//! Shared visual language, 3-layer color depth, typography, and layout components.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextStyle, Ui, Vec2, ViewportCommand, viewport::ResizeDirection,
};

pub const TOP_BAR_HEIGHT: f32 = 42.0;
pub const BOTTOM_BAR_HEIGHT: f32 = 28.0;
pub const TOOL_SIZE: f32 = 30.0;
pub const PANEL_MARGIN: i8 = 10;
pub const COMPACT_WIDTH: f32 = 600.0;
pub const WIDE_BREAKPOINT: f32 = 860.0;
#[allow(dead_code)]
pub const EDITOR_SHEET_MAX_WIDTH: f32 = 780.0;

pub const NAV_BREAKPOINT: f32 = 600.0;
#[allow(dead_code)]
pub const EXPANDED_NAV_BREAKPOINT: f32 = 960.0;
#[allow(dead_code)]
pub const NAV_RAIL_WIDTH: f32 = 52.0;
#[allow(dead_code)]
pub const NAV_PANEL_WIDTH: f32 = 260.0;
pub const INSPECTOR_PANEL_WIDTH: f32 = 250.0;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Icon {
    Editor,
    Notes,
    Graph,
    Trash,
    Settings,
    Add,
    Folder,
    Restore,
    Minimize,
    Maximize,
    Close,
    SidebarLeft,
    SidebarRight,
    Outline,
    Backlinks,
    Tag,
    Calendar,
    Daily,
    Inbox,
}

pub fn layer0_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(13, 15, 20) // Deep dark space (#0d0f14)
    } else {
        Color32::from_rgb(238, 240, 245)
    }
}

pub fn layer1_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(20, 23, 31) // Matte sidebar/bar (#14171f)
    } else {
        Color32::from_rgb(247, 248, 251)
    }
}

pub fn layer2_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(27, 31, 42) // Elevated editor sheet / modal (#1b1f2a)
    } else {
        Color32::WHITE
    }
}

fn paint_icon(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let center = rect.center();
    let stroke = Stroke::new(1.6, color);
    let r = 7.0;
    match icon {
        Icon::Editor => {
            painter.line_segment(
                [center + Vec2::new(-5.0, 5.0), center + Vec2::new(5.0, -5.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(-6.0, 6.0), center + Vec2::new(-2.0, 5.0)],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(15.0)),
                CornerRadius::same(3),
                Stroke::new(1.0, color.gamma_multiply(0.55)),
                StrokeKind::Inside,
            );
        }
        Icon::Notes => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(14.0, 16.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            for y in [-4.0, 0.0, 4.0] {
                painter.line_segment(
                    [center + Vec2::new(-4.0, y), center + Vec2::new(4.5, y)],
                    Stroke::new(1.2, color),
                );
            }
        }
        Icon::Graph => {
            let points = [
                center + Vec2::new(0.0, -6.0),
                center + Vec2::new(-6.0, 5.0),
                center + Vec2::new(7.0, 4.0),
            ];
            painter.line_segment([points[0], points[1]], stroke);
            painter.line_segment([points[0], points[2]], stroke);
            painter.line_segment([points[1], points[2]], stroke);
            for point in points {
                painter.circle_filled(point, 2.4, color);
            }
        }
        Icon::Trash => {
            painter.rect_stroke(
                Rect::from_min_max(center + Vec2::new(-5.5, -3.5), center + Vec2::new(5.5, 7.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-7.0, -6.0),
                    center + Vec2::new(7.0, -6.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-2.5, -8.0),
                    center + Vec2::new(2.5, -8.0),
                ],
                stroke,
            );
        }
        Icon::Settings => {
            painter.circle_stroke(center, 6.0, stroke);
            painter.circle_filled(center, 2.0, color);
            for index in 0..8 {
                let direction = Vec2::angled(index as f32 * std::f32::consts::TAU / 8.0);
                painter.line_segment([center + direction * 7.0, center + direction * 9.0], stroke);
            }
        }
        Icon::Add => {
            painter.line_segment(
                [center + Vec2::new(-r, 0.0), center + Vec2::new(r, 0.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(0.0, -r), center + Vec2::new(0.0, r)],
                stroke,
            );
        }
        Icon::Folder => {
            let folder =
                Rect::from_min_max(center + Vec2::new(-8.0, -5.0), center + Vec2::new(8.0, 6.0));
            painter.rect_stroke(folder, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [
                    center + Vec2::new(-6.0, -7.0),
                    center + Vec2::new(0.0, -7.0),
                ],
                stroke,
            );
        }
        Icon::Restore => {
            painter.circle_stroke(center + Vec2::new(1.0, 0.0), 6.0, stroke);
            painter.line_segment(
                [
                    center + Vec2::new(-7.0, -1.0),
                    center + Vec2::new(-2.0, -5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-7.0, -1.0),
                    center + Vec2::new(-7.0, -6.0),
                ],
                stroke,
            );
        }
        Icon::Close => {
            painter.line_segment(
                [center + Vec2::new(-5.0, -5.0), center + Vec2::new(5.0, 5.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(5.0, -5.0), center + Vec2::new(-5.0, 5.0)],
                stroke,
            );
        }
        Icon::Minimize => {
            painter.line_segment(
                [center + Vec2::new(-6.0, 4.0), center + Vec2::new(6.0, 4.0)],
                stroke,
            );
        }
        Icon::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(12.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
        }
        Icon::SidebarLeft => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(16.0, 14.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-3.0, -7.0),
                    center + Vec2::new(-3.0, 7.0),
                ],
                stroke,
            );
        }
        Icon::SidebarRight => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(16.0, 14.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [center + Vec2::new(3.0, -7.0), center + Vec2::new(3.0, 7.0)],
                stroke,
            );
        }
        Icon::Outline => {
            for (y, w) in [(-5.0, 12.0), (-1.0, 8.0), (3.0, 10.0), (7.0, 6.0)] {
                painter.line_segment(
                    [center + Vec2::new(-6.0, y), center + Vec2::new(-6.0 + w, y)],
                    stroke,
                );
            }
        }
        Icon::Backlinks => {
            painter.line_segment(
                [center + Vec2::new(-6.0, 0.0), center + Vec2::new(6.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-3.0, -4.0),
                    center + Vec2::new(-6.0, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(-3.0, 4.0), center + Vec2::new(-6.0, 0.0)],
                stroke,
            );
        }
        Icon::Tag => {
            painter.line_segment(
                [
                    center + Vec2::new(-6.0, -6.0),
                    center + Vec2::new(2.0, -6.0),
                ],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(2.0, -6.0), center + Vec2::new(6.0, -2.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(6.0, -2.0), center + Vec2::new(-2.0, 6.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(-2.0, 6.0), center + Vec2::new(-6.0, 2.0)],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-6.0, 2.0),
                    center + Vec2::new(-6.0, -6.0),
                ],
                stroke,
            );
            painter.circle_filled(center + Vec2::new(-2.0, -2.0), 1.5, color);
        }
        Icon::Calendar => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(14.0, 14.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-7.0, -3.0),
                    center + Vec2::new(7.0, -3.0),
                ],
                Stroke::new(1.0, color),
            );
            painter.line_segment(
                [
                    center + Vec2::new(-4.0, -7.0),
                    center + Vec2::new(-4.0, -5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(4.0, -7.0), center + Vec2::new(4.0, -5.0)],
                stroke,
            );
        }
        Icon::Daily => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(14.0, 14.0)),
                CornerRadius::same(3),
                stroke,
                StrokeKind::Inside,
            );
            painter.circle_filled(center + Vec2::new(0.0, 1.0), 2.2, color);
        }
        Icon::Inbox => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::new(15.0, 13.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [center + Vec2::new(-7.5, 1.0), center + Vec2::new(-3.0, 1.0)],
                Stroke::new(1.2, color),
            );
            painter.line_segment(
                [center + Vec2::new(3.0, 1.0), center + Vec2::new(7.5, 1.0)],
                Stroke::new(1.2, color),
            );
            painter.line_segment(
                [center + Vec2::new(-3.0, 1.0), center + Vec2::new(-2.0, 4.0)],
                Stroke::new(1.2, color),
            );
            painter.line_segment(
                [center + Vec2::new(3.0, 1.0), center + Vec2::new(2.0, 4.0)],
                Stroke::new(1.2, color),
            );
            painter.line_segment(
                [center + Vec2::new(-2.0, 4.0), center + Vec2::new(2.0, 4.0)],
                Stroke::new(1.2, color),
            );
        }
    }
}

fn painted_button(
    ui: &mut Ui,
    icon: Icon,
    selected: bool,
    label: &str,
    expanded: bool,
    fill_width: bool,
) -> Response {
    let size = Vec2::new(
        if expanded {
            if fill_width {
                (ui.available_width() - 2.0).max(TOOL_SIZE)
            } else {
                (42.0 + label.chars().count() as f32 * 7.0).max(TOOL_SIZE)
            }
        } else {
            TOOL_SIZE
        },
        TOOL_SIZE,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect.expand(visuals.expansion),
            visuals.corner_radius,
            if selected {
                ui.visuals().selection.bg_fill
            } else {
                visuals.weak_bg_fill
            },
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let icon_center = if expanded {
            Pos2::new(rect.left() + 16.0, rect.center().y)
        } else {
            rect.center()
        };
        let icon_rect = Rect::from_center_size(icon_center, Vec2::splat(20.0));
        let color = if selected {
            ui.visuals().hyperlink_color
        } else {
            visuals.fg_stroke.color
        };
        paint_icon(ui, icon_rect, icon, color);
        if expanded {
            ui.painter().text(
                Pos2::new(rect.left() + 32.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(14.0),
                color,
            );
        }
    }
    response.on_hover_text(label)
}

pub fn apply_theme(ctx: &egui::Context, dark: bool, accent: Color32, ui_font_size: f32) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    let mut style = (*ctx.style_of(theme)).clone();
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let _layer0 = layer0_color(dark);
    let layer1 = layer1_color(dark);
    let layer2 = layer2_color(dark);

    let (hover, border, text, muted) = if dark {
        (
            Color32::from_rgb(38, 44, 58),
            Color32::from_rgb(36, 41, 54),
            Color32::from_rgb(235, 238, 245),
            Color32::from_rgb(140, 148, 165),
        )
    } else {
        (
            Color32::from_rgb(228, 232, 240),
            Color32::from_rgb(218, 223, 232),
            Color32::from_rgb(24, 28, 36),
            Color32::from_rgb(105, 112, 128),
        )
    };

    visuals.panel_fill = layer1;
    visuals.window_fill = layer2;
    visuals.extreme_bg_color = layer2;
    visuals.text_edit_bg_color = Some(Color32::TRANSPARENT);
    visuals.faint_bg_color = hover.gamma_multiply(0.45);
    visuals.code_bg_color = hover.gamma_multiply(0.65);
    visuals.override_text_color = Some(text);
    visuals.weak_text_color = Some(muted);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, accent);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0, border);

    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(7);
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_fill = layer1;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.7));
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.35);
    visuals.widgets.active.bg_fill = accent.gamma_multiply(0.45);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(7);
    visuals.widgets.open.weak_bg_fill = hover;
    visuals.widgets.open.bg_fill = hover;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.75));
    visuals.widgets.open.corner_radius = CornerRadius::same(7);

    style.spacing.item_spacing = Vec2::new(7.0, 6.0);
    style.spacing.button_padding = Vec2::new(9.0, 5.0);
    style.spacing.interact_size = Vec2::new(36.0, 30.0);
    style.spacing.window_margin = egui::Margin::same(PANEL_MARGIN);
    style.visuals = visuals;

    // Dynamic UI Typography
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(ui_font_size + 6.0)),
        (
            TextStyle::Name("Title".into()),
            FontId::proportional(ui_font_size + 4.0),
        ),
        (TextStyle::Body, FontId::proportional(ui_font_size)),
        (TextStyle::Button, FontId::proportional(ui_font_size)),
        (
            TextStyle::Small,
            FontId::proportional((ui_font_size - 2.0).max(9.0)),
        ),
        (TextStyle::Monospace, FontId::monospace(ui_font_size)),
    ]
    .into();

    ctx.set_style_of(theme, style);
}

pub fn screen_rect(ctx: &egui::Context) -> Rect {
    ctx.input(|i| {
        i.viewport()
            .inner_rect
            .or(i.raw.screen_rect)
            .unwrap_or_else(|| Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)))
    })
}

/// Renders edge and corner resize handles for frameless windows.
pub fn show_window_resize_handles(ctx: &egui::Context) {
    let screen = screen_rect(ctx);
    if screen.width() < 100.0 || screen.height() < 100.0 {
        return;
    }

    let edge_thickness = 6.0;
    let corner_size = 14.0;

    let handles = [
        // Corners
        (
            "resize_area_nw",
            Rect::from_min_size(screen.min, Vec2::splat(corner_size)),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        (
            "resize_area_ne",
            Rect::from_min_size(
                Pos2::new(screen.max.x - corner_size, screen.min.y),
                Vec2::splat(corner_size),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            "resize_area_sw",
            Rect::from_min_size(
                Pos2::new(screen.min.x, screen.max.y - corner_size),
                Vec2::splat(corner_size),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (
            "resize_area_se",
            Rect::from_min_size(
                screen.max - Vec2::splat(corner_size),
                Vec2::splat(corner_size),
            ),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
        // Edges
        (
            "resize_area_n",
            Rect::from_min_size(
                Pos2::new(screen.min.x + corner_size, screen.min.y),
                Vec2::new(screen.width() - corner_size * 2.0, edge_thickness),
            ),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        (
            "resize_area_s",
            Rect::from_min_size(
                Pos2::new(screen.min.x + corner_size, screen.max.y - edge_thickness),
                Vec2::new(screen.width() - corner_size * 2.0, edge_thickness),
            ),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        (
            "resize_area_w",
            Rect::from_min_size(
                Pos2::new(screen.min.x, screen.min.y + corner_size),
                Vec2::new(edge_thickness, screen.height() - corner_size * 2.0),
            ),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        (
            "resize_area_e",
            Rect::from_min_size(
                Pos2::new(screen.max.x - edge_thickness, screen.min.y + corner_size),
                Vec2::new(edge_thickness, screen.height() - corner_size * 2.0),
            ),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
    ];

    for (id_str, rect, direction, cursor) in handles {
        egui::Area::new(egui::Id::new(id_str))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let response = ui.allocate_response(rect.size(), Sense::click_and_drag());
                if response.hovered() {
                    ctx.set_cursor_icon(cursor);
                }
                if response.drag_started()
                    || (response.hovered()
                        && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)))
                {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                }
            });
    }
}

pub fn icon_button(ui: &mut Ui, icon: Icon, selected: bool, label: &str) -> Response {
    painted_button(ui, icon, selected, label, false, false)
}

pub fn navigation_button(
    ui: &mut Ui,
    icon: Icon,
    selected: bool,
    label: &str,
    expanded: bool,
) -> Response {
    painted_button(ui, icon, selected, label, expanded, true)
}

pub fn compact_action(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let compact = ui.available_width() < COMPACT_WIDTH;
    painted_button(ui, icon, false, label, !compact, false)
}

pub fn screen_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(22.0).strong());
}

pub fn card_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.inactive.bg_stroke.color,
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
}

#[allow(dead_code)]
pub fn sheet_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.inactive.bg_stroke.color,
        ))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(24, 20))
        .shadow(egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(100),
        })
}

pub fn pill_frame(ui: &Ui, selected: bool) -> egui::Frame {
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else {
        ui.visuals().faint_bg_color
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(
            1.0,
            if selected {
                ui.visuals().hyperlink_color
            } else {
                ui.visuals()
                    .widgets
                    .inactive
                    .bg_stroke
                    .color
                    .gamma_multiply(0.5)
            },
        ))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(8, 4))
}

pub fn paint_resize_grip(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let br = rect.max;
    let stroke = Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4));
    let painter = ui.painter();
    for offset in [4.0, 7.0, 10.0] {
        painter.line_segment(
            [
                Pos2::new(br.x - offset, br.y),
                Pos2::new(br.x, br.y - offset),
            ],
            stroke,
        );
    }
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(
        RichText::new(text.into())
            .small()
            .color(ui.visuals().weak_text_color()),
    )
}

#[allow(dead_code)]
pub fn status_color(visuals: &egui::Visuals, is_error: bool) -> Color32 {
    if is_error {
        visuals.error_fg_color
    } else {
        visuals.hyperlink_color
    }
}
