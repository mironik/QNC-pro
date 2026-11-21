//! The projects of this computer, as the Project form works with them. The list is the registry
//! read again every second (the database is the truth; the tray may add a project any time);
//! this module owns only what the form is doing: the draft of a new project, the question before
//! a delete, the one job running in the background (make or delete) and the last status line.
//! Making, protecting and deleting are the blocks' work; this module only calls them in order.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use qnc_project_templates::TemplateRow as Template;

pub const READ_EVERY: Duration = Duration::from_secs(1);

/// One project as the form lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub project_id: String,
    pub name: String,
    pub created: String,
    pub active: bool,
}

/// The draft of a new project, edited by the form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub template_id: String,
}

/// What the form asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Make a new project from the draft.
    New,
    Open(String),
    /// Ask before deleting this project.
    Delete(String),
    ConfirmDelete,
    CancelDelete,
    ChooseTemplate(String),
    /// Opens or closes the template picker.
    TogglePicker,
}

enum Ended {
    Made(String),
    Deleted(String),
    Failed(String),
}

pub struct Projects {
    root: PathBuf,
    projects_root: PathBuf,
    templates: Vec<Template>,
    lines: Vec<Line>,
    pub draft: Draft,
    /// What the list is searched for (name or date); edited by the form.
    pub search: String,
    picker_open: bool,
    asking_delete: Option<String>,
    status: Option<String>,
    job: Option<Receiver<Ended>>,
    last_read: Instant,
    rev: u64,
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Projects {
    /// Opens the projects of the QNC `root`: the templates and the registry are read now.
    pub fn open(root: &Path) -> Result<Projects, String> {
        let projects_root = qnc_project_make::projects_root(root)?;
        let writer = qnc_project_templates::writer(root)?;
        let reader = qnc_registry::reader(root)?;
        let templates = qnc_project_templates::list(&reader)?;
        let draft = Draft { name: String::new(), template_id: qnc_project_templates::selected_id(&reader)? };
        drop((writer, reader));
        let mut projects = Projects { root: root.to_path_buf(), projects_root, templates, lines: Vec::new(), draft, search: String::new(), picker_open: false, asking_delete: None, status: Some(qnc_text::text("project.ready").to_string()), job: None, last_read: Instant::now(), rev: 0 };
        projects.read()?;
        Ok(projects)
    }

    fn read(&mut self) -> Result<(), String> {
        // A computer without a registry yet has no projects; the first one makes the file.
        let lines: Vec<Line> = match qnc_registry::file(&self.root).is_file() {
            false => Vec::new(),
            true => {
                let reader = qnc_registry::reader(&self.root)?;
                let active = qnc_registry::active(&reader)?.map(|a| a.project_id);
                qnc_registry::projects(&reader)?
                    .into_iter()
                    .map(|p| Line { active: active.as_deref() == Some(p.project_id.as_str()), created: p.created_at.as_deref().and_then(epoch_text).unwrap_or_default(), project_id: p.project_id, name: p.name })
                    .collect()
            }
        };
        if lines != self.lines {
            self.lines = lines;
            self.rev += 1;
        }
        self.last_read = Instant::now();
        Ok(())
    }

    /// Reads the registry when the second is up and takes in the job that ended; `true` when
    /// something to show changed. Never waits.
    pub fn poll(&mut self) -> bool {
        let before = self.rev;
        if let Some(ended) = self.job.as_ref().and_then(|j| j.try_recv().ok()) {
            self.job = None;
            self.status = Some(match ended {
                Ended::Made(name) => qnc_text::fill("project.made", &[("name", &name)]),
                Ended::Deleted(name) => qnc_text::fill("project.deleted", &[("name", &name)]),
                Ended::Failed(error) => error,
            });
            self.rev += 1;
            let _ = self.read();
        }
        if self.last_read.elapsed() >= READ_EVERY {
            if let Err(error) = self.read() {
                self.status = Some(error);
                self.rev += 1;
            }
        }
        self.rev != before
    }

    /// The lines the search leaves (name or date, case apart); all when it is empty.
    pub fn shown(&self) -> Vec<&Line> {
        let wanted = self.search.trim().to_lowercase();
        self.lines.iter().filter(|l| wanted.is_empty() || l.name.to_lowercase().contains(&wanted) || l.created.contains(&wanted)).collect()
    }

    pub fn picker_open(&self) -> bool {
        self.picker_open
    }

    /// The template the draft names.
    pub fn chosen_template(&self) -> Option<&Template> {
        self.templates.iter().find(|t| t.template_id == self.draft.template_id)
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// `true` while a job runs in the background.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The project the form must confirm deleting, if asked.
    pub fn asking_delete(&self) -> Option<&Line> {
        self.asking_delete.as_deref().and_then(|id| self.lines.iter().find(|l| l.project_id == id))
    }

    fn start<F: FnOnce() -> Ended + Send + 'static>(&mut self, job: F) {
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tell.send(job());
        });
        self.job = Some(heard);
        self.status = Some(qnc_text::text("project.busy").to_string());
        self.rev += 1;
    }

    /// One action of the form; refused with a status while a job runs.
    pub fn act(&mut self, act: Act) {
        if self.busy() {
            return;
        }
        match act {
            Act::ChooseTemplate(id) => {
                self.draft.template_id = id;
                self.picker_open = false;
            }
            Act::TogglePicker => self.picker_open = !self.picker_open,
            Act::Delete(id) => self.asking_delete = Some(id),
            Act::CancelDelete => self.asking_delete = None,
            Act::ConfirmDelete => {
                let Some(id) = self.asking_delete.take() else { return };
                let root = self.root.clone();
                self.start(move || match qnc_project_delete::delete(&root, &id) {
                    Ok(deleted) => Ended::Deleted(deleted.name),
                    Err(error) => Ended::Failed(error),
                });
            }
            Act::Open(id) => {
                let outcome = qnc_registry::writer(&self.root).and_then(|w| w.execute(qnc_registry::MODULE, serde_json::json!({ "activate": id })));
                self.status = Some(match outcome {
                    Ok(_) => qnc_text::fill("project.opened", &[("name", &self.lines.iter().find(|l| l.project_id == id).map(|l| l.name.clone()).unwrap_or(id))]),
                    Err(error) => error,
                });
                let _ = self.read();
            }
            Act::New => self.make(),
        }
        self.rev += 1;
    }

    fn make(&mut self) {
        let template = match qnc_registry::reader(&self.root).and_then(|r| qnc_project_templates::get(&r, &self.draft.template_id)?.ok_or_else(|| qnc_text::fill("project.store.template_missing", &[("id", &self.draft.template_id)]))) {
            Ok(template) => template,
            Err(error) => {
                self.status = Some(error);
                return;
            }
        };
        let name = match self.draft.name.trim() {
            "" => format!("{} {}", template.name, qnc_ids::time_stamp(unix_secs())),
            name => name.to_string(),
        };
        let (root, projects_root) = (self.root.clone(), self.projects_root.clone());
        self.draft.name.clear();
        self.start(move || match qnc_project_make::make(&root, qnc_project_make::Make { name: name.clone(), template_id: template.template_id, projects_root: Some(projects_root), ..Default::default() }) {
            Ok(_) => Ended::Made(name),
            Err(error) => Ended::Failed(error),
        });
    }
}

/// `epoch_<secs>` as the time stamp shown; another text as it is.
fn epoch_text(at: &str) -> Option<String> {
    Some(at.strip_prefix("epoch_").and_then(|s| s.parse::<u64>().ok()).map(qnc_ids::date_stamp).unwrap_or_else(|| at.to_string()))
}

#[cfg(test)]
mod tests;
