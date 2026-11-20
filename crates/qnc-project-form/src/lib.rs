//! The Project form: one window, one panel. The projects module is polled every frame (it reads
//! the registry every second); what the panel returns goes to the module; Close ends the program.

use qnc_project_panel::Asked;
use qnc_projects::Projects;
use qnc_window_host::Next;

/// Runs the Project window until the user closes it.
pub fn main() -> Result<(), String> {
    let root = qnc_settings::root()?;
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-project");
    qnc_diagnostics::log_panics();
    let mut projects = Projects::open(&root)?;
    qnc_diagnostics::log("project form started");
    let outcome = qnc_window_host::run(qnc_text::text("project.window_title"), [720.0, 520.0], true, move |_ctx, ui| {
        projects.poll();
        let mut next = Next::Show;
        for asked in qnc_project_panel::show(ui, qnc_theme::theme(), &mut projects) {
            match asked {
                Asked::Act(act) => projects.act(act),
                Asked::Close => next = Next::Close,
            }
        }
        next
    });
    if let Err(error) = &outcome {
        qnc_diagnostics::log(format!("window host ended with an error: {error}"));
    }
    outcome
}
