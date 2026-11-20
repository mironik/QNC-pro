//! Deleting a project is the one destructive action of QNC and lives here alone. The folder is
//! deleted only when it is the registered folder of that project and holds its `project.db`
//! (never a folder that merely has the name); its protection is lifted first; a folder that is
//! already gone is no error, the registry row goes anyway. The caller asked the user first.

use std::path::{Path, PathBuf};

/// What was deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    pub project_id: String,
    pub name: String,
    pub folder: PathBuf,
}

fn is_project_folder(folder: &Path) -> bool {
    folder.join(qnc_project_make::PROJECT_DB).is_file()
}

/// Deletes the project `project_id` of the QNC `root`: its folder and its registry row.
pub fn delete(root: &Path, project_id: &str) -> Result<Deleted, String> {
    let reader = qnc_registry::reader(root)?;
    let registered = qnc_registry::project(&reader, project_id)?.ok_or_else(|| qnc_text::fill("project.unknown", &[("id", project_id)]))?;
    drop(reader);
    let folder = registered.local_path.clone();
    if folder.exists() {
        if !is_project_folder(&folder) {
            return Err(qnc_text::fill("project.folder_not_a_project", &[("path", &folder.display().to_string())]));
        }
        qnc_folder_lock::unlock(&folder, &qnc_project_make::CACHE_FOLDERS)?;
        std::fs::remove_dir_all(&folder).map_err(|e| qnc_text::fill("project.folder_not_removed", &[("path", &folder.display().to_string()), ("error", &e.to_string())]))?;
    }
    let writer = qnc_registry::writer(root)?;
    writer.execute(qnc_registry::MODULE, serde_json::json!({ "delete": project_id }))?;
    Ok(Deleted { project_id: registered.project_id, name: registered.name, folder })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../../../contracts/project/templates/field_sony.json");

    #[test]
    fn a_project_is_deleted_with_its_folder_and_the_next_one_is_active_a_folder_that_is_no_project_stays() {
        let root = std::env::temp_dir().join(format!("qnc-project-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let template: serde_json::Value = serde_json::from_str(TEMPLATE).unwrap();
        let first = qnc_project_make::make(&root, &root.join("projects"), "First", &template["settings"], "epoch_1").unwrap();
        let second = qnc_project_make::make(&root, &root.join("projects"), "Second", &template["settings"], "epoch_2").unwrap();
        qnc_folder_lock::lock(&second.folder, &qnc_project_make::CACHE_FOLDERS).unwrap();
        let deleted = delete(&root, &second.project_id).unwrap();
        assert_eq!((deleted.name.as_str(), deleted.folder.exists()), ("Second", false));
        let reader = qnc_registry::reader(&root).unwrap();
        assert_eq!(qnc_registry::active(&reader).unwrap().unwrap().project_id, first.project_id);
        assert!(qnc_registry::project(&reader, &second.project_id).unwrap().is_none());
        drop(reader);
        std::fs::remove_file(first.project_db()).unwrap();
        assert!(delete(&root, &first.project_id).is_err(), "a folder without project.db is not deleted");
        assert!(first.folder.is_dir());
        assert!(delete(&root, "nobody").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
