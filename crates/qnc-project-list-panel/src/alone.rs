//! The project list on its own (copied from the old QNC's `projects_alone`): read top to bottom,
//! the title with its search, the projects, and right after the last one the row that makes a
//! new one, in the look of a row, not a glaring button. No status line, no empty space: the
//! caller sizes the window to the height given back.

use eframe::egui::{self, RichText, Sense, Vec2};
use qnc_desktop_theme::Theme;
use qnc_project_layout::ProjectListMetrics;
use qnc_project_session::{ProjectIntent, ProjectSession};

/// What the list alone gives back: a new project asked for, and how tall its content is.
pub struct ProjectsAlone {
    pub new_asked: bool,
    pub height: f32,
}

/// Draws the list alone in the rect the ui has; `new_label` is the row of a new project.
pub fn show_alone(ui: &mut egui::Ui, theme: &Theme, metrics: &ProjectListMetrics, session: &mut ProjectSession, new_label: &str, intents: &mut Vec<ProjectIntent>) -> ProjectsAlone {
    let font = theme.font_ui;
    let pad = metrics.panel_pad.max(20.0);
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, theme.bg);
    let content = full.shrink(pad);
    let width = content.width().max(40.0);
    let mut new_asked = false;
    let mut used = 0.0;
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let top = ui.cursor().top();
        let (title_rect, _) = ui.allocate_exact_size(Vec2::new(width, metrics.title_row_height), Sense::hover());
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(title_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            let title = RichText::new(&metrics.title).size(font + 2.0).strong().color(theme.text);
            qnc_settings_rows::title_with_search(ui, title, &mut session.project_search, width * 0.5);
        });
        ui.painter().hline(title_rect.x_range(), title_rect.bottom() - 0.5, egui::Stroke::new(1.0, theme.border));
        ui.add_space(metrics.below_title_pad.max(8.0));
        let shown = session.shown_projects();
        // Up to eight rows show without a scroll; more scroll inside the same place.
        let rows = shown.len().clamp(1, 8) as f32;
        let list_h = rows * metrics.row_height + (rows - 1.0) * metrics.row_gap;
        egui::ScrollArea::vertical().id_salt("projects_alone").max_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_max_width(width);
            if shown.is_empty() {
                let (empty, _) = ui.allocate_exact_size(Vec2::new(width, metrics.row_height), Sense::hover());
                ui.painter().text(egui::pos2(empty.left() + 20.0, empty.center().y), egui::Align2::LEFT_CENTER, &metrics.empty_text, egui::FontId::proportional(font), theme.text_muted);
            }
            for (at, index) in shown.into_iter().enumerate() {
                if at > 0 {
                    ui.add_space(metrics.row_gap);
                }
                let project = session.projects[index].clone();
                crate::panel::row(ui, theme, metrics, index, &project, session.selected_project == Some(index), intents);
            }
        });
        ui.add_space(metrics.row_gap.max(6.0) + 6.0);
        new_asked = new_row(ui, width, metrics.row_height, new_label, font, theme);
        used = ui.cursor().top() - top;
    });
    ProjectsAlone { new_asked, height: used + pad * 2.0 }
}

/// The row that makes a new project: as tall as a project row, framed by the border line, "+" and
/// the label in the text colour; hovered it takes the fill of a chosen row.
fn new_row(ui: &mut egui::Ui, width: f32, height: f32, label: &str, font: f32, theme: &Theme) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 3.0, theme.surface);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.0, if hovered { theme.text_muted } else { theme.border }), egui::StrokeKind::Inside);
    ui.painter().text(egui::pos2(rect.left() + 20.0, rect.center().y), egui::Align2::LEFT_CENTER, format!("+   {label}"), egui::FontId::proportional(font), theme.text);
    response.clicked()
}
