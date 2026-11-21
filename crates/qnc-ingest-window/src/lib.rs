//! The session of one ingest window (copied from the old QNC's `qnc-ai-card-ingest`, its work;
//! the drawing is `qnc-ingest-window-form`): one card into the project the tray made, the
//! progress of its functions and how long each took, Cancel with the question what becomes of the
//! partial import (kept, or the project deleted), the place and description typed for the project
//! (kept as its search text), and the form the computer's mode opens once the card is copied.
//! The form draws the view and hands back what the user did; every rule is here.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qnc_card_ingest::{Ingests, Said};
use qnc_card_select::Card;
use qnc_ingest_run::Progress;

/// One bar of the window: the code of its name, what its X stops, how many of all are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub text: &'static str,
    pub function: &'static str,
    pub done: usize,
    pub stopped: bool,
}

/// The function of the one bar the ingest has: the import of the clips.
pub const IMPORT: &str = "import";
/// The form Breaking news opens once the card is copied, and its pool tab.
const NEXT_FORM: &str = "qnc-media-assist-audio-ai";
const NEXT_ARGS: [&str; 2] = ["--pool-tab", "editorial_tab_all"];

/// What the window asks the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Cancel the ingest?
    Cancel,
    /// Keep or delete the partial import?
    Partial,
}

/// Where the ingest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    Cancelled,
}

/// What the form draws.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub phase: Phase,
    /// The card and its camera.
    pub card: String,
    /// How long the ingest ran, or runs.
    pub elapsed: Duration,
    pub project_name: String,
    pub clips: usize,
    pub bars: Vec<Bar>,
    pub failed: usize,
    pub bytes_copied: u64,
    pub ask: Option<Ask>,
    /// The window ends.
    pub close: bool,
}

/// The place and description typed for the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub location: String,
    pub description: String,
    written: (String, String),
}

pub struct Session {
    root: PathBuf,
    project_id: String,
    name: String,
    camera: String,
    card: Card,
    ingests: Ingests,
    last: Progress,
    started: Instant,
    took: Option<Duration>,
    done_after: Option<Duration>,
    ask: Option<Ask>,
    phase: Phase,
    close: bool,
    opened_form: bool,
    stopped: bool,
    /// The project is deleted once the ingest thread has let its files go.
    delete_pending: bool,
    pub info: Info,
}

/// The value after `--name` in the arguments.
fn arg(args: &[String], name: &str) -> Result<String, String> {
    args.iter().position(|a| a == name).and_then(|at| args.get(at + 1)).cloned().ok_or_else(|| qnc_text::fill("ingest.arg_missing", &[("name", name)]))
}

impl Session {
    /// The session of the arguments the tray gave: `--project-id`, `--folder`, `--name`, `--disk`,
    /// `--serial`, `--label`, `--root-path`, `--camera`. The ingest starts at once.
    pub fn start(root: &Path, args: &[String]) -> Result<Session, String> {
        let project_id = arg(args, "--project-id")?;
        let folder = PathBuf::from(arg(args, "--folder")?);
        let name = arg(args, "--name")?;
        let camera = arg(args, "--camera").unwrap_or_default();
        let card = Card { disk: PathBuf::from(arg(args, "--disk")?), serial: arg(args, "--serial")?, volume_name: arg(args, "--label").unwrap_or_default(), root_path: arg(args, "--root-path")? };
        let projects = qnc_project_make::projects_root(root)?;
        let (profiles, refused) = qnc_camera_profile::read_profiles(&root.join("contracts").join("cameras").join("profiles"));
        for why in refused {
            qnc_diagnostics::log(format!("camera profile refused: {why}"));
        }
        let template = { let _writer = qnc_project_templates::writer(root)?; qnc_project_templates::selected(&qnc_registry::reader(root)?)?.settings };
        let mut ingests = Ingests::new(root, &projects, template, profiles);
        ingests.start_into(card.clone(), &project_id, &folder)?;
        qnc_diagnostics::log(format!("ingest window: card {} into project {project_id}", card.serial));
        Ok(Session { root: root.to_path_buf(), project_id, name, camera, card, ingests, last: Progress::default(), started: Instant::now(), took: None, done_after: None, ask: None, phase: Phase::Running, close: false, opened_form: false, stopped: false, delete_pending: false, info: Info::default() })
    }

    /// Listens to the ingest, times it, opens the next form when the mode says, deletes the
    /// project when asked; gives what the form draws.
    pub fn poll(&mut self) -> View {
        let mut ended = false;
        for running in self.ingests.running_mut() {
            for said in running.poll() {
                match said {
                    Said::Progress(p) => self.last = p,
                    Said::Done(p) => {
                        self.last = p;
                        ended = true;
                    }
                    Said::Failed(error) => {
                        qnc_diagnostics::log(format!("ingest window: {error}"));
                        ended = true;
                    }
                    Said::ProjectMade { .. } => {}
                }
            }
        }
        if ended && self.took.is_none() {
            self.took = Some(self.started.elapsed());
            self.done_after = Some(self.started.elapsed());
            self.phase = if self.stopped { Phase::Cancelled } else { Phase::Done };
            qnc_diagnostics::log(format!("ingest window: ended after {} ms, {:?}", self.started.elapsed().as_millis(), self.last));
        }
        self.open_next_form();
        self.delete_when_free();
        let elapsed = self.took.unwrap_or_else(|| self.started.elapsed());
        let card = match self.camera.is_empty() {
            true => self.card.volume_name.clone(),
            false => format!("{} ({})", self.card.volume_name, self.camera),
        };
        View {
            phase: self.phase,
            card,
            elapsed,
            project_name: self.name.clone(),
            clips: self.last.clips,
            bars: vec![Bar { text: "ingest.imported", function: IMPORT, done: self.last.finished, stopped: self.stopped }],
            failed: self.last.failed,
            bytes_copied: self.last.bytes_copied,
            ask: self.ask,
            close: self.close,
        }
    }

