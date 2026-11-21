//! The registry of the projects of this computer: `data/qnc-projects.db`, in the shape the old
//! QNC saved it (copied from its project store): `projects`, `app_settings` with
//! `active_project_id`, `project_storage_locations` with the private folder of each project,
//! `project_search` (what the list is searched by), `project_origin` (the workstation and user
//! that made it, never recaptured), `local_runtime_settings` (this computer's: the projects'
//! folder, the selected template), `users`, `sessions`, `module_state` and the public views.
//! This block owns those tables: it makes the file when missing, writes through the database
//! writer and reads for everyone. The public identity of a project is its URI; its folder is
//! a private binding. The template tables of the same file belong to `qnc-project-templates`.

mod reads;

use std::path::{Path, PathBuf};

use qnc_db::{Reader, TableModule, Writer};
pub use reads::{active, local_setting, project, project_dir, projects, rows, setting, ProjectRow, Registered};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.registry";
pub const FILE: &str = "qnc-projects.db";
pub const ACTIVE_KEY: &str = "active_project_id";
/// The local setting that names where this computer makes its projects.
pub const PROJECTS_ROOT_KEY: &str = "projects_root_path";
/// The local setting that names the template New project starts from.
pub const SELECTED_TEMPLATE_KEY: &str = "selected_template_id";

/// The tables this block owns, as the old QNC made them.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
    project_id TEXT PRIMARY KEY, name TEXT NOT NULL, project_uri TEXT NOT NULL,
    created_at TEXT, updated_at TEXT, created_by TEXT, updated_by TEXT, last_opened_at TEXT
);
CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS project_search (project_id TEXT PRIMARY KEY, search_text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY, display_name TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'editor',
    active INTEGER NOT NULL DEFAULT 1, created_at TEXT, updated_at TEXT
);
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY, user_id TEXT NOT NULL, station_id TEXT NOT NULL,
    client_label TEXT NOT NULL DEFAULT '', created_at TEXT, last_seen_at TEXT,
    FOREIGN KEY(user_id) REFERENCES users(user_id)
);
CREATE TABLE IF NOT EXISTS module_state (module_id TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 1, updated_at TEXT);
CREATE TABLE IF NOT EXISTS local_runtime_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT);
CREATE TABLE IF NOT EXISTS project_storage_locations (
    project_id TEXT PRIMARY KEY, local_path TEXT NOT NULL, created_at TEXT, updated_at TEXT
);
CREATE VIEW IF NOT EXISTS public_projects AS
    SELECT project_id, name, project_uri, created_at, updated_at, last_opened_at FROM projects;
CREATE VIEW IF NOT EXISTS public_app_settings AS SELECT key, value FROM app_settings;
CREATE VIEW IF NOT EXISTS public_module_state AS SELECT module_id, enabled, updated_at FROM module_state;
";

pub fn file(root: &Path) -> PathBuf {
    qnc_settings::data_dir(root).join(FILE)
}

/// The writer of the registry with this block's tables; the file is made when missing.
pub fn writer(root: &Path) -> Result<Writer, String> {
    writer_with(root, Vec::new())
}

/// The writer of the registry with this block's tables and `others` that keep their own tables
/// in the same file (the project templates).
pub fn writer_with(root: &Path, others: Vec<Box<dyn TableModule>>) -> Result<Writer, String> {
    let file = file(root);
    if !file.is_file() {
        std::fs::create_dir_all(qnc_settings::data_dir(root)).map_err(|e| e.to_string())?;
        qnc_db::create(&file)?;
    }
    let mut modules: Vec<Box<dyn TableModule>> = vec![Box::new(RegistryTables)];
    modules.extend(others);
    Writer::start(&file, modules)
}

pub fn reader(root: &Path) -> Result<Reader, String> {
    Reader::open(&file(root))
}

/// The time text of the records: `epoch_<seconds>`, as the old QNC wrote it.
pub fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("epoch_{secs}")
}

pub struct RegistryTables;

fn fail(e: rusqlite::Error) -> String {
    e.to_string()
}

fn known(conn: &Connection, id: &str) -> Result<(), String> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM projects WHERE project_id=?1", [id], |r| r.get(0)).map_err(fail)?;
    if count == 0 {
        return Err(qnc_text::fill("project.store.project_missing", &[("id", id)]));
    }
    Ok(())
}

fn set_app_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT OR REPLACE INTO app_settings(key,value) VALUES(?1,?2)", params![key, value]).map_err(fail).map(drop)
}

