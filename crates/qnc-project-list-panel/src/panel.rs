//! The project list (copied from the old QNC's `qnc-project-blocks/list.rs`): the title row with
//! the search, one row per project (name, strong when active, the date in muted text at the
//! right, x to remove), a click on the row opens it, the status line at the foot.

use eframe::egui::{self, RichText, Sense, Vec2};
use qnc_desktop_theme::Theme;
use qnc_project_application::ProjectRow;
use qnc_project_layout::ProjectListMetrics;
use qnc_project_session::{ProjectIntent, ProjectSession};

/// Draws the list in `rect` and returns what the user asked.
pub fn show(ui: &mut egui::Ui, theme: &Theme, metrics: &ProjectListMetrics, rect: egui::Rect, session: &mut ProjectSession) -> Vec<ProjectIntent> {
    let mut intents = Vec::new();
    ui.painter().rect_filled(rect, 0.0, theme.bg);
    let content = rect.shrink(metrics.panel_pad);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(content);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        contents(ui, theme, metrics, content, session, &mut intents);
    });
    intents
}

fn contents(ui: &mut egui::Ui, theme: &Theme, metrics: &ProjectListMetrics, content: egui::Rect, session: &mut ProjectSession, intents: &mut Vec<ProjectIntent>) {
    let content_w = content.width().max(40.0);
    let (title_rect, _) = ui.allocate_exact_size(Vec2::new(content_w, metrics.title_row_height), Sense::hover());
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(title_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        let title = RichText::new(&metrics.title).size(theme.font_ui).strong().color(theme.text);
        qnc_settings_rows::title_with_search(ui, title, &mut session.project_search, content_w * 0.55);
    });
    ui.painter().hline(title_rect.x_range(), title_rect.bottom() - 0.5, egui::Stroke::new(1.0, theme.border));
    ui.add_space(metrics.below_title_pad);

    let foot_h = theme.chrome_row_height;
    let table_top = ui.cursor().top();
    let table_bottom = (content.bottom() - foot_h).max(table_top + 40.0);
    let table_rect = egui::Rect::from_min_max(egui::pos2(content.left(), table_top), egui::pos2(content.right(), table_bottom));
    let foot_rect = egui::Rect::from_min_size(egui::pos2(content.left(), table_bottom), Vec2::new(content_w, foot_h));

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(table_rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(table_rect);
        ui.set_max_width(table_rect.width().max(40.0));
        egui::ScrollArea::vertical().id_salt("project_list").max_height(table_rect.height()).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_max_width(table_rect.width().max(40.0));
            if session.projects.is_empty() {
                ui.label(RichText::new(&metrics.empty_text).size(theme.font_ui).color(theme.text_muted));
                return;
            }
            for (at, index) in session.shown_projects().into_iter().enumerate() {
                if at > 0 {
                    ui.add_space(metrics.row_gap);
                }
                let project = session.projects[index].clone();
                row(ui, theme, metrics, index, &project, session.selected_project == Some(index), intents);
            }
        });
    });

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(foot_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.set_clip_rect(foot_rect);
        ui.label(RichText::new(&session.status).size(theme.font_ui).color(theme.text_muted));
    });
}

pub(crate) fn row(ui: &mut egui::Ui, theme: &Theme, metrics: &ProjectListMetrics, index: usize, project: &ProjectRow, selected: bool, intents: &mut Vec<ProjectIntent>) {
    let table_w = ui.available_width().max(40.0);
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(table_w, metrics.row_height), Sense::hover());
    let delete_hit_w = metrics.delete_column_width.max(48.0);
    let delete_rect = egui::Rect::from_min_size(egui::pos2(row_rect.right() - delete_hit_w, row_rect.top()), Vec2::new(delete_hit_w, metrics.row_height));
    let select_rect = egui::Rect::from_min_max(row_rect.min, egui::pos2(delete_rect.left() - metrics.column_gap, row_rect.bottom()));
    let response = ui.interact(select_rect, ui.id().with(("project_list_row_select", index)), Sense::click());
    if selected || response.hovered() {
        let fill = if selected { theme.surface } else { theme.surface.linear_multiply(0.55) };
        ui.painter().rect_filled(row_rect, 0.0, fill);
    }
    if response.clicked() {
        intents.push(ProjectIntent::OpenProjectRow { index });
    }

    let label_rect = egui::Rect::from_min_max(egui::pos2(row_rect.left() + 20.0, row_rect.top()), select_rect.right_bottom());
    let date = format!("- {}", project.created_date);
    let date_galley = ui.painter().layout_no_wrap(date, egui::FontId::proportional(theme.font_ui), theme.text_muted);
    let date_x = (label_rect.right() - date_galley.size().x).max(label_rect.left());
    let name_rect = egui::Rect::from_min_max(label_rect.min, egui::pos2((date_x - metrics.column_gap).max(label_rect.left()), label_rect.bottom()));
    ui.painter().with_clip_rect(label_rect).galley(egui::pos2(date_x, label_rect.center().y - date_galley.size().y * 0.5), date_galley, theme.text_muted);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(name_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.set_clip_rect(name_rect.intersect(ui.clip_rect()));
        let mut label = RichText::new(&project.name).size(theme.font_ui).color(theme.text);
        if project.active {
            label = label.strong();
        }
        ui.add(egui::Label::new(label).truncate().selectable(false));
    });

    let delete_response = ui.interact(delete_rect, ui.id().with(("project_list_row_delete", index)), Sense::click()).on_hover_text(qnc_text::text("project.remove_project"));
    let delete_color = if delete_response.hovered() { theme.text } else { theme.text_muted };
    ui.painter().text(delete_rect.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(theme.font_ui + 5.0), delete_color);
    if delete_response.clicked() {
        intents.push(ProjectIntent::RequestDeleteProject { index });
    }
}
