//! The location fields: the projects' folder and the export folder, each with Choose and,
//! while open, its browser with Select/Cancel (pieces `qnc-location-field` and its view).

use eframe::egui;
use qnc_project_session::{LocationTarget, ProjectIntent};

use crate::panel::Panel;

pub(crate) fn locations(panel: &mut Panel<'_>, ui: &mut egui::Ui, width: f32) {
    if panel.settings_only {
        let size = panel.layout.right_settings_panel.label_font_size;
        ui.label(egui::RichText::new(qnc_text::text("project.locations_title")).size(size).strong().color(panel.theme.text_muted));
    }
    path_row(panel, ui, width, LocationTarget::ProjectsRoot);
    path_row(panel, ui, width, LocationTarget::ExportDir);
}

/// A location field: the path, Choose, and while open its browser with Select/Cancel.
fn path_row(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32, target: LocationTarget) {
    let (theme, texts) = (panel.theme, &panel.layout.texts);
    let metrics = &panel.layout.right_settings_panel;
    let (label, field, id, height) = match target {
        LocationTarget::ProjectsRoot => (&texts.projects_root_label, &mut panel.session.projects_root, "project_root", 170.0),
        LocationTarget::ExportDir => (&texts.export_directory_label, &mut panel.session.export_dir, "export_dir", 150.0),
    };
    let (mut typed, mut pick, mut left) = (false, false, false);
    qnc_settings_rows::inline_row(
        ui,
        content_w,
        label,
        &metrics,
        theme,
        |ui, width| {
            let response = ui.add(qnc_ui_kit::text_field(&mut field.text).desired_width(width.max(40.0)).hint_text(&texts.path_hint));
            typed = response.changed();
            // A typed path is kept on Enter or when the field is left.
            left = response.lost_focus();
        },
        |ui, _| pick = qnc_settings_rows::primary_btn(ui, &texts.choose, true, theme).clicked(),
    );
    let labels = qnc_location_field_view::FieldLabels {
        sources: &texts.sources,
        up: &texts.up,
        disks: &texts.disks,
        internet: &texts.internet,
        empty_folder: &texts.empty_folder,
        empty_disks: &texts.empty_disks,
        empty_lan: &texts.empty_lan,
        empty_internet: &texts.empty_internet,
        confirm: &texts.confirm,
        cancel: &texts.cancel,
    };
    let answer = field.open.then(|| qnc_location_field_view::show_browser(ui, field, id, height, &labels, theme)).flatten();
    if typed {
        panel.session.location_typed(target);
        panel.session.status = texts.settings_changed.clone();
    }
    panel.intents.extend(left.then_some(ProjectIntent::CommitLocation { target }));
    panel.intents.extend(pick.then_some(ProjectIntent::PickLocation { target }));
    panel.intents.extend(answer.map(|answer| ProjectIntent::AnswerLocation { target, answer }));
}
