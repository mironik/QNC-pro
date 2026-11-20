//! The registry of the projects of this computer: `data/qnc-projects.db`, in the shape the old
//! QNC saved it (`projects`, `app_settings` with `active_project_id`, `project_storage_locations`
//! with the private folder of each project, and the public views). This block owns those
//! tables: it makes the file when missing, writes through the database writer and reads for
//! everyone. The public identity of a project is its URI; its folder is a private binding.

use std::path::{Path, PathBuf};

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.registry";
pub const FILE: &str = "qnc-projects.db";
pub const ACTIVE_KEY: &str = "active_project_id";
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS projects (
    project_id TEXT PRIMARY KEY, name TEXT NOT NULL, project_uri TEXT NOT NULL,
    created_at TEXT, updated_at TEXT, created_by TEXT, updated_by TEXT, last_opened_at TEXT
);
CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS project_storage_locations (
    project_id TEXT PRIMARY KEY, local_path TEXT NOT NULL, created_at TEXT, updated_at TEXT
);
CREATE VIEW IF NOT EXISTS public_projects AS
    SELECT project_id, name, project_uri, created_at, updated_at, last_opened_at FROM projects;
CREATE VIEW IF NOT EXISTS public_app_settings AS SELECT key, value FROM app_settings;";

/// A registered project: its identity and, privately, its folder on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registered {
    pub project_id: String,
    pub name: String,
    pub project_uri: String,
    pub local_path: PathBuf,
    pub created_at: Option<String>,
}

impl Registered {
    pub fn project_db(&self) -> PathBuf {
        self.local_path.join("project.db")
    }
}

pub fn file(root: &Path) -> PathBuf {
    qnc_settings::data_dir(root).join(FILE)
}

/// The writer of the registry with this block's tables; the file is made when missing.
pub fn writer(root: &Path) -> Result<Writer, String> {
    let file = file(root);
    if !file.is_file() {
        std::fs::create_dir_all(qnc_settings::data_dir(root)).map_err(|e| e.to_string())?;
        qnc_db::create(&file)?;
    }
    Writer::start(&file, vec![Box::new(RegistryTables)])
}

pub fn reader(root: &Path) -> Result<Reader, String> {
    Reader::open(&file(root))
}

pub struct RegistryTables;

impl TableModule for RegistryTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["projects", "app_settings", "project_storage_locations"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
    }

    /// `{"register": {project_id, name, project_uri, local_path, at}}` (and makes it active),
    /// `{"activate": project_id}`, `{"deactivate": true}`, `{"delete": project_id}` (the
    /// newest remaining project becomes active when the deleted one was).
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        let fail = |e: rusqlite::Error| e.to_string();
        if let Some(r) = request.get("register") {
            let id = r["project_id"].as_str().ok_or("register needs a project_id")?;
            qnc_uri::valid_id(id)?;
            let uri = r["project_uri"].as_str().ok_or("register needs a project_uri")?;
            qnc_uri::parse(uri)?;
            let name = r["name"].as_str().filter(|n| !n.trim().is_empty()).ok_or("register needs a name")?;
            let path = r["local_path"].as_str().ok_or("register needs a local_path")?;
            let at = r["at"].as_str();
            conn.execute("INSERT INTO projects(project_id,name,project_uri,created_at,updated_at,last_opened_at) VALUES(?1,?2,?3,?4,?4,?4)", params![id, name, uri, at]).map_err(fail)?;
            conn.execute("INSERT INTO project_storage_locations(project_id,local_path,created_at,updated_at) VALUES(?1,?2,?3,?3)", params![id, path, at]).map_err(fail)?;
            conn.execute("INSERT OR REPLACE INTO app_settings(key,value) VALUES(?1,?2)", params![ACTIVE_KEY, id]).map_err(fail)?;
            return Ok(json!({ "project_id": id }));
        }
        if let Some(id) = request.get("activate").and_then(Value::as_str) {
            let known: bool = conn.query_row("SELECT COUNT(*) FROM projects WHERE project_id=?1", [id], |r| r.get::<_, i64>(0)).map_err(fail)? > 0;
            if !known {
                return Err(format!("no project {id} in the registry"));
            }
            conn.execute("UPDATE projects SET last_opened_at=updated_at WHERE project_id=?1", [id]).map_err(fail)?;
            conn.execute("INSERT OR REPLACE INTO app_settings(key,value) VALUES(?1,?2)", params![ACTIVE_KEY, id]).map_err(fail)?;
            return Ok(json!({ "project_id": id }));
        }
        if let Some(id) = request.get("delete").and_then(Value::as_str) {
            let known: bool = conn.query_row("SELECT COUNT(*) FROM projects WHERE project_id=?1", [id], |r| r.get::<_, i64>(0)).map_err(fail)? > 0;
            if !known {
                return Err(format!("no project {id} in the registry"));
            }
            conn.execute("DELETE FROM project_storage_locations WHERE project_id=?1", [id]).map_err(fail)?;
            conn.execute("DELETE FROM projects WHERE project_id=?1", [id]).map_err(fail)?;
            let active: Option<String> = conn.query_row("SELECT value FROM app_settings WHERE key=?1", [ACTIVE_KEY], |r| r.get(0)).optional().map_err(fail)?;
            if active.as_deref() == Some(id) {
                let next: Option<String> = conn.query_row("SELECT project_id FROM projects ORDER BY created_at DESC, project_id LIMIT 1", [], |r| r.get(0)).optional().map_err(fail)?;
                conn.execute("DELETE FROM app_settings WHERE key=?1", [ACTIVE_KEY]).map_err(fail)?;
                if let Some(next) = next {
                    conn.execute("INSERT OR REPLACE INTO app_settings(key,value) VALUES(?1,?2)", params![ACTIVE_KEY, next]).map_err(fail)?;
                }
            }
            return Ok(json!({ "project_id": id }));
        }
        if request.get("deactivate").is_some() {
            conn.execute("DELETE FROM app_settings WHERE key=?1", [ACTIVE_KEY]).map_err(fail)?;
            return Ok(Value::Null);
        }
        Err("unknown request".into())
    }
}

