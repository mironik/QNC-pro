//! The state of the Project form and everything its panels and keys ask for (copied from the
//! old QNC's `qnc-project-session`): forms are boards of pieces, the work lives in public
//! pieces. Panels only draw and return [`ProjectIntent`]s; [`ProjectSession::dispatch`]
//! carries them out through the Project component, the owner of the registry worker.

mod intents;
mod locations;
mod replies;

use std::collections::BTreeSet;

use qnc_newsroom_sheet_model::{SheetOverlay, SheetSpec};
pub use qnc_newsroom_sheet_model::Answer as SheetAnswer;
use qnc_project_application::{ProjectComponent, ProjectRow, Request, StorySheet, TemplateRow};
use serde_json::Value;

pub use intents::{ProjectIntent, ACTION_IDS};
pub use qnc_location_field::{Answer as LocationAnswer, FieldStep, LocationField, SourceKind as LocationSourceKind};
pub use qnc_project_application::ProjectBrowserTarget as LocationTarget;

pub struct ProjectSession {
    pub component: ProjectComponent,
    pub project_name: String,
    /// Where new projects go and where exports go (piece `qnc-location-field`).
    pub projects_root: LocationField,
    pub export_dir: LocationField,
    pub picker_open: bool,
    pub template_create_open: bool,
    pub advanced_open: bool,
    pub selected_project: Option<usize>,
    pub delete_candidate: Option<ProjectRow>,
    pub delete_template_candidate: Option<TemplateRow>,
    pub projects: Vec<ProjectRow>,
    pub selected_template_id: String,
    pub templates: Vec<TemplateRow>,
    pub draft_settings: Value,
    pub template_draft_name: String,
    pub template_draft_description: String,
    pub export_preset_draft_name: String,
    /// The story sheet over the form (piece `qnc-newsroom-sheet`): of a new project (its name is
    /// typed in it) or of the open one; none while it is closed.
    pub sheet: Option<SheetOverlay>,
    /// The open project and its story sheet as kept in its database.
    pub story_project: Option<String>,
    story_kept: Option<StorySheet>,
    /// What the project list is searched by (name, date, story sheet values).
    pub project_search: String,
    pub status: String,
    pub(crate) ready_text: String,
    known_actions: BTreeSet<String>,
}

impl ProjectSession {
    /// `ready_text` is the list status of the layout contract; `known_actions` the Project
    /// action ids of the keyboard catalog (an intent outside it is refused).
    pub fn new(component: ProjectComponent, ready_text: String, known_actions: BTreeSet<String>) -> Self {
        let mut session = Self {
            component,
            project_name: String::new(),
            projects_root: LocationField::new(String::new()),
            export_dir: LocationField::new("exports/projekti"),
            picker_open: false,
            template_create_open: false,
            advanced_open: false,
            selected_project: None,
            delete_candidate: None,
            delete_template_candidate: None,
            projects: Vec::new(),
            selected_template_id: String::new(),
            templates: Vec::new(),
            draft_settings: Value::Object(Default::default()),
            template_draft_name: String::new(),
            template_draft_description: String::new(),
            export_preset_draft_name: String::new(),
            sheet: None,
            story_project: None,
            story_kept: None,
            project_search: String::new(),
            status: ready_text.clone(),
            ready_text,
            known_actions,
        };
        // Shown once the registry answers (piece `qnc-project-jobs`).
        session.component.request(Request::LoadProjects);
        session.component.request(Request::LoadTemplates);
        session
    }

    /// The rows of the project list its search finds (all without a search).
    pub fn shown_projects(&self) -> Vec<usize> {
        (0..self.projects.len()).filter(|&index| self.projects[index].matches(&self.project_search)).collect()
    }

    pub fn footer_status(&self) -> &str {
        qnc_project_application::selected_project_label(&self.projects, self.selected_project)
    }

    pub fn take_navigation_trigger(&mut self) -> bool {
        self.component.take_navigation_trigger()
    }

    pub fn navigation_sequence(&self) -> Result<Vec<qnc_project_application::NavigationStep>, String> {
        self.component.navigation_sequence()
    }

    /// A dialog, picker or browser holds the keys.
    pub fn keys_held(&self) -> bool {
        self.delete_candidate.is_some() || self.delete_template_candidate.is_some() || self.picker_open || self.projects_root.open || self.export_dir.open || self.template_create_open || self.sheet.is_some()
    }

    /// Whether the registry is still working on a request.
    pub fn busy(&self) -> bool {
        self.component.busy()
    }

    pub fn selected_template_name(&self) -> String {
        self.templates.iter().find(|template| template.template_id == self.selected_template_id).map(|template| template.name.clone()).unwrap_or_else(|| "—".to_string())
    }

