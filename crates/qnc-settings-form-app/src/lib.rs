//! The Settings form (copied from the old QNC's `qnc-ai-settings`): the settings of the Project
//! form on their own, in their window with QNC's title bar: the locations first, the template
//! and its advanced settings, the AI switches and the own templates; without the project list
//! and without New project. It is rarely used, so it is apart from the wizard. One copy runs.

use qnc_project_layout::ProjectLayout;
use qnc_project_session::{ProjectIntent, ProjectSession};
use qnc_window_host::Next;

/// Runs the Settings window until the user closes it.
pub fn main() -> Result<(), String> {
    let Some(started) = qnc_project_start::start("qnc-settings", "settings", &["window_width", "window_height"])? else { return Ok(()) };
    let qnc_project_start::Started { root, look, mut layout, mut session, .. } = started;
    let mut look = qnc_form_look::FormLook::of(&root, look);
    let mut stamp = look.stamp();
    let theme = qnc_desktop_theme::Theme::current();
    session.advanced_open = true;
    let size = [look.get("window_width"), look.get("window_height")];
    qnc_window_host::run_framed(qnc_text::text("settings.title"), size, true, move |ctx, ui| {
        look.editor(ctx);
        look.window(ctx, qnc_title_bar::HEIGHT);
        if let Some(moved) = qnc_project_start::relayout(look.model(), &mut stamp) {
            layout = moved;
        }
        if session.component.applications.poll() | session.poll() || session.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        let rect = ui.available_rect_before_wrap();
        let intents = qnc_project_settings_panel::show_settings(ui, &layout, &theme, rect, &mut session, &root);
        for intent in intents {
            session.dispatch(intent);
        }
        question(ctx, &layout, &mut session);
        Next::Show
    })
}

/// The delete question of a user template (NE / DA).
fn question(ctx: &eframe::egui::Context, layout: &ProjectLayout, session: &mut ProjectSession) {
    let Some(template) = session.delete_template_candidate.clone() else { return };
    let (no, yes) = (qnc_text::text("project.no"), qnc_text::text("project.yes"));
    let asked = qnc_confirm::Question { id: "delete_template", title: &layout.texts.delete_template_question, subject: &template.name, no, yes };
    match qnc_confirm::show(ctx, qnc_theme::theme(), asked) {
        Some(true) => session.dispatch(ProjectIntent::ConfirmDeleteTemplate),
        Some(false) => session.dispatch(ProjectIntent::CancelDeleteTemplate),
        None => {}
    }
}
