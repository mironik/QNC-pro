//! The Project form (copied from the old QNC's `qnc-project-blocks/lib.rs` and its desktop):
//! the one board of QNC with the project list in the body of the left column and the settings
//! in the right panel (monitor, head row and dock one pixel), the delete questions over it and
//! the story sheet overlay. The form only passes what the panels and keys ask for to the
//! session, which carries it out through the Project component.

use qnc_board::{BoardNames, BoardSizes};
use qnc_project_layout::ProjectLayout;
use qnc_project_session::{ProjectIntent, ProjectSession, ACTION_IDS};
use qnc_window_host::Next;

const NAMES: BoardNames<'static> = BoardNames { monitor: "", head: "", body: "project-list", right: "project-settings", dock: "" };
const PRESET: &str = "qnc";
const SCOPE: &str = "project";

fn sizes(layout: &ProjectLayout) -> BoardSizes {
    let board = layout.board;
    BoardSizes { left_ratio: board.left_ratio, divider_width: board.divider_width, left_min_width: board.left_min_width, right_min_width: board.right_min_width, monitor_height: board.monitor_height, head_height: board.head_height, dock_height: board.dock_height, monitor_aspect: None }
}

/// The layout with the look of the form put in.
fn layout_with(look: Option<&qnc_form_look::FormLook>) -> Result<ProjectLayout, String> {
    ProjectLayout::load(&look.map(|look| look.pointed()).unwrap_or_default())
}

/// Runs the Project window until the user closes it.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-project");
    qnc_diagnostics::log_panics();
    qnc_settings::choose_language(&root)?;
    let missing = ProjectLayout::actions_missing(ACTION_IDS);
    if !missing.is_empty() {
        return Err(format!("the key catalog lacks the Project actions {missing:?}"));
    }
    let mut look = Some(qnc_form_look::FormLook::load(&root, "projects", &["window_width"])?);
    let mut layout = layout_with(look.as_ref())?;
    let mut stamp = look.as_ref().map(|look| look.stamp()).unwrap_or_default();
    let theme = qnc_desktop_theme::Theme::current();
    let faces = theme.board_faces();
    let ready = layout.left_project_list.ready_text.clone();
    let component = qnc_project_application::ProjectComponent::new(&root);
    let mut session = ProjectSession::new(component, ready, ACTION_IDS.iter().map(|id| id.to_string()).collect());
    qnc_diagnostics::log("project form started");
    let window = layout.window;
    let outcome = qnc_window_host::run(qnc_text::text("project.window_title"), [window.width, window.height], true, move |ctx, ui| {
        if let Some(look) = look.as_mut() {
            look.editor(ctx);
            if look.has("window_height") {
                look.window(ctx, 0.0);
            }
            let now = look.stamp();
            if now != stamp {
                stamp = now;
                match layout_with(Some(look)) {
                    Ok(moved) => layout = moved,
                    Err(error) => qnc_diagnostics::log(format!("project look not applied: {error}")),
                }
            }
        }
        // The registry works in the background: its replies are taken here.
        if session.component.applications.poll() | session.poll() || session.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        let mut intents = keys(ctx, &session);
        qnc_board::show(ui, &sizes(&layout), &faces, &NAMES, |ui, name, rect| {
            match name {
                "project-list" => intents.extend(qnc_project_list_panel::show(ui, &theme, &layout.left_project_list, rect, &mut session)),
                "project-settings" => intents.extend(qnc_project_settings_panel::show(ui, &layout, &theme, rect, &mut session, &root)),
                _ => {}
            }
            None::<()>
        });
        intents.extend(questions(ctx, &layout, &session));
        // The story sheet over the board: name and story data.
        if let Some(overlay) = session.sheet.as_mut() {
            if let Some(answer) = qnc_newsroom_sheet::show(ctx, overlay, &theme) {
                intents.push(ProjectIntent::AnswerStorySheet { answer });
            }
        }
        for intent in intents {
            session.dispatch(intent);
        }
        Next::Show
    });
    if let Err(error) = &outcome {
        qnc_diagnostics::log(format!("window host ended with an error: {error}"));
    }
    outcome
}

/// The keys of the form: while a field is typed in, its keys move between fields; while a
/// dialog, picker, browser or the sheet is open no Project key works.
fn keys(ctx: &eframe::egui::Context, session: &ProjectSession) -> Vec<ProjectIntent> {
    if ctx.wants_keyboard_input() || session.keys_held() {
        return Vec::new();
    }
    let catalog = qnc_actions::catalog();
    let open = qnc_ui_kit::chords(ctx).iter().any(|chord| catalog.action_for(PRESET, SCOPE, chord) == Some("project_open_selected"));
    open.then_some(ProjectIntent::OpenSelectedProject).into_iter().collect()
}

/// The delete questions (NE / DA) over the board.
fn questions(ctx: &eframe::egui::Context, layout: &ProjectLayout, session: &ProjectSession) -> Vec<ProjectIntent> {
    let theme = qnc_theme::theme();
    let (no, yes) = (qnc_text::text("project.no"), qnc_text::text("project.yes"));
    let mut intents = Vec::new();
    if let Some(project) = &session.delete_candidate {
        let asked = qnc_confirm::Question { id: "delete_project", title: &layout.texts.delete_project_question, subject: &project.name, no, yes };
        match qnc_confirm::show(ctx, theme, asked) {
            Some(true) => intents.push(ProjectIntent::ConfirmDeleteProject),
            Some(false) => intents.push(ProjectIntent::CancelDeleteProject),
            None => {}
        }
    }
    if let Some(template) = &session.delete_template_candidate {
        let asked = qnc_confirm::Question { id: "delete_template", title: &layout.texts.delete_template_question, subject: &template.name, no, yes };
        match qnc_confirm::show(ctx, theme, asked) {
            Some(true) => intents.push(ProjectIntent::ConfirmDeleteTemplate),
            Some(false) => intents.push(ProjectIntent::CancelDeleteTemplate),
            None => {}
        }
    }
    intents
}
