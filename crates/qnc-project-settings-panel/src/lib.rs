//! The settings panel of the Project board (copied from the old QNC's
//! `qnc-project-blocks/settings.rs` and `project_advanced.rs`): the template picker, New
//! project, and under the Advanced bar the AI switches, the location fields with their
//! browsers, the template actions, the Advanced sections of the layout contract and the own
//! template. Pure drawing over the session; what the user asked goes back as intents.

mod advanced;
mod locations;
mod panel;
mod picker;

pub use panel::show;
