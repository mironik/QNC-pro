//! The tray's work (copied from the old QNC's `qnc-ai-tray`, its rules). A card the watch says is
//! ready gets its project (under a name of the mode and the moment) and its ingest window beside
//! the tray, held in one job with every program the window starts, so quitting or a crash ends
//! them all. The mode of the computer is a row of the computer database, chosen in the menu.
//! Stopping the work asks what becomes of the partial ingest: kept, or its projects deleted. What
//! the icon shows (the status, the menu, notices) is given back to the run; the window does the
//! ingest, the mode block keeps the mode.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use qnc_card_ingest::Ingests;
use qnc_mode::Mode;
use qnc_text::{fill, text};

/// How long the processes get to end after a stop before the partial projects are deleted.
const DELETE_WAIT: Duration = Duration::from_secs(10);
/// The program of one card's ingest window.
const WINDOW: &str = "qnc-ingest-window";
/// The form Breaking news opens, and its pool tab.
const NEXT_FORM: &str = "qnc-media-assist-audio-ai";
const NEXT_ARGS: [&str; 2] = ["--pool-tab", "editorial_tab_all"];

/// One item of the icon's menu, as the run gives it (the form draws it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub choice: Option<Chosen>,
    pub text: String,
    pub enabled: bool,
    pub check: Option<bool>,
}

/// What the user can choose in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chosen {
    StopAll,
    Mode(Mode),
    Projects,
    Settings,
    OpenForm,
    Quit,
}

/// The question before the work stops: keep the partial ingest, delete it, or not stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopAsk {
    pub text: String,
    pub quit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAnswer {
    Keep,
    Delete,
    Cancel,
}

/// What a tick tells the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Status(String),
    Menu(Vec<Item>),
    Notice { title: String, text: String },
    /// The question to show, or none.
    Ask(Option<StopAsk>),
    /// The tray ends.
    Quit,
}

/// A window this tray opened: its card, the process.
struct Window {
    serial: String,
    child: Child,
}

pub struct Work {
    root: PathBuf,
    ingests: Ingests,
    job: qnc_process_job::Job,
    _autoplay: Option<qnc_autoplay_guard::Guard>,
    windows: Vec<Window>,
    /// The projects of the windows opened since the last stop: a stop that deletes deletes these.
    projects: Vec<String>,
    ask: Option<StopAsk>,
    to_delete: Option<(Vec<String>, bool, Instant)>,
    quit: bool,
    shown: Option<(usize, usize, Option<Mode>)>,
}

impl Work {
    pub fn start(root: &Path) -> Result<Work, String> {
        let projects = qnc_project_make::projects_root(root)?;
        let (profiles, refused) = qnc_camera_profile::read_profiles(&root.join("contracts").join("cameras").join("profiles"));
        for why in refused {
            qnc_diagnostics::log(format!("camera profile refused: {why}"));
        }
        let template = { let _writer = qnc_project_templates::writer(root)?; qnc_project_templates::selected(&qnc_registry::reader(root)?)?.settings };
        let job = qnc_process_job::Job::new()?;
        // A card of a camera QNC knows: the system opens nothing for it, the tray's ingest does.
        let autoplay = qnc_autoplay_guard::guard(Box::new(|disk, serial| {
            let volume = qnc_disks::Volume { root: disk.to_path_buf(), serial: format!("{serial:08x}"), label: None, kind: qnc_disks::DiskKind::Removable };
            matches!(qnc_card::what_is(volume), qnc_card::Found::Card { .. })
        }))
        .map_err(|error| qnc_diagnostics::log(format!("autoplay not taken: {error}")))
        .ok();
        qnc_diagnostics::log(format!("tray run: projects in {}, mode {:?}", projects.display(), read_mode(root)));
        Ok(Work {
            root: root.to_path_buf(),
            ingests: Ingests::new(root, &projects, template, profiles),
            job,
            _autoplay: autoplay,
            windows: Vec::new(),
            projects: Vec::new(),
            ask: None,
            to_delete: None,
            quit: false,
            shown: None,
        })
    }

    /// Routes a menu choice.
    pub fn choose(&mut self, chosen: Chosen) -> Vec<Told> {
        match chosen {
            Chosen::Quit if self.work() > 0 && self.ask.is_none() => self.ask_stop(true),
            Chosen::Quit => {
                qnc_diagnostics::log("tray quit");
                self.quit = true;
                vec![Told::Quit]
            }
            Chosen::StopAll if self.ask.is_none() && self.work() > 0 => self.ask_stop(false),
            Chosen::StopAll => Vec::new(),
            Chosen::Mode(mode) => self.set_mode(mode),
            Chosen::Projects => self.open("qnc-projects", &[]),
            Chosen::Settings => self.open("qnc-settings", &[]),
            Chosen::OpenForm => self.open(NEXT_FORM, &NEXT_ARGS),
        }
    }

