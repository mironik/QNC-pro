//! The project list panel of the Project board (copied from the old QNC's
//! `qnc-project-blocks/list.rs`): the title row with the search, one row per project (name,
//! strong when active, the date in muted text at the right, x to remove), a click on the row
//! opens it, the status line at the foot. Pure drawing: the rows, the search text and the
//! status come from the session; what the user asked goes back as intents.

mod panel;

pub use panel::show;
