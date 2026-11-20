//! The one wave reader of a form. A wave is the stored peaks of a clip, one lane per channel,
//! one value per frame (per mille of full scale). Whoever owns the stored waves gives this
//! block a `WaveSource`; the form asks for the clips it shows and polls. A clip is read once;
//! a clip the source does not have yet is asked again only when the form asks again. No other
//! block keeps a copy of a wave.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub struct Wave {
    pub frames_per_second: f64,
    /// One lane per channel, one peak per frame.
    pub lanes: Vec<Vec<u16>>,
}

impl Wave {
    pub fn frames(&self) -> u64 {
        self.lanes.first().map_or(0, |lane| lane.len() as u64)
    }

    /// The peaks of a lane over `[from, to)`, short where the wave ends.
    pub fn lane(&self, lane: usize, from: u64, to: u64) -> &[u16] {
        let Some(peaks) = self.lanes.get(lane) else { return &[] };
        let from = (from as usize).min(peaks.len());
        let to = (to as usize).clamp(from, peaks.len());
        &peaks[from..to]
    }
}

/// Whoever stores waves. `None` when the clip has no wave yet; `Err` says why it cannot be read.
pub trait WaveSource: Send + 'static {
    fn read(&mut self, clip_id: &str) -> Result<Option<Wave>, String>;
}

pub struct Reader {
    asks: mpsc::Sender<String>,
    answers: mpsc::Receiver<(String, Result<Option<Wave>, String>)>,
    waves: BTreeMap<String, Arc<Wave>>,
    asked: BTreeSet<String>,
    errors: BTreeMap<String, String>,
}

impl Reader {
    /// Starts the reader of one form over its wave source.
    pub fn over(mut source: impl WaveSource) -> Reader {
        let (asks, inbox) = mpsc::channel::<String>();
        let (outbox, answers) = mpsc::channel();
        thread::Builder::new()
            .name("qnc-wave-read".to_string())
            .spawn(move || {
                while let Ok(clip_id) = inbox.recv() {
                    let answer = source.read(&clip_id);
                    if outbox.send((clip_id, answer)).is_err() {
                        return;
                    }
                }
            })
            .expect("a thread for the wave reader");
        Reader { asks, answers, waves: BTreeMap::new(), asked: BTreeSet::new(), errors: BTreeMap::new() }
    }

    /// The form shows this clip: its wave is read in the background unless it is here or being
    /// read. Returns whether a read was started.
    pub fn want(&mut self, clip_id: &str) -> bool {
        if self.waves.contains_key(clip_id) || self.asked.contains(clip_id) {
            return false;
        }
        self.asked.insert(clip_id.to_string());
        self.errors.remove(clip_id);
        self.asks.send(clip_id.to_string()).is_ok()
    }

    /// Takes the answers that arrived. Returns the clips whose wave is now here.
    pub fn poll(&mut self) -> Vec<String> {
        let mut arrived = Vec::new();
        while let Ok((clip_id, answer)) = self.answers.try_recv() {
            self.asked.remove(&clip_id);
            match answer {
                Ok(Some(wave)) => {
                    self.waves.insert(clip_id.clone(), Arc::new(wave));
                    arrived.push(clip_id);
                }
                Ok(None) => {}
                Err(error) => {
                    self.errors.insert(clip_id, error);
                }
            }
        }
        arrived
    }

    /// The wave of a clip, once read.
    pub fn wave(&self, clip_id: &str) -> Option<Arc<Wave>> {
        self.waves.get(clip_id).cloned()
    }

    /// Why a clip's wave could not be read, if that is what the source said.
    pub fn error(&self, clip_id: &str) -> Option<&str> {
        self.errors.get(clip_id).map(String::as_str)
    }

    /// Is a read on its way.
    pub fn reading(&self) -> bool {
        !self.asked.is_empty()
    }

    /// The form no longer shows this clip.
    pub fn forget(&mut self, clip_id: &str) {
        self.waves.remove(clip_id);
        self.errors.remove(clip_id);
    }

    /// Another project: nothing read before is true any more.
    pub fn forget_all(&mut self) {
        self.waves.clear();
        self.errors.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc as SyncArc, Mutex};

    struct Counting(SyncArc<Mutex<Vec<String>>>);

    impl WaveSource for Counting {
        fn read(&mut self, clip_id: &str) -> Result<Option<Wave>, String> {
            self.0.lock().unwrap().push(clip_id.to_string());
            match clip_id {
                "has" => Ok(Some(Wave { frames_per_second: 50.0, lanes: vec![vec![1, 2, 3]] })),
                "not_yet" => Ok(None),
                _ => Err("broken".to_string()),
            }
        }
    }

    fn settled(reader: &mut Reader) -> Vec<String> {
        let mut arrived = Vec::new();
        for _ in 0..200 {
            arrived.extend(reader.poll());
            if !reader.reading() {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
        arrived
    }

    #[test]
    fn a_clip_is_read_once_a_missing_one_may_be_asked_again_and_a_broken_one_says_why() {
        let reads = SyncArc::new(Mutex::new(Vec::new()));
        let mut reader = Reader::over(Counting(reads.clone()));
        assert!(reader.want("has"));
        assert!(!reader.want("has"), "already being read");
        assert!(reader.want("not_yet"));
        assert!(reader.want("broken"));
        assert_eq!(settled(&mut reader), vec!["has"]);
        assert_eq!(reader.wave("has").unwrap().lane(0, 1, 10), &[2, 3]);
        assert!(!reader.want("has"), "read once");
        assert!(reader.want("not_yet"), "asked again when the form asks again");
        assert_eq!(reader.error("broken"), Some("broken"));
        settled(&mut reader);
        assert_eq!(reads.lock().unwrap().len(), 4);
        reader.forget("has");
        assert!(reader.wave("has").is_none());
    }
}
