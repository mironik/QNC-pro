//! A new project, as the old QNC laid it out: a folder under the projects' folder named by
//! the id (`<slug of the name>_<fresh id>`), the media folders (`original`, `proxy`, `audio`,
//! `incoming/card`, `incoming/ftp`), the product folders the settings name, the export folder,
//! a `project.db` with the settings and the catalog tables, and the row in the registry that
//! makes it the active project. The settings come from a template given as data (the
//! caller chose it), completed by the defaults block; `storage.projects_root` is set to the
//! folder the project is in. Nothing
//! here decides what the project is for.

use std::path::{Path, PathBuf};

use qnc_db::Writer;
use serde_json::Value;

/// The computer setting that names the folder projects are made in: `{"folder": ...}`.
pub const PROJECTS_ROOT_SETTING: &str = "projects-root";
pub const PROJECT_DB: &str = "project.db";
pub const MEDIA_FOLDERS: [&str; 5] = ["original", "proxy", "audio", "incoming/card", "incoming/ftp"];
pub const DEFAULT_EXPORT_DIRECTORY: &str = "exports/projekti";
/// The cache folders of a project whose files may go (prerender, exports).
pub const CACHE_FOLDERS: [&str; 2] = ["products/prerender", "exports"];

/// What was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub project_id: String,
    pub project_uri: String,
    pub folder: PathBuf,
}

impl Made {
    pub fn project_db(&self) -> PathBuf {
        self.folder.join(PROJECT_DB)
    }
}

/// The projects folder of this computer, from its setting; unset is an error, never a guess.
pub fn projects_root(root: &Path) -> Result<PathBuf, String> {
    qnc_settings::setting(root, PROJECTS_ROOT_SETTING)?
        .and_then(|v| v.get("folder").and_then(|f| f.as_str()).filter(|f| !f.trim().is_empty()).map(PathBuf::from))
        .ok_or_else(|| qnc_text::fill("project.no_projects_root", &[("setting", PROJECTS_ROOT_SETTING)]))
}

/// The folders a project with these settings has, relative to its folder.
pub fn folders_of(settings: &Value) -> Vec<String> {
    let mut folders: Vec<String> = MEDIA_FOLDERS.iter().map(|f| f.to_string()).collect();
    if let Some(products) = settings["products"].as_object() {
        folders.extend(products.values().filter_map(Value::as_str).map(str::to_string));
    }
    let export = settings["export"]["directory"].as_str().filter(|d| !d.trim().is_empty()).unwrap_or(DEFAULT_EXPORT_DIRECTORY);
    folders.push(export.to_string());
    folders.retain(|f| !f.is_empty() && !f.starts_with('/') && !f.contains("..") && !f.contains(':'));
    folders.sort();
    folders.dedup();
    folders
}

/// Makes the project `name` from `template` in `projects_root`, registers it in the registry of
/// `root` and makes it active. `at` is the time text written into the records.
pub fn make(root: &Path, projects_root: &Path, name: &str, template: &Value, at: &str) -> Result<Made, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a project needs a name".into());
    }
    if !template.is_object() {
        return Err("the settings template is not an object".into());
    }
    let project_id = format!("{}_{}", qnc_ids::slug(name), qnc_ids::fresh());
    qnc_uri::valid_id(&project_id)?;
    let project_uri = format!("qnc://local/project/{project_id}");
    std::fs::create_dir_all(projects_root).map_err(|e| format!("{}: {e}", projects_root.display()))?;
    let folder = projects_root.join(qnc_ids::folder_name(&project_id));
    // A new folder is claimed, never one already there.
    std::fs::create_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let made = (|| {
        let mut settings = template.clone();
        qnc_project_defaults::complete(&mut settings)?;
        if let Some(storage) = settings["storage"].as_object_mut() {
            storage.insert("projects_root".into(), Value::String(projects_root.to_string_lossy().into_owned()));
        }
        if settings["export"]["directory"].as_str().is_none_or(|d| d.trim().is_empty()) {
            settings["export"]["directory"] = Value::String(DEFAULT_EXPORT_DIRECTORY.into());
        }
        for sub in folders_of(&settings) {
            std::fs::create_dir_all(folder.join(&sub)).map_err(|e| format!("{}: {e}", folder.join(&sub).display()))?;
        }
        let db = folder.join(PROJECT_DB);
        qnc_db::create(&db)?;
        let writer = Writer::start(&db, vec![Box::new(qnc_project_settings::SettingsTable), Box::new(qnc_catalog::CatalogTables)])?;
        writer.execute(qnc_project_settings::MODULE, serde_json::json!({ "set": { "project_id": project_id, "settings": settings, "at": at } }))?;
        drop(writer);
        let registry = qnc_registry::writer(root)?;
        registry.execute(qnc_registry::MODULE, serde_json::json!({ "register": { "project_id": project_id, "name": name, "project_uri": project_uri, "local_path": folder.to_string_lossy(), "at": at } }))?;
        Ok(Made { project_id: project_id.clone(), project_uri: project_uri.clone(), folder: folder.clone() })
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir_all(&folder);
    }
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../../../contracts/project/templates/breaking_news.json");

    #[test]
    fn a_project_is_made_with_its_folders_its_settings_its_empty_catalog_and_is_the_active_one() {
        let root = std::env::temp_dir().join(format!("qnc-project-make-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let template: Value = serde_json::from_str::<Value>(TEMPLATE).unwrap()["settings"].clone();
        let made = make(&root, &root.join("projects"), "Breaking news 2026-11-20 13:28", &template, "epoch_1").unwrap();
        assert!(made.project_id.starts_with("breaking_news_2026-11-20_13_28_"));
        assert_eq!(made.project_uri, format!("qnc://local/project/{}", made.project_id));
        for sub in ["original", "incoming/card", "products/thumbnails", "products/voice", "exports/projekti"] {
            assert!(made.folder.join(sub).is_dir(), "{sub}");
        }
        let reader = qnc_db::Reader::open(&made.project_db()).unwrap();
        let settings = qnc_project_settings::read(&reader).unwrap();
        assert_eq!(settings.project_id, made.project_id);
        assert_eq!(settings.saved["storage"]["projects_root"].as_str().map(PathBuf::from), Some(root.join("projects")));
        assert_eq!(settings.playback().unwrap().workspace_db_uri, format!("qnc://local/db/project_workspace/{}", made.project_id));
        assert!(qnc_catalog::summaries(&reader).unwrap().is_empty());
        let active = qnc_registry::active(&qnc_registry::reader(&root).unwrap()).unwrap().unwrap();
        assert_eq!((active.project_id.as_str(), active.project_db()), (made.project_id.as_str(), made.project_db()));
        assert!(make(&root, &root.join("projects"), "  ", &template, "epoch_2").is_err());
        drop(reader);
        let _ = std::fs::remove_dir_all(&root);
    }
}
