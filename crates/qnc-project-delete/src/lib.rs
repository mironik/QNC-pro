//! Deleting a project is the one destructive action of QNC and lives here alone (copied from the
//! old QNC's `delete_project`): the folder goes only when it is the registered folder of that
//! project, named by its id, a real folder and no link; the protection of the folder and the
//! read-only marks of its files are lifted first, the database files go before the folder; a
//! folder that is already gone is no error, the registry rows go anyway, and when the deleted
//! project was the active one the first of the rest becomes active. The caller asked the user.

use std::path::{Path, PathBuf};

/// What was deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    pub project_id: String,
    pub name: String,
    pub folder: PathBuf,
}

/// Refuses a folder that is not the folder of this project.
fn validate_folder(project_id: &str, folder: &Path) -> Result<(), String> {
    let shown = folder.display().to_string();
    if folder.file_name().and_then(|n| n.to_str()) != Some(qnc_ids::folder_name(project_id).as_str()) {
        return Err(qnc_text::fill("project.store.delete_stopped_id", &[("v0", &shown), ("v1", project_id)]));
    }
    if !folder.exists() {
        return Ok(());
    }
    let kind = std::fs::symlink_metadata(folder).map_err(|e| e.to_string())?.file_type();
    if kind.is_symlink() {
        return Err(qnc_text::fill("project.store.delete_stopped_link", &[("v0", &shown)]));
    }
    if !kind.is_dir() {
        return Err(qnc_text::fill("project.store.delete_stopped_not_folder", &[("v0", &shown)]));
    }
    Ok(())
}

/// Lifts the read-only mark of everything under `path` (never through a link).
fn clear_read_only(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            clear_read_only(&entry.map_err(|e| e.to_string())?.path())?;
        }
    }
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions).map_err(|e| qnc_text::fill("project.store.readonly_clear_failed", &[("v0", &path.display().to_string()), ("error", &e.to_string())]))?;
    }
    Ok(())
}

/// The database and its sidecars go first, so the folder is never left with a half of them.
fn delete_database_files(project_db: &Path) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = project_db.as_os_str().to_os_string();
        name.push(suffix);
        let file = PathBuf::from(name);
        if file.exists() {
            clear_read_only(&file)?;
            std::fs::remove_file(&file).map_err(|e| qnc_text::fill("project.store.db_delete_failed", &[("v0", &file.display().to_string()), ("error", &e.to_string())]))?;
        }
    }
    Ok(())
}

/// Deletes the project `project_id` of the QNC `root`: its folder and its registry rows.
pub fn delete(root: &Path, project_id: &str) -> Result<Deleted, String> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(qnc_text::text("project.store.project_id_empty").into());
    }
    let reader = qnc_registry::reader(root)?;
    let registered = qnc_registry::project(&reader, project_id)?.ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", project_id)]))?;
    drop(reader);
    let folder = registered.local_path.clone();
    validate_folder(project_id, &folder)?;
    if folder.exists() {
        qnc_folder_lock::unlock(&folder, &qnc_project_make::CACHE_FOLDERS)?;
        let gone = clear_read_only(&folder)
            .and_then(|()| delete_database_files(&registered.project_db()))
            .and_then(|()| std::fs::remove_dir_all(&folder).map_err(|e| qnc_text::fill("project.store.folder_delete_failed", &[("v0", &folder.display().to_string()), ("error", &e.to_string())])));
        if let Err(error) = gone {
            let _ = qnc_folder_lock::lock(&folder, &qnc_project_make::CACHE_FOLDERS);
            return Err(error);
        }
    }
    let writer = qnc_registry::writer(root)?;
    writer.execute(qnc_registry::MODULE, serde_json::json!({ "delete": project_id }))?;
    Ok(Deleted { project_id: registered.project_id, name: registered.name, folder })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_deleted_with_its_folder_and_the_first_of_the_rest_is_active_a_moved_folder_stays() {
        let root = std::env::temp_dir().join(format!("qnc-project-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let first = qnc_project_make::make(&root, qnc_project_make::Make { name: "First".into(), ..Default::default() }).unwrap();
        let second = qnc_project_make::make(&root, qnc_project_make::Make { name: "Second".into(), ..Default::default() }).unwrap();
        let mut file = std::fs::File::create(second.folder.join("original").join("kept.txt")).unwrap();
        std::io::Write::write_all(&mut file, b"x").unwrap();
        drop(file);
        let mut read_only = std::fs::metadata(second.folder.join("original").join("kept.txt")).unwrap().permissions();
        read_only.set_readonly(true);
        std::fs::set_permissions(second.folder.join("original").join("kept.txt"), read_only).unwrap();
        let deleted = delete(&root, &second.project_id).unwrap();
        assert_eq!((deleted.name.as_str(), deleted.folder.exists()), ("Second", false));
        let reader = qnc_registry::reader(&root).unwrap();
        assert_eq!(qnc_registry::active(&reader).unwrap().unwrap().project_id, first.project_id);
        assert!(qnc_registry::project(&reader, &second.project_id).unwrap().is_none());
        drop(reader);
        let moved = root.join("projects").join("elsewhere");
        qnc_folder_lock::unlock(&first.folder, &qnc_project_make::CACHE_FOLDERS).unwrap();
        std::fs::rename(&first.folder, &moved).unwrap();
        std::fs::create_dir(&first.folder).unwrap();
        std::fs::rename(&first.folder, first.folder.with_extension("x")).unwrap();
        // The registered folder is gone: the rows go, nothing else is touched.
        assert!(delete(&root, &first.project_id).is_ok());
        assert!(moved.is_dir());
        assert!(delete(&root, "nobody").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
