//! The Project component (copied from the old QNC's `qnc-project-application`): it owns the
//! background worker of the registry (`qnc-project-jobs`), the choice of applications
//! (`qnc-application-selection`) and the background browsers of the two location fields; the
//! form only shows state and sends requests through it, and takes the replies with `poll`.

use std::path::{Path, PathBuf};

use qnc_application_selection::ApplicationSelection;
use qnc_dir_browse_jobs::BackgroundBrowser;
pub use qnc_dir_browse_jobs::BrowseStep;
use qnc_dir_browser::BrowserState;
pub use qnc_project_jobs::{Answer, NavigationStep, ProjectRow, ProjectsState, Reply, Request, StorySheet, TemplateRow, TemplatesState};
use qnc_store_jobs::StoreJobs;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectBrowserTarget {
    ProjectsRoot,
    ExportDir,
}

pub struct ProjectComponent {
    /// The QNC root: where the story sheets of this computer are.
    pub root: PathBuf,
    pub applications: ApplicationSelection,
    jobs: StoreJobs<Result<qnc_project_jobs::Store, String>, Answer>,
    projects_root_browser: BackgroundBrowser,
    export_dir_browser: BackgroundBrowser,
    navigation: Option<(String, Result<Vec<NavigationStep>, String>)>,
    navigation_pending: bool,
}

impl ProjectComponent {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            applications: ApplicationSelection::new(root),
            jobs: qnc_project_jobs::start(root.to_path_buf()),
            projects_root_browser: BackgroundBrowser::start("qnc-project-root-browser"),
            export_dir_browser: BackgroundBrowser::start("qnc-project-export-browser"),
            navigation: None,
            navigation_pending: false,
        }
    }

    /// Sends a request to the background worker.
    pub fn request(&mut self, request: Request) {
        self.navigation_pending &= !matches!(request, Request::Create { .. } | Request::Open { .. });
        self.jobs.submit(move |store| qnc_project_jobs::run(store, request));
    }

    /// A new project from the template, with the applications chosen now.
    pub fn create_project(&mut self, name: &str, template_id: &str, settings: &Value, sheet: StorySheet) -> Result<(), String> {
        let selection = self.applications.snapshot_value()?;
        self.request(Request::Create { name: name.into(), template_id: template_id.into(), settings: settings.clone(), selection, sheet });
        Ok(())
    }

    /// A user template from the template, with the applications chosen now.
    pub fn create_user_template(&mut self, name: &str, description: &str, base_template_id: &str, settings: &Value) -> Result<(), String> {
        let selection = self.applications.snapshot_value()?;
        let mut settings = settings.clone();
        qnc_project_defaults::apply_application_selection(&mut settings, &selection)?;
        self.request(Request::CreateTemplate { name: name.into(), description: description.into(), base_template_id: base_template_id.into(), settings });
        Ok(())
    }

    /// The replies that are there; an opened or created project arms the desktop's move on.
    pub fn poll(&mut self) -> Vec<Reply> {
        let answers = self.jobs.poll();
        let mut replies = Vec::with_capacity(answers.len());
        for (reply, navigation) in answers {
            let came = navigation.is_some();
            self.navigation = navigation.or(self.navigation.take());
            self.navigation_pending |= came;
            replies.push(reply);
        }
        replies
    }

    /// Whether a request still waits for its reply.
    pub fn busy(&self) -> bool {
        self.jobs.busy() || self.projects_root_browser.busy() || self.export_dir_browser.busy()
    }

    pub fn take_navigation_trigger(&mut self) -> bool {
        std::mem::take(&mut self.navigation_pending)
    }

    /// The applications of the project last opened or created, read with it in the worker.
    pub fn navigation_sequence(&self) -> Result<Vec<NavigationStep>, String> {
        match &self.navigation {
            Some((_, sequence)) => sequence.clone(),
            None => Err(qnc_text::text("project.msg.no_open_project").to_string()),
        }
    }

    /// Sends a step to the browser `target`; its listing comes with `poll_browsers`.
    pub fn browse(&mut self, target: ProjectBrowserTarget, step: BrowseStep) {
        self.browser_mut(target).browse(step);
    }

    /// The listings that came, each with its browser, in the order of their steps.
    pub fn poll_browsers(&mut self) -> Vec<(ProjectBrowserTarget, Result<BrowserState, String>)> {
        let mut listings = Vec::new();
        for target in [ProjectBrowserTarget::ProjectsRoot, ProjectBrowserTarget::ExportDir] {
            listings.extend(self.browser_mut(target).poll().into_iter().map(|listing| (target, listing)));
        }
        listings
    }

    /// The private path of the folder the browser `target` shows, when it is `uri`.
    pub fn browser_private_path_for_uri(&self, target: ProjectBrowserTarget, uri: &str) -> Option<PathBuf> {
        match target {
            ProjectBrowserTarget::ProjectsRoot => self.projects_root_browser.path_for_uri(uri),
            ProjectBrowserTarget::ExportDir => self.export_dir_browser.path_for_uri(uri),
        }
    }

    fn browser_mut(&mut self, target: ProjectBrowserTarget) -> &mut BackgroundBrowser {
        match target {
            ProjectBrowserTarget::ProjectsRoot => &mut self.projects_root_browser,
            ProjectBrowserTarget::ExportDir => &mut self.export_dir_browser,
        }
    }
}

/// Footer/status label for the selected project row.
pub fn selected_project_label(projects: &[ProjectRow], selected: Option<usize>) -> &str {
    selected.and_then(|index| projects.get(index)).map(|project| project.name.as_str()).unwrap_or(qnc_text::text("project.msg.no_project_chosen"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_selection_not_active_flag_and_never_keeps_removed_name() {
        let row = |id: &str, name: &str, active: bool| ProjectRow { project_id: id.into(), name: name.into(), created_date: String::new(), project_uri: format!("qnc://local/project/{id}"), active, search_text: String::new() };
        let mut rows = vec![row("p1", "First project", true), row("p2", "Selected project", false)];
        assert_eq!(selected_project_label(&rows, Some(0)), "First project");
        assert_eq!(selected_project_label(&rows, Some(1)), "Selected project");
        assert_eq!(selected_project_label(&rows, None), qnc_text::text("project.msg.no_project_chosen"));
        rows.clear();
        assert_eq!(selected_project_label(&rows, Some(1)), qnc_text::text("project.msg.no_project_chosen"));
    }
}
