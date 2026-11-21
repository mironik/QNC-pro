//! The drawing kit. Panels and forms draw only through it, so every button, field and frame of
//! QNC looks the same and the theme lives in one place. It holds no state and knows no block
//! above the theme and the action catalog.

mod chrome_button;
mod keys;
mod text_field;

use egui::{Color32, CornerRadius, Response, Stroke, StrokeKind, Ui, Vec2};
use qnc_theme::{Rgb, Theme};

pub use keys::chords;
pub use chrome_button::{chrome_button, fill_block, on_dark, check_box, framed_button, look_button, press_feedback, sized_button, small_button, strong_button, ButtonLook, ChromeButtonStyle, PRESS_FEEDBACK_KEY};
pub use text_field::{row_field, search_field, text_area, text_field};
mod action_bar;
mod option_columns;
pub use option_columns::{show_option_columns, OptionColumn, OptionColumnsStyle, OptionItem, OptionSelection};
pub use action_bar::{show_form_action_bar, tab, FormActionBarResponse, FormActionBarStyle, TabStyle, STANDARD_ACTION_BUTTON_GAP, STANDARD_ACTION_BUTTON_WIDTH};

/// The thin frame of what the keyboard is on (a panel, a button, a tab), the same in every form.
pub fn paint_focus(ui: &Ui, rect: egui::Rect, focused: bool, color: Color32) {
    if focused {
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, color.gamma_multiply(0.7)), StrokeKind::Inside);
    }
}

/// A theme colour as the drawing library's colour.
pub fn colour(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0[0], rgb.0[1], rgb.0[2])
}

/// Applies the theme to a window as the old QNC shell did: background, text, widget colours,
/// font sizes, chrome paddings.
pub fn apply(ctx: &egui::Context, theme: &Theme) {
    let c = &theme.colours;
    let m = &theme.metrics;
    let mut style = (*ctx.style()).clone();
    let font = |size: f32| egui::FontId::new(size, egui::FontFamily::Proportional);
    style.text_styles = [
        (egui::TextStyle::Heading, font(m.font_size + 4.0)),
        (egui::TextStyle::Body, font(m.font_size)),
        (egui::TextStyle::Button, font(m.font_size)),
        (egui::TextStyle::Small, font(m.font_size - 1.0)),
        (egui::TextStyle::Monospace, font(m.font_timecode)),
    ]
    .into();
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    visuals.panel_fill = colour(c.background);
    visuals.window_fill = colour(c.panel);
    visuals.extreme_bg_color = colour(c.background);
    visuals.faint_bg_color = colour(c.panel);
    visuals.override_text_color = Some(colour(c.text));
    visuals.window_stroke = Stroke::new(1.0, colour(c.focus));
    visuals.widgets.noninteractive.bg_fill = colour(c.panel);
    visuals.widgets.noninteractive.weak_bg_fill = colour(c.raised);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colour(c.muted));
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colour(c.border));
    visuals.widgets.inactive.bg_fill = colour(c.raised);
    visuals.widgets.inactive.weak_bg_fill = colour(c.panel);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, colour(c.text));
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, colour(c.border));
    visuals.widgets.hovered.bg_fill = colour(c.raised);
    visuals.widgets.hovered.weak_bg_fill = colour(c.raised);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, colour(c.text));
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, colour(c.accent));
    visuals.widgets.active.bg_fill = colour(c.accent);
    visuals.widgets.active.weak_bg_fill = colour(c.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, colour(c.accent));
    visuals.selection.bg_fill = colour(c.accent).linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, colour(c.focus));
    visuals.hyperlink_color = colour(c.accent);
    visuals.text_cursor.stroke = Stroke::new(2.0, colour(c.text));
    style.spacing.item_spacing = Vec2::new(m.gap, 6.0);
    style.spacing.button_padding = Vec2::new(m.gap, m.pad_y);
    ctx.set_style(style);
}

/// The one button, as the old settings forms drew it: a plain button has a border and no fill;
/// `on` is the strong button (white bold text on the accent fill); a disabled one is muted.
pub fn button(ui: &mut Ui, theme: &Theme, text: &str, enabled: bool, on: bool) -> Response {
    let c = &theme.colours;
    let (fill, border, label, strong) = match (enabled, on) {
        (false, _) => (Color32::TRANSPARENT, colour(c.border), colour(c.muted), false),
        (true, true) => (colour(c.accent), colour(c.accent), Color32::WHITE, true),
        (true, false) => (Color32::TRANSPARENT, colour(c.border), colour(c.text), false),
    };
    let mut rich = egui::RichText::new(text).color(label);
    if strong {
        rich = rich.strong();
    }
    let widget = egui::Button::new(rich).fill(fill).stroke(Stroke::new(1.0, border)).corner_radius(CornerRadius::same(2)).min_size(Vec2::new(0.0, theme.metrics.control_height));
    ui.add_enabled(enabled, widget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_colour_becomes_the_same_rgb() {
        assert_eq!(colour(Rgb([1, 2, 3])), Color32::from_rgb(1, 2, 3));
    }
}
