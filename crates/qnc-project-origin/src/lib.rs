//! The origin of a project (copied from the old QNC's `project_origin`): the workstation and
//! the user that made it, captured once at creation and never again (reopening or copying a
//! project recaptures nothing). The same table is kept in the registry of the computer that made
//! the project and in the project's own database; this block owns that table wherever it is and
//! is the one that writes it. Unavailable hardware is kept as null, never made up.

use rusqlite::{params, Connection};
use serde_json::Value;

pub const TABLE: &str = "project_origin";

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS project_origin (
    project_id TEXT PRIMARY KEY, project_name TEXT NOT NULL, created_at TEXT NOT NULL,
    identity_json TEXT NOT NULL CHECK(json_valid(identity_json))
);
CREATE VIEW IF NOT EXISTS public_project_origin AS
    SELECT project_id, project_name, created_at,
        json_extract(identity_json, '$.contract_version') AS identity_contract_version,
        json_extract(identity_json, '$.workstation_name') AS workstation_name,
        json_extract(identity_json, '$.user_name') AS user_name,
        json_extract(identity_json, '$.hardware_serial.value') AS serial_number,
        json_extract(identity_json, '$.hardware_serial.kind') AS serial_kind,
        json_extract(identity_json, '$.hardware_serial.source') AS serial_source,
        identity_json
    FROM project_origin;";

/// The table and its public view in `conn`.
pub fn ensure(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
}

/// Captures the origin of `project_id`: only once, on creation; a second capture is refused.
pub fn insert(conn: &Connection, project_id: &str, name: &str, created_at: &str, identity: &Value) -> Result<(), String> {
    if !identity.is_object() {
        return Err("the identity of the workstation is not an object".into());
    }
    conn.execute("INSERT INTO project_origin (project_id, project_name, created_at, identity_json) VALUES (?1, ?2, ?3, ?4)", params![project_id, name, created_at, identity.to_string()]).map_err(|e| e.to_string()).map(drop)
}

/// The origin rows of `project_id` go with the project.
pub fn remove(conn: &Connection, project_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM project_origin WHERE project_id = ?1", [project_id]).map_err(|e| e.to_string()).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_hardware_is_stored_as_null_not_fabricated_and_the_origin_is_captured_once() {
        let db = Connection::open_in_memory().unwrap();
        ensure(&db).unwrap();
        let identity = serde_json::json!({ "contract_version": "0.1.0", "workstation_name": "Test workstation", "user_name": "test-user", "hardware_serial": null, "issues": [] });
        insert(&db, "test-id", "Synthetic", "epoch_1788696000", &identity).unwrap();
        let serial: Option<String> = db.query_row("SELECT serial_number FROM public_project_origin", [], |r| r.get(0)).unwrap();
        assert!(serial.is_none());
        assert!(insert(&db, "test-id", "Synthetic", "epoch_1788696001", &identity).is_err());
        assert!(insert(&db, "other", "X", "epoch_1", &serde_json::json!(3)).is_err());
        let stored: String = db.query_row("SELECT identity_json FROM public_project_origin", [], |r| r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), identity);
        remove(&db, "test-id").unwrap();
        let count: i64 = db.query_row("SELECT COUNT(*) FROM project_origin", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
