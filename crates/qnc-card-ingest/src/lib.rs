//! A ready card into a new project, in the background. The project is made from the template
//! the caller names (data), its folder protected, the card selected and ingested; progress and
//! the end come back over a channel, so whoever asked (the tray) never waits. One ingest runs per
//! card at a time; a card asked again while it runs is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use qnc_camera_profile::Profile;
use qnc_card_select::Card;
use qnc_ingest_run::Progress;

/// What a running ingest says.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    ProjectMade { project_id: String, folder: PathBuf, clips: usize },
    Progress(Progress),
    Done(Progress),
    Failed(String),
}

pub struct Running {
    pub card: Card,
    pub project: Option<(String, PathBuf)>,
    pub last: Option<Progress>,
    cancel: Arc<AtomicBool>,
    heard: Receiver<Said>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Everything said since the last poll; `Done`/`Failed` end it.
    pub fn poll(&mut self) -> Vec<Said> {
        let mut said = Vec::new();
        while let Ok(s) = self.heard.try_recv() {
            if let Said::ProjectMade { project_id, folder, .. } = &s {
                self.project = Some((project_id.clone(), folder.clone()));
            }
            if let Said::Progress(p) | Said::Done(p) = &s {
                self.last = Some(*p);
            }
            said.push(s);
        }
        said
    }

    pub fn ended(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
}

/// What every ingest of this computer needs.
pub struct Ingests {
    root: PathBuf,
    projects_root: PathBuf,
    template: serde_json::Value,
    profiles: Arc<Vec<Profile>>,
    running: BTreeMap<String, Running>,
}


impl Ingests {
    /// `root` is the QNC root (registry, diagnostics), `projects_root` where projects are made,
    /// `template` the settings of a new project.
    pub fn new(root: &Path, projects_root: &Path, template: serde_json::Value, profiles: Vec<Profile>) -> Ingests {
        Ingests { root: root.to_path_buf(), projects_root: projects_root.to_path_buf(), template, profiles: Arc::new(profiles), running: BTreeMap::new() }
    }

    pub fn running(&self) -> usize {
        self.running.values().filter(|r| !r.ended()).count()
    }

    pub fn running_mut(&mut self) -> impl Iterator<Item = &mut Running> {
        self.running.values_mut()
    }

    pub fn stop_all(&self) {
        for running in self.running.values() {
            running.stop();
        }
    }

    /// Makes the project of a card under the name `name` (from the template, in the projects folder),
    /// without starting its ingest: the ingest window starts it with [`Ingests::start_into`].
    pub fn make_project(&self, name: &str) -> Result<qnc_project_make::Made, String> {
        qnc_project_make::make(&self.root, qnc_project_make::Make { name: name.to_string(), settings: Some(self.template.clone()), projects_root: Some(self.projects_root.clone()), ..Default::default() })
    }

    /// Starts the ingest of `card` into the project already made (`project_id`, its `folder`);
    /// refused while one runs for the card.
    /// `leave_out`: originals (URIs) another project already has, not taken.
    pub fn start_into(&mut self, card: Card, project_id: &str, folder: &Path, leave_out: std::collections::BTreeSet<String>) -> Result<(), String> {
        self.begin(card, Target::Made { project_id: project_id.to_string(), folder: folder.to_path_buf(), leave_out })

    }

    /// Starts the ingest of `card` under the project name `name` (the project is made first);
    /// refused while one runs for it.
    pub fn start(&mut self, card: Card, name: &str) -> Result<(), String> {
        self.begin(card, Target::Named(name.to_string()))
    }

    fn begin(&mut self, card: Card, target: Target) -> Result<(), String> {
        let key = card.serial.to_ascii_lowercase();
        if self.running.get(&key).is_some_and(|r| !r.ended()) {
            return Err(format!("an ingest of card {key} already runs"));
        }
        let (tell, heard) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job { root: self.root.clone(), projects_root: self.projects_root.clone(), template: self.template.clone(), profiles: self.profiles.clone(), card: card.clone(), target, cancel: cancel.clone() };
        let thread = std::thread::spawn(move || {
            let outcome = job.run(&tell);
            let _ = tell.send(match outcome {
                Ok(progress) => Said::Done(progress),
                Err(error) => {
                    qnc_diagnostics::log(format!("card ingest failed: {error}"));
                    Said::Failed(error)
                }
            });
        });
        self.running.insert(key, Running { card, project: None, last: None, cancel, heard, thread: Some(thread) });
        Ok(())
    }

