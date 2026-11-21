//! The functions of an ingest the user stopped (the X beside a bar), in the project's own
//! database: the one record between the window that stops and the work that leaves those
//! functions alone. Stopped is stopped for that project; a resume clears it all (an ingest
//! cancelled and kept goes on when the card is back).

use std::collections::BTreeSet;

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::Connection;
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.stopped-work";
pub const TABLE: &str = "stopped_work";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS stopped_work (function TEXT PRIMARY KEY)";

/// The table module: `{"stop": "waves"}` stops one function, `{"resume_all": true}` clears them.
pub struct StoppedWork;

impl TableModule for StoppedWork {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &[TABLE]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
    }

    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(function) = request.get("stop").and_then(Value::as_str) {
            if !qnc_progress::FUNCTIONS.contains(&function) {
                return Err(format!("no function {function} to stop"));
            }
            conn.execute("INSERT OR IGNORE INTO stopped_work (function) VALUES (?1)", [function]).map_err(|e| e.to_string())?;
            return Ok(json!({ "stopped": function }));
        }
        if request.get("resume_all").is_some() {
            conn.execute("DELETE FROM stopped_work", []).map_err(|e| e.to_string())?;
            return Ok(json!({ "resumed": true }));
        }
        Err(format!("stopped-work: unknown request {request}"))
    }
}

/// The functions stopped (none before the table exists).
pub fn read(reader: &Reader) -> Result<BTreeSet<String>, String> {
    let conn = reader.connection();
    let mut statement = match conn.prepare("SELECT function FROM stopped_work") {
        Ok(statement) => statement,
        Err(rusqlite::Error::SqliteFailure(_, Some(message))) if message.contains("no such table") => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Stops `function` (the others stopped stay stopped).
pub fn stop(writer: &Writer, function: &str) -> Result<(), String> {
    writer.execute(MODULE, json!({ "stop": function })).map(|_| ())
}

/// Nothing stopped any more.
pub fn resume_all(writer: &Writer) -> Result<(), String> {
    writer.execute(MODULE, json!({ "resume_all": true })).map(|_| ())
}

/// The stopped functions as the bits of [`qnc_progress::Counts::stopped`].
pub fn bits(stopped: &BTreeSet<String>) -> u8 {
    let mut counts = qnc_progress::Counts::default();
    for function in stopped {
        counts.stop(function);
    }
    counts.stopped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_is_read_back_an_unknown_function_is_refused_and_a_resume_clears_all() {
        let dir = std::env::temp_dir().join(format!("qnc-stopped-work-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(read(&reader).unwrap().is_empty(), "no table yet is nothing stopped");
        let writer = Writer::start(&db, vec![Box::new(StoppedWork)]).unwrap();
        stop(&writer, "waves").unwrap();
        stop(&writer, "waves").unwrap();
        assert!(stop(&writer, "teleport").is_err());
        let stopped = read(&reader).unwrap();
        assert_eq!(stopped.iter().collect::<Vec<_>>(), vec!["waves"]);
        assert_ne!(bits(&stopped), 0);
        resume_all(&writer).unwrap();
        assert!(read(&reader).unwrap().is_empty());
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
