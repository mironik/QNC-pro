//! The preview. One worker thread owns the player session of the shown clip: it opens the
//! session (a second of process and hello), sends the commands the timeline asks for, asks
//! the player for its state in rhythm and hands events back. The interface thread only folds
//! events into the shown state, reads the newest posted picture and passes the confirmed
//! frame to the source timeline. A new clip ends the session before (user rule: nothing of the
//! old session shows in the new).

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use qnc_frame_map::{Picture, Posted, Reader};
use qnc_player_session::{Player, Session};
use qnc_player_state::Shown;
use qnc_player_wire::{Command, Event, TransportStatus};
use qnc_source_timeline::{Ask, SourceTimeline};
use serde_json::{Map, Value};

/// How often the worker asks the player where it is while nothing else is asked.
const STATE_EVERY: Duration = Duration::from_millis(20);

enum Order {
    Command(Command),
    End,
}

enum Word {
    Opened(std::path::PathBuf),
    Events(Vec<Event>),
    Failed(String),
}

struct Worker {
    orders: mpsc::Sender<Order>,
    words: mpsc::Receiver<Word>,
}

#[derive(Default)]
pub struct Preview {
    player: Option<Player>,
    worker: Option<Worker>,
    shown: Shown,
    mailbox: Option<Reader>,
    picture: Option<Picture>,
    generation: u64,
    error: Option<String>,
    /// The worker gives the player (its process) back when the session ends.
    player_back: Option<mpsc::Receiver<Player>>,
}

impl Preview {
    pub fn new() -> Preview {
        Preview::default()
    }

    /// Opens a session that plays `played` (the clip's prepared input and bindings). The
    /// session before ends first; its pictures and state are gone.
    pub fn open(&mut self, played: Map<String, Value>) {
        self.close();
        self.generation += 1;
        let generation = self.generation;
        let mut player = self.player.take().unwrap_or_default();
        let (orders, inbox) = mpsc::channel::<Order>();
        let (say, words) = mpsc::channel::<Word>();
        let (give_player, take_player) = mpsc::channel::<Player>();
        thread::Builder::new()
            .name("qnc-preview".into())
            .spawn(move || {
                let session = match player.open(played, generation) {
                    Ok(session) => session,
                    Err(error) => {
                        let _ = say.send(Word::Failed(error));
                        let _ = give_player.send(player);
                        return;
                    }
                };
                let _ = say.send(Word::Opened(session.frame_map().to_path_buf()));
                serve(session, &mut player, &inbox, &say);
                let _ = give_player.send(player);
            })
            .expect("a thread for the preview");
        self.worker = Some(Worker { orders, words });
        self.player_back = Some(take_player);
    }

    /// Ends the session, if any.
    pub fn close(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.orders.send(Order::End);
        }
        if let Some(back) = self.player_back.take() {
            if let Ok(player) = back.recv_timeout(Duration::from_secs(5)) {
                self.player = Some(player);
            }
        }
        self.shown = Shown::new();
        self.mailbox = None;
        self.picture = None;
        self.error = None;
    }

    /// The timeline asked: a command for the player.
    pub fn ask(&mut self, ask: Ask, timeline: &SourceTimeline) {
        let command = match ask {
            Ask::Cue(frame) => Command::CueFrame { frame, present_frame: true },
            Ask::PlayPause if self.shown.playing() => Command::Pause,
            Ask::PlayPause => Command::Play,
            Ask::Step(by) => {
                let at = self.shown.frame.unwrap_or(timeline.playhead());
                Command::CueFrame { frame: at.saturating_add_signed(by), present_frame: true }
            }
        };
        if let Some(worker) = &self.worker {
            let _ = worker.orders.send(Order::Command(command));
        }
    }

    /// Takes what the worker said, folds it in and gives the timeline the confirmed frame.
    /// Returns whether something shown changed.
    pub fn poll(&mut self, timeline: &mut SourceTimeline) -> bool {
        let mut changed = false;
        let mut words = Vec::new();
        if let Some(worker) = &self.worker {
            while let Ok(word) = worker.words.try_recv() {
                words.push(word);
            }
        }
        for word in words {
            match word {
                Word::Opened(map) => {
                    self.mailbox = Reader::open(&map).ok();
                    changed = true;
                }
                Word::Events(events) => {
                    if self.shown.fold(&events) {
                        if let Some(frame) = self.shown.frame {
                            timeline.playhead_from_player(frame);
                        }
                        changed = true;
                    }
                }
                Word::Failed(error) => {
                    qnc_diagnostics::log(format!("preview failed: {error}"));
                    self.error = Some(error);
                    changed = true;
                }
            }
        }
        if let Some(mailbox) = &mut self.mailbox {
            while let Ok(Some(posted)) = mailbox.read_newest() {
                self.picture = match posted {
                    Posted::Picture(picture) => Some(picture),
                    Posted::Clear => None,
                };
                changed = true;
            }
        }
        changed
    }

    pub fn shown(&self) -> &Shown {
        &self.shown
    }

    pub fn picture(&self) -> Option<&Picture> {
        self.picture.as_ref()
    }

    /// Why the session could not open, if it could not.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref().or(self.shown.error.as_deref())
    }

    pub fn status(&self) -> TransportStatus {
        self.shown.status
    }
}

/// The worker: commands as they come, the state in rhythm, until the end.
fn serve(mut session: Session, player: &mut Player, inbox: &mpsc::Receiver<Order>, say: &mpsc::Sender<Word>) {
    loop {
        match inbox.recv_timeout(STATE_EVERY) {
            Ok(Order::Command(command)) => match session.command(command) {
                Ok(events) => {
                    let _ = say.send(Word::Events(events));
                }
                Err(error) => {
                    let _ = say.send(Word::Failed(error));
                }
            },
            Ok(Order::End) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => match session.state() {
                Ok(events) => {
                    if !events.is_empty() && say.send(Word::Events(events)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = say.send(Word::Failed(error));
                    break;
                }
            },
        }
    }
    player.end(session);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_without_a_player_program_says_so_and_asks_go_nowhere() {
        let mut preview = Preview::new();
        let mut timeline = SourceTimeline::new();
        preview.open(Map::new());
        for _ in 0..200 {
            if preview.poll(&mut timeline) && preview.error().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(preview.error().is_some(), "no program beside, or a refused empty bootstrap: either is an error");
        preview.ask(Ask::PlayPause, &timeline);
        preview.close();
        assert!(preview.error().is_none());
    }
}
