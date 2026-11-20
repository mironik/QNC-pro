//! The run behind the tray icon. Each tick (half a second) the disks are looked at; a card the
//! watch says is ready goes to the card ingest under a name of the mode and the moment; what
//! the ingest says comes back as status lines and notices for the icon. Nothing is decided
//! here: the watch says when a card is ready, the ingest says what happened, the mode says the
//! name. The projects' folder is the computer setting `projects-root`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qnc_card_ingest::{Ingests, Said};
use qnc_card_watch::{Seen, Watch};

pub const TICK: Duration = Duration::from_millis(500);
pub const PROJECTS_ROOT_SETTING: &str = "projects-root";

/// What a tick tells the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Status(String),
    Notice { title: String, text: String },
}

pub struct Run {
    watch: Watch,
    ingests: Ingests,
    mode: String,
    started: Instant,
    last_tick: Instant,
}

/// The projects' folder of this computer, from its setting.
pub fn projects_root(root: &Path) -> Result<PathBuf, String> {
    qnc_settings::setting(root, PROJECTS_ROOT_SETTING)?
        .and_then(|v| v.get("folder").and_then(|f| f.as_str()).filter(|f| !f.trim().is_empty()).map(PathBuf::from))
        .ok_or_else(|| format!("the computer setting {PROJECTS_ROOT_SETTING} names no folder"))
}

impl Run {
    pub fn start(root: &Path) -> Result<Run, String> {
        let projects = projects_root(root)?;
        let (profiles, refused) = qnc_camera_profile::read_profiles(&root.join("contracts").join("cameras").join("profiles"));
        for why in refused {
            qnc_diagnostics::log(format!("camera profile refused: {why}"));
        }
        let template = qnc_card_ingest::template(root)?;
        let mode = qnc_db::Reader::open(&qnc_mode::computer_db(root)).ok().and_then(|r| qnc_mode::read_mode(&r).ok().flatten()).map(|m| qnc_text::text(m.label()).to_string()).unwrap_or_else(|| qnc_text::text("mode.story").to_string());
        qnc_diagnostics::log(format!("tray run: projects in {}, mode {mode}", projects.display()));
        Ok(Run { watch: Watch::new(), ingests: Ingests::new(root, &projects, template, profiles), mode, started: Instant::now(), last_tick: Instant::now() })
    }

    pub fn stop_all(&self) {
        self.ingests.stop_all();
    }

    /// Waits the rest of the tick, then looks and listens; what the icon should show.
    pub fn tick(&mut self) -> Vec<Told> {
        let left = TICK.saturating_sub(self.last_tick.elapsed());
        std::thread::sleep(left);
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
                Seen::Unreadable { volume, error } => told.push(Told::Notice { title: qnc_text::text("tray.title").into(), text: qnc_text::fill("tray.card_unreadable", &[("error", &format!("{} {error}", volume.root.display()))]) }),
                Seen::NotACard(_) | Seen::Gone(_) => {}
            }
        }
        let mut lines = Vec::new();
        for running in self.ingests.running_mut() {
            for said in running.poll() {
                match said {
                    Said::ProjectMade { project_id, .. } => told.push(Told::Notice { title: qnc_text::text("tray.title").into(), text: qnc_text::fill("tray.project_made", &[("name", &project_id)]) }),
                    Said::Done(_) => told.push(Told::Notice { title: qnc_text::text("tray.title").into(), text: qnc_text::fill("tray.card_done", &[("name", &running.card.volume_name)]) }),
                    Said::Failed(error) => told.push(Told::Notice { title: qnc_text::text("tray.title").into(), text: error }),
                    Said::Progress(_) => {}
                }
            }
            let progress = running.last.unwrap_or_default();
            lines.push(qnc_text::fill("tray.progress", &[("name", &running.card.volume_name), ("finished", &progress.finished.to_string()), ("clips", &progress.clips.to_string())]));
        }
        for ended in self.ingests.take_ended() {
            qnc_diagnostics::log(format!("card ingest ended: {:?}", ended.last));
        }
        let status = match self.ingests.running() {
            0 => qnc_text::text("tray.waiting").to_string(),
            count => format!("{} | {}", qnc_text::fill("tray.ingesting", &[("count", &count.to_string())]), lines.join(" | ")),
        };
        told.push(Told::Status(status));
        told
    }
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
        assert!(Run::start(&root).is_err());
        qnc_settings::set_setting(&root, PROJECTS_ROOT_SETTING, &serde_json::json!({ "folder": root.join("projects") })).unwrap();
        let qnc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::create_dir_all(root.join("contracts").join("project")).unwrap();
        std::fs::copy(qnc.join("contracts/project/news.json"), root.join("contracts/project/news.json")).unwrap();
        let mut run = Run::start(&root).unwrap();
        let told = run.tick();
        assert!(matches!(told.last(), Some(Told::Status(s)) if !s.is_empty()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