    /// Drops the ingests that ended; gives them back.
    pub fn take_ended(&mut self) -> Vec<Running> {
        let ended: Vec<String> = self.running.iter().filter(|(_, r)| r.ended()).map(|(k, _)| k.clone()).collect();
        ended.into_iter().filter_map(|k| self.running.remove(&k)).collect()
    }
}

/// Where the ingest goes: a project to make under a name, or one already made.
enum Target {
    Named(String),
    Made { project_id: String, folder: PathBuf, leave_out: std::collections::BTreeSet<String> },
}

struct Job {
    root: PathBuf,
    projects_root: PathBuf,
    template: serde_json::Value,
    profiles: Arc<Vec<Profile>>,
    card: Card,
    target: Target,
    cancel: Arc<AtomicBool>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Job {
    fn run(&self, tell: &Sender<Said>) -> Result<Progress, String> {
        let (project_id, folder) = match &self.target {
            Target::Named(name) => {
                let made = qnc_project_make::make(&self.root, qnc_project_make::Make { name: name.clone(), settings: Some(self.template.clone()), projects_root: Some(self.projects_root.clone()), ..Default::default() })?;
                (made.project_id, made.folder)
            }
            Target::Made { project_id, folder, .. } => (project_id.clone(), folder.clone()),
        };
        let leave_out = match &self.target {
            Target::Made { leave_out, .. } => leave_out.clone(),
            Target::Named(_) => Default::default(),
        };
        let project = qnc_ingest_run::Project::open(&folder)?;
        let selected = qnc_ingest_run::select_into(&self.card, &project, &self.profiles, now_ms(), &leave_out)?;

        let _ = tell.send(Said::ProjectMade { project_id: project_id.clone(), folder: folder.clone(), clips: selected.clips.len() });
        qnc_diagnostics::log(format!("card ingest: project {project_id} with {} clips", selected.clips.len()));
        let mut report = |p: Progress| {
            let _ = tell.send(Said::Progress(p));
        };
        qnc_ingest_run::run(&self.card, &project, &selected, &self.cancel, now_ms(), &mut report)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ingest_of_a_fixture_card_runs_in_the_background_makes_the_project_and_ends() {
        let root = std::env::temp_dir().join(format!("qnc-card-ingest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let disk = root.join("card");
        let card_root = disk.join("PRIVATE").join("XDROOT");
        std::fs::create_dir_all(card_root.join("Clip")).unwrap();
        std::fs::write(card_root.join("MEDIAPRO.XML"), include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML")).unwrap();
        std::fs::write(card_root.join("Clip").join("Mironik 1483M01.XML"), include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML")).unwrap();
        let qnc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (profiles, _) = qnc_camera_profile::read_profiles(&qnc.join("contracts/cameras/profiles"));
        let template = { let _writer = qnc_project_templates::writer(&root).unwrap(); qnc_project_templates::get(&qnc_registry::reader(&root).unwrap(), "tpl_news_package").unwrap().unwrap().settings };
        let mut ingests = Ingests::new(&root, &root.join("projects"), template, profiles);
        let card = Card { disk, serial: "67870D0C".into(), volume_name: "T".into(), root_path: "PRIVATE/XDROOT".into() };
        ingests.start(card.clone(), "Fixture card").unwrap();
        assert!(ingests.start(card, "Again").is_err(), "one per card");
        let mut said = Vec::new();
        for _ in 0..600 {
            for running in ingests.running_mut() {
                said.extend(running.poll());
            }
            if ingests.running() == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let ended = ingests.take_ended();
        assert_eq!(ended.len(), 1);
        assert!(matches!(said.first(), Some(Said::ProjectMade { clips: 1, .. })), "{said:?}");
        assert!(matches!(said.last(), Some(Said::Done(p)) if p.failed == 1 && p.clips == 1), "{said:?}");
        let (_, folder) = ended[0].project.clone().unwrap();
        let _ = qnc_folder_lock::unlock(&folder, &qnc_project_make::CACHE_FOLDERS);
        let _ = std::fs::remove_dir_all(&root);
    }
}