    fn ask_stop(&mut self, quit: bool) -> Vec<Told> {
        let ask = StopAsk { text: text(if quit { "tray.quit_ask" } else { "tray.stop_ask" }).to_string(), quit };
        self.ask = Some(ask.clone());
        vec![Told::Ask(Some(ask))]
    }

    /// Routes the answer to the question before the work stops.
    pub fn answer_stop(&mut self, answer: StopAnswer) -> Vec<Told> {
        let Some(ask) = self.ask.take() else { return Vec::new() };
        qnc_diagnostics::log(format!("stop asked quit={}: {answer:?}", ask.quit));
        let mut told = vec![Told::Ask(None)];
        match answer {
            StopAnswer::Keep if ask.quit => {
                self.quit = true;
                told.push(Told::Quit);
            }
            StopAnswer::Keep => {
                told.push(self.stop_job());
                self.projects.clear();
            }
            StopAnswer::Delete => {
                told.push(self.stop_job());
                self.to_delete = Some((std::mem::take(&mut self.projects), ask.quit, Instant::now()));
            }
            StopAnswer::Cancel => {}
        }
        told
    }

    fn stop_job(&mut self) -> Told {
        match self.job.stop() {
            Ok(()) => notice(text("tray.stopped").to_string()),
            Err(error) => notice(fill("tray.ingest_failed", &[("error", &error)])),
        }
    }

    fn set_mode(&mut self, mode: Mode) -> Vec<Told> {
        match qnc_mode::writer(&self.root).and_then(|w| qnc_mode::set_mode(&w, mode)) {
            Ok(()) => {
                qnc_diagnostics::log(format!("mode chosen {mode:?}"));
                Vec::new()
            }
            Err(error) => vec![notice(error)],
        }
    }

    /// Opens the program `name` beside the tray, held in the job.
    fn open(&mut self, name: &str, args: &[&str]) -> Vec<Told> {
        match qnc_launch::start_detached(name, args, qnc_launch::Priority::Same) {
            Ok(child) => {
                qnc_diagnostics::log(format!("tray opened {name}"));
                let _ = self.job.take(&child).map_err(|error| qnc_diagnostics::log(format!("{name} not held by the tray: {error}")));
                Vec::new()
            }
            Err(error) => vec![notice(fill("tray.form_failed", &[("error", &error)]))],
        }
    }

    /// How many programs work: the windows and everything they started.
    fn work(&self) -> usize {
        self.job.active()
    }

    fn running(&mut self) -> usize {
        self.windows.retain_mut(|window| matches!(window.child.try_wait(), Ok(None)));
        self.windows.len()
    }

    fn runs_card(&mut self, serial: &str) -> bool {
        self.running();
        self.windows.iter().any(|window| window.serial.eq_ignore_ascii_case(serial))
    }

    /// A card the watch says is ready: its project and its ingest window, unless a window has it.
    pub fn card_ready(&mut self, card: qnc_card_select::Card, camera: &str) -> Vec<Told> {
        match self.runs_card(&card.serial) {
            true => Vec::new(),
            false => self.open_window(card, camera),
        }
    }

    /// Once a tick: the projects of a stop that deletes, and the status and the menu when they changed.
    pub fn settle(&mut self) -> Vec<Told> {
        let mut told = self.delete_when_free();
        let (running, active, mode) = (self.running(), self.work(), read_mode(&self.root));
        if self.shown != Some((running, active, mode)) {
            self.shown = Some((running, active, mode));
            told.push(Told::Status(status(running, active)));
            told.push(Told::Menu(self.menu(active, mode)));
        }
        if self.quit {
            told.push(Told::Quit);
        }
        told
    }

    /// The project of a ready card and its ingest window.
    fn open_window(&mut self, card: qnc_card_select::Card, camera: &str) -> Vec<Told> {
        let mode = read_mode(&self.root).map(Mode::label).map(text).unwrap_or(text("mode.story"));
        let name = format!("{mode} {}", qnc_ids::time_stamp(unix_secs()));
        qnc_diagnostics::log(format!("card ready {} ({camera}) as {name}", card.disk.display()));
        let made = match self.ingests.make_project(&name) {
            Ok(made) => made,
            Err(error) => return vec![notice(fill("tray.ingest_failed", &[("error", &error)]))],
        };
        let folder = made.folder.display().to_string();
        let disk = card.disk.display().to_string();
        let args = ["--project-id", &made.project_id, "--folder", &folder, "--name", &made.name, "--disk", &disk, "--serial", &card.serial, "--label", &card.volume_name, "--root-path", &card.root_path, "--camera", camera];
        match qnc_launch::start_detached(WINDOW, &args, qnc_launch::Priority::Same) {
            Ok(child) => {
                let _ = self.job.take(&child).map_err(|error| qnc_diagnostics::log(format!("ingest window not held by the tray: {error}")));
                self.windows.push(Window { serial: card.serial.clone(), child });
                self.projects.push(made.project_id);
                vec![notice(fill("tray.card_ingest", &[("card", &card.volume_name), ("camera", camera), ("project", &name)]))]
            }
            Err(error) => vec![notice(fill("tray.ingest_failed", &[("error", &error)]))],
        }
    }

