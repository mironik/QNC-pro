//! Opening a project, as the old QNC did (copied from its project store's `open_project`): the
//! project must be registered, its workspace must hold a valid sequence of applications and its
//! `project.db` must be there; the settings saved before a field existed are completed (one
//! short write, only when something is missing); then the registry marks it opened and active.
//! Opening walks no folder tree and sets no protection: it is a short database update.

use std::path::Path;

use qnc_db::{Reader, Writer};
use serde_json::json;

pub use qnc_project_workspace::NavigationStep;

/// Completes the settings of a project made before a field existed; nothing else is touched.
fn complete_saved_settings(project_db: &Path) -> Result<(), String> {
    let reader = Reader::open(project_db)?;
    let settings = qnc_project_settings::read(&reader)?;
    drop(reader);
    let mut completed = settings.saved.clone();
    qnc_project_defaults::complete(&mut completed)?;
    if completed == settings.saved {
        return Ok(());
    }
    let writer = Writer::start(project_db, vec![Box::new(qnc_project_settings::SettingsTable)])?;
    writer.execute(qnc_project_settings::MODULE, json!({ "set": { "project_id": settings.project_id, "template_id": settings.template_id, "settings": completed, "at": qnc_registry::now() } })).map(drop)
}

/// Opens the project `project_id` of the QNC `root`: it is the active one from now on. Answers
/// the sequence of its applications, for the desktop to move on.
pub fn open(root: &Path, project_id: &str) -> Result<Vec<NavigationStep>, String> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(qnc_text::text("project.store.project_id_empty").into());
    }
    let reader = qnc_registry::reader(root)?;
    let registered = qnc_registry::project(&reader, project_id)?.ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", project_id)]))?;
    drop(reader);
    let project_db = registered.project_db();
    if !project_db.is_file() {
        return Err(qnc_text::fill("project.store.folder_invalid", &[("v0", &registered.local_path.display().to_string())]));
    }
    let steps = qnc_project_workspace::navigation_sequence(&Reader::open(&project_db)?, project_id)?;
    complete_saved_settings(&project_db)?;
    let writer = qnc_registry::writer(root)?;
    writer.execute(qnc_registry::MODULE, json!({ "activate": project_id }))?;
    Ok(steps)
}

/// The sequence of applications of a registered project, read now.
pub fn navigation_sequence(root: &Path, project_id: &str) -> Result<Vec<NavigationStep>, String> {
    let reader = qnc_registry::reader(root)?;
    let registered = qnc_registry::project(&reader, project_id)?.ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", project_id)]))?;
    qnc_project_workspace::navigation_sequence(&Reader::open(&registered.project_db())?, project_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opened_project_is_the_active_one_its_settings_completed_and_a_missing_one_opens_nothing() {
        let root = std::env::temp_dir().join(format!("qnc-project-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let first = qnc_project_make::make(&root, qnc_project_make::Make { name: "First".into(), ..Default::default() }).unwrap();
        let second = qnc_project_make::make(&root, qnc_project_make::Make { name: "Second".into(), ..Default::default() }).unwrap();
        let steps = open(&root, &first.project_id).unwrap();
        assert_eq!(steps[0].tab_id, "project");
        let registry = qnc_registry::reader(&root).unwrap();
        assert_eq!(qnc_registry::active(&registry).unwrap().unwrap().project_id, first.project_id);
        assert!(open(&root, "nobody").is_err());
        assert!(open(&root, " ").is_err());
        drop(registry);
        for made in [first, second] {
            let _ = qnc_folder_lock::unlock(&made.folder, &qnc_project_make::CACHE_FOLDERS);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
