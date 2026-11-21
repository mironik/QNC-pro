//! The applications a project works with, as the Project form chooses them (copied from the
//! old QNC's `qnc-application-selection`): the catalog of this computer read in the background
//! at start, the applications a template's settings name (`workspace.application_selection`,
//! else `workspace.tabs`), one choice per priority group (group `a` always has one), the view
//! the form draws (groups, choices, no-selection) and the snapshot a project is made with.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};

use qnc_application_catalog::{self as catalog, ApplicationGroup, Catalog, SelectionSnapshot};
use serde_json::Value;

#[derive(Clone, Default)]
pub struct ApplicationSelectionView {
    pub groups: Vec<ApplicationGroup>,
    pub required_groups: Vec<String>,
    pub error: Option<String>,
    pub loading: bool,
}

pub struct ApplicationSelection {
    file: std::path::PathBuf,
    pending: Option<Receiver<Result<Catalog, String>>>,
    catalog: Option<Catalog>,
    ids: Vec<String>,
    initial_settings: Option<Value>,
    error: Option<String>,
}

impl ApplicationSelection {
    /// Starts reading the catalog of the QNC `root` in the background.
    pub fn new(root: &Path) -> Self {
        let mut result = Self { file: catalog::file(root), pending: None, catalog: None, ids: vec![], initial_settings: None, error: None };
        result.load_at_startup();
        result
    }

    fn load_at_startup(&mut self) {
        if self.pending.is_some() {
            return;
        }
        self.catalog = None;
        self.error = None;
        let (tx, rx) = mpsc::channel();
        let file = self.file.clone();
        std::thread::spawn(move || {
            let _ = tx.send(catalog::read_catalog(&file).map_err(|e| e.to_string()));
        });
        self.pending = Some(rx);
    }

    /// Takes the catalog when it came; `true` while it is still being read.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.pending else { return false };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => Err(qnc_text::text("app.loading_catalog_interrupted").into()),
        };
        self.pending = None;
        match result {
            Ok(catalog) => {
                self.ids.retain(|id| catalog.applications.iter().any(|app| &app.application_id == id));
                self.catalog = Some(catalog);
                if let Some(settings) = self.initial_settings.take() {
                    self.set_settings(&settings);
                }
            }
            Err(error) => self.error = Some(error),
        }
        false
    }

    /// The applications `settings` name become the choice (kept until the catalog is read).
    pub fn set_settings(&mut self, settings: &Value) {
        self.ids.clear();
        let Some(catalog) = &self.catalog else {
            self.initial_settings = Some(settings.clone());
            return;
        };
        self.initial_settings = None;
        self.error = None;
        if let Some(raw) = settings.pointer("/workspace/application_selection") {
            match serde_json::from_value::<SelectionSnapshot>(raw.clone()) {
                Ok(selection) => {
                    self.ids = selection.applications.into_iter().filter(|selected| catalog.applications.iter().any(|app| app.application_id == selected.application_id)).map(|app| app.application_id).collect();
                }
                Err(error) => self.error = Some(qnc_text::fill("app.saved_choice_invalid", &[("error", &error.to_string())])),
            }
        } else if let Some(tabs) = settings.pointer("/workspace/tabs").and_then(Value::as_array) {
            for tab in tabs.iter().filter_map(Value::as_str) {
                if let Some(app) = catalog.applications.iter().find(|app| app.tab_id == tab) {
                    if !self.ids.contains(&app.application_id) {
                        self.ids.push(app.application_id.clone());
                    }
                }
            }
        }
        let required: Vec<_> = catalog.applications.iter().filter(|app| app.priority_group == "a").collect();
        if required.len() == 1 && !self.ids.contains(&required[0].application_id) {
            self.ids.push(required[0].application_id.clone());
        }
    }

    pub fn view(&self) -> ApplicationSelectionView {
        ApplicationSelectionView {
            required_groups: vec!["a".into()],
            groups: self.catalog.as_ref().map(|c| catalog::groups(c, &self.ids)).unwrap_or_default(),
            error: self.error.clone(),
            loading: self.pending.is_some(),
        }
    }

    /// Chooses `id` in `group` (none: the group has no application; never for `a`).
    pub fn choose(&mut self, group: &str, id: Option<&str>) -> Result<(), String> {
        if group == "a" && id.is_none() {
            return Err(qnc_text::text("app.group_a_needs_app").into());
        }
        let catalog = self.catalog.as_ref().ok_or(qnc_text::text("app.catalog_loaded"))?;
        catalog::choose_group(catalog, &mut self.ids, group, id).map_err(|e| e.to_string())
    }

    /// The snapshot a project or template is made with.
    pub fn snapshot(&self) -> Result<SelectionSnapshot, String> {
        let catalog = self.catalog.as_ref().ok_or(qnc_text::text("app.application_catalog_loaded"))?;
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.ids.is_empty() {
            return Err(qnc_text::text("app.choose_at_least_one_application").into());
        }
        let snapshot = SelectionSnapshot::from_catalog(catalog, &self.ids).map_err(|e| e.to_string())?;
        if !snapshot.applications.iter().any(|app| app.priority_group == "a") {
            return Err(qnc_text::text("app.choose_application_from_group").into());
        }
        Ok(snapshot)
    }

    /// The snapshot as the settings keep it.
    pub fn snapshot_value(&self) -> Result<Value, String> {
        serde_json::to_value(self.snapshot()?).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
