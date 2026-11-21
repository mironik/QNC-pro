//! The template picker, New project / Story sheet, the template actions and the own template.

use eframe::egui::{self, RichText, Sense, Vec2};
use qnc_project_session::ProjectIntent;

use crate::panel::{bar, Panel};

pub(crate) fn template_picker(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32) {
    let (theme, settings) = (panel.theme, &panel.layout.right_settings_panel);
    let selected_name = panel.session.templates.iter().find(|template| template.selected).map(|template| template.name.clone()).unwrap_or_else(|| "—".to_string());
    ui.set_max_width(content_w);
    ui.label(RichText::new(qnc_text::text("project.workflow_title")).size(settings.label_font_size).strong().color(theme.text_muted));
    ui.add_space(6.0);
    let title = RichText::new(selected_name).size(theme.font_ui).strong().color(theme.text);
    if bar(ui, content_w, theme.surface_alt, theme, title, panel.session.picker_open) {
        panel.session.picker_open = !panel.session.picker_open;
    }
    if !panel.session.picker_open {
        return;
    }
    ui.add_space(settings.section_gap);
    if panel.session.templates.is_empty() {
        ui.label(RichText::new(&panel.layout.texts.templates_empty).size(theme.font_ui).color(theme.text_muted));
        return;
    }
    for template in panel.session.templates.clone() {
        let mut delete_requested = false;
        let row = egui::Frame::NONE.fill(if template.selected { theme.surface } else { theme.bg }).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
            ui.set_max_width(content_w);
            ui.horizontal(|ui| {
                let mut label = RichText::new(&template.name).size(theme.font_ui).color(theme.text);
                if template.selected {
                    label = label.strong();
                }
                ui.label(label);
                if !template.system {
                    ui.label(RichText::new("custom").size(11.0).color(theme.text_muted));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let x = egui::Button::new(RichText::new("X").size(theme.font_ui).color(theme.text_muted)).frame(false);
                    if !template.system && ui.add_sized(Vec2::new(32.0, 24.0), x).on_hover_text(&panel.layout.texts.delete_template_hint).clicked() {
                        delete_requested = true;
                    }
                });
            });
            if !template.description.trim().is_empty() {
                ui.label(RichText::new(&template.description).size(11.0).color(theme.text_muted));
            }
        });
        if delete_requested {
            panel.intents.push(ProjectIntent::RequestDeleteTemplate { template_id: template.template_id });
        } else if row.response.interact(Sense::click()).clicked() {
            panel.intents.push(ProjectIntent::SelectTemplate { template_id: template.template_id });
        }
    }
}

/// After the template: New project opens the story sheet, where its name is typed; the open
/// project's sheet opens with Story sheet.
pub(crate) fn project_create(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32) {
    let (texts, theme) = (&panel.layout.texts, panel.theme);
    let metrics = &panel.layout.right_settings_panel;
    let can_create = !panel.session.selected_template_id.trim().is_empty();
    let open = panel.session.story_project.is_some();
    let mut intent = None;
    qnc_settings_rows::trailing_actions(ui, content_w, metrics, |ui| {
        if open && qnc_settings_rows::action_btn(ui, &texts.story_sheet, theme).clicked() {
            intent = Some(ProjectIntent::OpenStorySheet);
        }
        if qnc_settings_rows::primary_btn(ui, &texts.new_project, can_create, theme).clicked() {
            intent = Some(ProjectIntent::CreateProject);
        }
    });
    panel.intents.extend(intent);
}

pub(crate) fn template_actions(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32) {
    let (texts, theme) = (&panel.layout.texts, panel.theme);
    let metrics = &panel.layout.right_settings_panel;
    let open = panel.session.template_create_open;
    let mut intents = Vec::new();
    qnc_settings_rows::trailing_actions(ui, content_w, metrics, |ui| {
        if open && qnc_settings_rows::action_btn(ui, &texts.cancel, theme).clicked() {
            intents.push(ProjectIntent::CancelTemplateCreate);
        }
        if qnc_settings_rows::action_btn(ui, &texts.new_template, theme).clicked() {
            intents.push(ProjectIntent::BeginTemplateCreate);
        }
    });
    panel.intents.extend(intents);
}

pub(crate) fn custom_template(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32) {
    let (texts, theme) = (&panel.layout.texts, panel.theme);
    let metrics = &panel.layout.right_settings_panel;
    let can_save = !panel.session.template_draft_name.trim().is_empty() && !panel.session.selected_template_id.is_empty();
    let mut should_save = false;
    let base_name = panel.session.selected_template_name();
    let session = &mut *panel.session;
    qnc_settings_rows::section(ui, content_w, &texts.new_template, metrics, theme, |ui| {
        let base = texts.template_base.replace("{base}", &base_name);
        ui.label(RichText::new(base).size(12.0).color(theme.text_muted));
        ui.add_space(8.0);
        qnc_settings_rows::inline_row(
            ui,
            content_w,
            &texts.template_name_label,
            &metrics,
            theme,
            |ui, width| {
                ui.add(qnc_ui_kit::text_field(&mut session.template_draft_name).desired_width(width.max(40.0)).hint_text(&texts.template_name_hint));
            },
            |ui, _| {
                if qnc_settings_rows::primary_btn(ui, &texts.save, can_save, theme).clicked() {
                    should_save = true;
                }
            },
        );
        ui.add_space(8.0);
        ui.add(qnc_ui_kit::text_area(&mut session.template_draft_description).desired_width((content_w - metrics.inner_pad_x * 2.0).max(80.0)).desired_rows(2).hint_text(&texts.short_description_hint));
    });
    if should_save {
        panel.intents.push(ProjectIntent::SaveUserTemplate);
    }
}
