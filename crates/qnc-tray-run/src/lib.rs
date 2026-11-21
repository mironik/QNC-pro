//! The run behind the tray icon. Each tick (half a second) the disks are looked at; a card the
//! watch says is ready goes to the card ingest under a name of the mode and the moment; what
//! the ingest says comes back as status lines and notices for the icon. Nothing is decided
//! here: the watch says when a card is ready, the ingest says what happened, the mode says the
//! name. The projects' folder is the computer setting `projects-root`.

use std::path::Path;
use std::time::{Duration, Instant};

use qnc_card_ingest::{Ingests, Said};
use qnc_card_watch::{Seen, Watch};

pub const TICK: Duration = Duration::from_millis(500);

/// What a tick tells the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Status(String),
    Notice { title: String, text: String },
    Cards(Vec<CardLine>),
}

pub struct Run {
    watch: Watch,
    ingests: Ingests,
    mode: String,
    started: Instant,
    last_tick: Instant,
}


impl Run {
    pub fn start(root: &Path) -> Result<Run, String> {
        let projects = qnc_project_make::projects_root(root)?;
        let (profiles, refused) = qnc_camera_profile::read_profiles(&root.join("contracts").join("cameras").join("profiles"));
        for why in refused {
            qnc_diagnostics::log(format!("camera profile refused: {why}"));
        }
        let mode = qnc_db::Reader::open(&qnc_mode::computer_db(root)).ok().and_then(|r| qnc_mode::read_mode(&r).ok().flatten()).unwrap_or(qnc_mode::Mode::Story);
        let template = { let _writer = qnc_project_templates::writer(root)?; qnc_project_templates::selected(&qnc_registry::reader(root)?)?.settings };
        let mode = qnc_text::text(mode.label()).to_string();
        qnc_diagnostics::log(format!("tray run: projects in {}, mode {mode}", projects.display()));
        Ok(Run { watch: Watch::new(), ingests: Ingests::new(root, &projects, template, profiles), mode, started: Instant::now(), last_tick: Instant::now() })
    }

    pub fn stop_all(&self) {
        self.ingests.stop_all();
    }
}

/// One card as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLine {
    pub serial: String,
    pub name: String,
    pub project: Option<String>,
    pub clips: usize,
    pub finished: usize,
    pub failed: usize,
    pub bytes_copied: u64,
}

impl Run {
    /// Stops the ingest of one card.
    pub fn stop(&mut self, serial: &str) {
        for running in self.ingests.running_mut().filter(|r| r.card.serial.eq_ignore_ascii_case(serial)) {
            running.stop();
        }
    }

    /// Looks and listens when the half second is up; nothing otherwise. Never waits.
    pub fn poll(&mut self) -> Vec<Told> {
        match self.last_tick.elapsed() >= TICK {
            true => self.look(),
            false => Vec::new(),
        }
    }

    fn look(&mut self) -> Vec<Told> {
        self.last_tick = Instant::now();
        let mut told = Vec::new();
        let now = self.started.elapsed();
        for seen in self.watch.look(now, qnc_disks::attached()) {
            match seen {
                Seen::Ready { card, layout } => {
                    let name = format!("{} {}", self.mode, qnc_ids::time_stamp(unix_secs()));
                    qnc_diagnostics::log(format!("card ready {} ({}) as {name}", card.disk.display(), layout.id));
                    match self.ingests.start(card, &name) {
                        Ok(()) => {}
                        Err(error) => qnc_diagnostics::log(format!("card not taken: {error}")),
                    }
                }
                Seen::Unreadable { volume, error } => told.push(notice(qnc_text::fill("tray.card_unreadable", &[("error", &format!("{} {error}", volume.root.display()))]))),
                Seen::NotACard(_) | Seen::Gone(_) => {}
            }
        }
        let mut lines = Vec::new();
        let mut cards = Vec::new();
        for running in self.ingests.running_mut() {
            for said in running.poll() {
                match said {
                    Said::ProjectMade { project_id, .. } => told.push(notice(qnc_text::fill("tray.project_made", &[("name", &project_id)]))),
                    Said::Done(_) => told.push(notice(qnc_text::fill("tray.card_done", &[("name", &running.card.volume_name)]))),
                    Said::Failed(error) => told.push(notice(error)),
                    Said::Progress(_) => {}
                }
            }
            let progress = running.last.unwrap_or_default();
            lines.push(qnc_text::fill("tray.progress", &[("name", &running.card.volume_name), ("finished", &progress.finished.to_string()), ("clips", &progress.clips.to_string())]));
            cards.push(CardLine { serial: running.card.serial.clone(), name: running.card.volume_name.clone(), project: running.project.as_ref().map(|(id, _)| id.clone()), clips: progress.clips, finished: progress.finished, failed: progress.failed, bytes_copied: progress.bytes_copied });
        }
        for ended in self.ingests.take_ended() {
            qnc_diagnostics::log(format!("card ingest ended: {:?}", ended.last));
        }
        let status = match self.ingests.running() {
            0 => qnc_text::text("tray.waiting").to_string(),
            count => format!("{} | {}", qnc_text::fill("tray.ingesting", &[("count", &count.to_string())]), lines.join(" | ")),
        };
        told.push(Told::Cards(cards));
        told.push(Told::Status(status));
        told
    }
}

fn notice(text: String) -> Told {
    Told::Notice { title: qnc_text::text("tray.title").into(), text }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_projects_folder_setting_the_run_does_not_start_and_with_it_a_tick_waits() {
        let root = std::env::temp_dir().join(format!("qnc-tray-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        qnc_registry::writer(&root).unwrap().execute(qnc_registry::MODULE, serde_json::json!({ "local": { "key": qnc_registry::PROJECTS_ROOT_KEY, "value": root.join("projects").to_string_lossy() } })).unwrap();
        let mut run = Run::start(&root).unwrap();
        std::thread::sleep(TICK);
        let told = run.poll();
        assert!(matches!(told.last(), Some(Told::Status(s)) if !s.is_empty()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
