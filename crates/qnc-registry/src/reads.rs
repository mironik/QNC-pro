//! What everyone reads of the registry.

use std::path::PathBuf;

use qnc_db::Reader;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{ACTIVE_KEY, PROJECTS_ROOT_KEY};

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

/// A row of the project list, as the old QNC showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
    pub project_id: String,
    pub name: String,
    /// `dd.mm.yyyy.` of the day it was made, in local time.
    pub created_date: String,
    pub project_uri: String,
    pub active: bool,
    /// The values of its story sheet (journalist, camera, editor, producer and the rest), by
    /// which the project list is searched.
    pub search_text: String,
}

impl ProjectRow {
    /// Whether every word of `query` is in its name, date or story sheet (any case).
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {}", self.name, self.created_date, self.search_text).to_lowercase();
        query.to_lowercase().split_whitespace().all(|word| haystack.contains(word))
    }
}

fn fail(e: rusqlite::Error) -> String {
    e.to_string()
}

fn row(conn: &Connection, id: &str) -> Result<Option<Registered>, String> {
    conn.query_row(
        "SELECT p.project_id, p.name, p.project_uri, l.local_path, p.created_at FROM public_projects p JOIN project_storage_locations l ON l.project_id = p.project_id WHERE p.project_id = ?1",
        [id],
        |r| Ok(Registered { project_id: r.get(0)?, name: r.get(1)?, project_uri: r.get(2)?, local_path: PathBuf::from(r.get::<_, String>(3)?), created_at: r.get(4)? }),
    )
    .optional()
    .map_err(fail)
}

fn value(conn: &Connection, sql: &str, key: &str) -> Result<Option<String>, String> {
    let found: Option<String> = conn.query_row(sql, [key], |r| r.get(0)).optional().map_err(fail)?;
    Ok(found.filter(|v| !v.trim().is_empty()))
}

/// A value of `app_settings` (the active project).
pub fn setting(reader: &Reader, key: &str) -> Result<Option<String>, String> {
    value(reader.connection(), "SELECT value FROM public_app_settings WHERE key=?1", key)
}

/// A setting of this computer kept in the registry (`local_runtime_settings`).
pub fn local_setting(reader: &Reader, key: &str) -> Result<Option<String>, String> {
    value(reader.connection(), "SELECT value FROM local_runtime_settings WHERE key=?1", key)
}

/// The folder this computer makes its projects in, when it chose one.
pub fn project_dir(reader: &Reader) -> Result<Option<PathBuf>, String> {
    Ok(local_setting(reader, PROJECTS_ROOT_KEY)?.map(PathBuf::from))
}

/// The active project, read from the database now.
pub fn active(reader: &Reader) -> Result<Option<Registered>, String> {
    match setting(reader, ACTIVE_KEY)? {
        Some(id) => row(reader.connection(), &id),
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
    let mut statement = conn.prepare("SELECT project_id FROM public_projects ORDER BY created_at DESC, project_id").map_err(fail)?;
    let ids = statement.query_map([], |r| r.get::<_, String>(0)).map_err(fail)?.collect::<Result<Vec<_>, _>>().map_err(fail)?;
    ids.iter().filter_map(|id| row(conn, id).transpose()).collect()
}

/// The project list as the old QNC showed it: oldest first, with the date and the search text.
pub fn rows(reader: &Reader) -> Result<Vec<ProjectRow>, String> {
    let conn = reader.connection();
    let active = setting(reader, ACTIVE_KEY)?.unwrap_or_default();
    let mut statement = conn
        .prepare(
            "SELECT p.project_id, p.name, p.project_uri,
                COALESCE(strftime('%d.%m.%Y.', substr(p.created_at, 7), 'unixepoch', 'localtime'), ''),
                COALESCE(s.search_text, '')
             FROM public_projects p LEFT JOIN project_search s ON s.project_id = p.project_id
             ORDER BY p.created_at, p.name, p.project_id",
        )
        .map_err(fail)?;
    let rows = statement
        .query_map([], |r| {
            let project_id: String = r.get(0)?;
            Ok(ProjectRow { active: project_id == active, project_id, name: r.get(1)?, project_uri: r.get(2)?, created_date: r.get(3)?, search_text: r.get(4)? })
        })
        .map_err(fail)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(fail)
}
