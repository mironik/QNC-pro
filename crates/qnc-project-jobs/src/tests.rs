use super::*;
use std::time::{Duration, Instant};

fn answers_of(jobs: &mut StoreJobs<Result<Store, String>, Answer>, count: usize) -> Vec<Answer> {
    let mut answers = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while answers.len() < count && Instant::now() < deadline {
        answers.extend(jobs.poll());
        std::thread::sleep(Duration::from_millis(5));
    }
    answers
}

#[test]
fn the_registry_answers_from_the_worker_in_the_order_asked() {
    let root = std::env::temp_dir().join(format!("qnc_project_jobs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut jobs = start(root.clone());
    for request in [Request::LoadProjects, Request::LoadTemplates, Request::Open { project_id: "missing".into() }] {
        assert!(jobs.submit(move |store| run(store, request)));
    }
    let answers = answers_of(&mut jobs, 3);
    assert_eq!(answers.len(), 3);
    assert!(matches!(&answers[0].0, Reply::Projects(Ok(state)) if state.projects.is_empty()));
    assert!(matches!(&answers[1].0, Reply::Templates(Ok(state)) if !state.templates.is_empty()), "the system templates are seeded");
    assert!(matches!(&answers[2], (Reply::Opened(Err(_)), None)), "a project that is not there opens nothing");
    drop(jobs);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_new_project_keeps_its_story_sheet_in_its_own_database_and_the_list_finds_it() {
    let root = std::env::temp_dir().join(format!("qnc_project_jobs_sheet_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut jobs = start(root.clone());
    jobs.submit(|store| run(store, Request::LoadTemplates));
    let Reply::Templates(Ok(templates)) = answers_of(&mut jobs, 1).remove(0).0 else { panic!("templates") };
    let (template, settings) = (templates.templates[0].clone(), templates.draft_settings);
    let sheet = StorySheet {
        sheet: serde_json::json!({ "sheet_id": "minimal", "name": "Minimal", "fields": [] }),
        values: [("slug".to_string(), serde_json::json!("TRZNICA"))].into_iter().collect(),
    };
    let create = Request::Create { name: "Prilog".into(), template_id: template.template_id, settings, selection: qnc_project_defaults::project_only(), sheet: sheet.clone() };
    jobs.submit(move |store| run(store, create));
    let (reply, navigation) = answers_of(&mut jobs, 1).remove(0);
    let project = match reply {
        Reply::Created(Ok((project, _))) => project,
        Reply::Created(Err(error)) => panic!("created: {error}"),
        _ => panic!("created"),
    };
    assert_eq!(navigation.as_ref().map(|(_, steps)| steps.as_ref().unwrap()[0].tab_id.as_str()), Some("project"));
    let project_id = project.project_id.clone();
    jobs.submit(move |store| run(store, Request::LoadStory { project_id }));
    let Reply::Story { result, .. } = answers_of(&mut jobs, 1).remove(0).0 else { panic!("story") };
    assert_eq!(result.expect("sheet"), sheet);
    jobs.submit(|store| run(store, Request::LoadProjects));
    let Reply::Projects(Ok(state)) = answers_of(&mut jobs, 1).remove(0).0 else { panic!("projects") };
    assert!(state.projects[0].matches("trznica prilog"), "the list finds the project by its sheet");
    assert!(!state.projects[0].matches("nema"));
    let project_id = project.project_id.clone();
    jobs.submit(move |store| run(store, Request::Delete { project_id }));
    assert!(matches!(answers_of(&mut jobs, 1).remove(0).0, Reply::Deleted(Ok((row, state))) if row.name == "Prilog" && state.projects.is_empty()));
    drop(jobs);
    let _ = std::fs::remove_dir_all(root);
}
