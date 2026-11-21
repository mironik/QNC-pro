//! The replies of the registry worker applied to the state of the form.

use std::path::Path;

use qnc_project_application::{ProjectsState, Reply, Request, StorySheet, TemplatesState};

use crate::{db_error, ProjectSession};

impl ProjectSession {
    /// The replies of the registry that are there (its work runs in the background).
    /// Returns whether one came.
    pub fn poll(&mut self) -> bool {
        let replies = self.component.poll();
        let mut came = !replies.is_empty();
        for reply in replies {
            self.apply_reply(reply);
        }
        for (target, listing) in self.component.poll_browsers() {
            self.apply_browser_listing(target, listing);
            came = true;
        }
        came
    }

    fn apply_reply(&mut self, reply: Reply) {
        match reply {
            Reply::Projects(Ok(state)) => self.apply_projects_state(state),
            Reply::Templates(Ok(state)) => self.apply_templates_state(state),
            Reply::Created(Ok((project, state))) => {
                self.project_name.clear();
                self.sheet = None;
                self.status = qnc_text::fill("project.msg.opened", &[("name", &project.name)]);
                self.apply_projects_state(state);
            }
            Reply::Opened(Ok(state)) => {
                self.status = self.ready_text.clone();
                self.apply_projects_state(state);
            }
            Reply::Deleted(Ok((project, state))) => {
                self.delete_candidate = None;
                self.status = qnc_text::fill("project.msg.removed", &[("v0", &project.name)]);
                self.apply_projects_state(state);
            }
            Reply::TemplateSelected(Ok(state)) => {
                self.apply_templates_state(state);
                self.picker_open = false;
                self.status = qnc_text::fill("project.msg.template_chosen", &[("name", &self.selected_template_name())]);
            }
            Reply::TemplateCreated(Ok((template, state))) => {
                self.selected_template_id = template.template_id;
                self.template_create_open = false;
                self.clear_template_draft();
                self.apply_templates_state(state);
                self.status = qnc_text::fill("project.msg.template_chosen", &[("name", &self.selected_template_name())]);
            }
            Reply::TemplateDeleted { name, result } => {
                self.delete_template_candidate = None;
                match result {
                    Ok(state) => {
                        self.apply_templates_state(state);
                        self.status = qnc_text::fill("project.msg.template_deleted", &[("v0", &name)]);
                    }
                    Err(error) => self.status = db_error(error),
                }
            }
            Reply::Story { project_id, saved, result } => self.apply_story(&project_id, saved, result),
            Reply::Created(Err(error)) => {
                // A project whose sheet was not kept is there all the same: the list shows it.
                self.sheet_message(db_error(error));
                self.component.request(Request::LoadProjects);
            }
            Reply::ProjectsRoot(Ok(projects_root)) => self.projects_root_set(&projects_root),
            Reply::Projects(Err(error)) | Reply::Templates(Err(error)) | Reply::Opened(Err(error)) | Reply::Deleted(Err(error)) | Reply::TemplateSelected(Err(error)) | Reply::TemplateCreated(Err(error)) | Reply::ProjectsRoot(Err(error)) => self.status = db_error(error),
        }
    }

    fn apply_story(&mut self, project_id: &str, saved: bool, result: Result<StorySheet, String>) {
        match result {
            Ok(sheet) if self.story_project.as_deref() == Some(project_id) => {
                self.story_kept = Some(sheet);
                if saved {
                    self.sheet = None;
                    self.status = qnc_text::text("project.msg.story_saved").to_string();
                    // The list searches the new values.
                    self.component.request(Request::LoadProjects);
                }
            }
            Ok(_) => {}
            Err(error) => self.sheet_message(db_error(error)),
        }
    }

    fn apply_projects_state(&mut self, state: ProjectsState) {
        self.selected_project = state.selected_project;
        self.projects = state.projects;
        let open = self.projects.iter().find(|project| project.active).map(|project| project.project_id.clone());
        if open != self.story_project {
            self.story_project = open.clone();
            self.story_kept = None;
            if let Some(project_id) = open {
                self.component.request(Request::LoadStory { project_id });
            }
        }
        if !self.projects_root.dirty {
            self.projects_root.text = qnc_dir_browser::display_private_path(Path::new(&state.projects_root));
        }
    }

    fn apply_templates_state(&mut self, state: TemplatesState) {
        self.component.applications.set_settings(&state.draft_settings);
        self.selected_template_id = state.selected_template_id;
        self.templates = state.templates;
        self.draft_settings = state.draft_settings;
        self.projects_root.dirty = false;
        self.export_dir.dirty = false;
    }
}
