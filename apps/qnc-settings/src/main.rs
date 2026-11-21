//! QNC Settings: only the start of its form.
#![windows_subsystem = "windows"]

fn main() {
    let outcome = qnc_settings_form_app::main();
    let _ = outcome.map_err(|error| eprintln!("{error}"));
}
