//! The template tables of the registry and what is done in them.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{Template, TemplateRow, SEED};

/// The tables this module owns; it writes no other.
pub(crate) const OWNED: [&str; 5] = ["project_templates", "project_template_kv", "project_template_sources", "source_templates", "source_template_kv"];

#[derive(Debug, Deserialize)]
struct SeedFile {
    #[serde(default)]
    source_templates: Vec<Value>,
    #[serde(default)]
    project_templates: Vec<Value>,
}

fn fail(e: rusqlite::Error) -> String {
    e.to_string()
}

fn text(value: &Value, field: &str) -> String {
    value.get(field).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

fn text_or(value: &Value, field: &str, fallback: &str) -> String {
    let text = text(value, field);
    if text.is_empty() { fallback.to_string() } else { text }
}

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS source_templates (
            source_template_id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            source_kind TEXT NOT NULL DEFAULT 'local', system INTEGER NOT NULL DEFAULT 0,
            config_json TEXT NOT NULL DEFAULT '{}', created_by TEXT, updated_by TEXT, created_at TEXT, updated_at TEXT
        );
        CREATE TABLE IF NOT EXISTS project_templates (
            template_id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            system INTEGER NOT NULL DEFAULT 0, settings_json TEXT NOT NULL DEFAULT '{}',
            source_template_ids_json TEXT NOT NULL DEFAULT '[]', created_by TEXT, updated_by TEXT, created_at TEXT, updated_at TEXT
        );
        CREATE TABLE IF NOT EXISTS project_template_kv (
            template_id TEXT NOT NULL, setting_key TEXT NOT NULL, setting_value TEXT NOT NULL, PRIMARY KEY (template_id, setting_key)
        );
        CREATE TABLE IF NOT EXISTS project_template_sources (
            template_id TEXT NOT NULL, source_template_id TEXT NOT NULL, PRIMARY KEY (template_id, source_template_id)
        );
        CREATE TABLE IF NOT EXISTS source_template_kv (
            source_template_id TEXT NOT NULL, setting_key TEXT NOT NULL, setting_value TEXT NOT NULL, PRIMARY KEY (source_template_id, setting_key)
        );
        CREATE VIEW IF NOT EXISTS public_project_templates AS
            SELECT template_id, name, description, system, created_at, updated_at FROM project_templates;
        CREATE VIEW IF NOT EXISTS public_source_templates AS
            SELECT source_template_id, name, description, source_kind, system, created_at, updated_at FROM source_templates;",
    )
    .map_err(fail)
}

/// The system templates of the seed, inserted or brought up to date; a user template stays.
pub(crate) fn seed(conn: &Connection) -> Result<(), String> {
    let seed: SeedFile = serde_json::from_str(SEED).map_err(|e| qnc_text::fill("project.store.seed_invalid", &[("error", &e.to_string())]))?;
    let now = qnc_registry::now();
    for source in seed.source_templates {
        let id = text(&source, "source_template_id");
        if id.is_empty() {
            continue;
        }
        let config = source.get("config").cloned().unwrap_or_else(|| json!({}));
        conn.execute(
            "INSERT OR REPLACE INTO source_templates (source_template_id, name, description, source_kind, system, config_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?6)",
            params![id, text_or(&source, "name", &id), text(&source, "description"), text_or(&source, "source_kind", "local"), config.to_string(), now],
        )
        .map_err(fail)?;
    }
    for template in seed.project_templates {
        let id = text(&template, "template_id");
        if id.is_empty() {
            continue;
        }
        let settings = template.get("settings").cloned().unwrap_or_else(|| json!({}));
        let sources = template.get("source_template_ids").cloned().unwrap_or_else(|| json!([]));
        conn.execute(
            "INSERT OR REPLACE INTO project_templates (template_id, name, description, system, settings_json, source_template_ids_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?6)",
            params![id, text_or(&template, "name", &id), text(&template, "description"), settings.to_string(), sources.to_string(), now],
        )
        .map_err(fail)?;
    }
    Ok(())
}

