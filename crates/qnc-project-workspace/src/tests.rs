use super::*;

#[test]
fn a_prepared_workspace_has_the_origin_once_the_snapshot_and_the_sequence_of_its_applications() {
    let dir = std::env::temp_dir().join(format!("qnc-project-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("project.db");
    qnc_db::create(&file).unwrap();
    let writer = Writer::start(&file, vec![Box::new(WorkspaceTables)]).unwrap();
    let mut settings = json!({ "video": { "fps": 50 } });
    let mut selection = qnc_project_defaults::project_only();
    selection["applications"].as_array_mut().unwrap().push(json!({ "application_id": "qnc.ingest", "tab_id": "ingest", "label": "Ingest", "priority_group": "b" }));
    qnc_project_defaults::apply_application_selection(&mut settings, &selection).unwrap();
    let identity = json!({ "contract_version": "0.1.0", "workstation_name": "w", "user_name": "u", "hardware_serial": null, "issues": [] });
    let template = json!({ "template_id": "tpl_x", "name": "X", "system": true, "settings": settings, "source_template_ids": [] });
    prepare(&writer, "p1", "epoch_1700000000", "Prilog", &identity, &template).unwrap();
    let reader = Reader::open(&file).unwrap();
    let steps = navigation_sequence(&reader, "p1").unwrap();
    assert_eq!(steps.iter().map(|s| (s.tab_id.as_str(), s.priority_group.as_str())).collect::<Vec<_>>(), [("project", "a"), ("ingest", "b")]);
    let (state, next): (String, Option<String>) = reader.connection().query_row("SELECT s.active_step_id, w.next_step_id FROM public_project_workflow_state s JOIN public_project_workflow_steps w ON w.step_id='step_project' WHERE s.project_id='p1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((state.as_str(), next.as_deref()), ("step_ingest", Some("step_ingest")));
    let name: String = reader.connection().query_row("SELECT template_name FROM public_project_template_snapshot", [], |r| r.get(0)).unwrap();
    assert_eq!(name, "X");
    assert!(prepare(&writer, "p1", "epoch_2", "Prilog", &identity, &template).is_err(), "the origin is captured once");
    let mut no_sequence = template.clone();
    no_sequence["settings"] = json!({});
    assert!(prepare(&writer, "p2", "epoch_2", "Drugi", &identity, &no_sequence).is_err());
    drop(reader);
    drop(writer);
    let _ = std::fs::remove_dir_all(&dir);
}
