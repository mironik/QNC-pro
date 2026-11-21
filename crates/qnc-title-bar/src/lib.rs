//! The title bar of every QNC window (copied from the old QNC's `qnc-title-bar`): QNC's own bar in
//! a window without the system frame ([`frameless`]): the title on the left; on the right the gear
//! (opens the Settings form), minimize, maximize and close. Dragging the bar moves the window, a
//! double click maximizes it, its edges resize it. The same on Windows, macOS and Linux. A hidden
//! button before the title opens and shuts the form's look editor. The archive search and the work
//! chip of the old bar come with the archive and the background work.

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Stroke, Vec2, ViewportCommand};
use qnc_text::text;

/// The height of the bar.
pub const HEIGHT: f32 = 34.0;

/// The margin under what a window draws.
pub const BOTTOM_MARGIN: f32 = 14.0;
/// The width of each button of the bar.
const BUTTON: f32 = 44.0;
/// How far inside the window's edge a press resizes it.
const EDGE: f32 = 6.0;
/// The program the gear opens.
const SETTINGS: &str = "qnc-settings";

/// The viewport of a window without the system frame, for this bar.
pub fn frameless(viewport: egui::ViewportBuilder) -> egui::ViewportBuilder {
    viewport.with_decorations(false).with_resizable(true)
}

/// What the bar was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Settings,
    Minimize,
    Maximize,
    Close,
}

/// The bar of one window.
pub struct TitleBar {
    title: String,
}

impl TitleBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into() }
    }

    /// Draws the bar on top of the window (before its content) and does what was asked.
    pub fn show(&mut self, ctx: &egui::Context) {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        if !maximized {
            resize_edges(ctx);
        }
        let mut pressed = None;
        egui::TopBottomPanel::top("qnc-title-bar")
            .exact_height(HEIGHT)
            .frame(egui::Frame::NONE.fill(ctx.style().visuals.panel_fill))
            .show_separator_line(true)
            .show(ctx, |ui| {
                let bar = ui.max_rect();
                let drag = ui.interact(bar, egui::Id::new("qnc-title-bar-drag"), Sense::click_and_drag());
                if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                let colour = ui.visuals().text_color();
                // The hidden button before the title; a faint dot shows it only under the mouse.
                let hidden = Rect::from_min_size(bar.left_top(), Vec2::new(12.0, bar.height()));
                let look = ui.interact(hidden, egui::Id::new("qnc-title-bar-look"), Sense::click());
                if look.hovered() {
                    ui.painter().circle_filled(hidden.center(), 2.5, ui.visuals().weak_text_color());
                }
                if look.clicked() {
                    qnc_form_look::ask_editor(ctx);
                }
                ui.painter().text(Pos2::new(bar.left() + 14.0, bar.center().y), egui::Align2::LEFT_CENTER, &self.title, egui::FontId::proportional(14.0), colour);
                let buttons = [(Button::Close, "title.close"), (Button::Maximize, "title.maximize"), (Button::Minimize, "title.minimize"), (Button::Settings, "title.settings")];
                for (at, (button, tip)) in buttons.into_iter().enumerate() {
                    let rect = Rect::from_min_size(Pos2::new(bar.right() - BUTTON * (at as f32 + 1.0), bar.top()), Vec2::new(BUTTON, bar.height()));
                    let response = ui.interact(rect, egui::Id::new(("qnc-title-bar", at)), Sense::click()).on_hover_text(text(tip));
                    let hovered = response.hovered();
                    if hovered {
                        let fill = if button == Button::Close { Color32::from_rgb(196, 43, 28) } else { ui.visuals().widgets.hovered.weak_bg_fill };
                        ui.painter().rect_filled(rect, 0.0, fill);
                    }
                    let stroke = Stroke::new(1.3, if hovered && button == Button::Close { Color32::WHITE } else { colour });
                    paint_icon(ui.painter(), rect.center(), button, maximized, stroke);
                    if response.clicked() {
                        pressed = Some(button);
                    }
                }
            });
        match pressed {
            Some(Button::Close) => ctx.send_viewport_cmd(ViewportCommand::Close),
            Some(Button::Maximize) => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
            Some(Button::Minimize) => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            Some(Button::Settings) => open_settings(),
            None => {}
        }
    }
}

/// The Settings form, the program beside this one.
fn open_settings() {
    let line = match qnc_launch::start_detached(SETTINGS, &[], qnc_launch::Priority::Same) {
        Ok(_) => format!("title-bar {SETTINGS} opened"),
        Err(error) => format!("title-bar {SETTINGS} not opened: {error}"),
    };
    qnc_diagnostics::log(line);
}

/// The icon of a button, drawn in lines (the same on every system and font).
fn paint_icon(painter: &egui::Painter, centre: Pos2, button: Button, maximized: bool, stroke: Stroke) {
    let at = |x: f32, y: f32| centre + Vec2::new(x, y);
    match button {
        Button::Close => {
            painter.line_segment([at(-5.0, -5.0), at(5.0, 5.0)], stroke);
            painter.line_segment([at(-5.0, 5.0), at(5.0, -5.0)], stroke);
        }
        Button::Maximize if maximized => {
            painter.rect_stroke(Rect::from_min_max(at(-5.0, -3.0), at(3.0, 5.0)), 0.0, stroke, egui::StrokeKind::Middle);
            painter.line_segment([at(-3.0, -5.0), at(5.0, -5.0)], stroke);
            painter.line_segment([at(5.0, -5.0), at(5.0, 3.0)], stroke);
        }
        Button::Maximize => {
            painter.rect_stroke(Rect::from_min_max(at(-5.0, -5.0), at(5.0, 5.0)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        Button::Minimize => {
            painter.line_segment([at(-5.0, 0.0), at(5.0, 0.0)], stroke);
        }
        Button::Settings => {
            painter.circle_stroke(centre, 2.5, stroke);
            painter.circle_stroke(centre, 5.5, stroke);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                painter.line_segment([at(cos * 5.5, sin * 5.5), at(cos * 7.5, sin * 7.5)], stroke);
            }
        }
    }
}

/// The edges of a window without the system frame resize it, as a system window's do.
fn resize_edges(ctx: &egui::Context) {
    let Some(pointer) = ctx.input(|input| input.pointer.hover_pos()) else { return };
    let screen = ctx.screen_rect();
    let (left, right) = (pointer.x <= screen.left() + EDGE, pointer.x >= screen.right() - EDGE);
    let (top, bottom) = (pointer.y <= screen.top() + EDGE, pointer.y >= screen.bottom() - EDGE);
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (.., true) => ResizeDirection::South,
        _ => return,
    };
    ctx.set_cursor_icon(match direction {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    });
    if ctx.input(|input| input.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

/// The margin at the bottom of a window with this bar: what the form draws last does not sit on
/// the window's edge.
pub fn bottom_margin(ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("qnc-window-bottom-margin")
        .exact_height(BOTTOM_MARGIN)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(ctx.style().visuals.panel_fill))
        .show(ctx, |_| {});
}