fn row(conn: &Connection, id: &str) -> Result<Option<Registered>, String> {
    conn.query_row(
        "SELECT p.project_id, p.name, p.project_uri, l.local_path, p.created_at FROM public_projects p JOIN project_storage_locations l ON l.project_id = p.project_id WHERE p.project_id = ?1",
        [id],
        |r| Ok(Registered { project_id: r.get(0)?, name: r.get(1)?, project_uri: r.get(2)?, local_path: PathBuf::from(r.get::<_, String>(3)?), created_at: r.get(4)? }),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// The active project, read from the database now.
pub fn active(reader: &Reader) -> Result<Option<Registered>, String> {
    let conn = reader.connection();
    let id: Option<String> = conn.query_row("SELECT value FROM public_app_settings WHERE key=?1", [ACTIVE_KEY], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    match id {
        Some(id) => row(conn, &id),
        None => Ok(None),
    }
}

/// One project by id.
pub fn project(reader: &Reader, id: &str) -> Result<Option<Registered>, String> {
    row(reader.connection(), id)
}

/// Every project, newest first.
pub fn projects(reader: &Reader) -> Result<Vec<Registered>, String> {
    let conn = reader.connection();
    let mut statement = conn.prepare("SELECT project_id FROM public_projects ORDER BY created_at DESC, project_id").map_err(|e| e.to_string())?;
    let ids = statement.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    ids.iter().filter_map(|id| row(conn, id).transpose()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_project_is_the_active_one_and_the_folder_stays_private_to_the_registry() {
        let root = std::env::temp_dir().join(format!("qnc-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let writer = writer(&root).unwrap();
        writer.execute(MODULE, json!({ "register": { "project_id": "p-1", "name": "First", "project_uri": "qnc://local/project/p-1", "local_path": root.join("p-1").to_string_lossy(), "at": "epoch_1" } })).unwrap();
        writer.execute(MODULE, json!({ "register": { "project_id": "p-2", "name": "Second", "project_uri": "qnc://local/project/p-2", "local_path": root.join("p-2").to_string_lossy(), "at": "epoch_2" } })).unwrap();
        let reader = reader(&root).unwrap();
        assert_eq!(active(&reader).unwrap().unwrap().project_id, "p-2");
        assert_eq!(projects(&reader).unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Second", "First"]);
        writer.execute(MODULE, json!({ "activate": "p-1" })).unwrap();
        assert_eq!(active(&reader).unwrap().unwrap().project_db(), root.join("p-1").join("project.db"));
        assert!(writer.execute(MODULE, json!({ "activate": "p-9" })).is_err());
        writer.execute(MODULE, json!({ "delete": "p-1" })).unwrap();
        assert_eq!(active(&reader).unwrap().unwrap().project_id, "p-2", "the newest remaining project is active");
        assert!(project(&reader, "p-1").unwrap().is_none());
        assert!(writer.execute(MODULE, json!({ "delete": "p-1" })).is_err());
        writer.execute(MODULE, json!({ "deactivate": true })).unwrap();
        assert!(active(&reader).unwrap().is_none());
        assert!(writer.execute(MODULE, json!({ "register": { "project_id": "bad id", "name": "x", "project_uri": "qnc://local/project/x", "local_path": "x" } })).is_err());
        drop(writer);
        let _ = std::fs::remove_dir_all(&root);
    }
}
