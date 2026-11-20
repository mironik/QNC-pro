//! The settings panel of the Project board, mirrored from the old form
//! (`qnc-project-blocks/settings.rs`): the title row "Settings" with a rule, the subtitle in
//! muted text, then in a scroll the WORKFLOW label, the template picker (a raised head with the
//! chosen template and an arrow; open, one row per template with its description) and the
//! strong New project button at the right. Pure drawing; the state is the projects module's.

use egui::{Layout, RichText, Sense, Ui, Vec2};
use qnc_project_layout::Settings;
use qnc_projects::{Act, Projects};
use qnc_theme::Theme;

fn title_row(ui: &mut Ui, theme: &Theme, width: f32) {
    let c = &theme.colours;
    let m = &theme.metrics;
    let output = ui.allocate_ui_with_layout(Vec2::new(width, m.row_height), Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().button_padding = Vec2::new(m.gap, m.pad_y);
        egui::Frame::NONE.fill(qnc_ui_kit::colour(c.background)).inner_margin(egui::Margin { left: m.gap as i8, right: m.gap as i8, top: m.pad_y as i8, bottom: m.pad_y as i8 }).show(ui, |ui| {
            ui.set_min_size(Vec2::new(ui.available_width(), m.control_height));
            ui.set_max_height(m.control_height);
            ui.label(RichText::new(qnc_text::text("project.settings")).size(m.font_size).strong().color(qnc_ui_kit::colour(c.text)));
        });
    });
    let rect = output.response.rect;
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, qnc_ui_kit::colour(c.border)));
}

fn picker(ui: &mut Ui, theme: &Theme, settings: &Settings, width: f32, projects: &Projects, asked: &mut Vec<Act>) {
    let c = &theme.colours;
    let font = theme.metrics.font_size;
    ui.set_max_width(width);
    ui.label(RichText::new(qnc_text::text("project.workflow_title")).size(settings.label_font_size).strong().color(qnc_ui_kit::colour(c.muted)));
    ui.add_space(6.0);
    let chosen = projects.chosen_template().map(|t| t.name.clone()).unwrap_or_else(|| "—".to_string());
    let head = egui::Frame::NONE
        .fill(qnc_ui_kit::colour(c.raised))
        .stroke(egui::Stroke::new(1.0, qnc_ui_kit::colour(c.border)))
        .inner_margin(egui::Margin::symmetric(settings.picker_pad_x as i8, settings.picker_pad_y as i8))
        .show(ui, |ui| {
            ui.set_max_width(width);
            ui.horizontal(|ui| {
                ui.label(RichText::new(chosen).size(font).strong().color(qnc_ui_kit::colour(c.text)));
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if projects.picker_open() { "▲" } else { "▼" }).size(11.0).color(qnc_ui_kit::colour(c.muted)));
                });
            });
        });
    if head.response.interact(Sense::click()).clicked() {
        asked.push(Act::TogglePicker);
    }
    if !projects.picker_open() {
        return;
    }
    ui.add_space(settings.section_gap);
    if projects.templates().is_empty() {
        ui.label(RichText::new(qnc_text::text("project.no_templates")).size(font).color(qnc_ui_kit::colour(c.muted)));
    }
    for template in projects.templates() {
        let selected = template.template_id == projects.draft.template_id;
        let fill = if selected { c.panel } else { c.background };
        let row = egui::Frame::NONE.fill(qnc_ui_kit::colour(fill)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
            ui.set_max_width(width);
            let mut name = RichText::new(&template.name).size(font).color(qnc_ui_kit::colour(c.text));
            if selected {
                name = name.strong();
            }
            ui.label(name);
            if !template.description.trim().is_empty() {
                ui.label(RichText::new(&template.description).size(11.0).color(qnc_ui_kit::colour(c.muted)));
            }
        });
        if row.response.interact(Sense::click()).clicked() {
            asked.push(Act::ChooseTemplate(template.template_id.clone()));
        }
    }
}

/// Draws the settings panel in its place (`rect`).
pub fn show(ui: &mut Ui, theme: &Theme, settings: &Settings, rect: egui::Rect, projects: &Projects) -> Vec<Act> {
    let mut asked = Vec::new();
    let c = &theme.colours;
    ui.painter().rect_filled(rect, 0.0, qnc_ui_kit::colour(c.background));
    let inner = rect.shrink(settings.panel_pad);
    let width = inner.width().max(40.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(inner);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        title_row(ui, theme, width);
        egui::Frame::NONE.inner_margin(egui::Margin { left: settings.inner_pad_x as i8, right: settings.inner_pad_x as i8, top: 0, bottom: settings.inner_pad_y as i8 }).show(ui, |ui| {
            ui.set_max_width(width);
            ui.label(RichText::new(qnc_text::text("project.choose_workflow_and_review")).size(12.0).color(qnc_ui_kit::colour(c.muted)));
        });
        let scroll_top = ui.cursor().top() + 1.0;
        let scroll_rect = egui::Rect::from_min_size(egui::pos2(inner.left(), scroll_top), Vec2::new(width, (inner.bottom() - scroll_top).max(48.0)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(scroll_rect).layout(Layout::top_down(egui::Align::Min)), |ui| {
            ui.set_clip_rect(scroll_rect);
            egui::ScrollArea::vertical().id_salt("project_settings").max_height(scroll_rect.height()).auto_shrink([false, true]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin::symmetric(settings.inner_pad_x as i8, settings.inner_pad_y as i8)).show(ui, |ui| {
                    let width = (width - 2.0 * settings.inner_pad_x).max(40.0);
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing.y = settings.section_gap;
                    picker(ui, theme, settings, width, projects, &mut asked);
                    ui.allocate_ui_with_layout(Vec2::new(width, settings.row_height), Layout::right_to_left(egui::Align::Center), |ui| {
                        let can = projects.chosen_template().is_some() && !projects.busy();
                        if qnc_ui_kit::button(ui, theme, qnc_text::text("project.new"), can, true).clicked() {
                            asked.push(Act::New);
                        }
                    });
                });
            });
        });
    });
    asked
}
