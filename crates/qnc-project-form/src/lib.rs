//! The Project form: the one board of QNC with the project list in the body of the left column
//! and the settings in the right panel (monitor, head row and dock one pixel), as the old
//! Project form was laid out. The projects module is polled every frame (it reads the registry
//! every second); what the panels return goes to the module.

use qnc_board::{BoardFaces, BoardNames, BoardSizes};
use qnc_project_layout::layout;
use qnc_projects::Projects;
use qnc_window_host::Next;

const NAMES: BoardNames<'static> = BoardNames { monitor: "", head: "", body: "project-list", right: "project-settings", dock: "" };

fn sizes() -> BoardSizes {
    let board = layout().board;
    BoardSizes { left_ratio: board.left_ratio, divider_width: board.divider_width, left_min_width: board.left_min_width, right_min_width: board.right_min_width, monitor_height: board.monitor_height, head_height: board.head_height, dock_height: board.dock_height, monitor_aspect: None }
}

/// Runs the Project window until the user closes it.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-project");
    qnc_diagnostics::log_panics();
    qnc_settings::choose_language(&root)?;
    let mut projects = Projects::open(&root)?;
    qnc_diagnostics::log("project form started");
    let theme = qnc_theme::theme();
    let bg = qnc_ui_kit::colour(theme.colours.background);
    let faces = BoardFaces { bg, left: bg, right: bg, divider: qnc_ui_kit::colour(theme.colours.border) };
    let window = layout().window;
    let outcome = qnc_window_host::run(qnc_text::text("project.window_title"), [window.width, window.height], true, move |_ctx, ui| {
        projects.poll();
        let mut asked = Vec::new();
        qnc_board::show(ui, &sizes(), &faces, &NAMES, |ui, name, rect| {
            match name {
                "project-list" => asked.extend(qnc_project_list_panel::show(ui, theme, &layout().list, rect, &mut projects)),
                "project-settings" => asked.extend(qnc_project_settings_panel::show(ui, theme, &layout().settings, rect, &projects)),
                _ => {}
            }
            None::<()>
        });
        for act in asked {
            projects.act(act);
        }
        Next::Show
    });
    if let Err(error) = &outcome {
        qnc_diagnostics::log(format!("window host ended with an error: {error}"));
    }
    outcome
}
