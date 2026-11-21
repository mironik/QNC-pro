//! One request carried out on the registry, in the worker.

use std::path::Path;

use qnc_db::{Reader, Writer};
use serde_json::{json, Value};

use crate::{Answer, ProjectsState, Reply, Request, Store, StorySheet, TemplatesState};

pub(crate) fn run(store: &mut Store, request: Request) -> Answer {
    match request {
        Request::LoadProjects => (Reply::Projects(projects(store)), None),
        Request::LoadTemplates => (Reply::Templates(templates(store)), None),
        Request::Create { name, template_id, settings, selection, sheet } => {
            // The sheet goes into the database the new project has; a project whose sheet
            // could not be kept is still there, and says so.
            let ask = qnc_project_make::Make { name, template_id, settings: Some(settings), selection: Some(selection), projects_root: None };
            let created = qnc_project_make::make(&store.root, ask).and_then(|made| {
                save_sheet(store, &made.project_id, sheet).map_err(|error| qnc_text::fill("project.msg.sheet_not_kept", &[("error", &error)]))?;
                let state = projects(store)?;
                let row = state.projects.iter().find(|row| row.project_id == made.project_id).cloned().ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", &made.project_id)]))?;
                Ok((row, state))
            });
            let navigation = created.as_ref().ok().map(|(project, _)| (project.project_id.clone(), qnc_project_open::navigation_sequence(&store.root, &project.project_id)));
            (Reply::Created(created), navigation)
        }
        Request::LoadStory { project_id } => {
            let result = project_db(&store.root, &project_id).and_then(|db| qnc_newsroom_sheet_db::read(&Reader::open(&db)?));
            (Reply::Story { project_id, saved: false, result }, None)
        }
        Request::SaveStory { project_id, sheet } => {
            let result = save_sheet(store, &project_id, sheet.clone()).map(|()| sheet);
            (Reply::Story { project_id, saved: true, result }, None)
        }
        Request::Open { project_id } => {
            let opened = qnc_project_open::open(&store.root, &project_id);
            let navigation = opened.as_ref().ok().map(|steps| (project_id.clone(), Ok(steps.clone())));
            (Reply::Opened(opened.and_then(|_| projects(store))), navigation)
        }
        Request::Delete { project_id } => {
            let deleted = projects(store).and_then(|before| {
                let row = before.projects.iter().find(|row| row.project_id == project_id).cloned().ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", &project_id)]))?;
                qnc_project_delete::delete(&store.root, &project_id)?;
                Ok((row, projects(store)?))
            });
            (Reply::Deleted(deleted), None)
        }
        Request::SelectTemplate { template_id } => {
            let selected = reader(store).and_then(|reader| qnc_project_templates::select(&store.writer, &reader, &template_id)).and_then(|()| templates(store));
            (Reply::TemplateSelected(selected), None)
        }
        Request::CreateTemplate { name, description, base_template_id, settings } => {
            let created = reader(store)
                .and_then(|reader| qnc_project_templates::create_user(&store.writer, &reader, &name, &description, &base_template_id, Some(&settings)))
                .and_then(|template| Ok((template, templates(store)?)));
            (Reply::TemplateCreated(created), None)
        }
        Request::DeleteTemplate { template_id, name } => {
            let result = reader(store).and_then(|reader| qnc_project_templates::delete_user(&store.writer, &reader, &template_id)).and_then(|()| templates(store));
            (Reply::TemplateDeleted { name, result }, None)
        }
        Request::SetProjectsRoot { path } => (Reply::ProjectsRoot(set_projects_root(store, &path)), None),
    }
}

/// The reply of a request when the store could not be opened.
pub(crate) fn fail(request: Request, error: String) -> Reply {
    match request {
        Request::LoadProjects => Reply::Projects(Err(error)),
        Request::LoadTemplates => Reply::Templates(Err(error)),
        Request::Create { .. } => Reply::Created(Err(error)),
        Request::LoadStory { project_id } => Reply::Story { project_id, saved: false, result: Err(error) },
        Request::SaveStory { project_id, .. } => Reply::Story { project_id, saved: true, result: Err(error) },
        Request::Open { .. } => Reply::Opened(Err(error)),
        Request::Delete { .. } => Reply::Deleted(Err(error)),
        Request::SelectTemplate { .. } => Reply::TemplateSelected(Err(error)),
        Request::CreateTemplate { .. } => Reply::TemplateCreated(Err(error)),
        Request::DeleteTemplate { name, .. } => Reply::TemplateDeleted { name, result: Err(error) },
        Request::SetProjectsRoot { .. } => Reply::ProjectsRoot(Err(error)),
    }
}

fn reader(store: &Store) -> Result<Reader, String> {
    qnc_registry::reader(&store.root)
}

/// The database of a registered project.
fn project_db(root: &Path, project_id: &str) -> Result<std::path::PathBuf, String> {
    let reader = qnc_registry::reader(root)?;
    let registered = qnc_registry::project(&reader, project_id)?.ok_or_else(|| qnc_text::fill("project.store.project_missing", &[("id", project_id)]))?;
    let db = registered.project_db();
    if !db.is_file() {
        return Err(qnc_text::fill("project.store.folder_invalid", &[("v0", &registered.local_path.display().to_string())]));
    }
    Ok(db)
}

/// Keeps the sheet in the project's database and its values as what the project list searches.
fn save_sheet(store: &Store, project_id: &str, sheet: StorySheet) -> Result<(), String> {
    let search = sheet.search_text();
    let db = project_db(&store.root, project_id)?;
    let writer = Writer::start(&db, vec![Box::new(qnc_newsroom_sheet_db::StorySheetTables)])?;
    qnc_newsroom_sheet_db::save(&writer, sheet)?;
    drop(writer);
    store.writer.execute(qnc_registry::MODULE, json!({ "search": { "project_id": project_id, "text": search } })).map(drop)
}

/// This computer's projects' folder is `path` from now on.
fn set_projects_root(store: &Store, path: &Path) -> Result<String, String> {
    if path.as_os_str().is_empty() {
        return Err(qnc_text::text("project.store.root_empty").into());
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    let shown = path.to_string_lossy().to_string();
    store.writer.execute(qnc_registry::MODULE, json!({ "local": { "key": qnc_registry::PROJECTS_ROOT_KEY, "value": shown } }))?;
    Ok(shown)
}

fn projects(store: &Store) -> Result<ProjectsState, String> {
    let projects = qnc_registry::rows(&reader(store)?)?;
    let selected_project = projects.iter().position(|project| project.active);
    Ok(ProjectsState { projects, selected_project, projects_root: qnc_project_make::projects_root(&store.root)?.to_string_lossy().to_string() })
}

fn templates(store: &Store) -> Result<TemplatesState, String> {
    let reader = reader(store)?;
    let templates = qnc_project_templates::list(&reader)?;
    let selected_template_id = templates.iter().find(|template| template.selected).or_else(|| templates.first()).map(|template| template.template_id.clone()).unwrap_or_default();
    let draft_settings = if selected_template_id.trim().is_empty() { Value::Object(Default::default()) } else { qnc_project_templates::selected(&reader)?.settings };
    Ok(TemplatesState { templates, selected_template_id, draft_settings })
}