    /// Breaking news on this computer: the next form opens as soon as every clip is copied; the
    /// story step is written. Story: the step says the ingest is done; Net: nothing opens.
    fn open_next_form(&mut self) {
        let copied = self.last.clips > 0 && self.last.finished + self.last.failed >= self.last.clips;
        if self.opened_form || !copied || self.stopped {
            return;
        }
        self.opened_form = true;
        let mode = qnc_db::Reader::open(&qnc_mode::computer_db(&self.root)).and_then(|r| qnc_mode::read_mode(&r)).unwrap_or(None);
        let step = match mode {
            Some(qnc_mode::Mode::BreakingNews) => "ai_media_assist",
            _ => "ingested",
        };
        match qnc_mode::writer(&self.root).and_then(|w| qnc_mode::set_step(&w, &self.project_id, step)) {
            Ok(()) => qnc_diagnostics::log(format!("ingest window: story step {step}")),
            Err(error) => qnc_diagnostics::log(format!("ingest window: story step not written: {error}")),
        }
        if mode == Some(qnc_mode::Mode::BreakingNews) {
            match qnc_launch::start_detached(NEXT_FORM, &NEXT_ARGS, qnc_launch::Priority::Same) {
                Ok(_) => qnc_diagnostics::log(format!("ingest window: breaking news opened {NEXT_FORM}")),
                Err(error) => qnc_diagnostics::log(format!("ingest window: {NEXT_FORM} not opened: {error}")),
            }
        }
    }

    /// The X beside a bar: the import stops; what is copied stays.
    pub fn stop_function(&mut self, function: &str) {
        if function == IMPORT {
            self.ingests.stop_all();
            self.stopped = true;
        }
    }

    pub fn ask_cancel(&mut self) {
        if self.phase == Phase::Running {
            self.ask = Some(Ask::Cancel);
        }
    }

    /// Cancel confirmed: the work stops and the question of the partial import follows.
    pub fn answer_cancel(&mut self, yes: bool) {
        self.ask = yes.then_some(Ask::Partial);
        if yes {
            qnc_diagnostics::log("ingest window: cancelled by the user");
            self.ingests.stop_all();
            self.stopped = true;
        }
    }

    /// Delete the partial import (the project goes once the ingest let its files go) or keep it.
    pub fn answer_partial(&mut self, delete: bool) {
        self.ask = None;
        self.delete_pending = delete;
        self.close = !delete;
        qnc_diagnostics::log(if delete { "ingest window: the partial import is deleted" } else { "ingest window: the partial import is kept" });
    }

    fn delete_when_free(&mut self) {
        if !self.delete_pending || self.ingests.running() > 0 {
            return;
        }
        self.delete_pending = false;
        match qnc_project_delete::delete(&self.root, &self.project_id) {
            Ok(deleted) => qnc_diagnostics::log(format!("ingest window: project {} deleted", deleted.name)),
            Err(error) => qnc_diagnostics::log(format!("ingest window: project {} not deleted: {error}", self.project_id)),
        }
        self.close = true;
    }

    /// How long the function of `bar` took, or runs.
    pub fn time_of(&self, bar: &Bar) -> Option<Duration> {
        match bar.done >= self.last.clips && self.last.clips > 0 {
            true => self.done_after,
            false => (!bar.stopped).then(|| self.took.unwrap_or_else(|| self.started.elapsed())),
        }
    }

    /// Writes the place and description typed when they differ from what is written: the
    /// project's search text (name, place, description).
    pub fn keep_info(&mut self) {
        let now = (self.info.location.trim().to_string(), self.info.description.trim().to_string());
        if now == self.info.written {
            return;
        }
        let text = format!("{} {} {}", self.name, now.0, now.1).trim().to_string();
        let written = qnc_registry::writer(&self.root).and_then(|w| w.execute(qnc_registry::MODULE, serde_json::json!({ "search": { "project_id": self.project_id, "text": text } })));
        match written {
            Ok(_) => self.info.written = now,
            Err(error) => qnc_diagnostics::log(format!("ingest window: project info not written: {error}")),
        }
    }

    /// The window is being closed by the user: what was typed is kept.
    pub fn closing(&mut self) {
        self.keep_info();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_needs_the_arguments_of_its_card_and_project() {
        let root = std::env::temp_dir().join(format!("qnc-ingest-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        let args: Vec<String> = ["--project-id", "p1"].iter().map(|s| s.to_string()).collect();
        let Err(error) = Session::start(&root, &args) else { panic!("an argument is missing") };
        assert!(error.contains("--folder"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
