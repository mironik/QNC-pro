//! The marks of the shown clip. Every clip is a virtual clip: without a mark the user set, IN is
//! the clip's first frame and OUT the frame after its last (OUT is exclusive). Ctrl+I / Ctrl+O
//! take a mark; while one is taken the arrows and the wheel move it and the playhead shows the
//! picture at the cut. Enter keeps what was moved and lets go. Esc lets go and puts the mark
//! back where it was when it was taken. Undo while a mark is taken puts it back too, but keeps
//! it taken. A click on the timeline while a mark is taken keeps, as Enter. Nothing else in the
//! program holds a copy of these marks or of "taken".

use qnc_actions::catalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    None,
    In,
    Out,
}

/// What an action did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Done {
    /// The marks or the taken one changed; the player should show this clip frame.
    Cue(u64),
    /// The marks or the taken one changed; the playhead stays.
    Changed,
    /// Nothing could change (no mark, a mark against the other one or the clip's edge).
    Refused,
    /// The action is not the marks' business.
    NotMine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marks {
    frames: u64,
    in_mark: Option<u64>,
    out_mark: Option<u64>,
    taken: Taken,
    /// The marks when the taken one was taken: what Esc and Undo go back to.
    origin: Option<(Option<u64>, Option<u64>)>,
    /// The first cut of the scissors, waiting for the second.
    pending_cut: Option<u64>,
}

impl Marks {
    /// The marks of a clip of `frames` frames, none set.
    pub fn of_clip(frames: u64) -> Marks {
        Marks { frames: frames.max(1), in_mark: None, out_mark: None, taken: Taken::None, origin: None, pending_cut: None }
    }

