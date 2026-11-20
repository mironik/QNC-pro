//! The settings of a project, in its own database, in the shape the old QNC saved them
//! (`project_settings` with `settings_json`, the `public_project_settings` view). This block
//! owns the table (makes it, writes the row through the database writer) and is the one place
//! that reads `settings_json`: every value is taken strictly from what is saved; a missing or
//! unusable value is an error with its key, never a substitute.

use qnc_db::{Reader, TableModule};
use qnc_player_input::{PlaybackInput, PlaybackSettings, ProjectAudio};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.project-settings";
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS project_settings (
    project_id TEXT PRIMARY KEY, template_id TEXT, settings_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT, updated_by TEXT, created_at TEXT, updated_at TEXT
);
CREATE VIEW IF NOT EXISTS public_project_settings AS
    SELECT project_id, template_id, created_at, updated_at, settings_json FROM project_settings;";

/// The row as saved: the id of the project and its settings as a JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub project_id: String,
    pub template_id: Option<String>,
    pub saved: Value,
}

fn missing(key: &str) -> String {
    format!("settings_json has no usable {key}")
}

impl Settings {
    fn text(&self, keys: &[&str]) -> Result<&str, String> {
        let mut value = &self.saved;
        for key in keys {
            value = value.get(key).ok_or_else(|| missing(&keys.join(".")))?;
        }
        value.as_str().ok_or_else(|| missing(&keys.join(".")))
    }

    fn number(&self, keys: &[&str]) -> Result<u64, String> {
        let mut value = &self.saved;
        for key in keys {
            value = value.get(key).ok_or_else(|| missing(&keys.join(".")))?;
        }
        value.as_u64().ok_or_else(|| missing(&keys.join(".")))
    }

    /// The database of this project as the player names it.
    pub fn workspace_db_uri(&self) -> Result<String, String> {
        match self.text(&["workspace_db_uri"]) {
            Ok(uri) => Ok(uri.to_string()),
            Err(_) => {
                qnc_uri::valid_id(&self.project_id)?;
                Ok(format!("qnc://local/db/project_workspace/{}", self.project_id))
            }
        }
    }

    /// The project folder as a media source: a copy in the project is read through it, by
    /// `<this>/file/<path>` (the old QNC names it so, local environment).
    pub fn project_media_source_uri(&self) -> Result<String, String> {
        qnc_uri::valid_id(&self.project_id)?;
        Ok(format!("qnc://local/source/project-{}", self.project_id))
    }

    pub fn playback_input(&self) -> Result<PlaybackInput, String> {
        serde_json::from_value(json!(self.text(&["playback", "input"])?)).map_err(|_| missing("playback.input"))
    }

    pub fn audio(&self) -> Result<ProjectAudio, String> {
        let channels = u16::try_from(self.number(&["audio", "channels"])?).map_err(|_| missing("audio.channels"))?;
        let sample_rate_hz = u32::try_from(self.number(&["audio", "sample_rate"])?).map_err(|_| missing("audio.sample_rate"))?;
        let audio = ProjectAudio { channels, sample_rate_hz };
        audio.validate().map_err(|e| e.to_string())?;
        Ok(audio)
    }

    /// What the player input needs of the project.
    pub fn playback(&self) -> Result<PlaybackSettings, String> {
        Ok(PlaybackSettings { workspace_db_uri: self.workspace_db_uri()?, playback_input: self.playback_input()?, audio: self.audio()? })
    }

    /// `storage.ingest_media`: what Import does with the medium (link, original, proxy, ...).
    pub fn ingest_media(&self) -> Result<String, String> {
        Ok(self.text(&["storage", "ingest_media"])?.to_string())
    }

    /// A product folder relative to the project folder (`products.<area>`), e.g. `thumbnails`.
    pub fn product_path(&self, area: &str) -> Result<String, String> {
        let path = self.text(&["products", area])?;
        if path.is_empty() || path.starts_with('/') || path.contains("..") || path.contains(':') {
            return Err(missing(&format!("products.{area}")));
        }
        Ok(path.to_string())
    }

    /// A feature of the AI template of the project (`ai.features` names it).
    pub fn ai_feature(&self, code: &str) -> bool {
        self.saved["ai"]["features"].as_array().is_some_and(|f| f.iter().any(|v| v.as_str() == Some(code)))
    }
}

pub struct SettingsTable;

impl TableModule for SettingsTable {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["project_settings"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
    }

    /// `{"set": {"project_id", "template_id"?, "settings": {...}, "at": "<utc text>"}}`: the one row.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        let set = request.get("set").ok_or("unknown request")?;
        let project_id = set["project_id"].as_str().ok_or("set needs a project_id")?;
        qnc_uri::valid_id(project_id)?;
        let settings = set.get("settings").filter(|s| s.is_object()).ok_or("set needs settings as an object")?;
        let at = set["at"].as_str();
        let text = serde_json::to_string(settings).map_err(|e| e.to_string())?;
        let changed = conn
            .execute("UPDATE project_settings SET template_id=?2, settings_json=?3, updated_at=?4 WHERE project_id=?1", params![project_id, set["template_id"].as_str(), text, at])
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            conn.execute("INSERT INTO project_settings(project_id,template_id,settings_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)", params![project_id, set["template_id"].as_str(), text, at])
                .map_err(|e| e.to_string())?;
        }
        Ok(json!({ "project_id": project_id }))
    }
}

/// The settings of the project in this database (a project database holds one row).
pub fn read(reader: &Reader) -> Result<Settings, String> {
    let row = reader
        .connection()
        .query_row("SELECT project_id, template_id, settings_json FROM public_project_settings LIMIT 1", [], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let (project_id, template_id, text) = row.ok_or("the project database has no settings row")?;
    let saved: Value = serde_json::from_str(&text).map_err(|e| format!("settings_json: {e}"))?;
    if !saved.is_object() {
        return Err("settings_json is not an object".into());
    }
    Ok(Settings { project_id, template_id, saved })
}

#[cfg(test)]
mod tests;
