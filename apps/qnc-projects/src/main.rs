//! QNC Projects: only the start of its form.
#![windows_subsystem = "windows"]

fn main() {
    let outcome = qnc_projects_form::main();
    let _ = outcome.map_err(|error| eprintln!("{error}"));
}
