//! The painter of a timeline window. The module gives it what to show (`Shown`); it draws and
//! returns the mouse gestures it saw as pixels from the window's left. It decides nothing: the
//! module maps pixels to frames through the same axis and the mouse map says what a gesture
//! means. The source timeline and its zoom are drawn by this one panel over different windows.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use qnc_axis::Axis;
use qnc_mouse_map::Gesture;
use qnc_theme::Theme;
use qnc_wave_read::Wave;

/// What the timeline shows in one window.
pub struct Shown<'a> {
    pub axis: Axis,
    pub wave: Option<&'a Wave>,
    /// IN and OUT (exclusive) of the clip.
    pub marks: (u64, u64),
    pub taken_in: bool,
    pub taken_out: bool,
    pub playhead: Option<u64>,
    /// A cut line (the zoom), if any.
    pub cut: Option<u64>,
}

/// A gesture the panel saw, with the pixel from the window's left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    pub gesture: Gesture,
    pub x: f32,
}

const LANE_HEIGHT: f32 = 18.0;

/// Draws the window into `height` pixels and returns the gestures on it.
pub fn show(ui: &mut Ui, theme: &Theme, shown: &Shown<'_>, height: f32) -> Vec<Seen> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(shown.axis.width, height), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let c = &theme.colours;
    painter.rect_filled(rect, 0.0, qnc_ui_kit::colour(c.panel));
    paint_lanes(&painter, rect, shown, theme);
    paint_dim(&painter, rect, shown);
    for (frame, taken) in [(shown.marks.0, shown.taken_in), (shown.marks.1, shown.taken_out)] {
        let x = rect.left() + shown.axis.x_of(frame);
        let width = if taken { 3.0 } else { 1.5 };
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(width, qnc_ui_kit::colour(c.marked)));
    }
    if let Some(cut) = shown.cut {
        let x = rect.left() + shown.axis.x_of(cut);
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, qnc_ui_kit::colour(c.danger)));
    }
    if let Some(playhead) = shown.playhead.filter(|&f| shown.axis.shows(f)) {
        let x = rect.left() + shown.axis.x_of(playhead);
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, qnc_ui_kit::colour(c.playhead)));
    }
    gestures(ui, &response, rect)
}

fn paint_lanes(painter: &egui::Painter, rect: Rect, shown: &Shown<'_>, theme: &Theme) {
    let Some(wave) = shown.wave else { return };
    let colour = qnc_ui_kit::colour(theme.colours.done);
    let columns = rect.width().max(1.0) as usize;
    for (lane, _) in wave.lanes.iter().enumerate() {
        let top = rect.top() + lane as f32 * LANE_HEIGHT;
        if top + LANE_HEIGHT > rect.bottom() {
            break;
        }
        let middle = top + LANE_HEIGHT / 2.0;
        for column in 0..columns {
            let from = shown.axis.frame_at(column as f32);
            let to = shown.axis.frame_at(column as f32 + 1.0).max(from + 1);
            let peak = wave.lane(lane, from, to).iter().copied().max().unwrap_or(0) as f32 / 1000.0;
            let half = peak * LANE_HEIGHT / 2.0;
            let x = rect.left() + column as f32 + 0.5;
            painter.line_segment([Pos2::new(x, middle - half), Pos2::new(x, middle + half)], Stroke::new(1.0, colour));
        }
    }
}

fn paint_dim(painter: &egui::Painter, rect: Rect, shown: &Shown<'_>) {
    let dim = Color32::from_black_alpha(110);
    let x_in = rect.left() + shown.axis.x_of(shown.marks.0).clamp(0.0, rect.width());
    let x_out = rect.left() + shown.axis.x_of(shown.marks.1).clamp(0.0, rect.width());
    painter.rect_filled(Rect::from_min_max(rect.min, Pos2::new(x_in, rect.bottom())), 0.0, dim);
    painter.rect_filled(Rect::from_min_max(Pos2::new(x_out, rect.top()), rect.max), 0.0, dim);
}

fn gestures(ui: &Ui, response: &egui::Response, rect: Rect) -> Vec<Seen> {
    let mut seen = Vec::new();
    let x = |pos: Option<Pos2>| pos.map(|p| p.x - rect.left());
    let pointer = x(response.interact_pointer_pos()).or_else(|| x(response.hover_pos())).unwrap_or(0.0);
    if response.double_clicked() {
        seen.push(Seen { gesture: Gesture::DoubleClick, x: pointer });
    } else if response.clicked() {
        seen.push(Seen { gesture: Gesture::LeftClick, x: pointer });
    }
    if response.secondary_clicked() {
        seen.push(Seen { gesture: Gesture::RightClick, x: pointer });
    }
    if response.middle_clicked() {
        seen.push(Seen { gesture: Gesture::MiddleClick, x: pointer });
    }
    if response.dragged() {
        seen.push(Seen { gesture: Gesture::Drag, x: pointer });
    }
    if response.hovered() {
        let (scroll, ctrl) = ui.input(|i| (i.raw_scroll_delta.y, i.modifiers.ctrl || i.modifiers.command));
        let notches = (scroll / 50.0).round() as i32;
        let gesture = match (notches.signum(), ctrl) {
            (1, false) => Some(Gesture::WheelForward),
            (-1, false) => Some(Gesture::WheelBack),
            (1, true) => Some(Gesture::CtrlWheelForward),
            (-1, true) => Some(Gesture::CtrlWheelBack),
            _ => None,
        };
        if let Some(gesture) = gesture {
            for _ in 0..notches.unsigned_abs() {
                seen.push(Seen { gesture, x: pointer });
            }
        }
    }
    seen
}
