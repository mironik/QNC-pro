//! A new project, as the old QNC made it (copied from its project store's `create_project`):
//! the template (named, or the one this computer selected) with the settings the form edited,
//! the applications chosen written into them, the settings completed; the id `<slug>_<fresh>`;
//! the origin of the project (this workstation and user) read now; the projects' folder from the
//! settings, else this computer's, else `projects` under the root; the export folder; a new
//! folder claimed (never one already there) with the media and product folders, `project.db`
//! with the settings, the catalog tables and the workspace (origin, template snapshot,
//! workflow); the folder protected; then the registry row that makes it the active project.
//! A failure anywhere removes only the unpublished folder and leaves the registry as it was.

mod folders;

use std::path::{Path, PathBuf};

use qnc_db::Writer;
use serde_json::{json, Value};

pub use folders::{export_directory_path, folders_of, project_export_directory, projects_root, CACHE_FOLDERS, DEFAULT_EXPORT_DIRECTORY, MEDIA_FOLDERS, PROJECT_DB};

/// What the caller asks for.
#[derive(Debug, Clone, Default)]
pub struct Make {
    pub name: String,
    /// The template to start from; empty: the one this computer selected.
    pub template_id: String,
    /// The settings as the form edited them; `None`: the template's.
    pub settings: Option<Value>,
    /// The applications chosen (a selection snapshot); `None`: the Project application alone.
    pub selection: Option<Value>,
    /// Where to make it; `None`: the settings', else this computer's, else `projects` under the root.
    pub projects_root: Option<PathBuf>,
}

/// What was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub project_id: String,
    pub name: String,
    pub project_uri: String,
    pub folder: PathBuf,
    /// `dd.mm.yyyy.` of today.
    pub created_date: String,
}

impl Made {
    pub fn project_db(&self) -> PathBuf {
        self.folder.join(PROJECT_DB)
    }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The template as the workspace keeps its snapshot.
fn template_value(template: &qnc_project_templates::Template, settings: &Value) -> Value {
    json!({ "template_id": template.template_id, "name": template.name, "system": template.system, "settings": settings, "source_template_ids": template.source_template_ids })
}

/// Removes a folder that was claimed but never published: only a real folder right under the
/// confirmed projects' root.
fn remove_unpublished(confirmed_root: &Path, project_dir: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(project_dir).map_err(|e| e.to_string())?;
    let resolved = project_dir.canonicalize().map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || resolved.parent() != Some(confirmed_root) {
        return Err(qnc_text::text("project.store.remove_outside").into());
    }
    qnc_folder_lock::unlock(project_dir, &CACHE_FOLDERS)?;
    std::fs::remove_dir_all(project_dir).map_err(|e| e.to_string())
}

/// Makes the project in the QNC `root` and makes it the active one.
pub fn make(root: &Path, ask: Make) -> Result<Made, String> {
    let name = ask.name.trim();
    if name.is_empty() {
        return Err(qnc_text::text("project.store.project_name_empty").into());
    }
    let registry = qnc_project_templates::writer(root)?;
    let reader = qnc_registry::reader(root)?;
    let template_id = match ask.template_id.trim() {
        "" => qnc_project_templates::selected_id(&reader)?,
        id => id.to_string(),
    };
    let template = qnc_project_templates::get(&reader, &template_id)?.ok_or_else(|| qnc_text::fill("project.store.template_missing", &[("id", &template_id)]))?;
    let mut settings = ask.settings.unwrap_or_else(|| template.settings.clone());
    let selection = ask.selection.unwrap_or_else(qnc_project_defaults::project_only);
    qnc_project_defaults::apply_application_selection(&mut settings, &selection)?;
    qnc_project_defaults::complete(&mut settings)?;
    let project_id = format!("{}_{}", qnc_ids::slug(name), qnc_ids::fresh());
    qnc_uri::valid_id(&project_id)?;
    let project_uri = format!("qnc://local/project/{project_id}");
    qnc_uri::parse(&project_uri)?;
    let secs = unix_secs();
    let at = format!("epoch_{secs}");
    // The origin is read on the originating workstation, outside any transaction.
    let identity = serde_json::to_value(qnc_workstation_identity::read_local_identity()).map_err(|e| e.to_string())?;
    let projects_root = folders::projects_root_of(root, &reader, &settings, ask.projects_root.as_deref())?;
    drop(reader);
    let project_dir = projects_root.join(qnc_ids::folder_name(&project_id));
    let export_path = project_export_directory(&project_dir, &mut settings)?;
    std::fs::create_dir_all(&projects_root).map_err(|e| e.to_string())?;
    let confirmed_root = projects_root.canonicalize().map_err(|e| e.to_string())?;
    // A new folder is claimed before anything is written; never one already there.
    std::fs::create_dir(&project_dir).map_err(|e| format!("{}: {e}", project_dir.display()))?;
    let made = (|| {
        if let Some(export_path) = &export_path {
            std::fs::create_dir_all(export_path).map_err(|e| qnc_text::fill("project.store.export_dir_create_failed", &[("v0", &export_path.display().to_string()), ("error", &e.to_string())]))?;
        }
        for sub in folders_of(&settings) {
            std::fs::create_dir_all(project_dir.join(&sub)).map_err(|e| format!("{}: {e}", project_dir.join(&sub).display()))?;
        }
        let db = project_dir.join(PROJECT_DB);
        qnc_db::create(&db)?;
        let writer = Writer::start(&db, vec![Box::new(qnc_project_settings::SettingsTable), Box::new(qnc_catalog::CatalogTables), Box::new(qnc_project_workspace::WorkspaceTables)])?;
        writer.execute(qnc_project_settings::MODULE, json!({ "set": { "project_id": project_id, "template_id": template_id, "settings": settings, "at": at } }))?;
        qnc_project_workspace::prepare(&writer, &project_id, &at, name, &identity, &template_value(&template, &settings))?;
        drop(writer);
        if let Err(error) = qnc_folder_lock::lock(&project_dir, &CACHE_FOLDERS) {
            if cfg!(windows) {
                return Err(error);
            }
            qnc_diagnostics::log(format!("project folder not protected: {error}"));
        }
        // Published only now, prepared, in one short registry write.
        registry.execute(
            qnc_registry::MODULE,
            json!({ "register": { "project_id": project_id, "name": name, "project_uri": project_uri, "local_path": project_dir.to_string_lossy(), "at": at, "identity": identity, "projects_root": projects_root.to_string_lossy() } }),
        )?;
        Ok(Made { project_id: project_id.clone(), name: name.to_string(), project_uri: project_uri.clone(), folder: project_dir.clone(), created_date: qnc_ids::date_stamp(secs) })
    })();
    match made {
        Ok(made) => Ok(made),
        Err(mut error) => {
            if let Err(cleanup) = remove_unpublished(&confirmed_root, &project_dir) {
                error.push_str(&qnc_text::fill("project.store.cleanup_failed", &[("cleanup_error", &cleanup)]));
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
