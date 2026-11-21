//! The Projects form (copied from the old QNC's `qnc-ai-projects`): the project list on its own,
//! read top to bottom, the projects and right after the last one the row of a new project; the
//! window is as tall as that, with QNC's title bar, brought above the others when it opens. A
//! project opened becomes the active one and the next form (AI Media Assist) opens; the new
//! row opens Ingest for a new story. One copy runs.

use qnc_project_layout::ProjectLayout;
use qnc_project_session::{ProjectIntent, ProjectSession};
use qnc_window_host::Next;

/// The program a project opened goes on in, and the one a new story starts in.
const NEXT: &str = "qnc-media-assist-audio-ai";
const INGEST: &str = "qnc-ingest";

/// Runs the Projects window until the user closes it or a project is handed on.
pub fn main() -> Result<(), String> {
    let Some(started) = qnc_project_start::start("qnc-projects", "projects", &["window_width"])? else { return Ok(()) };
    let qnc_project_start::Started { root, look, mut layout, mut session, .. } = started;
    let mut look = qnc_form_look::FormLook::of(&root, look);
    let mut stamp = look.stamp();
    let theme = qnc_desktop_theme::Theme::current();
    let new_label = qnc_text::text("projects.new");
    let mut front = qnc_window_front::WindowFront::default();
    let (mut shown, mut handed, mut size) = (false, false, [0.0_f32, 0.0_f32]);
    qnc_window_host::run_framed(qnc_text::text("projects.title"), [look.get("window_width"), 360.0], true, move |ctx, ui| {
        if !shown {
            shown = true;
            front.bring(ctx);
        }
        front.follow(ctx);
        look.editor(ctx);
        if let Some(moved) = qnc_project_start::relayout(look.model(), &mut stamp) {
            layout = moved;
        }
        if session.component.applications.poll() | session.poll() || session.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        // Closing takes a frame or two; nothing is opened again meanwhile.
        if handed {
            return Next::Show;
        }
        let mut intents = Vec::new();
        let alone = qnc_project_list_panel::show_alone(ui, &theme, &layout.left_project_list, &mut session, new_label, &mut intents);
        for intent in intents {
            session.dispatch(intent);
        }
        question(ctx, &layout, &mut session);
        // The width is the look's (the look editor moves it), the height the content's.
        let wanted = [look.get("window_width"), (alone.height + qnc_title_bar::HEIGHT + qnc_title_bar::BOTTOM_MARGIN).ceil()];
        if alone.height > 0.0 && ((wanted[1] - size[1]).abs() > 1.0 || wanted[0] != size[0]) {
            size = wanted;
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(wanted.into()));
        }
        if alone.new_asked {
            handed = open(INGEST, &["--new-story"]);
        } else if session.take_navigation_trigger() {
            handed = open(NEXT, &[]);
        }
        if handed { Next::Close } else { Next::Show }
    })
}

/// The next program, beside this one; whether it was opened.
fn open(name: &str, args: &[&str]) -> bool {
    match qnc_launch::start_detached(name, args, qnc_launch::Priority::Same) {
        Ok(_) => {
            qnc_diagnostics::log(format!("projects handed over to {name}"));
            true
        }
        Err(error) => {
            qnc_diagnostics::log(format!("projects {name} not opened: {error}"));
            false
        }
    }
}

/// The delete question (NE / DA) over the list.
fn question(ctx: &eframe::egui::Context, layout: &ProjectLayout, session: &mut ProjectSession) {
    let Some(project) = session.delete_candidate.clone() else { return };
    let (no, yes) = (qnc_text::text("project.no"), qnc_text::text("project.yes"));
    let asked = qnc_confirm::Question { id: "delete_project", title: &layout.texts.delete_project_question, subject: &project.name, no, yes };
    match qnc_confirm::show(ctx, qnc_theme::theme(), asked) {
        Some(true) => session.dispatch(ProjectIntent::ConfirmDeleteProject),
        Some(false) => session.dispatch(ProjectIntent::CancelDeleteProject),
        None => {}
    }
}