fn set_local(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    match value.filter(|v| !v.trim().is_empty()) {
        Some(value) => conn.execute("INSERT OR REPLACE INTO local_runtime_settings(key,value,updated_at) VALUES(?1,?2,?3)", params![key, value, now()]).map_err(fail).map(drop),
        None => conn.execute("DELETE FROM local_runtime_settings WHERE key=?1", [key]).map_err(fail).map(drop),
    }
}

impl TableModule for RegistryTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["projects", "app_settings", "project_storage_locations", "project_search", "project_origin", "users", "sessions", "module_state", "local_runtime_settings"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        qnc_project_origin::ensure(conn)?;
        conn.execute_batch(SCHEMA).map_err(fail)
    }

    /// `{"register": {project_id, name, project_uri, local_path, at, identity?, projects_root?}}`
    /// (the project, its folder, its origin once, and makes it active; `projects_root` is kept as
    /// this computer's), `{"activate": project_id}` (opened now), `{"deactivate": true}`,
    /// `{"delete": project_id}` (its rows go; the first of the rest (by id) becomes active when
    /// the deleted one was), `{"search": {project_id, text}}`, `{"local": {key, value|null}}`.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(r) = request.get("register") {
            let id = r["project_id"].as_str().ok_or("register needs a project_id")?;
            qnc_uri::valid_id(id)?;
            let uri = r["project_uri"].as_str().ok_or("register needs a project_uri")?;
            qnc_uri::parse(uri)?;
            let name = r["name"].as_str().filter(|n| !n.trim().is_empty()).ok_or("register needs a name")?;
            let path = r["local_path"].as_str().ok_or("register needs a local_path")?;
            let at = r["at"].as_str().map(str::to_string).unwrap_or_else(now);
            conn.execute("INSERT INTO projects(project_id,name,project_uri,created_at,updated_at,last_opened_at) VALUES(?1,?2,?3,?4,?4,?4)", params![id, name, uri, at]).map_err(fail)?;
            if let Some(identity) = r.get("identity").filter(|i| !i.is_null()) {
                qnc_project_origin::insert(conn, id, name, &at, identity)?;
            }
            conn.execute("INSERT INTO project_storage_locations(project_id,local_path,created_at,updated_at) VALUES(?1,?2,?3,?3)", params![id, path, at]).map_err(fail)?;
            if let Some(projects_root) = r["projects_root"].as_str() {
                set_local(conn, PROJECTS_ROOT_KEY, Some(projects_root))?;
            }
            set_app_setting(conn, ACTIVE_KEY, id)?;
            return Ok(json!({ "project_id": id }));
        }
        if let Some(id) = request.get("activate").and_then(Value::as_str) {
            known(conn, id)?;
            conn.execute("UPDATE projects SET updated_at=?2, last_opened_at=?2 WHERE project_id=?1", params![id, now()]).map_err(fail)?;
            set_app_setting(conn, ACTIVE_KEY, id)?;
            return Ok(json!({ "project_id": id }));
        }
        if let Some(id) = request.get("delete").and_then(Value::as_str) {
            known(conn, id)?;
            qnc_project_origin::remove(conn, id)?;
            for table in ["project_storage_locations", "project_search", "projects"] {
                conn.execute(&format!("DELETE FROM {table} WHERE project_id=?1"), [id]).map_err(fail)?;
            }
            let active: Option<String> = conn.query_row("SELECT value FROM app_settings WHERE key=?1", [ACTIVE_KEY], |r| r.get(0)).optional().map_err(fail)?;
            if active.as_deref() == Some(id) {
                let next: Option<String> = conn.query_row("SELECT project_id FROM projects ORDER BY project_id LIMIT 1", [], |r| r.get(0)).optional().map_err(fail)?;
                set_app_setting(conn, ACTIVE_KEY, next.as_deref().unwrap_or(""))?;
            }
            return Ok(json!({ "project_id": id }));
        }
        if let Some(s) = request.get("search") {
            let id = s["project_id"].as_str().ok_or("search needs a project_id")?;
            let text = s["text"].as_str().ok_or("search needs a text")?;
            conn.execute("INSERT OR REPLACE INTO project_search(project_id,search_text) VALUES(?1,?2)", params![id, text]).map_err(fail)?;
            return Ok(Value::Null);
        }
        if let Some(l) = request.get("local") {
            let key = l["key"].as_str().filter(|k| !k.trim().is_empty()).ok_or("local needs a key")?;
            set_local(conn, key, l["value"].as_str())?;
            return Ok(Value::Null);
        }
        if request.get("deactivate").is_some() {
            conn.execute("DELETE FROM app_settings WHERE key=?1", [ACTIVE_KEY]).map_err(fail)?;
            return Ok(Value::Null);
        }
        Err("unknown request".into())
    }
}

#[cfg(test)]
mod tests;
