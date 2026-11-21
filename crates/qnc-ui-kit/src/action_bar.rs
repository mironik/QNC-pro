//! The confirm and cancel bar of a form (copied from the old QNC's `qnc-ui-kit`): two buttons of
//! one width at the right, the main one filled, the other bordered.

use egui::{Button, Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::on_dark;

pub const STANDARD_ACTION_BUTTON_WIDTH: f32 = 96.0;
pub const STANDARD_ACTION_BUTTON_GAP: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormActionBarStyle {
    pub text: Color32,
    pub primary_text: Color32,
    pub primary_fill: Color32,
    pub border: Color32,
    pub font_size: f32,
    pub height: f32,
    pub button_width: f32,
    pub gap: f32,
    pub right_margin: f32,
}

impl FormActionBarStyle {
    pub fn new(text: Color32, primary_fill: Color32, border: Color32, font_size: f32, height: f32) -> Self {
        // On the dark desktop the main button is never light (`on_dark`).
        let (primary_fill, primary_text, _) = on_dark(primary_fill, Color32::WHITE);
        Self { text, primary_text, primary_fill, border, font_size, height, button_width: STANDARD_ACTION_BUTTON_WIDTH, gap: STANDARD_ACTION_BUTTON_GAP, right_margin: STANDARD_ACTION_BUTTON_GAP }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FormActionBarResponse {
    pub confirm_clicked: bool,
    pub cancel_clicked: bool,
    /// Where the two buttons are drawn (a form may frame the one the keyboard is on).
    pub confirm_rect: Option<egui::Rect>,
    pub cancel_rect: Option<egui::Rect>,
}

pub fn show_form_action_bar(ui: &mut egui::Ui, style: &FormActionBarStyle, confirm_label: &str, confirm_enabled: bool, cancel_label: &str) -> FormActionBarResponse {
    let mut response = FormActionBarResponse::default();
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), style.height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = style.gap;
        ui.add_space(style.right_margin);
        let confirm = primary_button(ui, style, confirm_label, confirm_enabled);
        (response.confirm_clicked, response.confirm_rect) = (confirm.clicked(), Some(confirm.rect));
        let cancel = action_button(ui, style, cancel_label);
        (response.cancel_clicked, response.cancel_rect) = (cancel.clicked(), Some(cancel.rect));
    });
    response
}

fn primary_button(ui: &mut egui::Ui, style: &FormActionBarStyle, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        Button::new(RichText::new(label).color(style.primary_text).strong().size(style.font_size))
            .fill(style.primary_fill)
            .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
            .corner_radius(CornerRadius::same(0))
            .min_size(action_button_size(style)),
    )
}

fn action_button(ui: &mut egui::Ui, style: &FormActionBarStyle, label: &str) -> egui::Response {
    ui.add(
        Button::new(RichText::new(label).color(style.text).size(style.font_size))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, style.border))
            .corner_radius(CornerRadius::same(0))
            .min_size(action_button_size(style)),
    )
}

fn action_button_size(style: &FormActionBarStyle) -> Vec2 {
    Vec2::new(style.button_width, style.height)
}

/// A tab of a row of tabs (copied from the old QNC's media pool head): the chosen one strong and
/// underlined in the accent.
#[derive(Debug, Clone, Copy)]
pub struct TabStyle {
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub font_ui: f32,
}

pub fn tab(ui: &mut egui::Ui, text: &str, selected: bool, style: &TabStyle) -> egui::Response {
    let label = if selected { RichText::new(text).color(style.text).strong().size(style.font_ui) } else { RichText::new(text).color(style.muted).size(style.font_ui) };
    let response = ui.add(egui::Label::new(label).sense(egui::Sense::click()).selectable(false));
    if selected {
        let y = response.rect.bottom() + 2.0;
        ui.painter().line_segment([egui::pos2(response.rect.left(), y), egui::pos2(response.rect.right(), y)], Stroke::new(2.0, style.accent));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_action_buttons_have_equal_width() {
        let style = FormActionBarStyle::new(Color32::WHITE, Color32::from_rgb(1, 2, 3), Color32::from_rgb(4, 5, 6), 14.0, 30.0);
        assert_eq!(style.button_width, STANDARD_ACTION_BUTTON_WIDTH);
        assert_eq!(style.gap, STANDARD_ACTION_BUTTON_GAP);
    }
}
