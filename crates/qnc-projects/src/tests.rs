use super::*;

fn wait(projects: &mut Projects) {
    for _ in 0..600 {
        projects.poll();
        if !projects.busy() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the job did not end");
}

#[test]
fn a_project_is_made_from_the_draft_opened_and_deleted_after_the_question() {
    let root = std::env::temp_dir().join(format!("qnc-projects-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    qnc_registry::writer(&root).unwrap().execute(qnc_registry::MODULE, serde_json::json!({ "local": { "key": qnc_registry::PROJECTS_ROOT_KEY, "value": root.join("projects").to_string_lossy() } })).unwrap();
    let mut projects = Projects::open(&root).unwrap();
    assert!(!projects.templates().is_empty(), "the system templates are seeded");
    assert!(projects.lines().is_empty());
    projects.act(Act::ChooseTemplate("tpl_ingest_field".into()));
    projects.act(Act::New);
    assert!(projects.busy());
    projects.act(Act::New);
    wait(&mut projects);
    assert_eq!(projects.lines().len(), 1, "a second New while busy is refused");
    assert!(projects.lines()[0].name.starts_with("Teren"), "{:?}", projects.lines()[0]);
    assert!(projects.lines()[0].active);
    assert!(projects.status().unwrap().contains(&projects.lines()[0].name), "{:?}", projects.status());
    projects.draft.name = "Named".into();
    projects.act(Act::New);
    wait(&mut projects);
    let named = projects.lines().iter().find(|l| l.name == "Named").unwrap().clone();
    assert!(named.active);
    let first = projects.lines().iter().find(|l| l.name != "Named").unwrap().clone();
    projects.act(Act::Open(first.project_id.clone()));
    assert!(projects.lines().iter().find(|l| l.project_id == first.project_id).unwrap().active);
    projects.act(Act::Delete(named.project_id.clone()));
    assert_eq!(projects.asking_delete().map(|l| l.name.as_str()), Some("Named"));
    projects.act(Act::CancelDelete);
    assert!(projects.asking_delete().is_none());
    projects.act(Act::Delete(named.project_id.clone()));
    projects.act(Act::ConfirmDelete);
    wait(&mut projects);
    assert_eq!(projects.lines().len(), 1);
    assert_eq!(projects.lines()[0].project_id, first.project_id);
    let folder = qnc_registry::project(&qnc_registry::reader(&root).unwrap(), &first.project_id).unwrap().unwrap().local_path;
    let _ = qnc_folder_lock::unlock(&folder, &qnc_project_make::CACHE_FOLDERS);
    let _ = std::fs::remove_dir_all(&root);
}
