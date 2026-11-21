//! The work of the Project form on the registry and the project folders (copied from the old
//! QNC's `qnc-project-jobs`): each request runs in one background worker (`qnc-store-jobs`), in
//! the order sent, and answers with the state the form shows; the form thread never waits for
//! the database. Opening or creating a project also reads its application sequence there, for
//! the desktop to move on. The work itself is the blocks' (make, open, delete, templates,
//! story sheet); this block only calls them in order.

mod run;

use std::path::PathBuf;

use qnc_store_jobs::StoreJobs;
pub use qnc_newsroom_sheet_db::StorySheet;
pub use qnc_project_open::NavigationStep;
pub use qnc_project_templates::TemplateRow;
pub use qnc_registry::ProjectRow;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ProjectsState {
    pub projects: Vec<ProjectRow>,
    pub selected_project: Option<usize>,
    /// Where new projects go, as shown.
    pub projects_root: String,
}

#[derive(Debug, Clone)]
pub struct TemplatesState {
    pub templates: Vec<TemplateRow>,
    pub selected_template_id: String,
    pub draft_settings: Value,
}

/// What the form asks of the registry.
pub enum Request {
    LoadProjects,
    LoadTemplates,
    /// A new project; its story sheet, filled with the name, is kept in its database.
    Create { name: String, template_id: String, settings: Value, selection: Value, sheet: StorySheet },
    /// The story sheet of a project, to show it.
    LoadStory { project_id: String },
    /// Keeps the story sheet of a project.
    SaveStory { project_id: String, sheet: StorySheet },
    Open { project_id: String },
    Delete { project_id: String },
    SelectTemplate { template_id: String },
    CreateTemplate { name: String, description: String, base_template_id: String, settings: Value },
    DeleteTemplate { template_id: String, name: String },
    SetProjectsRoot { path: PathBuf },
}

/// What came of a request.
pub enum Reply {
    Projects(Result<ProjectsState, String>),
    Templates(Result<TemplatesState, String>),
    Created(Result<(ProjectRow, ProjectsState), String>),
    Opened(Result<ProjectsState, String>),
    Deleted(Result<(ProjectRow, ProjectsState), String>),
    TemplateSelected(Result<TemplatesState, String>),
    TemplateCreated(Result<(TemplateRow, TemplatesState), String>),
    TemplateDeleted { name: String, result: Result<TemplatesState, String> },
    ProjectsRoot(Result<String, String>),
    /// The story sheet of a project, read or kept.
    Story { project_id: String, saved: bool, result: Result<StorySheet, String> },
}

/// A reply and, for an opened or created project, its id and application sequence.
pub type Answer = (Reply, Option<(String, Result<Vec<NavigationStep>, String>)>);

/// The registry of the QNC root as the worker holds it: its writer (with the template tables)
/// and the root.
pub struct Store {
    root: PathBuf,
    writer: qnc_db::Writer,
}

impl Store {
    pub fn open(root: &std::path::Path) -> Result<Store, String> {
        Ok(Store { root: root.to_path_buf(), writer: qnc_project_templates::writer(root)? })
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}

/// The worker of the registry of `root` (opened in the worker).
pub fn start(root: PathBuf) -> StoreJobs<Result<Store, String>, Answer> {
    StoreJobs::start("qnc-project-jobs", move || Store::open(&root))
}

/// Runs a request on the store (in the worker).
pub fn run(store: &mut Result<Store, String>, request: Request) -> Answer {
    match store {
        Ok(store) => run::run(store, request),
        Err(error) => (run::fail(request, error.clone()), None),
    }
}

#[cfg(test)]
mod tests;
