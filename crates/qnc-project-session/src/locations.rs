//! The two location fields of the form (the projects' folder, the export folder): their
//! browsers run in the background, a confirmed folder becomes the projects root (through
//! the registry) or the export folder of the draft.

use std::path::{Path, PathBuf};

use qnc_dir_browser::BrowserState;
use qnc_project_application::Request;
use serde_json::Value;

use crate::{FieldStep, LocationField, LocationTarget, ProjectSession};

impl ProjectSession {
    /// The field `target` and the other one (only one browser is open at a time).
    pub(crate) fn locations_mut(&mut self, target: LocationTarget) -> (&mut LocationField, &mut LocationField) {
        match target {
            LocationTarget::ProjectsRoot => (&mut self.projects_root, &mut self.export_dir),
            LocationTarget::ExportDir => (&mut self.export_dir, &mut self.projects_root),
        }
    }

    pub(crate) fn pick_location(&mut self, target: LocationTarget) {
        let (field, other) = self.locations_mut(target);
        other.open = false;
        let step = field.toggle(other);
        self.location_step(target, step);
    }

    /// What a location field asks: a browser step runs in the background (its listing comes
    /// with `poll`); a confirmed folder becomes the projects root or the export folder.
    pub(crate) fn location_step(&mut self, target: LocationTarget, step: FieldStep) {
        let uri = match step {
            FieldStep::Nothing => return,
            FieldStep::Browse(step) => return self.component.browse(target, step),
            FieldStep::Confirm(uri) => uri,
        };
        let Some(path) = self.component.browser_private_path_for_uri(target, &uri) else {
            let missing = if target == LocationTarget::ProjectsRoot { "project.msg.browser_no_project_path" } else { "project.msg.browser_no_export_path" };
            self.status = qnc_text::text(missing).to_string();
            return;
        };
        let shown = qnc_dir_browser::display_private_path(&path);
        if shown.trim().is_empty() {
            return;
        }
        self.keep_location(target, path, shown);
    }

    /// A path typed into a location field is kept as one chosen in its browser: the projects'
    /// folder through the registry, the export folder in the draft.
    pub(crate) fn commit_typed(&mut self, target: LocationTarget) {
        let (field, _) = self.locations_mut(target);
        if !field.dirty {
            return;
        }
        field.dirty = false;
        let text = field.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.keep_location(target, PathBuf::from(&text), text);
    }

    fn keep_location(&mut self, target: LocationTarget, path: PathBuf, shown: String) {
        match target {
            LocationTarget::ProjectsRoot => self.component.request(Request::SetProjectsRoot { path }),
            LocationTarget::ExportDir => {
                self.export_dir.chosen(shown);
                qnc_settings_path::set_string_path(&mut self.draft_settings, "export.directory", self.export_dir.text.clone());
                self.status = qnc_text::text("project.msg.export_dir_set").to_string();
            }
        }
    }

    /// The path of a location field was typed: it goes into the draft of the new project.
    pub fn location_typed(&mut self, target: LocationTarget) {
        let (field, _) = self.locations_mut(target);
        field.dirty = true;
        let text = field.text.clone();
        let path = if target == LocationTarget::ProjectsRoot { "storage.projects_root" } else { "export.directory" };
        qnc_settings_path::set_string_path(&mut self.draft_settings, path, text);
    }

    pub(crate) fn apply_browser_listing(&mut self, target: LocationTarget, listing: Result<BrowserState, String>) {
        self.locations_mut(target).0.apply_listing(listing);
    }

    pub(crate) fn projects_root_set(&mut self, projects_root: &str) {
        self.projects_root.chosen(qnc_dir_browser::display_private_path(Path::new(projects_root)));
        qnc_settings_path::set_string_path(&mut self.draft_settings, "storage.projects_root", self.projects_root.text.clone());
        self.status = qnc_text::text("project.msg.projects_root_set").to_string();
    }

    pub fn settings_draft_for_save(&self) -> Value {
        let mut settings = self.draft_settings.clone();
        qnc_settings_path::set_string_path(&mut settings, "storage.projects_root", self.projects_root.text.clone());
        if self.export_dir.dirty && !self.export_dir.text.trim().is_empty() {
            qnc_settings_path::set_string_path(&mut settings, "export.directory", self.export_dir.text.clone());
        }
        settings
    }
}
