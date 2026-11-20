use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::{create, Reader, TableModule, Writer};

struct Notes;

impl TableModule for Notes {
    fn id(&self) -> &'static str {
        "notes"
    }
    fn tables(&self) -> &'static [&'static str] {
        &["notes"]
    }
    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, text TEXT NOT NULL); CREATE TABLE IF NOT EXISTS others (id INTEGER PRIMARY KEY)")
            .map_err(|e| e.to_string())
    }
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        match request["do"].as_str() {
            Some("add") => {
                conn.execute("INSERT INTO notes (text) VALUES (?1)", [request["text"].as_str().unwrap_or("")]).map_err(|e| e.to_string())?;
                Ok(json!({ "id": conn.last_insert_rowid() }))
            }
            Some("add_then_fail") => {
                conn.execute("INSERT INTO notes (text) VALUES ('lost')", []).map_err(|e| e.to_string())?;
                Err("no".to_string())
            }
            Some("touch_others") => {
                conn.execute("INSERT INTO others DEFAULT VALUES", []).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            _ => Err("unknown request".to_string()),
        }
    }
}

fn fresh(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("qnc-db-{}-{name}.db", std::process::id()));
    for stale in [path.clone(), path.with_extension("db-wal"), path.with_extension("db-shm")] {
        let _ = std::fs::remove_file(stale);
    }
    create(&path).unwrap();
    path
}

fn count(path: &std::path::Path, table: &str) -> i64 {
    let reader = Reader::open(path).unwrap();
    reader.connection().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn a_module_writes_its_own_table_a_failed_request_writes_nothing_and_the_writer_is_the_only_way_in() {
    let path = fresh("own");
    let writer = Writer::start(&path, vec![Box::new(Notes)]).unwrap();
    let reply = writer.execute("notes", json!({ "do": "add", "text": "first" })).unwrap();
    assert_eq!(reply["id"], 1);
    assert!(writer.execute("notes", json!({ "do": "add_then_fail" })).unwrap_err().contains("no"));
    assert_eq!(count(&path, "notes"), 1);
    assert!(writer.execute("nobody", json!({})).unwrap_err().contains("nobody"));
    let pending = writer.submit("notes", json!({ "do": "add", "text": "second" }));
    assert_eq!(pending.wait().unwrap()["id"], 2);
    let reader = Reader::open(&path).unwrap();
    assert!(reader.connection().execute("INSERT INTO notes (text) VALUES ('x')", []).is_err());
}

#[test]
fn a_module_may_not_write_a_table_that_is_not_its_own() {
    let path = fresh("others");
    let writer = Writer::start(&path, vec![Box::new(Notes)]).unwrap();
    let error = writer.execute("notes", json!({ "do": "touch_others" })).unwrap_err();
    assert!(error.contains("not authorized") || error.contains("authoriz"), "{error}");
    assert_eq!(count(&path, "others"), 0);
}

#[test]
fn a_missing_database_is_not_made_by_the_writer_or_the_reader() {
    let path = std::env::temp_dir().join(format!("qnc-db-{}-missing.db", std::process::id()));
    assert!(Writer::start(&path, vec![Box::new(Notes)]).is_err());
    assert!(Reader::open(&path).is_err());
    assert!(!path.exists());
}