    /// The marks of a clip with the marks it was saved with.
    pub fn with(frames: u64, in_mark: Option<u64>, out_mark: Option<u64>) -> Marks {
        let mut marks = Marks::of_clip(frames);
        marks.in_mark = in_mark.map(|f| f.min(marks.frames - 1));
        marks.out_mark = out_mark.map(|f| f.clamp(marks.in_().saturating_add(1), marks.frames));
        marks
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The IN the user set, if any.
    pub fn set_in(&self) -> Option<u64> {
        self.in_mark
    }

    pub fn set_out(&self) -> Option<u64> {
        self.out_mark
    }

    /// IN as shown: the user's or the clip's first frame.
    pub fn in_(&self) -> u64 {
        self.in_mark.unwrap_or(0)
    }

    /// OUT as shown (exclusive): the user's or the frame after the clip's last.
    pub fn out(&self) -> u64 {
        self.out_mark.unwrap_or(self.frames)
    }

    pub fn taken(&self) -> Taken {
        self.taken
    }

    /// The frame of the picture at the taken mark: IN itself, the last frame before OUT.
    pub fn taken_picture(&self) -> Option<u64> {
        match self.taken {
            Taken::None => None,
            Taken::In => Some(self.in_()),
            Taken::Out => Some(self.out() - 1),
        }
    }

    /// An action of the catalog with the playhead's clip frame.
    pub fn act(&mut self, action: &str, playhead: u64) -> Done {
        debug_assert!(catalog().has(action), "{action} is not in the catalog");
        match action {
            "mark_in" => self.put(Taken::In, playhead),
            "mark_out" => self.put(Taken::Out, playhead.saturating_add(1)),
            "select_mark_in" => self.take(Taken::In),
            "select_mark_out" => self.take(Taken::Out),
            "step_back_frame" => self.nudge(-1),
            "step_forward_frame" => self.nudge(1),
            "activate_focused_item" => self.keep(),
            "clear_focus" => self.let_go(),
            "undo_object" => self.back(),
            "go_to_mark_in" => Done::Cue(self.in_()),
            "navigate_prev_object" => self.stop(playhead, false),
            "navigate_next_object" => self.stop(playhead, true),
            "timeline_take_nearer" => self.take_nearer(playhead),
            "timeline_drag_mark" => self.drag(self.taken, playhead),
            "timeline_cue_frame" => self.click(),
            "source_cut" => self.cut(playhead),
            _ => Done::NotMine,
        }
    }

    /// Right click (Ctrl+) at `frame`: takes IN or OUT, whichever is nearer.
    fn take_nearer(&mut self, frame: u64) -> Done {
        let to_in = self.in_().abs_diff(frame);
        let to_out = (self.out() - 1).abs_diff(frame);
        self.take(if to_in <= to_out { Taken::In } else { Taken::Out })
    }

    /// The scissors (middle click, double click): the first cut is IN without OUT; the second
    /// marks the range (the earlier frame IN, the later OUT); the third starts a new pair.
    fn cut(&mut self, frame: u64) -> Done {
        self.taken = Taken::None;
        self.origin = None;
        match self.pending_cut.take() {
            None => {
                let frame = frame.min(self.frames - 1);
                self.in_mark = Some(frame);
                self.out_mark = None;
                self.pending_cut = Some(frame);
                Done::Changed
            }
            Some(first) => {
                let (low, high) = (first.min(frame), first.max(frame));
                self.in_mark = Some(low.min(self.frames - 1));
                self.out_mark = Some((high + 1).clamp(low + 1, self.frames));
                Done::Changed
            }
        }
    }

    /// A mark dragged with the mouse to `frame`: taken while it moves, never across the other
    /// one or out of the clip.
    pub fn drag(&mut self, which: Taken, frame: u64) -> Done {
        if which == Taken::None {
            return Done::Refused;
        }
        if self.taken != which {
            self.remember();
            self.taken = which;
        }
        self.place(which, if which == Taken::Out { frame.saturating_add(1) } else { frame })
    }

    /// A left click on the timeline while a mark is taken keeps it, as Enter; otherwise the
    /// click is not the marks' business.
    pub fn click(&mut self) -> Done {
        match self.taken {
            Taken::None => Done::NotMine,
            _ => self.keep(),
        }
    }

    fn remember(&mut self) {
        if self.origin.is_none() {
            self.origin = Some((self.in_mark, self.out_mark));
        }
    }

    /// I / O: the mark goes to the playhead and whatever was taken is kept and let go: a key
    /// that sets never leaves a mark taken.
    fn put(&mut self, which: Taken, frame: u64) -> Done {
        self.taken = Taken::None;
        self.origin = None;
        match self.place(which, frame) {
            Done::Cue(_) => Done::Changed,
            other => other,
        }
    }

    fn place(&mut self, which: Taken, frame: u64) -> Done {
        match which {
            Taken::In => {
                let limit = self.out().saturating_sub(1);
                let frame = frame.min(limit);
                self.in_mark = Some(frame);
                Done::Cue(frame)
            }
            Taken::Out => {
                let frame = frame.clamp(self.in_().saturating_add(1), self.frames);
                self.out_mark = Some(frame);
                Done::Cue(frame - 1)
            }
            Taken::None => Done::Refused,
        }
    }

    fn take(&mut self, which: Taken) -> Done {
        self.remember();
        self.taken = which;
        Done::Cue(self.taken_picture().unwrap_or(0))
    }

    fn nudge(&mut self, by: i64) -> Done {
        let moved = |frame: u64| frame.checked_add_signed(by);
        match self.taken {
            Taken::None => Done::NotMine,
            Taken::In => match moved(self.in_()) {
                Some(next) if next < self.out() => {
                    self.in_mark = Some(next);
                    Done::Cue(next)
                }
                _ => Done::Refused,
            },
            Taken::Out => match moved(self.out()) {
                Some(next) if next > self.in_() && next <= self.frames => {
                    self.out_mark = Some(next);
                    Done::Cue(next - 1)
                }
                _ => Done::Refused,
            },
        }
    }

    /// Enter: what was moved stays, nothing is taken any more.
    fn keep(&mut self) -> Done {
        if self.taken == Taken::None {
            return Done::NotMine;
        }
        self.taken = Taken::None;
        self.origin = None;
        Done::Changed
    }

    /// Esc: the taken mark goes back to where it was taken, nothing is taken any more.
    fn let_go(&mut self) -> Done {
        if self.taken == Taken::None {
            return Done::NotMine;
        }
        let picture = self.restore();
        self.taken = Taken::None;
        self.origin = None;
        picture
    }

    /// Undo while a mark is taken: back to where it was taken, still taken.
    fn back(&mut self) -> Done {
        if self.taken == Taken::None {
            return Done::NotMine;
        }
        self.restore()
    }

    fn restore(&mut self) -> Done {
        if let Some((in_mark, out_mark)) = self.origin {
            self.in_mark = in_mark;
            self.out_mark = out_mark;
        }
        self.taken_picture().map_or(Done::Changed, Done::Cue)
    }

    /// Alt+arrows: the clip's start, IN and the last picture before OUT, from the playhead.
    /// Nothing is taken; with a mark taken they do nothing.
    fn stop(&self, playhead: u64, forward: bool) -> Done {
        if self.taken != Taken::None {
            return Done::Refused;
        }
        let stops = [0, self.in_(), self.out() - 1];
        let next = if forward {
            stops.iter().copied().filter(|&s| s > playhead).min()
        } else {
            stops.iter().copied().filter(|&s| s < playhead).max()
        };
        next.map_or(Done::Refused, Done::Cue)
    }
}
