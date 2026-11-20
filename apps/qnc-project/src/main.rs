//! QNC Project: only the start of its form.
#![windows_subsystem = "windows"]

fn main() {
    let outcome = qnc_project_form::main();
    let _ = outcome.map_err(|error| eprintln!("{error}"));
}