    /// Carries out one intent of a Project panel or key.
    pub fn dispatch(&mut self, action: ProjectIntent) {
        let action_id = action.action_id();
        if !self.known_actions.contains(action_id) {
            self.status = qnc_text::fill("project.msg.unknown_action", &[("action", action_id)]);
            return;
        }
        match action {
            ProjectIntent::SelectWorkflowGroup { priority_group, application_id } => {
                self.status = match self.component.applications.choose(&priority_group, application_id.as_deref()) {
                    Ok(()) => qnc_text::text("project.settings_changed").into(),
                    Err(error) => error,
                };
            }
            ProjectIntent::OpenSelectedProject => {
                if let Some(index) = self.selected_project {
                    self.open_project(index);
                }
            }
            ProjectIntent::OpenProjectRow { index } => self.open_project(index),
            ProjectIntent::CreateProject => {
                let confirm = qnc_text::text("project.create_project").to_string();
                self.sheet = Some(SheetOverlay::new(&self.component.root, self.project_name.clone(), true, Value::Null, None, confirm));
            }
            ProjectIntent::OpenStorySheet => self.open_story_sheet(),
            ProjectIntent::AnswerStorySheet { answer: SheetAnswer::Cancel } => self.sheet = None,
            ProjectIntent::AnswerStorySheet { answer: SheetAnswer::Confirm } => self.confirm_story_sheet(),
            ProjectIntent::RequestDeleteProject { index } => {
                if let Some(project) = self.projects.get(index).cloned() {
                    self.selected_project = Some(index);
                    self.delete_candidate = Some(project);
                    self.status = qnc_text::text("project.msg.confirm_remove").to_string();
                }
            }
            ProjectIntent::CancelDeleteProject => {
                self.delete_candidate = None;
                self.status = qnc_text::text("project.msg.delete_cancelled").to_string();
            }
            ProjectIntent::ConfirmDeleteProject => {
                if let Some(project) = self.delete_candidate.clone() {
                    self.component.request(Request::Delete { project_id: project.project_id });
                }
            }
            ProjectIntent::SelectTemplate { template_id } => self.component.request(Request::SelectTemplate { template_id }),
            ProjectIntent::BeginTemplateCreate => {
                let selected_name = self.selected_template_name();
                self.template_create_open = true;
                if self.template_draft_name.trim().is_empty() && selected_name != "—" {
                    self.template_draft_name = format!("{selected_name} custom");
                }
            }
            ProjectIntent::CancelTemplateCreate => {
                self.template_create_open = false;
                self.clear_template_draft();
            }
            ProjectIntent::SaveUserTemplate => self.save_custom_template(),
            ProjectIntent::RequestDeleteTemplate { template_id } => {
                self.delete_template_candidate = self.templates.iter().find(|template| template.template_id == template_id).cloned();
            }
            ProjectIntent::CancelDeleteTemplate => {
                self.delete_template_candidate = None;
                self.status = qnc_text::text("project.msg.template_delete_cancelled").to_string();
            }
            ProjectIntent::ConfirmDeleteTemplate => {
                if let Some(template) = self.delete_template_candidate.clone() {
                    self.component.request(Request::DeleteTemplate { template_id: template.template_id, name: template.name });
                }
            }
            ProjectIntent::PickLocation { target } => self.pick_location(target),
            ProjectIntent::AnswerLocation { target, answer } => {
                let step = self.locations_mut(target).0.answer(&answer);
                self.location_step(target, step);
            }
            ProjectIntent::CommitLocation { target } => self.commit_typed(target),
        }
    }

    fn open_project(&mut self, index: usize) {
        if let Some(project) = self.projects.get(index).cloned() {
            self.selected_project = Some(index);
            self.component.request(Request::Open { project_id: project.project_id });
        }
    }

    /// The sheet of the open project as kept in it (or the sheet of this computer, empty).
    fn open_story_sheet(&mut self) {
        let Some(project_id) = self.story_project.clone() else { return };
        let name = self.projects.iter().find(|project| project.project_id == project_id).map(|project| project.name.clone()).unwrap_or_default();
        let kept = self.story_kept.clone().unwrap_or_default();
        let sheet = serde_json::from_value::<SheetSpec>(kept.sheet).ok();
        let values = Value::Object(kept.values.into_iter().collect());
        let confirm = qnc_text::text("sheet.save_values").to_string();
        self.sheet = Some(SheetOverlay::new(&self.component.root, name, false, values, sheet, confirm));
    }

