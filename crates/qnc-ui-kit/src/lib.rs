//! The drawing kit. Panels and forms draw only through it, so every button, field and frame of
//! QNC looks the same and the theme lives in one place. It holds no state and knows no block
//! above the theme and the action catalog.

mod keys;

use egui::{Color32, CornerRadius, Response, Stroke, StrokeKind, TextEdit, Ui, Vec2};
use qnc_theme::{Rgb, Theme};

pub use keys::chords;

/// A theme colour as the drawing library's colour.
pub fn colour(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0[0], rgb.0[1], rgb.0[2])
}

/// Applies the theme to a window: background, text, widget colours and font size.
pub fn apply(ctx: &egui::Context, theme: &Theme) {
    let c = &theme.colours;
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.panel_fill = colour(c.background);
    visuals.window_fill = colour(c.panel);
    visuals.override_text_color = Some(colour(c.text));
    visuals.widgets.noninteractive.bg_fill = colour(c.panel);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colour(c.border));
    visuals.widgets.inactive.bg_fill = colour(c.button);
    visuals.widgets.inactive.weak_bg_fill = colour(c.button);
    visuals.widgets.hovered.bg_fill = colour(c.button_on);
    visuals.widgets.hovered.weak_bg_fill = colour(c.button_on);
    visuals.widgets.active.bg_fill = colour(c.button_on);
    visuals.widgets.active.weak_bg_fill = colour(c.button_on);
    visuals.selection.bg_fill = colour(c.focus).linear_multiply(0.4);
    visuals.selection.stroke = Stroke::new(1.0, colour(c.focus));
    visuals.text_cursor.stroke = Stroke::new(2.0, colour(c.text));
    for (_, font) in style.text_styles.iter_mut() {
        font.size = theme.metrics.font_size;
    }
    style.spacing.item_spacing = Vec2::splat(theme.metrics.gap);
    style.spacing.button_padding = Vec2::new(theme.metrics.gap, theme.metrics.gap / 2.0);
    ctx.set_style(style);
}

/// The one button: `on` is drawn in the focus colour, a disabled one is muted.
pub fn button(ui: &mut Ui, theme: &Theme, text: &str, enabled: bool, on: bool) -> Response {
    let c = &theme.colours;
    let (fill, border, label) = match (enabled, on) {
        (false, _) => (colour(c.button), colour(c.border), colour(c.muted)),
        (true, true) => (colour(c.button_on), colour(c.focus), colour(c.text)),
        (true, false) => (colour(c.button), colour(c.border), colour(c.text)),
    };
    let widget = egui::Button::new(egui::RichText::new(text).color(label))
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(3))
        .min_size(Vec2::new(0.0, theme.metrics.row_height - theme.metrics.gap));
    ui.add_enabled(enabled, widget)
}

/// The one check box.
pub fn check_box(ui: &mut Ui, checked: &mut bool, text: &str, enabled: bool) -> Response {
    ui.add_enabled(enabled, egui::Checkbox::new(checked, text))
}

/// A one-line text field of the given width.
pub fn text_field(text: &mut String, width: f32) -> TextEdit<'_> {
    TextEdit::singleline(text).desired_width(width)
}

/// A text area of the given width that grows with its rows.
pub fn text_area(text: &mut String, width: f32, rows: usize) -> TextEdit<'_> {
    TextEdit::multiline(text).desired_width(width).desired_rows(rows)
}

/// The focus frame around the panel or item that has the keyboard.
pub fn paint_focus(ui: &Ui, rect: egui::Rect, theme: &Theme, focused: bool) {
    if focused {
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(theme.metrics.focus_width, colour(theme.colours.focus)), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_colour_becomes_the_same_rgb() {
        assert_eq!(colour(Rgb([1, 2, 3])), Color32::from_rgb(1, 2, 3));
    }
}
