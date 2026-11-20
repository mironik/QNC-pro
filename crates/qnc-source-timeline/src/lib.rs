//! The source timeline. It shows one clip: its marks (owned by `qnc-marks`), its window (cut
//! by `qnc-axis`: the whole clip or a range enlarged 15/70/15) and the playhead as the player
//! last reported it. The player owns the playhead; the timeline only shows it and asks the
//! player to move it. A key or a mouse gesture becomes catalog actions (the mouse through
//! `qnc-mouse-map`), every action goes to the marks first; what the marks do not take is the
//! player's (play, step, cue). The zoom is the same timeline over a smaller window and sends
//! its actions through the same `act`.

use qnc_axis::Axis;
use qnc_marks::{Done, Marks, Taken};
use qnc_mouse_map::Gesture;

pub const PANEL: &str = "source_timeline";

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub id: String,
    pub frames: u64,
    pub fps: f64,
}

/// What the player is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Cue(u64),
    PlayPause,
    Step(i64),
}

/// What an action did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The marks did it (and maybe asked the player to cue).
    Marks(Done),
    /// The player was asked.
    Player(Ask),
    /// Nothing for this timeline.
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Whole,
    /// `[from, to)` of the clip at 70 % of the width.
    Enlarged { from: u64, to: u64 },
}

#[derive(Debug)]
pub struct SourceTimeline {
    clip: Option<Clip>,
    marks: Marks,
    playhead: u64,
    view: View,
    width: f32,
    asks: Vec<Ask>,
}

impl Default for SourceTimeline {
    fn default() -> Self {
        SourceTimeline { clip: None, marks: Marks::of_clip(1), playhead: 0, view: View::Whole, width: 1.0, asks: Vec::new() }
    }
}

impl SourceTimeline {
    pub fn new() -> SourceTimeline {
        SourceTimeline::default()
    }

    /// Shows a clip with the marks it was saved with (none for a fresh clip). The marks of the
    /// clip before are gone with it.
    pub fn show(&mut self, clip: Clip, in_mark: Option<u64>, out_mark: Option<u64>) {
        self.marks = Marks::with(clip.frames, in_mark, out_mark);
        self.playhead = in_mark.unwrap_or(0);
        self.view = View::Whole;
        self.clip = Some(clip);
        self.asks.clear();
    }

    pub fn clear(&mut self) {
        *self = SourceTimeline { width: self.width, ..SourceTimeline::default() };
    }

    pub fn clip(&self) -> Option<&Clip> {
        self.clip.as_ref()
    }

    pub fn marks(&self) -> &Marks {
        &self.marks
    }

    pub fn playhead(&self) -> u64 {
        self.playhead
    }

    /// The player reports where it is.
    pub fn playhead_from_player(&mut self, frame: u64) {
        self.playhead = frame;
    }

    /// The width the panel gives the timeline, in pixels.
    pub fn set_width(&mut self, width: f32) {
        self.width = width.max(1.0);
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Shows `[from, to)` enlarged, or the whole clip again.
    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// The window the timeline shows now.
    pub fn axis(&self) -> Axis {
        let frames = self.clip.as_ref().map_or(1, |c| c.frames);
        match self.view {
            View::Whole => Axis::of_clip(frames, self.width),
            View::Enlarged { from, to } => Axis::enlarged(frames, from, to, self.width),
        }
    }

    /// The asks for the player since the last take.
    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// A key: an action of the catalog, with the playhead as its frame.
    pub fn key(&mut self, action: &str) -> Outcome {
        let playhead = self.playhead;
        self.act(action, playhead)
    }

    /// A mouse gesture at pixel `x` of the window `axis` (the timeline's own, or its zoom's):
    /// the actions the mouse map binds, each with the frame under the pointer.
    pub fn pointer(&mut self, gesture: Gesture, x: f32, axis: Axis) -> Vec<Outcome> {
        let frame = axis.frame_at(x);
        let actions: Vec<String> = qnc_mouse_map::map().actions(PANEL, gesture).to_vec();
        actions.iter().map(|action| self.act(action, frame)).collect()
    }

    /// One action of the catalog with a frame: the marks first, then the player.
    pub fn act(&mut self, action: &str, frame: u64) -> Outcome {
        if self.clip.is_none() {
            return Outcome::Nothing;
        }
        match self.marks.act(action, frame) {
            Done::NotMine => self.player(action, frame),
            Done::Cue(at) => {
                self.asks.push(Ask::Cue(at));
                Outcome::Marks(Done::Cue(at))
            }
            done => Outcome::Marks(done),
        }
    }

    fn player(&mut self, action: &str, frame: u64) -> Outcome {
        let ask = match action {
            "play_pause" => Ask::PlayPause,
            "step_back_frame" => Ask::Step(-1),
            "step_forward_frame" => Ask::Step(1),
            "timeline_cue_frame" => Ask::Cue(frame),
            "playlist_input_start" => Ask::Cue(0),
            _ => return Outcome::Nothing,
        };
        self.asks.push(ask);
        Outcome::Player(ask)
    }

    /// The picture at the taken mark, where the zoom and the monitor look while a mark is taken.
    pub fn taken_picture(&self) -> Option<u64> {
        self.marks.taken_picture()
    }

    pub fn taken(&self) -> Taken {
        self.marks.taken()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown() -> SourceTimeline {
        let mut t = SourceTimeline::new();
        t.set_width(1000.0);
        t.show(Clip { id: "c".into(), frames: 1000, fps: 50.0 }, Some(100), Some(400));
        t
    }

    #[test]
    fn keys_go_to_the_marks_first_and_what_is_not_theirs_asks_the_player() {
        let mut t = shown();
        assert_eq!(t.key("step_forward_frame"), Outcome::Player(Ask::Step(1)));
        assert_eq!(t.key("select_mark_in"), Outcome::Marks(Done::Cue(100)));
        assert_eq!(t.key("step_forward_frame"), Outcome::Marks(Done::Cue(101)), "a taken mark moves instead");
        assert_eq!(t.key("play_pause"), Outcome::Player(Ask::PlayPause));
        assert_eq!(t.take_asks(), vec![Ask::Step(1), Ask::Cue(100), Ask::Cue(101), Ask::PlayPause]);
        let mut empty = SourceTimeline::new();
        assert_eq!(empty.key("play_pause"), Outcome::Nothing, "no clip, nothing");
    }

    #[test]
    fn the_mouse_goes_through_the_map_with_the_frame_under_the_pointer() {
        let mut t = shown();
        let axis = t.axis();
        assert_eq!(t.pointer(Gesture::LeftClick, 500.0, axis), vec![Outcome::Player(Ask::Cue(500))]);
        assert_eq!(t.pointer(Gesture::RightClick, 380.0, axis), vec![Outcome::Marks(Done::Cue(399))], "the nearer mark: OUT");
        assert_eq!(t.pointer(Gesture::WheelBack, 0.0, axis), vec![Outcome::Marks(Done::Cue(398))]);
        assert_eq!(t.pointer(Gesture::LeftClick, 10.0, axis), vec![Outcome::Marks(Done::Changed)], "a click keeps the taken mark");
        assert_eq!(t.marks().set_out(), Some(399));
        t.set_view(View::Enlarged { from: 100, to: 399 });
        let enlarged = t.axis();
        assert_eq!(enlarged.frame_at(0.0), 36);
        assert_eq!(t.pointer(Gesture::MiddleClick, 150.0, enlarged), vec![Outcome::Marks(Done::Changed)], "the scissors");
    }
}
