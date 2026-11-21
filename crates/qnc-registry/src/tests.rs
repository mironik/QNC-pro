use super::*;
use serde_json::json;

#[test]
fn a_registered_project_is_the_active_one_and_the_folder_stays_private_to_the_registry() {
    let root = std::env::temp_dir().join(format!("qnc-registry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let writer = writer(&root).unwrap();
    writer.execute(MODULE, json!({ "register": { "project_id": "p-1", "name": "First", "project_uri": "qnc://local/project/p-1", "local_path": root.join("p-1").to_string_lossy(), "at": "epoch_1700000000", "identity": { "contract_version": "0.1.0", "workstation_name": "w", "user_name": "u", "hardware_serial": null, "issues": [] }, "projects_root": root.to_string_lossy() } })).unwrap();
    writer.execute(MODULE, json!({ "register": { "project_id": "p-2", "name": "Second", "project_uri": "qnc://local/project/p-2", "local_path": root.join("p-2").to_string_lossy(), "at": "epoch_1700086400" } })).unwrap();
    let reader = reader(&root).unwrap();
    assert_eq!(active(&reader).unwrap().unwrap().project_id, "p-2");
    assert_eq!(projects(&reader).unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Second", "First"]);
    assert_eq!(project_dir(&reader).unwrap(), Some(root.clone()));
    let origin: String = reader.connection().query_row("SELECT workstation_name FROM public_project_origin WHERE project_id='p-1'", [], |r| r.get(0)).unwrap();
    assert_eq!(origin, "w");
    writer.execute(MODULE, json!({ "search": { "project_id": "p-1", "text": "PROTEST Ivan" } })).unwrap();
    let rows = rows(&reader).unwrap();
    assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["First", "Second"], "the list is oldest first");
    assert!(rows[0].matches("ivan first") && !rows[0].matches("nema"));
    assert!(rows[0].created_date.ends_with(".2023."), "{}", rows[0].created_date);
    assert!(rows[1].active && !rows[0].active);
    writer.execute(MODULE, json!({ "activate": "p-1" })).unwrap();
    assert_eq!(active(&reader).unwrap().unwrap().project_db(), root.join("p-1").join("project.db"));
    assert!(writer.execute(MODULE, json!({ "activate": "p-9" })).is_err());
    writer.execute(MODULE, json!({ "local": { "key": SELECTED_TEMPLATE_KEY, "value": "tpl_x" } })).unwrap();
    assert_eq!(local_setting(&reader, SELECTED_TEMPLATE_KEY).unwrap().as_deref(), Some("tpl_x"));
    writer.execute(MODULE, json!({ "local": { "key": SELECTED_TEMPLATE_KEY, "value": null } })).unwrap();
    assert_eq!(local_setting(&reader, SELECTED_TEMPLATE_KEY).unwrap(), None);
    writer.execute(MODULE, json!({ "delete": "p-1" })).unwrap();
    assert_eq!(active(&reader).unwrap().unwrap().project_id, "p-2", "the newest remaining project is active");
    assert!(project(&reader, "p-1").unwrap().is_none());
    let origins: i64 = reader.connection().query_row("SELECT COUNT(*) FROM public_project_origin", [], |r| r.get(0)).unwrap();
    assert_eq!(origins, 0, "the origin and the search go with the project");
    assert!(writer.execute(MODULE, json!({ "delete": "p-1" })).is_err());
    writer.execute(MODULE, json!({ "deactivate": true })).unwrap();
    assert!(active(&reader).unwrap().is_none());
    assert!(writer.execute(MODULE, json!({ "register": { "project_id": "bad id", "name": "x", "project_uri": "qnc://local/project/x", "local_path": "x" } })).is_err());
    drop(writer);
    let _ = std::fs::remove_dir_all(&root);
}