/// A template names no locations of a project: `products` is the project's, made when it is.
pub(crate) fn strip_project_locations(settings: &mut Value) {
    if let Some(root) = settings.as_object_mut() {
        root.remove("products");
    }
}

/// Every template without the locations of a project.
pub(crate) fn sanitize(conn: &Connection) -> Result<(), String> {
    let mut statement = conn.prepare("SELECT template_id, settings_json FROM project_templates ORDER BY template_id").map_err(fail)?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(fail)?.collect::<Result<Vec<_>, _>>().map_err(fail)?;
    drop(statement);
    let now = qnc_registry::now();
    for (id, saved) in rows {
        let mut settings: Value = serde_json::from_str(&saved).unwrap_or_else(|_| json!({}));
        strip_project_locations(&mut settings);
        let sanitized = settings.to_string();
        if sanitized != saved {
            conn.execute("UPDATE project_templates SET settings_json = ?2, updated_at = ?3 WHERE template_id = ?1", params![id, sanitized, now]).map_err(fail)?;
        }
    }
    Ok(())
}

pub(crate) fn list(conn: &Connection) -> Result<Vec<TemplateRow>, String> {
    let selected = selected_id(conn)?;
    let mut statement = conn.prepare("SELECT template_id, name, description, system FROM project_templates ORDER BY system DESC, name").map_err(fail)?;
    let rows = statement
        .query_map([], |r| {
            let template_id: String = r.get(0)?;
            Ok(TemplateRow { selected: template_id == selected, template_id, name: r.get(1)?, description: r.get(2)?, system: r.get::<_, i64>(3)? != 0 })
        })
        .map_err(fail)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(fail)
}

pub(crate) fn get(conn: &Connection, template_id: &str) -> Result<Option<Template>, String> {
    conn.query_row(
        "SELECT template_id, name, system, settings_json, source_template_ids_json FROM project_templates WHERE template_id = ?1",
        [template_id.trim()],
        |r| {
            Ok(Template {
                template_id: r.get(0)?,
                name: r.get(1)?,
                system: r.get::<_, i64>(2)? != 0,
                settings: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_else(|_| json!({})),
                source_template_ids: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_else(|_| json!([])),
            })
        },
    )
    .optional()
    .map_err(fail)
}

/// The selected template while it exists, else the first template.
pub(crate) fn selected_id(conn: &Connection) -> Result<String, String> {
    let chosen: Option<String> = conn.query_row("SELECT value FROM local_runtime_settings WHERE key = ?1", [qnc_registry::SELECTED_TEMPLATE_KEY], |r| r.get(0)).optional().map_err(fail)?;
    if let Some(chosen) = chosen.filter(|c| !c.trim().is_empty()) {
        if get(conn, &chosen)?.is_some() {
            return Ok(chosen);
        }
    }
    conn.query_row("SELECT template_id FROM project_templates ORDER BY system DESC, name LIMIT 1", [], |r| r.get(0)).optional().map_err(fail).map(Option::unwrap_or_default)
}

pub(crate) fn insert_user(conn: &Connection, template_id: &str, name: &str, description: &str, settings: &Value, sources: &Value) -> Result<(), String> {
    let now = qnc_registry::now();
    conn.execute(
        "INSERT INTO project_templates (template_id, name, description, system, settings_json, source_template_ids_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?6)",
        params![template_id, name, description, settings.to_string(), sources.to_string(), now],
    )
    .map_err(fail)
    .map(drop)
}

pub(crate) fn delete_user(conn: &Connection, template_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM project_template_kv WHERE template_id = ?1", [template_id]).map_err(fail)?;
    conn.execute("DELETE FROM project_template_sources WHERE template_id = ?1", [template_id]).map_err(fail)?;
    let deleted = conn.execute("DELETE FROM project_templates WHERE template_id = ?1 AND system = 0", [template_id]).map_err(fail)?;
    if deleted == 0 {
        return Err(qnc_text::fill("project.store.template_not_deleted", &[("template", template_id)]));
    }
    Ok(())
}
