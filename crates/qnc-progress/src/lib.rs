//! The progress of a base. Whoever counts (the catalog) gives the counts; this block says what
//! they mean, once, for the tray, the ingest window and the eject: a bar per function the
//! project has on, "done" when every bar is full or stopped, "ready for QNC" when every clip is
//! imported and its first channel heard. Pure.

/// The functions a base works through, in order; a stopped one is a bit of `Counts::stopped`.
pub const FUNCTIONS: [&str; 8] = ["import", "posters", "filmstrips", "waves", "transcripts", "shots", "content", "captions"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub clips: usize,
    pub imported: usize,
    /// Whose import failed (a card taken out): they go again when the card is back.
    pub failed: usize,
    pub posters: usize,
    /// A function the project's settings leave off is `None` and has no bar.
    pub filmstrips: Option<usize>,
    pub waves: Option<usize>,
    pub transcripts: Option<usize>,
    pub shots: Option<usize>,
    pub content: Option<usize>,
    pub captions: Option<usize>,
    /// One bit per function of `FUNCTIONS` the user stopped.
    pub stopped: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub function: &'static str,
    pub done: usize,
    pub of: usize,
    pub stopped: bool,
}

impl Bar {
    pub fn full(&self) -> bool {
        self.done >= self.of
    }
}

impl Counts {
    pub fn is_stopped(&self, function: &str) -> bool {
        FUNCTIONS.iter().position(|f| *f == function).is_some_and(|bit| self.stopped & (1 << bit) != 0)
    }

    /// Marks a function stopped.
    pub fn stop(&mut self, function: &str) {
        if let Some(bit) = FUNCTIONS.iter().position(|f| *f == function) {
            self.stopped |= 1 << bit;
        }
    }

    /// One bar per function the project has on.
    pub fn bars(&self) -> Vec<Bar> {
        [
            ("import", Some(self.imported)),
            ("posters", Some(self.posters)),
            ("filmstrips", self.filmstrips),
            ("waves", self.waves),
            ("transcripts", self.transcripts),
            ("shots", self.shots),
            ("content", self.content),
            ("captions", self.captions),
        ]
        .into_iter()
        .filter_map(|(function, done)| Some(Bar { function, done: done?, of: self.clips, stopped: self.is_stopped(function) }))
        .collect()
    }

    /// Every clip imported and every function on made for it; a stopped one is not waited for.
    pub fn done(&self) -> bool {
        self.clips > 0 && self.bars().iter().all(|bar| bar.stopped || bar.full())
    }

    /// Every clip imported; nothing of the card is needed any more.
    pub fn copied(&self) -> bool {
        self.clips > 0 && self.imported >= self.clips
    }

    /// QNC may start: imported and the first channel heard (or the transcript off or stopped);
    /// the rest goes on in the background.
    pub fn ready_for_qnc(&self) -> bool {
        let heard = match self.transcripts {
            Some(done) => done >= self.clips || self.is_stopped("transcripts"),
            None => true,
        };
        self.copied() && heard
    }

    /// The share of all the work done, 0..=1.
    pub fn share(&self) -> f32 {
        let bars = self.bars();
        if self.clips == 0 || bars.is_empty() {
            return 0.0;
        }
        let total = (bars.len() * self.clips) as f32;
        bars.iter().map(|bar| bar.done.min(bar.of)).sum::<usize>() as f32 / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_copied_and_ready_follow_the_counts_and_a_stopped_function_is_not_waited_for() {
        let mut counts = Counts { clips: 4, imported: 4, posters: 4, filmstrips: Some(2), waves: None, transcripts: Some(1), ..Counts::default() };
        assert_eq!(counts.bars().len(), 4, "only the functions on");
        assert!(counts.copied());
        assert!(!counts.ready_for_qnc(), "the transcripts are not heard");
        assert!(!counts.done());
        counts.stop("transcripts");
        assert!(counts.ready_for_qnc());
        counts.stop("filmstrips");
        assert!(counts.done());
        assert_eq!(Counts::default().share(), 0.0);
        assert!((counts.share() - 11.0 / 16.0).abs() < 1e-6);
    }
}
