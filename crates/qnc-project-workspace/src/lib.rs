//! The workspace of a project in its own database, as the old QNC made it (copied from its
//! project store): the origin of the project (never recaptured), the snapshot of the template
//! it was made from, the workflow steps of its applications (the sequence the desktop walks),
//! the workflow state, the members, the data revisions, the key-value tables and the public
//! views. A table module of the project database (`qnc-db`); the settings row is
//! `qnc-project-settings`', the catalog `qnc-catalog`'s. Nothing here chooses a project.

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.project-workspace";

/// One step of the sequence of applications of a project, as the desktop walks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationStep {
    pub application_id: String,
    pub tab_id: String,
    pub priority_group: String,
}

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS project_members (
    project_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'editor',
    joined_at TEXT, last_seen_at TEXT, PRIMARY KEY(project_id, user_id)
);
CREATE TABLE IF NOT EXISTS project_template_snapshot (
    project_id TEXT PRIMARY KEY, template_id TEXT, template_name TEXT NOT NULL DEFAULT '',
    template_version TEXT NOT NULL DEFAULT '', snapshot_json TEXT NOT NULL DEFAULT '{}', created_at TEXT
);
CREATE TABLE IF NOT EXISTS project_workflow_steps (
    step_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, plugin_id TEXT NOT NULL, tab_id TEXT NOT NULL,
    label TEXT NOT NULL, position INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'locked',
    next_step_id TEXT, settings_json TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS project_workflow_state (
    project_id TEXT PRIMARY KEY, active_step_id TEXT, entry_step_id TEXT, updated_at TEXT
);
CREATE TABLE IF NOT EXISTS project_data_revisions (
    scope TEXT PRIMARY KEY, revision INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS project_settings_kv (
    project_id TEXT NOT NULL, setting_key TEXT NOT NULL, setting_value TEXT NOT NULL, PRIMARY KEY (project_id, setting_key)
);
CREATE TABLE IF NOT EXISTS project_snapshot_kv (
    project_id TEXT NOT NULL, setting_key TEXT NOT NULL, setting_value TEXT NOT NULL, PRIMARY KEY (project_id, setting_key)
);
CREATE TABLE IF NOT EXISTS project_workflow_step_kv (
    step_id TEXT NOT NULL, setting_key TEXT NOT NULL, setting_value TEXT NOT NULL, PRIMARY KEY (step_id, setting_key)
);
CREATE VIEW IF NOT EXISTS public_project_members AS
    SELECT project_id, user_id, role, joined_at, last_seen_at FROM project_members;
CREATE VIEW IF NOT EXISTS public_project_template_snapshot AS
    SELECT project_id, template_id, template_name, template_version, created_at FROM project_template_snapshot;
CREATE VIEW IF NOT EXISTS public_project_workflow_steps AS
    SELECT step_id, project_id, plugin_id, tab_id, label, position, status, next_step_id FROM project_workflow_steps;
CREATE VIEW IF NOT EXISTS public_project_workflow_state AS
    SELECT project_id, active_step_id, entry_step_id, updated_at FROM project_workflow_state;
CREATE VIEW IF NOT EXISTS public_project_application_sequence AS
    SELECT project_id, step_id, plugin_id AS application_id, tab_id, label, position,
        json_extract(settings_json, '$.priority_group') AS priority_group, status, next_step_id
    FROM project_workflow_steps;
CREATE VIEW IF NOT EXISTS public_project_data_revisions AS
    SELECT scope, revision, updated_at FROM project_data_revisions;";

fn fail(e: rusqlite::Error) -> String {
    e.to_string()
}

fn step_id_for_tab(tab_id: &str) -> String {
    format!("step_{tab_id}")
}

/// The workflow steps of the applications the settings select, as the old QNC wrote them.
fn write_workflow(conn: &Connection, project_id: &str, settings: &Value, at: &str) -> Result<(), String> {
    let selection = settings.pointer("/workspace/application_selection").ok_or(qnc_text::text("project.store.no_sequence"))?;
    qnc_project_defaults::validate_selection(selection).map_err(|_| qnc_text::text("project.store.bad_sequence"))?;
    let apps = selection["applications"].as_array().cloned().unwrap_or_default();
    if apps.first().is_none_or(|app| app["priority_group"] != "a") {
        return Err(qnc_text::text("project.store.sequence_starts_a").into());
    }
    conn.execute("DELETE FROM project_workflow_steps WHERE project_id = ?1", [project_id]).map_err(fail)?;
    let mut entry_step_id = String::new();
    let mut previous_step_id = String::new();
    for (index, app) in apps.iter().enumerate() {
        let tab_id = app["tab_id"].as_str().unwrap_or("");
        let step_id = step_id_for_tab(tab_id);
        let initial_group = app["priority_group"] == "a";
        if !initial_group && entry_step_id.is_empty() {
            entry_step_id = step_id.clone();
        }
        if !previous_step_id.is_empty() {
            conn.execute("UPDATE project_workflow_steps SET next_step_id = ?2 WHERE project_id = ?1 AND step_id = ?3", params![project_id, step_id, previous_step_id]).map_err(fail)?;
        }
        let status = if initial_group { "complete" } else if step_id == entry_step_id { "active" } else { "locked" };
        conn.execute(
            "INSERT INTO project_workflow_steps (step_id, project_id, plugin_id, tab_id, label, position, status, next_step_id, settings_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![step_id, project_id, app["application_id"].as_str().unwrap_or(""), tab_id, app["label"].as_str().unwrap_or(""), index as i64, status, app.to_string()],
        )
        .map_err(fail)?;
        previous_step_id = step_id;
    }
    let active_step_id = if entry_step_id.is_empty() { step_id_for_tab(apps[0]["tab_id"].as_str().unwrap_or("")) } else { entry_step_id.clone() };
    conn.execute(
        "INSERT OR REPLACE INTO project_workflow_state (project_id, active_step_id, entry_step_id, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![project_id, active_step_id, entry_step_id, at],
    )
    .map_err(fail)
    .map(drop)
}

pub struct WorkspaceTables;

impl TableModule for WorkspaceTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["project_origin", "project_members", "project_template_snapshot", "project_workflow_steps", "project_workflow_state", "project_data_revisions", "project_settings_kv", "project_snapshot_kv", "project_workflow_step_kv"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        qnc_project_origin::ensure(conn)?;
        conn.execute_batch(SCHEMA).map_err(fail)
    }

    /// `{"prepare": {project_id, at, origin: {name, identity}, template: {template_id, name,
    /// system, settings, source_template_ids}}}`: the origin (once, on creation only), the
    /// snapshot of the template and the workflow of the applications its settings select.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        let p = request.get("prepare").ok_or("unknown request")?;
        let project_id = p["project_id"].as_str().ok_or("prepare needs a project_id")?;
        let at = p["at"].as_str().ok_or("prepare needs the time")?;
        let origin = &p["origin"];
        let name = origin["name"].as_str().ok_or("prepare needs the name of the project")?;
        let identity = origin.get("identity").filter(|i| i.is_object()).ok_or("prepare needs the identity of the workstation")?;
        qnc_project_origin::insert(conn, project_id, name, at, identity)?;
        let template = &p["template"];
        let snapshot = json!({
            "template_id": template["template_id"], "name": template["name"], "system": template["system"],
            "settings": template["settings"], "source_template_ids": template["source_template_ids"],
        });
        conn.execute(
            "INSERT OR REPLACE INTO project_template_snapshot (project_id, template_id, template_name, template_version, snapshot_json, created_at) VALUES (?1, ?2, ?3, '', ?4, ?5)",
            params![project_id, template["template_id"].as_str(), template["name"].as_str().unwrap_or(""), snapshot.to_string(), at],
        )
        .map_err(fail)?;
        write_workflow(conn, project_id, &template["settings"], at)?;
        Ok(json!({ "project_id": project_id }))
    }
}