    /// Create (a new project with the name and sheet typed) or Save (the open project's sheet).
    /// The overlay stays open until the registry answers.
    fn confirm_story_sheet(&mut self) {
        let Some(overlay) = self.sheet.as_ref() else { return };
        let Some((sheet, values)) = overlay.filled() else { return };
        let (new_project, name) = (overlay.name_editable, overlay.name.trim().to_string());
        let sheet = StorySheet { sheet: serde_json::to_value(sheet).unwrap_or(Value::Null), values: values.as_object().cloned().unwrap_or_default().into_iter().collect() };
        if !new_project {
            if let Some(project_id) = self.story_project.clone() {
                self.component.request(Request::SaveStory { project_id, sheet });
            }
            return;
        }
        let settings = self.settings_draft_for_save();
        let template_id = self.selected_template_id.clone();
        if let Err(error) = self.component.create_project(&name, &template_id, &settings, sheet) {
            self.sheet_message(db_error(error));
        }
    }

    /// A message in the open sheet, or in the status line when it is closed.
    pub(crate) fn sheet_message(&mut self, message: String) {
        match self.sheet.as_mut() {
            Some(overlay) => overlay.message = message,
            None => self.status = message,
        }
    }

    pub(crate) fn clear_template_draft(&mut self) {
        self.template_draft_name.clear();
        self.template_draft_description.clear();
        self.export_preset_draft_name.clear();
    }

    fn save_custom_template(&mut self) {
        let settings = self.settings_draft_for_save();
        let (name, description) = (self.template_draft_name.clone(), self.template_draft_description.clone());
        let base = self.selected_template_id.clone();
        if let Err(error) = self.component.create_user_template(&name, &description, &base, &settings) {
            self.status = db_error(error);
        }
    }
}

/// The status line of a registry error.
pub(crate) fn db_error(error: String) -> String {
    qnc_text::fill("project.msg.db_error", &[("error", &error)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// The prepared sheets and the application catalog of the repository are the test root's.
    fn with_sheets(root: &std::path::Path) {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::copy(repo.join("data/application-catalog.json"), root.join("data/application-catalog.json")).unwrap();
        let prepared = root.join("catalogs").join("story-sheets");
        std::fs::create_dir_all(&prepared).unwrap();
        let source = repo.join("catalogs/story-sheets");
        for entry in std::fs::read_dir(source).unwrap().flatten() {
            std::fs::copy(entry.path(), prepared.join(entry.file_name())).unwrap();
        }
    }

    fn session(root: &std::path::Path) -> ProjectSession {
        ProjectSession::new(ProjectComponent::new(root), "ready".into(), ACTION_IDS.iter().map(|id| id.to_string()).collect())
    }

    fn settle(session: &mut ProjectSession) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while (session.component.applications.poll() | session.busy()) && Instant::now() < deadline {
            session.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        session.poll();
    }

    #[test]
    fn an_intent_outside_the_catalog_is_refused_and_the_registry_fills_the_state() {
        let root = std::env::temp_dir().join(format!("qnc_project_session_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut session = ProjectSession::new(ProjectComponent::new(&root), "ready".into(), BTreeSet::new());
        session.dispatch(ProjectIntent::CreateProject);
        assert!(session.status.contains("project_create"), "{}", session.status);
        assert!(session.sheet.is_none());
        settle(&mut session);
        assert!(session.projects.is_empty());
        assert!(!session.templates.is_empty(), "the system templates came from the registry");
        assert!(!session.selected_template_id.is_empty());
        drop(session);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_project_is_created_through_the_story_sheet_then_deleted_after_a_question() {
        let root = std::env::temp_dir().join(format!("qnc_project_session_make_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        with_sheets(&root);
        let mut session = session(&root);
        settle(&mut session);
        session.project_name = "Prilog".into();
        session.dispatch(ProjectIntent::CreateProject);
        assert!(session.keys_held(), "the sheet holds the keys");
        session.dispatch(ProjectIntent::AnswerStorySheet { answer: SheetAnswer::Confirm });
        settle(&mut session);
        assert!(session.sheet.is_none(), "{}", session.status);
        assert_eq!(session.projects.len(), 1);
        assert_eq!(session.projects[0].name, "Prilog");
        assert!(session.take_navigation_trigger());
        assert_eq!(session.navigation_sequence().unwrap()[0].tab_id, "project");
        assert_eq!(session.story_project.as_deref(), Some(session.projects[0].project_id.as_str()));
        session.dispatch(ProjectIntent::RequestDeleteProject { index: 0 });
        assert!(session.delete_candidate.is_some());
        session.dispatch(ProjectIntent::CancelDeleteProject);
        assert!(session.delete_candidate.is_none());
        session.dispatch(ProjectIntent::RequestDeleteProject { index: 0 });
        session.dispatch(ProjectIntent::ConfirmDeleteProject);
        settle(&mut session);
        assert!(session.projects.is_empty(), "{}", session.status);
        drop(session);
        let _ = std::fs::remove_dir_all(root);
    }
}
