use super::*;

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("qnc-project-make-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn cleanup(root: &Path) {
    if let Ok(entries) = std::fs::read_dir(root.join("projects")) {
        for entry in entries.flatten() {
            let _ = qnc_folder_lock::unlock(&entry.path(), &CACHE_FOLDERS);
        }
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_project_is_made_with_its_folders_its_settings_its_workspace_and_is_the_active_one() {
    let root = temp_root("create");
    let made = make(&root, Make { name: "Test Projekt".into(), ..Default::default() }).unwrap();
    assert!(made.project_id.starts_with("test_projekt_"));
    assert_eq!(made.project_uri, format!("qnc://local/project/{}", made.project_id));
    assert!(qnc_registry::file(&root).is_file());
    assert_eq!(made.folder, root.join("projects").join(qnc_ids::folder_name(&made.project_id)));
    for sub in ["original", "incoming/card", "products/thumbnails", "products/voice", "products/ai_shots", DEFAULT_EXPORT_DIRECTORY] {
        assert!(made.folder.join(sub).is_dir(), "{sub}");
    }
    let reader = qnc_db::Reader::open(&made.project_db()).unwrap();
    let settings = qnc_project_settings::read(&reader).unwrap();
    assert_eq!(settings.project_id, made.project_id);
    assert_eq!(settings.template_id.as_deref(), Some("tpl_breaking_news"), "the first system template is selected until one is chosen");
    assert_eq!(settings.saved["export"]["directory"], DEFAULT_EXPORT_DIRECTORY);
    assert_eq!(settings.saved["workspace"]["tabs"], serde_json::json!(["project"]));
    assert!(settings.playback().is_ok());
    assert_eq!(qnc_project_workspace::navigation_sequence(&reader, &made.project_id).unwrap()[0].tab_id, "project");
    let origin: String = reader.connection().query_row("SELECT project_name FROM public_project_origin", [], |r| r.get(0)).unwrap();
    assert_eq!(origin, "Test Projekt");
    assert!(qnc_catalog::summaries(&reader).unwrap().is_empty());
    let registry = qnc_registry::reader(&root).unwrap();
    let active = qnc_registry::active(&registry).unwrap().unwrap();
    assert_eq!((active.project_id.as_str(), active.project_db()), (made.project_id.as_str(), made.project_db()));
    assert_eq!(qnc_registry::rows(&registry).unwrap()[0].created_date, made.created_date);
    assert_eq!(projects_root(&root).unwrap(), root.join("projects"), "the projects' folder is this computer's now");
    assert!(make(&root, Make { name: "  ".into(), ..Default::default() }).is_err());
    assert!(make(&root, Make { name: "x".into(), template_id: "tpl_nothing".into(), ..Default::default() }).is_err());
    drop((reader, registry));
    cleanup(&root);
}

#[test]
fn a_failed_make_keeps_the_registry_and_removes_only_the_unpublished_folder() {
    let root = temp_root("failed");
    let first = make(&root, Make { name: "Existing".into(), template_id: "tpl_breaking_news".into(), ..Default::default() }).unwrap();
    let error = make(&root, Make { name: "Invalid".into(), settings: Some(serde_json::json!({ "export": { "directory": "../invalid" } })), ..Default::default() }).unwrap_err();
    assert!(error.contains("export"), "{error}");
    let error = make(&root, Make { name: "Failed workspace".into(), settings: Some(serde_json::json!({ "export": { "directory": "project.db" } })), ..Default::default() }).unwrap_err();
    assert!(!error.is_empty());
    let registry = qnc_registry::reader(&root).unwrap();
    assert_eq!(qnc_registry::projects(&registry).unwrap().len(), 1);
    assert_eq!(qnc_registry::active(&registry).unwrap().unwrap().project_id, first.project_id);
    assert_eq!(std::fs::read_dir(root.join("projects")).unwrap().count(), 1);
    drop(registry);
    cleanup(&root);
}

#[test]
fn the_projects_folder_asked_for_and_the_one_of_the_settings_are_kept_to() {
    let root = temp_root("roots");
    let elsewhere = root.join("elsewhere");
    let made = make(&root, Make { name: "Away".into(), projects_root: Some(elsewhere.clone()), ..Default::default() }).unwrap();
    assert_eq!(made.folder.parent(), Some(elsewhere.as_path()));
    assert_eq!(projects_root(&root).unwrap(), elsewhere);
    let error = make(&root, Make { name: "Bound".into(), settings: Some(serde_json::json!({ "storage": { "projects_root": "qnc://local/x" } })), ..Default::default() }).unwrap_err();
    assert!(!error.is_empty());
    let _ = qnc_folder_lock::unlock(&made.folder, &CACHE_FOLDERS);
    cleanup(&root);
}