/// Prepares the workspace of `project_id` through its writer.
pub fn prepare(writer: &Writer, project_id: &str, at: &str, name: &str, identity: &Value, template: &Value) -> Result<(), String> {
    writer.execute(MODULE, json!({ "prepare": { "project_id": project_id, "at": at, "origin": { "name": name, "identity": identity }, "template": template } })).map(drop)
}

/// The sequence of applications of the project, as its workspace keeps it; refused when it is
/// not a valid sequence (group `a` first, one-letter groups ascending, QNC applications).
pub fn navigation_sequence(reader: &Reader, project_id: &str) -> Result<Vec<NavigationStep>, String> {
    let mut statement = reader
        .connection()
        .prepare("SELECT application_id, tab_id, priority_group FROM public_project_application_sequence WHERE project_id=?1 ORDER BY position")
        .map_err(fail)?;
    let steps = statement
        .query_map([project_id], |r| Ok(NavigationStep { application_id: r.get(0)?, tab_id: r.get(1)?, priority_group: r.get(2)? }))
        .map_err(fail)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fail)?;
    let bad = |step: &NavigationStep| step.priority_group.len() != 1 || !step.priority_group.as_bytes()[0].is_ascii_lowercase() || !step.application_id.starts_with("qnc.") || step.tab_id.is_empty();
    if steps.first().is_none_or(|step| step.priority_group != "a") || steps.iter().any(bad) || steps.windows(2).any(|pair| pair[0].priority_group >= pair[1].priority_group) {
        return Err(qnc_text::text("project.store.no_valid_sequence").into());
    }
    Ok(steps)
}

#[cfg(test)]
mod tests;
