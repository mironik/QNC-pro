//! The card taken out by itself: after a confirmed Cancel, and once the import has copied every
//! clip into the project when the project keeps nothing on the card (it plays the originals it
//! copied, not a proxy left on the card). A card the project still reads stays in. The eject is
//! the system's own (`qnc-eject`), never by force: a disk still in use (the work stopping) is
//! tried again for a while, then said. After the import the window says the card is out for
//! ten seconds and closes; the work goes on in the background.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// How long a disk still in use is tried again.
const TRIES: u32 = 40;
const AGAIN: Duration = Duration::from_millis(500);
/// How long the window shows that the card is out before it closes.
pub const NOTICE: Duration = Duration::from_secs(10);

/// Whether a project that plays `input` keeps nothing on the card once its originals are
/// copied: a proxy is not copied, so a project that plays the proxy still reads the card.
pub fn free_when_imported(input: qnc_player_input::PlaybackInput) -> bool {
    matches!(input, qnc_player_input::PlaybackInput::Original | qnc_player_input::PlaybackInput::OptimizedIfAvailable)
}

pub struct CardOut {
    disk: PathBuf,
    free_when_imported: bool,
    started: bool,
    closes_window: bool,
    answer: Option<Receiver<Result<(), String>>>,
    said: Option<Result<(), String>>,
    out_at: Option<Instant>,
}

impl CardOut {
    /// For the card at `disk` (a drive root); `free_when_imported` as the project says.
    pub fn new(disk: &Path, free_when_imported: bool) -> Self {
        Self { disk: disk.to_path_buf(), free_when_imported, started: false, closes_window: false, answer: None, said: None, out_at: None }
    }

    /// The import has copied `imported` of `clips` clips, `failed` failed: the card goes out once
    /// all are copied without a failure and the project needs it no more.
    pub fn imported(&mut self, clips: usize, imported: usize, failed: usize) {
        if self.free_when_imported && clips > 0 && imported >= clips && failed == 0 && !self.started {
            self.closes_window = true;
            self.start("import done");
        }
    }

    /// The eject is over, or none comes (the project still reads the card): what holds the card
    /// (a form opened over it) may start now without vetoing the eject.
    pub fn settled(&self) -> bool {
        !self.free_when_imported || self.said.is_some()
    }

    /// The card is out after the import: the window shows that for `NOTICE`, then closes (said once).
    pub fn window_closes(&mut self) -> bool {
        if self.closes_window && matches!(self.said, Some(Ok(()))) && self.out_at.is_none() {
            self.out_at = Some(Instant::now());
        }
        let closes = self.closes_window && self.out_at.is_some_and(|at| at.elapsed() >= NOTICE);
        if closes {
            self.closes_window = false;
        }
        closes
    }

    /// The notice the window shows while it waits to close: the text and the seconds left.
    pub fn notice(&self) -> Option<(String, u64)> {
        let at = self.out_at.filter(|_| self.closes_window)?;
        let left = NOTICE.saturating_sub(at.elapsed()).as_secs() + 1;
        Some((qnc_text::text("ingest.card_out_notice").to_string(), left))
    }

    /// The card goes out (once).
    pub fn start(&mut self, why: &str) {
        if self.started {
            return;
        }
        self.started = true;
        let disk = self.disk.clone();
        qnc_diagnostics::log(format!("card out {}: {why}", disk.display()));
        let (send, answer) = channel();
        self.answer = Some(answer);
        std::thread::spawn(move || {
            let mut last = String::new();
            for _ in 0..TRIES {
                if !qnc_disks::attached().iter().any(|volume| volume.root == disk) {
                    let _ = send.send(Ok(()));
                    return;
                }
                match qnc_eject::eject(&disk) {
                    Ok(()) => {
                        let _ = send.send(Ok(()));
                        return;
                    }
                    Err(qnc_eject::Refused::InUse) => last = qnc_eject::Refused::InUse.to_string(),
                    Err(error) => {
                        let _ = send.send(Err(error.to_string()));
                        return;
                    }
                }
                std::thread::sleep(AGAIN);
            }
            let _ = send.send(Err(last));
        });
    }

    /// What became of the eject, once known: the line the window says.
    pub fn said(&mut self) -> Option<String> {
        if let Some(result) = self.answer.as_ref().and_then(|answer| answer.try_recv().ok()) {
            qnc_diagnostics::log(format!("card out {}: {result:?}", self.disk.display()));
            self.said = Some(result);
            self.answer = None;
        }
        self.said.as_ref().map(|result| match result {
            Ok(()) => qnc_text::text("ingest.card_ejected").to_string(),
            Err(error) => qnc_text::fill("ingest.card_not_ejected", &[("error", error)]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_playing_the_originals_frees_the_card_one_playing_the_proxy_keeps_it() {
        assert!(free_when_imported(qnc_player_input::PlaybackInput::Original));
        assert!(!free_when_imported(qnc_player_input::PlaybackInput::Proxy));
        assert!(!free_when_imported(qnc_player_input::PlaybackInput::ProxyIfAvailable));
        let mut out = CardOut::new(Path::new("no-such-disk"), false);
        out.imported(3, 3, 0);
        assert!(!out.started && out.settled(), "a project that reads the card never ejects it and never waits");
        let mut out = CardOut::new(Path::new("no-such-disk"), true);
        out.imported(3, 2, 0);
        assert!(!out.started, "not all copied yet");
        out.imported(3, 3, 1);
        assert!(!out.started, "a failure keeps the card in");
        out.imported(3, 3, 0);
        assert!(out.started && !out.settled());
        for _ in 0..50 {
            if out.said().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(out.said(), Some(qnc_text::text("ingest.card_ejected").to_string()), "a disk not attached is out already");
        assert!(out.settled());
        assert!(!out.window_closes(), "the notice shows first");
        assert!(out.notice().is_some_and(|(_, left)| left >= 10));
    }
}