    /// The partial projects of a stop that deletes, once their processes ended (or after a while).
    fn delete_when_free(&mut self) -> Vec<Told> {
        let active = self.job.active();
        let Some((projects, quit, _)) = self.to_delete.take_if(|(_, _, since)| active == 0 || since.elapsed() >= DELETE_WAIT) else { return Vec::new() };
        let mut told = Vec::new();
        for id in projects {
            told.push(match qnc_project_delete::delete(&self.root, &id) {
                Ok(deleted) => notice(fill("tray.project_deleted", &[("name", &deleted.name)])),
                Err(error) => notice(fill("tray.project_delete_failed", &[("name", &id), ("error", &error)])),
            });
        }
        if quit {
            self.quit = true;
        }
        told
    }

    /// The menu: what runs (a line that only tells), Stop all the work, the modes (the chosen one
    /// ticked), Projects, Settings, Open the application, Quit.
    fn menu(&self, active: usize, mode: Option<Mode>) -> Vec<Item> {
        let mut items = vec![
            Item { choice: None, text: status(self.windows.len(), active), enabled: false, check: None },
            Item { choice: Some(Chosen::StopAll), text: text("tray.stop_all").into(), enabled: active > 0, check: None },
        ];
        for choice in Mode::ALL {
            items.push(Item { choice: Some(Chosen::Mode(choice)), text: text(choice.label()).into(), enabled: true, check: Some(mode == Some(choice)) });
        }
        items.extend([
            Item { choice: Some(Chosen::Projects), text: text("tray.projects").into(), enabled: true, check: None },
            Item { choice: Some(Chosen::Settings), text: text("tray.settings").into(), enabled: true, check: None },
            Item { choice: Some(Chosen::OpenForm), text: text("tray.open_form").into(), enabled: mode == Some(Mode::BreakingNews), check: None },
            Item { choice: Some(Chosen::Quit), text: text("tray.quit").into(), enabled: true, check: None },
        ]);
        items
    }
}

/// What runs, in one line: the ingest windows, else the work left in the background, else waiting.
fn status(running: usize, active: usize) -> String {
    match (running, active) {
        (0, 0) => text("tray.waiting").to_string(),
        (0, active) => fill("tray.background", &[("count", &active.to_string())]),
        (running, _) => fill("tray.ingesting", &[("count", &running.to_string())]),
    }
}

/// The computer's mode, read from its database; none while none is chosen or the database is not there.
fn read_mode(root: &Path) -> Option<Mode> {
    qnc_db::Reader::open(&qnc_mode::computer_db(root)).ok().and_then(|r| qnc_mode::read_mode(&r).ok().flatten())
}

/// A notice beside the icon.
pub fn notice(text: String) -> Told {
    Told::Notice { title: qnc_text::text("tray.title").into(), text }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settling_tells_the_status_and_the_menu_with_the_modes_and_quitting_without_work_quits() {
        let root = std::env::temp_dir().join(format!("qnc-tray-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        qnc_registry::writer(&root).unwrap().execute(qnc_registry::MODULE, serde_json::json!({ "local": { "key": qnc_registry::PROJECTS_ROOT_KEY, "value": root.join("projects").to_string_lossy() } })).unwrap();
        let mut run = Work::start(&root).unwrap();
        let told = run.settle();
        assert!(told.iter().any(|t| matches!(t, Told::Status(s) if !s.is_empty())));
        let menu = told.iter().find_map(|t| match t { Told::Menu(items) => Some(items.clone()), _ => None }).expect("the menu");
        assert_eq!(menu.iter().filter(|item| item.check.is_some()).count(), Mode::ALL.len(), "a tick box per mode");
        assert!(menu.iter().all(|item| item.check != Some(true)), "no mode chosen yet");
        assert!(run.choose(Chosen::Mode(Mode::Story)).is_empty());
        assert_eq!(read_mode(&root), Some(Mode::Story));
        assert_eq!(run.choose(Chosen::Quit), vec![Told::Quit]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
