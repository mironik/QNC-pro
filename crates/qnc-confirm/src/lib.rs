//! A modal yes/no question over the whole window, mirrored from the old Project delete
//! dialogs: a dark backdrop, a 418x162 box, the title, the subject in muted text, NE / DA.
//! Passive: it draws and answers; the caller decides what the answer does.

use egui::{Color32, Context, RichText, Sense, Stroke, StrokeKind, Vec2};
use qnc_theme::Theme;

pub const SIZE: [f32; 2] = [418.0, 162.0];

/// The texts of one question.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    /// Unique among the dialogs of the window.
    pub id: &'a str,
    pub title: &'a str,
    pub subject: &'a str,
    pub no: &'a str,
    pub yes: &'a str,
}

/// Shows the question; `Some(true)` for yes, `Some(false)` for no, `None` while open.
pub fn show(ctx: &Context, theme: &Theme, question: Question<'_>) -> Option<bool> {
    let c = &theme.colours;
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new((question.id, "backdrop"))).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(130));
    });
    let size = Vec2::new(SIZE[0], SIZE[1]);
    egui::Area::new(egui::Id::new((question.id, "confirm")))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.center() - size * 0.5)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect_filled(rect, 0.0, qnc_ui_kit::colour(c.panel));
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, qnc_ui_kit::colour(c.border)), StrokeKind::Inside);
            let font = egui::FontId::proportional(theme.metrics.font_size);
            let line = |top: f32| egui::Rect::from_min_size(egui::pos2(rect.left() + 20.0, rect.top() + top), Vec2::new(rect.width() - 40.0, 24.0)).center();
            ui.painter().text(line(20.0), egui::Align2::CENTER_CENTER, question.title, font.clone(), qnc_ui_kit::colour(c.text));
            ui.painter().text(line(58.0), egui::Align2::CENTER_CENTER, question.subject, font, qnc_ui_kit::colour(c.muted));
            let (button_w, gap, height) = (48.0, 8.0, theme.metrics.control_height);
            let x = rect.center().x - (button_w * 2.0 + gap) * 0.5;
            let no_rect = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - 22.0 - height), Vec2::new(button_w, height));
            let yes_rect = egui::Rect::from_min_size(egui::pos2(no_rect.right() + gap, no_rect.top()), Vec2::new(button_w, height));
            let no = egui::Button::new(RichText::new(question.no).color(qnc_ui_kit::colour(c.text))).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, qnc_ui_kit::colour(c.border)));
            let yes = egui::Button::new(RichText::new(question.yes).color(Color32::WHITE).strong()).fill(qnc_ui_kit::colour(c.accent));
            let no_clicked = ui.put(no_rect, no).clicked();
            let yes_clicked = ui.put(yes_rect, yes).clicked();
            match (no_clicked, yes_clicked) {
                (true, _) => Some(false),
                (false, true) => Some(true),
                (false, false) => None,
            }
        })
        .inner
}
