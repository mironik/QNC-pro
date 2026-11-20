//! The Project panel: the projects of this computer, the active one marked, a new project from a
//! template, Open and Delete (after its question). Pure drawing: it shows what the projects
//! module holds and returns the actions of the catalog the user asked for.

use egui::{RichText, Ui};
use qnc_projects::{Act, Line, Projects};
use qnc_theme::Theme;

/// What the user asked, named as in the action catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Act(Act),
    Close,
}

impl Asked {
    /// The action's id in `contracts/keyboard-shortcuts.json`.
    pub fn code(&self) -> &'static str {
        match self {
            Asked::Act(Act::New) => "project_new",
            Asked::Act(Act::Open(_)) => "project_open",
            Asked::Act(Act::Delete(_)) => "project_delete",
            Asked::Act(Act::ConfirmDelete) => "project_delete_confirm",
            Asked::Act(Act::CancelDelete) => "project_delete_cancel",
            Asked::Act(Act::ChooseTemplate(_)) => "project_choose_template",
            Asked::Close => "close_window",
        }
    }
}

fn line(ui: &mut Ui, theme: &Theme, project: &Line, busy: bool, asked: &mut Vec<Asked>) {
    let text = qnc_ui_kit::colour(theme.colours.text);
    let muted = qnc_ui_kit::colour(theme.colours.muted);
    let focus = qnc_ui_kit::colour(theme.colours.focus);
    ui.horizontal(|ui| {
        let name = RichText::new(&project.name).color(if project.active { focus } else { text }).strong();
        ui.label(name);
        ui.label(RichText::new(&project.created).color(muted));
        if project.active {
            ui.label(RichText::new(qnc_text::text("project.active")).color(focus));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if qnc_ui_kit::button(ui, theme, qnc_text::text("common.delete"), !busy, false).clicked() {
                asked.push(Asked::Act(Act::Delete(project.project_id.clone())));
            }
            if qnc_ui_kit::button(ui, theme, qnc_text::text("project.open"), !busy && !project.active, false).clicked() {
                asked.push(Asked::Act(Act::Open(project.project_id.clone())));
            }
        });
    });
}

/// Draws the projects; the draft is edited in place.
pub fn show(ui: &mut Ui, theme: &Theme, projects: &mut Projects) -> Vec<Asked> {
    let mut asked = Vec::new();
    let text = qnc_ui_kit::colour(theme.colours.text);
    let muted = qnc_ui_kit::colour(theme.colours.muted);
    let danger = qnc_ui_kit::colour(theme.colours.danger);
    let busy = projects.busy();
    ui.heading(RichText::new(qnc_text::text("project.window_title")).color(text));
    ui.add_space(theme.metrics.gap);
    if let Some(asking) = projects.asking_delete().cloned() {
        ui.label(RichText::new(qnc_text::fill("project.delete_ask", &[("name", &asking.name)])).color(danger));
        ui.horizontal(|ui| {
            if qnc_ui_kit::button(ui, theme, qnc_text::text("common.delete"), !busy, true).clicked() {
                asked.push(Asked::Act(Act::ConfirmDelete));
            }
            if qnc_ui_kit::button(ui, theme, qnc_text::text("common.cancel"), true, false).clicked() {
                asked.push(Asked::Act(Act::CancelDelete));
            }
        });
        ui.add_space(theme.metrics.gap);
    }
    if projects.lines().is_empty() {
        ui.label(RichText::new(qnc_text::text("project.none")).color(muted));
    }
    for project in projects.lines().to_vec() {
        line(ui, theme, &project, busy, &mut asked);
    }
    ui.add_space(theme.metrics.gap);
    ui.separator();
    ui.label(RichText::new(qnc_text::text("project.new")).color(text).strong());
    ui.horizontal(|ui| {
        ui.label(RichText::new(qnc_text::text("project.name")).color(muted));
        ui.add(qnc_ui_kit::text_field(&mut projects.draft.name, 320.0).hint_text(qnc_text::text("project.name_hint")));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(qnc_text::text("project.template")).color(muted));
        for template in projects.templates().to_vec() {
            let on = template.template_id == projects.draft.template_id;
            if qnc_ui_kit::button(ui, theme, &template.name, !busy, on).on_hover_text(&template.description).clicked() {
                asked.push(Asked::Act(Act::ChooseTemplate(template.template_id.clone())));
            }
        }
    });
    ui.horizontal(|ui| {
        if qnc_ui_kit::button(ui, theme, qnc_text::text("project.make"), !busy, false).clicked() {
            asked.push(Asked::Act(Act::New));
        }
        if qnc_ui_kit::button(ui, theme, qnc_text::text("common.close"), true, false).clicked() {
            asked.push(Asked::Close);
        }
    });
    if let Some(status) = projects.status() {
        ui.add_space(theme.metrics.gap);
        ui.label(RichText::new(status).color(muted));
    }
    asked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_of_the_panel_is_in_the_catalog() {
        let catalog = qnc_actions::catalog();
        for asked in [Asked::Act(Act::New), Asked::Act(Act::Open(String::new())), Asked::Act(Act::Delete(String::new())), Asked::Act(Act::ConfirmDelete), Asked::Act(Act::CancelDelete), Asked::Act(Act::ChooseTemplate(String::new())), Asked::Close] {
            assert!(catalog.has(asked.code()), "{} is not in the catalog", asked.code());
        }
    }
}
