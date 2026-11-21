//! Where a project is and what folders it has, as the old QNC laid it out.

use std::path::{Component, Path, PathBuf};

use qnc_db::Reader;
use serde_json::Value;

pub const PROJECT_DB: &str = "project.db";
/// The media folders of every project.
pub const MEDIA_FOLDERS: [&str; 5] = ["proxy", "original", "audio", "incoming/card", "incoming/ftp"];
pub const DEFAULT_EXPORT_DIRECTORY: &str = "exports/projekti";
/// The folders of a project whose files may go while the folder stays protected: the
/// prerender cache and the exports.
pub const CACHE_FOLDERS: [&str; 2] = ["products/prerender", "exports"];
/// The projects' folder a template may name to mean this computer's.
const STANDARD_ROOT: &str = "$QNC_STANDARD_PROJECTS_ROOT";

/// The folder this computer makes its projects in: the one it chose, else `projects` under the root.
pub fn projects_root(root: &Path) -> Result<PathBuf, String> {
    let chosen = match qnc_registry::file(root).is_file() {
        true => qnc_registry::project_dir(&qnc_registry::reader(root)?)?,
        false => None,
    };
    Ok(chosen.unwrap_or_else(|| root.join("projects")))
}

fn text_at(settings: &Value, path: &[&str]) -> String {
    let mut current = settings;
    for segment in path {
        let Some(next) = current.get(*segment) else { return String::new() };
        current = next;
    }
    current.as_str().unwrap_or("").trim().to_string()
}

fn set_text_at(settings: &mut Value, path: &[&str], value: &str) {
    let mut current = settings;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
        current = current.as_object_mut().expect("an object").entry((*segment).to_string()).or_insert_with(|| Value::Object(Default::default()));
    }
    if !current.is_object() {
        *current = Value::Object(Default::default());
    }
    current.as_object_mut().expect("an object").insert(path[path.len() - 1].to_string(), Value::String(value.to_string()));
}

/// The projects' folder a new project goes in: what the settings name (`storage.projects_root`,
/// absolute or under the root), else the one asked, else this computer's, else `projects`.
pub(crate) fn projects_root_of(root: &Path, reader: &Reader, settings: &Value, asked: Option<&Path>) -> Result<PathBuf, String> {
    let requested = text_at(settings, &["storage", "projects_root"]);
    let location = if requested.is_empty() || requested == STANDARD_ROOT {
        match asked {
            Some(asked) => asked.to_path_buf(),
            None => qnc_registry::project_dir(reader)?.unwrap_or_else(|| root.join("projects")),
        }
    } else {
        if requested.starts_with("qnc://") {
            return Err(qnc_text::text("project.store.location_needs_binding").into());
        }
        if requested.starts_with('$') {
            return Err(qnc_text::fill("project.store.unknown_location", &[("location", &requested)]));
        }
        PathBuf::from(requested)
    };
    if location.is_absolute() {
        Ok(location)
    } else if location.components().any(|part| !matches!(part, Component::Normal(_) | Component::CurDir)) {
        Err(qnc_text::text("project.store.relative_outside").into())
    } else {
        Ok(root.join(location))
    }
}

/// The export folder of a project: absolute as it is, relative under the project (never with
/// `..`), refused otherwise.
pub fn export_directory_path(project_dir: &Path, export_directory: &str) -> Result<PathBuf, String> {
    let export_path = PathBuf::from(export_directory);
    if export_path.is_absolute() {
        return Ok(export_path);
    }
    if export_path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(qnc_text::fill("project.store.export_dir_rule", &[("export_directory", export_directory)]));
    }
    Ok(project_dir.join(export_path))
}

/// The export folder the settings name (`export.directory`, else `export.output_directory`,
/// else the default written in); a QNC URI names no folder to make here.
pub fn project_export_directory(project_dir: &Path, settings: &mut Value) -> Result<Option<PathBuf>, String> {
    let mut export_directory = text_at(settings, &["export", "directory"]);
    if export_directory.is_empty() {
        export_directory = text_at(settings, &["export", "output_directory"]);
    }
    if export_directory.is_empty() {
        export_directory = DEFAULT_EXPORT_DIRECTORY.to_string();
        set_text_at(settings, &["export", "directory"], DEFAULT_EXPORT_DIRECTORY);
    }
    if export_directory.starts_with("qnc://") {
        qnc_uri::parse(&export_directory)?;
        return Ok(None);
    }
    export_directory_path(project_dir, &export_directory).map(Some)
}

/// The folders a project with these settings has, relative to its folder: the media folders
/// and the product folders the settings name.
pub fn folders_of(settings: &Value) -> Vec<String> {
    let mut folders: Vec<String> = MEDIA_FOLDERS.iter().map(|f| f.to_string()).collect();
    if let Some(products) = settings["products"].as_object() {
        folders.extend(products.values().filter_map(Value::as_str).map(str::to_string));
    }
    folders.retain(|f| !f.is_empty() && !f.starts_with('/') && !f.contains("..") && !f.contains(':'));
    folders.sort();
    folders.dedup();
    folders
}
