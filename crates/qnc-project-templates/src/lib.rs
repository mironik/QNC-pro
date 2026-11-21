//! The project templates, as the old QNC kept them (copied from its project store): the system
//! templates seeded from `seed/system_seed.json` (the source templates with them), the user
//! templates made from a base (`tpl_user_<slug>`), the one New project starts from (a setting
//! of this computer in the registry) and the settings of each. A table module of the registry
//! file (`qnc-registry`): it owns only the template tables and writes them only through the
//! registry writer. Which template a project is made from is the caller's choice.

mod store;

use std::path::Path;

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::Connection;
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.project-templates";
pub const SEED: &str = include_str!("../../../seed/system_seed.json");

/// A template as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRow {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub system: bool,
    pub selected: bool,
}

/// A template with its settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub template_id: String,
    pub name: String,
    pub system: bool,
    pub settings: Value,
    pub source_template_ids: Value,
}

pub struct TemplateTables;

impl TableModule for TemplateTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &store::OWNED
    }

    /// The tables, the system templates from the seed (kept up to date) and every template
    /// without the locations of a project (`products`).
    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        store::ensure_schema(conn)?;
        store::seed(conn)?;
        store::sanitize(conn)
    }

    /// `{"create_user": {name, description, base_template_id?, settings?}}` (a user template from
    /// the base, or the selected one; the settings given are its settings; answers the id) and
    /// `{"delete_user": template_id}` (never a system template).
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(c) = request.get("create_user") {
            let name = c["name"].as_str().unwrap_or("").trim();
            if name.is_empty() {
                return Err(qnc_text::text("project.store.template_name_empty").into());
            }
            let base_id = match c["base_template_id"].as_str().map(str::trim).filter(|b| !b.is_empty()) {
                Some(base) => base.to_string(),
                None => store::selected_id(conn)?,
            };
            let base = store::get(conn, &base_id)?.ok_or_else(|| qnc_text::fill("project.store.template_missing", &[("id", &base_id)]))?;
            let mut settings = c.get("settings").filter(|s| s.is_object()).cloned().unwrap_or(base.settings);
            store::strip_project_locations(&mut settings);
            let template_id = format!("tpl_user_{}", slug_id(name));
            store::insert_user(conn, &template_id, name, c["description"].as_str().unwrap_or("").trim(), &settings, &base.source_template_ids)?;
            return Ok(json!({ "template_id": template_id }));
        }
        if let Some(id) = request.get("delete_user").and_then(Value::as_str) {
            let id = id.trim();
            if id.is_empty() {
                return Err(qnc_text::text("project.store.template_id_empty").into());
            }
            let template = store::get(conn, id)?.ok_or_else(|| qnc_text::fill("project.store.template_missing", &[("id", id)]))?;
            if template.system {
                return Err(qnc_text::text("project.store.system_template_kept").into());
            }
            store::delete_user(conn, id)?;
            return Ok(json!({ "template_id": id }));
        }
        Err("unknown request".into())
    }
}

/// The writer of the registry with the template tables.
pub fn writer(root: &Path) -> Result<Writer, String> {
    qnc_registry::writer_with(root, vec![Box::new(TemplateTables)])
}

/// Every template, system first then by name, with the selected one marked.
pub fn list(reader: &Reader) -> Result<Vec<TemplateRow>, String> {
    store::list(reader.connection())
}

/// The template `template_id`.
pub fn get(reader: &Reader, template_id: &str) -> Result<Option<Template>, String> {
    store::get(reader.connection(), template_id)
}

/// The id of the template New project starts from: the one this computer selected while it
/// exists, else the first (system first, then by name).
pub fn selected_id(reader: &Reader) -> Result<String, String> {
    store::selected_id(reader.connection())
}

/// The template New project starts from, with its settings.
pub fn selected(reader: &Reader) -> Result<Template, String> {
    let id = selected_id(reader)?;
    get(reader, &id)?.ok_or_else(|| qnc_text::fill("project.store.template_missing", &[("id", &id)]))
}

/// Makes `template_id` the one New project starts from (a setting of this computer).
pub fn select(writer: &Writer, reader: &Reader, template_id: &str) -> Result<(), String> {
    let template_id = template_id.trim();
    if template_id.is_empty() {
        return Err(qnc_text::text("project.store.template_id_empty").into());
    }
    if get(reader, template_id)?.is_none() {
        return Err(qnc_text::fill("project.store.template_missing", &[("id", template_id)]));
    }
    writer.execute(qnc_registry::MODULE, json!({ "local": { "key": qnc_registry::SELECTED_TEMPLATE_KEY, "value": template_id } })).map(drop)
}

/// A user template from the base (or the selected template), selected from now on; answers it.
pub fn create_user(writer: &Writer, reader: &Reader, name: &str, description: &str, base_template_id: &str, settings: Option<&Value>) -> Result<TemplateRow, String> {
    let answer = writer.execute(MODULE, json!({ "create_user": { "name": name, "description": description, "base_template_id": base_template_id, "settings": settings } }))?;
    let template_id = answer["template_id"].as_str().ok_or("no template id came back")?.to_string();
    select(writer, reader, &template_id)?;
    Ok(TemplateRow { template_id, name: name.trim().to_string(), description: description.trim().to_string(), system: false, selected: true })
}

/// Removes a user template; a selection of it goes with it (the first template is selected then).
pub fn delete_user(writer: &Writer, reader: &Reader, template_id: &str) -> Result<(), String> {
    let was_selected = qnc_registry::local_setting(reader, qnc_registry::SELECTED_TEMPLATE_KEY)?.as_deref() == Some(template_id.trim());
    writer.execute(MODULE, json!({ "delete_user": template_id }))?;
    if was_selected {
        writer.execute(qnc_registry::MODULE, json!({ "local": { "key": qnc_registry::SELECTED_TEMPLATE_KEY, "value": null } }))?;
    }
    Ok(())
}

/// `<slug>_<milliseconds>`: the id of a user template, as the old QNC made it.
fn slug_id(name: &str) -> String {
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{}_{millis}", qnc_ids::slug(name))
}

#[cfg(test)]
mod tests;
