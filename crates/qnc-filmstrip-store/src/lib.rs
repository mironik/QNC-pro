//! The filmstrips of a project's clips, in its own database: one row per clip with its thirteen
//! pictures, each the clip frame it shows, the second of the clip, and the picture file relative
//! to the project folder. The one owner of the table; whoever makes a filmstrip writes it through
//! the database writer, the forms read it, the bars count the clips that have one.

use std::collections::BTreeSet;

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.filmstrip-store";
pub const TABLE: &str = "filmstrips";
/// Raised when the shape of a stored filmstrip changes: older rows are made again.
pub const VERSION: i64 = 1;
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS filmstrips (
    clip_id TEXT PRIMARY KEY, version INTEGER NOT NULL, pictures_json TEXT NOT NULL, made_at_unix_ms INTEGER NOT NULL
)";

/// One picture of a filmstrip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Picture {
    /// The clip frame the picture shows.
    pub frame: u64,
    pub seek_sec: f64,
    /// The JPEG file, relative to the project folder, with `/` between its parts.
    pub file: String,
}

/// A filmstrip as it is written: `{"save": Saved}`; `{"forget": "clip"}` removes one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub clip_id: String,
    pub pictures: Vec<Picture>,
    pub made_at_unix_ms: u64,
}

pub struct FilmstripTables;

impl TableModule for FilmstripTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &[TABLE]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| format!("filmstrip-store: {e}"))
    }

    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(saved) = request.get("save") {
            let saved: Saved = serde_json::from_value(saved.clone()).map_err(|e| format!("filmstrip-store: {e}"))?;
            if saved.pictures.is_empty() || saved.pictures.iter().any(|p| p.file.is_empty() || p.file.starts_with('/') || p.file.contains("..")) {
                return Err(format!("filmstrip-store: the filmstrip of {} is empty or names a file outside the project", saved.clip_id));
            }
            conn.execute(
                "INSERT OR REPLACE INTO filmstrips (clip_id, version, pictures_json, made_at_unix_ms) VALUES (?1, ?2, ?3, ?4)",
                params![saved.clip_id, VERSION, serde_json::to_string(&saved.pictures).map_err(|e| e.to_string())?, saved.made_at_unix_ms as i64],
            )
            .map_err(|e| e.to_string())?;
            return Ok(json!({ "clip_id": saved.clip_id }));
        }
        if let Some(clip_id) = request.get("forget").and_then(Value::as_str) {
            conn.execute("DELETE FROM filmstrips WHERE clip_id = ?1", [clip_id]).map_err(|e| e.to_string())?;
            return Ok(json!({ "clip_id": clip_id }));
        }
        Err(format!("filmstrip-store: unknown request {request}"))
    }
}

/// Writes a filmstrip.
pub fn save(writer: &Writer, saved: &Saved) -> Result<(), String> {
    writer.execute(MODULE, json!({ "save": saved })).map(|_| ())
}

fn no_table(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("no such table"))
}

/// The pictures of a clip's filmstrip of this version; none before the table exists or when
/// the clip has none.
pub fn read(reader: &Reader, clip_id: &str) -> Result<Option<Vec<Picture>>, String> {
    let row = reader.connection().query_row("SELECT pictures_json FROM filmstrips WHERE clip_id = ?1 AND version = ?2", params![clip_id, VERSION], |r| r.get::<_, String>(0)).optional();
    match row {
        Ok(Some(text)) => serde_json::from_str(&text).map(Some).map_err(|e| format!("filmstrip-store: {e}")),
        Ok(None) => Ok(None),
        Err(e) if no_table(&e) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The clips that have a filmstrip of this version.
pub fn made(reader: &Reader) -> Result<BTreeSet<String>, String> {
    let mut statement = match reader.connection().prepare("SELECT clip_id FROM filmstrips WHERE version = ?1") {
        Ok(statement) => statement,
        Err(e) if no_table(&e) => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement.query_map([VERSION], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_filmstrip_is_read_back_counted_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("qnc-filmstrip-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(read(&reader, "c").unwrap().is_none(), "no table yet is no filmstrip");
        let writer = Writer::start(&db, vec![Box::new(FilmstripTables)]).unwrap();
        let strip = Saved { clip_id: "c".into(), pictures: vec![Picture { frame: 0, seek_sec: 0.0, file: "products/filmstrip/c/000_0_00.jpg".into() }], made_at_unix_ms: 7 };
        save(&writer, &strip).unwrap();
        assert!(save(&writer, &Saved { pictures: vec![Picture { frame: 0, seek_sec: 0.0, file: "../x.jpg".into() }], ..strip.clone() }).is_err(), "a file outside the project is refused");
        assert_eq!(read(&reader, "c").unwrap().unwrap(), strip.pictures);
        assert_eq!(made(&reader).unwrap().len(), 1);
        writer.execute(MODULE, json!({ "forget": "c" })).unwrap();
        assert!(made(&reader).unwrap().is_empty());
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
