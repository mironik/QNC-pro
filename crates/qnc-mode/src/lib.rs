//! The way this computer works and where each project's story is. Both are rows of the
//! computer database (`qnc-computer.db` under the computer's data), never a JSON file nor a
//! process's memory: the tray, the ingest and the forms read the same row through the
//! database and change it only through its writer. This block owns the two tables.

use std::path::{Path, PathBuf};

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::Connection;
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.mode";
pub const COMPUTER_DB: &str = "qnc-computer.db";

/// What this computer does after a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A laptop in the field: the card is linked and AI Media Assist opens at once.
    BreakingNews,
    /// A laptop in the field: the card is copied, Ingest opens when it is in.
    Story,
    /// An ingest point on the network: ingest and AI only, no form.
    Net,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::BreakingNews, Mode::Story, Mode::Net];

    pub fn code(self) -> &'static str {
        match self {
            Mode::BreakingNews => "breaking_news",
            Mode::Story => "story",
            Mode::Net => "net",
        }
    }

    pub fn of_code(code: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.code() == code)
    }

    /// The label code of the menu.
    pub fn label(self) -> &'static str {
        match self {
            Mode::BreakingNews => "mode.breaking_news",
            Mode::Story => "mode.story",
            Mode::Net => "mode.net",
        }
    }
}

/// The computer database file.
pub fn computer_db(root: &Path) -> PathBuf {
    qnc_settings::data_dir(root).join(COMPUTER_DB)
}

/// The writer of the computer database with this block's tables; the file is made when missing.
pub fn writer(root: &Path) -> Result<Writer, String> {
    let file = computer_db(root);
    if !file.is_file() {
        std::fs::create_dir_all(qnc_settings::data_dir(root)).map_err(|e| e.to_string())?;
        qnc_db::create(&file)?;
    }
    Writer::start(&file, vec![Box::new(ModeTables)])
}

/// The two tables.
pub struct ModeTables;

impl TableModule for ModeTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["computer_mode", "story_step"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS computer_mode (one INTEGER PRIMARY KEY CHECK (one = 1), mode TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS story_step (project_id TEXT PRIMARY KEY, step TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())
    }

    /// `{"set_mode": "story"}` or `{"set_step": {"project_id": "...", "step": "..."}}`.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(code) = request.get("set_mode").and_then(Value::as_str) {
            let mode = Mode::of_code(code).ok_or_else(|| format!("no mode is called {code}"))?;
            conn.execute("INSERT OR REPLACE INTO computer_mode (one, mode) VALUES (1, ?1)", [mode.code()])
                .map_err(|e| e.to_string())?;
            return Ok(json!({ "mode": mode.code() }));
        }
        if let Some(step) = request.get("set_step") {
            let project = step["project_id"].as_str().filter(|p| !p.trim().is_empty()).ok_or("set_step needs a project_id")?;
            let step = step["step"].as_str().filter(|s| !s.trim().is_empty()).ok_or("set_step needs a step")?;
            conn.execute("INSERT OR REPLACE INTO story_step (project_id, step) VALUES (?1, ?2)", [project, step])
                .map_err(|e| e.to_string())?;
            return Ok(json!({ "project_id": project, "step": step }));
        }
        Err("unknown request".into())
    }
}

/// Sets the computer's mode.
pub fn set_mode(writer: &Writer, mode: Mode) -> Result<(), String> {
    writer.execute(MODULE, json!({ "set_mode": mode.code() })).map(|_| ())
}

/// Sets where a project's story is.
pub fn set_step(writer: &Writer, project_id: &str, step: &str) -> Result<(), String> {
    writer.execute(MODULE, json!({ "set_step": { "project_id": project_id, "step": step } })).map(|_| ())
}

/// The computer's mode; `None` while none is chosen.
pub fn read_mode(reader: &Reader) -> Result<Option<Mode>, String> {
    let code: Option<String> = reader
        .connection()
        .query_row("SELECT mode FROM computer_mode WHERE one = 1", [], |r| r.get(0))
        .map(Some)
        .or_else(|e| if no_rows_or_table(&e) { Ok(None) } else { Err(e.to_string()) })?;
    Ok(code.as_deref().and_then(Mode::of_code))
}

/// Where a project's story is; `None` while nothing was recorded.
pub fn read_step(reader: &Reader, project_id: &str) -> Result<Option<String>, String> {
    reader
        .connection()
        .query_row("SELECT step FROM story_step WHERE project_id = ?1", [project_id], |r| r.get(0))
        .map(Some)
        .or_else(|e| if no_rows_or_table(&e) { Ok(None) } else { Err(e.to_string()) })
}

fn no_rows_or_table(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::QueryReturnedNoRows) || error.to_string().contains("no such table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_and_the_story_step_are_rows_read_back_from_the_computer_database() {
        let root = std::env::temp_dir().join(format!("qnc-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        let writer = writer(&root).unwrap();
        let reader = Reader::open(&computer_db(&root)).unwrap();
        assert_eq!(read_mode(&reader).unwrap(), None);
        set_mode(&writer, Mode::Story).unwrap();
        assert_eq!(read_mode(&reader).unwrap(), Some(Mode::Story));
        set_mode(&writer, Mode::BreakingNews).unwrap();
        assert_eq!(read_mode(&reader).unwrap(), Some(Mode::BreakingNews));
        assert!(writer.execute(MODULE, json!({ "set_mode": "fast" })).is_err());
        assert_eq!(read_step(&reader, "p1").unwrap(), None);
        set_step(&writer, "p1", "ai_media_assist").unwrap();
        set_step(&writer, "p1", "ma_script").unwrap();
        assert_eq!(read_step(&reader, "p1").unwrap().as_deref(), Some("ma_script"));
        drop(reader);
        drop(writer);
        let _ = std::fs::remove_dir_all(&root);
    }
}
