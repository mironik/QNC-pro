//! The story sheet tables of a project database and their requests.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::{Result, StorySheet};

/// The tables this module owns; it writes no other.
pub(crate) const OWNED: [&str; 2] = ["project_story_sheet", "project_story_values"];

pub(crate) fn save(conn: &Connection, sheet: &StorySheet) -> Result<()> {
    if !sheet.sheet.is_object() {
        return Err(qnc_text::text("db.story_sheet_invalid").into());
    }
    if let Some(code) = sheet.values.keys().find(|code| !valid_code(code)) {
        return Err(qnc_text::fill("db.story_sheet_code", &[("code", code)]));
    }
    conn.execute(
        "INSERT OR REPLACE INTO project_story_sheet (id, sheet_json, saved_at_utc) VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        params![sheet.sheet.to_string()],
    )
    .map_err(err)?;
    conn.execute("DELETE FROM project_story_values", []).map_err(err)?;
    for (code, value) in &sheet.values {
        conn.execute("INSERT INTO project_story_values (code, value_json) VALUES (?1, ?2)", params![code, value.to_string()]).map_err(err)?;
    }
    Ok(())
}

/// The kept sheet; a database without the tables (no sheet kept yet) reads an empty one.
pub(crate) fn read(conn: &Connection) -> Result<StorySheet> {
    let ready: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='view' AND name='public_project_story_values')", [], |r| r.get(0))
        .map_err(err)?;
    if !ready {
        return Ok(StorySheet::default());
    }
    let sheet = conn
        .query_row("SELECT sheet_json FROM public_project_story_sheet", [], |r| r.get::<_, String>(0))
        .map(|json| serde_json::from_str(&json).unwrap_or(Value::Null))
        .or_else(|error| if error == rusqlite::Error::QueryReturnedNoRows { Ok(Value::Null) } else { Err(err(error)) })?;
    let mut statement = conn.prepare("SELECT code, value_json FROM public_project_story_values ORDER BY code").map_err(err)?;
    let rows = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    let values = rows.into_iter().map(|(code, json)| (code, serde_json::from_str(&json).unwrap_or(Value::Null))).collect();
    Ok(StorySheet { sheet, values })
}

/// A field code: lower-case letters, digits and `_`, starting with a letter.
pub(crate) fn valid_code(code: &str) -> bool {
    code.chars().next().is_some_and(|first| first.is_ascii_lowercase()) && code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_story_sheet (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            sheet_json TEXT NOT NULL CHECK (json_valid(sheet_json)),
            saved_at_utc TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS project_story_values (
            code TEXT PRIMARY KEY,
            value_json TEXT NOT NULL CHECK (json_valid(value_json))
        );
        CREATE VIEW IF NOT EXISTS public_project_story_sheet AS
            SELECT sheet_json, saved_at_utc FROM project_story_sheet;
        CREATE VIEW IF NOT EXISTS public_project_story_values AS
            SELECT code, value_json FROM project_story_values;",
    )
    .map_err(err)
}

pub(crate) fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
