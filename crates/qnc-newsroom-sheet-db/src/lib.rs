//! The story sheet of a project (user: the data of the news story is a sheet a newsroom can
//! shape; its fields are kept by code, so their names are free): the sheet as it was when it was
//! filled (its fields, names and choices, as JSON) and the value of each field by its code. Kept
//! in the project database, so the script form and every export read the names and values from
//! the project itself, on any computer.
//!
//! A table module of the project database writer (`qnc-db`): it owns only its tables, is
//! written only through that writer and knows no form or application.

mod store;

use std::collections::BTreeMap;

use qnc_db::{Reader, TableModule, Writer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MODULE_ID: &str = "qnc.module.newsroom-sheet-db";

pub type Result<T> = std::result::Result<T, String>;

/// The sheet of a project and its values; an empty sheet (`Null`) when none was kept.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorySheet {
    /// The description of the fields as it was when the sheet was filled.
    pub sheet: Value,
    /// The value of each field, by its code.
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// The sheet and all its values are kept (they replace what was kept).
    Save(StorySheet),
}

impl StorySheet {
    /// The sheet as people read it, in its order: the name of each field with a value and the
    /// value (a choice by its name, a length as `m:ss`).
    pub fn shown(&self) -> Vec<(String, String)> {
        let options = self.sheet.get("options");
        let Some(fields) = self.sheet.get("fields").and_then(Value::as_array) else { return Vec::new() };
        fields
            .iter()
            .filter_map(|field| {
                let id = field.get("id")?.as_str()?;
                let label = field.get("label").and_then(Value::as_str).unwrap_or(id);
                let label = label.strip_prefix('@').and_then(|code| qnc_text::text_of(code)).map_or_else(|| label.to_string(), str::to_string);
                let value = self.values.get(id)?;
                let text = match (field.get("kind").and_then(Value::as_str), value) {
                    (Some("duration"), Value::Number(seconds)) => {
                        let seconds = seconds.as_f64()?.round() as u64;
                        (seconds > 0).then(|| format!("{}:{:02}", seconds / 60, seconds % 60))?
                    }
                    (_, Value::String(text)) => text.trim().to_string(),
                    (_, Value::Number(number)) => number.to_string(),
                    _ => return None,
                };
                if text.is_empty() {
                    return None;
                }
                let set = field.get("options").and_then(Value::as_str);
                let named = set.and_then(|set| options?.get(set)?.as_array()?.iter().find_map(|pair| {
                    let pair = pair.as_array()?;
                    (pair.first()?.as_str()? == text).then(|| pair.get(1)?.as_str().map(str::to_string)).flatten()
                }));
                Some((label, named.unwrap_or(text)))
            })
            .collect()
    }

    /// What a search of projects finds this sheet by: every typed value, and the names of the
    /// chosen choices (the code is kept, its name is what people type).
    pub fn search_text(&self) -> String {
        let options = self.sheet.get("options");
        let mut words = Vec::new();
        for value in self.values.values() {
            let text = match value {
                Value::String(text) => text.trim().to_string(),
                Value::Number(number) => number.to_string(),
                _ => continue,
            };
            if text.is_empty() {
                continue;
            }
            let named = options.and_then(Value::as_object).into_iter().flat_map(|sets| sets.values()).filter_map(Value::as_array).flatten().find_map(|pair| {
                let pair = pair.as_array()?;
                (pair.first()?.as_str()? == text).then(|| pair.get(1)?.as_str().map(str::to_string)).flatten()
            });
            words.push(text);
            words.extend(named);
        }
        words.join(" ")
    }
}

/// The story sheet tables of a project database.
pub struct StorySheetTables;

impl TableModule for StorySheetTables {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn tables(&self) -> &'static [&'static str] {
        &store::OWNED
    }

    fn attach(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        store::ensure_schema(conn)
    }

    fn execute(&mut self, conn: &rusqlite::Connection, request: Value) -> Result<Value> {
        let operation: Operation = serde_json::from_value(request).map_err(|e| e.to_string())?;
        match operation {
            Operation::Save(sheet) => store::save(conn, &sheet).map(|()| Value::Null),
        }
    }
}

/// The story sheet of the project in this database; empty when none was kept yet.
pub fn read(reader: &Reader) -> Result<StorySheet> {
    store::read(reader.connection())
}

/// Keeps `sheet` through the project's writer.
pub fn save(writer: &Writer, sheet: StorySheet) -> Result<()> {
    let request = serde_json::to_value(Operation::Save(sheet)).map_err(|e| e.to_string())?;
    writer.execute(MODULE_ID, request).map(drop)
}

#[cfg(test)]
mod tests;
