//! The run behind the tray icon. Each tick (half a second) the disks are looked at; what the
//! watch sees (a ready card, an unreadable disk) goes to the tray work, as do the user's menu
//! choices and the answers to the stop question; what the icon shows comes back from the work.
//! Nothing is decided here: the watch says when a card is ready, the work holds the windows, the
//! job and the question.

use std::path::Path;
use std::time::{Duration, Instant};

use qnc_card_watch::{Seen, Watch};
pub use qnc_tray_work::{Chosen, Item, StopAnswer, StopAsk, Told};
use qnc_tray_work::Work;

pub const TICK: Duration = Duration::from_millis(500);

pub struct Run {
    watch: Watch,
    work: Work,
    started: Instant,
    last_tick: Instant,
}

impl Run {
    pub fn start(root: &Path) -> Result<Run, String> {
        Ok(Run { watch: Watch::new(), work: Work::start(root)?, started: Instant::now(), last_tick: Instant::now() })
    }

    /// Routes a menu choice to the work.
    pub fn choose(&mut self, chosen: Chosen) -> Vec<Told> {
        self.work.choose(chosen)
    }

    /// Routes the answer to the question before the work stops.
    pub fn answer_stop(&mut self, answer: StopAnswer) -> Vec<Told> {
        self.work.answer_stop(answer)
    }

    /// Looks and listens when the half second is up; nothing otherwise. Never waits.
    pub fn poll(&mut self) -> Vec<Told> {
        match self.last_tick.elapsed() >= TICK {
            true => self.tick(),
            false => Vec::new(),
        }
    }

    fn tick(&mut self) -> Vec<Told> {
        self.last_tick = Instant::now();
        let mut told = Vec::new();
        let now = self.started.elapsed();
        for seen in self.watch.look(now, qnc_disks::attached()) {
            match seen {
                Seen::Ready { card, layout } => told.extend(self.work.card_ready(card, &format!("{} {}", layout.manufacturer, layout.family))),
                Seen::Unreadable { volume, error } => told.push(qnc_tray_work::notice(qnc_text::fill("tray.card_unreadable", &[("error", &format!("{} {error}", volume.root.display()))]))),
                Seen::NotACard(_) | Seen::Gone(_) => {}
            }
        }
        told.extend(self.work.settle());
        told
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_tells_the_status() {
        let root = std::env::temp_dir().join(format!("qnc-tray-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        qnc_registry::writer(&root).unwrap().execute(qnc_registry::MODULE, serde_json::json!({ "local": { "key": qnc_registry::PROJECTS_ROOT_KEY, "value": root.join("projects").to_string_lossy() } })).unwrap();
        let mut run = Run::start(&root).unwrap();
        std::thread::sleep(TICK);
        let told = run.poll();
        assert!(told.iter().any(|t| matches!(t, Told::Status(s) if !s.is_empty())), "{told:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
