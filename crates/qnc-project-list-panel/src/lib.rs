//! The project list, mirrored from the old Project board (`qnc-project-blocks/list.rs`): the
//! title row with the search field and a rule under it, one row per project (name, strong when
//! active, the date in muted text at the right, `x` to remove), a click on the row opens it,
//! the delete question as the modal NE / DA dialog, and the status line at the foot. Pure
//! drawing: the list, the search text and the question come from the projects module.

use egui::{Align2, FontId, Layout, RichText, Sense, Ui, Vec2};
use qnc_project_layout::List;
use qnc_projects::{Act, Line, Projects};
use qnc_theme::Theme;

/// The actions of the catalog this panel returns.
pub fn code(act: &Act) -> &'static str {
    match act {
        Act::New => "project_new",
        Act::Open(_) => "project_open",
        Act::Delete(_) => "project_delete",
        Act::ConfirmDelete => "project_delete_confirm",
        Act::CancelDelete => "project_delete_cancel",
        Act::ChooseTemplate(_) => "project_choose_template",
        Act::TogglePicker => "project_toggle_picker",
    }
}

fn row(ui: &mut Ui, theme: &Theme, list: &List, index: usize, project: &Line, asked: &mut Vec<Act>) {
    let c = &theme.colours;
    let font = theme.metrics.font_size;
    let table_w = ui.available_width().max(40.0);
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(table_w, list.row_height), Sense::hover());
    let delete_w = list.delete_column_width.max(48.0);
    let delete_rect = egui::Rect::from_min_size(egui::pos2(row_rect.right() - delete_w, row_rect.top()), Vec2::new(delete_w, list.row_height));
    let select_rect = egui::Rect::from_min_max(row_rect.min, egui::pos2(delete_rect.left() - list.column_gap, row_rect.bottom()));
    let response = ui.interact(select_rect, ui.id().with(("project_row", index)), Sense::click());
    if project.active || response.hovered() {
        let fill = qnc_ui_kit::colour(c.panel);
        ui.painter().rect_filled(row_rect, 0.0, if project.active { fill } else { fill.linear_multiply(0.55) });
    }
    if response.clicked() {
        asked.push(Act::Open(project.project_id.clone()));
    }
    let label_rect = egui::Rect::from_min_max(egui::pos2(row_rect.left() + list.row_text_inset, row_rect.top()), select_rect.right_bottom());
    let date = ui.painter().layout_no_wrap(format!("- {}", project.created), FontId::proportional(font), qnc_ui_kit::colour(c.muted));
    let date_x = (label_rect.right() - date.size().x).max(label_rect.left());
    ui.painter().with_clip_rect(label_rect).galley(egui::pos2(date_x, label_rect.center().y - date.size().y * 0.5), date, qnc_ui_kit::colour(c.muted));
    let name_rect = egui::Rect::from_min_max(label_rect.min, egui::pos2((date_x - list.column_gap).max(label_rect.left()), label_rect.bottom()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(name_rect).layout(Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.set_clip_rect(name_rect.intersect(ui.clip_rect()));
        let mut name = RichText::new(&project.name).size(font).color(qnc_ui_kit::colour(c.text));
        if project.active {
            name = name.strong();
        }
        ui.add(egui::Label::new(name).truncate().selectable(false));
    });
    let delete = ui.interact(delete_rect, ui.id().with(("project_row_delete", index)), Sense::click()).on_hover_text(qnc_text::text("project.remove_project"));
    let colour = if delete.hovered() { c.text } else { c.muted };
    ui.painter().text(delete_rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(font + 5.0), qnc_ui_kit::colour(colour));
    if delete.clicked() {
        asked.push(Act::Delete(project.project_id.clone()));
    }
}

/// Draws the list in its place (`rect`); the search text is edited in place.
pub fn show(ui: &mut Ui, theme: &Theme, list: &List, rect: egui::Rect, projects: &mut Projects) -> Vec<Act> {
    let mut asked = Vec::new();
    let c = &theme.colours;
    let font = theme.metrics.font_size;
    ui.painter().rect_filled(rect, 0.0, qnc_ui_kit::colour(c.background));
    let content = rect.shrink(list.panel_pad);
    let content_w = content.width().max(40.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(content).layout(Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(content);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let (title_rect, _) = ui.allocate_exact_size(Vec2::new(content_w, list.title_row_height), Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(title_rect).layout(Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.label(RichText::new(qnc_text::text("project.window_title")).size(font).strong().color(qnc_ui_kit::colour(c.text)));
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(qnc_ui_kit::text_field(&mut projects.search).desired_width((content_w * 0.55).max(120.0)).hint_text(qnc_text::text("project.search_hint")));
            });
        });
        ui.painter().hline(title_rect.x_range(), title_rect.bottom() - 0.5, egui::Stroke::new(1.0, qnc_ui_kit::colour(c.border)));
        ui.add_space(list.below_title_pad);
        let foot_h = theme.metrics.row_height;
        let table_top = ui.cursor().top();
        let table_bottom = (content.bottom() - foot_h).max(table_top + 40.0);
        let table_rect = egui::Rect::from_min_max(egui::pos2(content.left(), table_top), egui::pos2(content.right(), table_bottom));
        ui.scope_builder(egui::UiBuilder::new().max_rect(table_rect).layout(Layout::top_down(egui::Align::Min)), |ui| {
            ui.set_clip_rect(table_rect);
            ui.set_max_width(content_w);
            egui::ScrollArea::vertical().id_salt("project_list").max_height(table_rect.height()).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_max_width(content_w);
                let shown: Vec<Line> = projects.shown().into_iter().cloned().collect();
                if shown.is_empty() {
                    ui.label(RichText::new(qnc_text::text("project.none")).size(font).color(qnc_ui_kit::colour(c.muted)));
                }
                for (at, project) in shown.iter().enumerate() {
                    if at > 0 {
                        ui.add_space(list.row_gap);
                    }
                    row(ui, theme, list, at, project, &mut asked);
                }
            });
        });
        let foot_rect = egui::Rect::from_min_size(egui::pos2(content.left(), table_bottom), Vec2::new(content_w, foot_h));
        ui.scope_builder(egui::UiBuilder::new().max_rect(foot_rect).layout(Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.set_clip_rect(foot_rect);
            ui.label(RichText::new(projects.status().unwrap_or_default()).size(font).color(qnc_ui_kit::colour(c.muted)));
        });
    });
    if let Some(asking) = projects.asking_delete() {
        let question = qnc_confirm::Question { id: "delete_project", title: qnc_text::text("project.remove_project_question"), subject: &asking.name, no: qnc_text::text("common.no"), yes: qnc_text::text("common.yes") };
        match qnc_confirm::show(ui.ctx(), theme, question) {
            Some(true) => asked.push(Act::ConfirmDelete),
            Some(false) => asked.push(Act::CancelDelete),
            None => {}
        }
    }
    asked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_of_the_panel_is_in_the_catalog() {
        let catalog = qnc_actions::catalog();
        for act in [Act::New, Act::Open(String::new()), Act::Delete(String::new()), Act::ConfirmDelete, Act::CancelDelete, Act::ChooseTemplate(String::new()), Act::TogglePicker] {
            assert!(catalog.has(code(&act)), "{} is not in the catalog", code(&act));
        }
    }
}
