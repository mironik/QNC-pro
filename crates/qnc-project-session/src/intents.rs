//! What a Project panel or key asks of the session, each with its id in the keyboard catalog.

use crate::{LocationAnswer, LocationTarget, SheetAnswer};

#[derive(Debug, Clone)]
pub enum ProjectIntent {
    SelectWorkflowGroup { priority_group: String, application_id: Option<String> },
    OpenSelectedProject,
    OpenProjectRow { index: usize },
    /// Opens the story sheet of a new project (its name is typed there).
    CreateProject,
    /// Opens the story sheet of the open project.
    OpenStorySheet,
    /// What the user did with the story sheet: Create or Save, or Cancel.
    AnswerStorySheet { answer: SheetAnswer },
    RequestDeleteProject { index: usize },
    CancelDeleteProject,
    ConfirmDeleteProject,
    SelectTemplate { template_id: String },
    BeginTemplateCreate,
    CancelTemplateCreate,
    SaveUserTemplate,
    RequestDeleteTemplate { template_id: String },
    CancelDeleteTemplate,
    ConfirmDeleteTemplate,
    /// Opens or closes the browser of a location field.
    PickLocation { target: LocationTarget },
    /// What the user asked of the open browser of a location field.
    AnswerLocation { target: LocationTarget, answer: LocationAnswer },
    /// The path typed into a location field is kept (Enter, or the field left).
    CommitLocation { target: LocationTarget },
}

impl ProjectIntent {
    pub fn action_id(&self) -> &'static str {
        match self {
            Self::SelectWorkflowGroup { .. } => "project_workflow_group_select",
            Self::OpenSelectedProject => "project_open_selected",
            Self::OpenProjectRow { .. } => "project_open_row",
            Self::CreateProject => "project_create",
            Self::OpenStorySheet => "project_story_open",
            Self::AnswerStorySheet { answer: SheetAnswer::Confirm } => "project_story_confirm",
            Self::AnswerStorySheet { answer: SheetAnswer::Cancel } => "project_story_cancel",
            Self::RequestDeleteProject { .. } => "project_delete_request",
            Self::CancelDeleteProject => "project_delete_cancel",
            Self::ConfirmDeleteProject => "project_delete_confirm",
            Self::SelectTemplate { .. } => "project_template_select",
            Self::BeginTemplateCreate => "project_template_create_begin",
            Self::CancelTemplateCreate => "project_template_create_cancel",
            Self::SaveUserTemplate => "project_template_save",
            Self::RequestDeleteTemplate { .. } => "project_template_delete_request",
            Self::CancelDeleteTemplate => "project_template_delete_cancel",
            Self::ConfirmDeleteTemplate => "project_template_delete_confirm",
            Self::PickLocation { target } | Self::CommitLocation { target } => location_action(*target, "pick"),
            Self::AnswerLocation { target, answer } => location_action(*target, match answer {
                LocationAnswer::Browser(qnc_dir_browser::BrowserAction::Kind(_)) => "browser_select_source",
                LocationAnswer::Browser(_) => "browser_open_path",
                LocationAnswer::Confirm => "browser_confirm",
                LocationAnswer::Cancel => "browser_cancel",
            }),
        }
    }
}

/// The catalog action of a location field's `what`, by the field.
fn location_action(target: LocationTarget, what: &str) -> &'static str {
    match (target, what) {
        (LocationTarget::ProjectsRoot, "pick") => "project_pick_projects_root",
        (LocationTarget::ProjectsRoot, "browser_select_source") => "project_projects_root_browser_select_source",
        (LocationTarget::ProjectsRoot, "browser_open_path") => "project_projects_root_browser_open_path",
        (LocationTarget::ProjectsRoot, "browser_confirm") => "project_projects_root_browser_confirm",
        (LocationTarget::ProjectsRoot, _) => "project_projects_root_browser_cancel",
        (LocationTarget::ExportDir, "pick") => "project_pick_export_dir",
        (LocationTarget::ExportDir, "browser_select_source") => "project_export_dir_browser_select_source",
        (LocationTarget::ExportDir, "browser_open_path") => "project_export_dir_browser_open_path",
        (LocationTarget::ExportDir, "browser_confirm") => "project_export_dir_browser_confirm",
        (LocationTarget::ExportDir, _) => "project_export_dir_browser_cancel",
    }
}

/// Every action id an intent can carry (the form checks them against the catalog).
pub const ACTION_IDS: &[&str] = &[
    "project_workflow_group_select",
    "project_open_selected",
    "project_open_row",
    "project_create",
    "project_story_open",
    "project_story_confirm",
    "project_story_cancel",
    "project_delete_request",
    "project_delete_cancel",
    "project_delete_confirm",
    "project_template_select",
    "project_template_create_begin",
    "project_template_create_cancel",
    "project_template_save",
    "project_template_delete_request",
    "project_template_delete_cancel",
    "project_template_delete_confirm",
    "project_pick_projects_root",
    "project_projects_root_browser_select_source",
    "project_projects_root_browser_open_path",
    "project_projects_root_browser_confirm",
    "project_projects_root_browser_cancel",
    "project_pick_export_dir",
    "project_export_dir_browser_select_source",
    "project_export_dir_browser_open_path",
    "project_export_dir_browser_confirm",
    "project_export_dir_browser_cancel",
];
