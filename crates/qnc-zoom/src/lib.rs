//! The zoom is the source timeline enlarged around the cut (QNC rule 0.12: the same tools, the
//! same code; only the picture differs). It exists while a mark is taken. Its window is `reach`
//! frames either side of the taken mark's picture; while the player runs the window follows
//! the playhead and the cut line stays where the mark is. A key or a gesture in the zoom is
//! the timeline's key or gesture, mapped through the zoom's own window.

use qnc_axis::Axis;
use qnc_mouse_map::Gesture;
use qnc_source_timeline::{Outcome, SourceTimeline};

/// Frames either side of the cut (one second at 50 fps).
pub const REACH: u64 = 50;

#[derive(Debug, Default)]
pub struct Zoom {
    /// The player runs: the window follows the playhead until it stops.
    following: bool,
}

impl Zoom {
    pub fn new() -> Zoom {
        Zoom::default()
    }

    /// The player says whether it runs.
    pub fn player_runs(&mut self, runs: bool) {
        self.following = runs;
    }

    /// The zoom's window, while a mark is taken; `None` when there is nothing to zoom on.
    pub fn window(&self, timeline: &SourceTimeline, width: f32) -> Option<Axis> {
        let clip = timeline.clip()?;
        let cut = timeline.taken_picture()?;
        let centre = if self.following { timeline.playhead() } else { cut };
        Some(Axis::zoom(clip.frames, centre, REACH, width))
    }

    /// The frame of the cut line in the zoom.
    pub fn cut(&self, timeline: &SourceTimeline) -> Option<u64> {
        timeline.taken_picture()
    }

    /// A key in the zoom is the timeline's key.
    pub fn key(&self, timeline: &mut SourceTimeline, action: &str) -> Outcome {
        timeline.key(action)
    }

    /// A gesture at pixel `x` of the zoom is the timeline's gesture at the frame under it.
    pub fn pointer(&self, timeline: &mut SourceTimeline, gesture: Gesture, x: f32, width: f32) -> Vec<Outcome> {
        match self.window(timeline, width) {
            Some(axis) => timeline.pointer(gesture, x, axis),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qnc_source_timeline::Clip;

    fn shown() -> SourceTimeline {
        let mut t = SourceTimeline::new();
        t.set_width(1000.0);
        t.show(Clip { id: "c".into(), frames: 1000, fps: 50.0 }, Some(300), Some(600));
        t
    }

    #[test]
    fn the_zoom_exists_around_the_taken_mark_and_follows_the_playhead_while_the_player_runs() {
        let mut t = shown();
        let mut zoom = Zoom::new();
        assert!(zoom.window(&t, 900.0).is_none());
        t.key("select_mark_out");
        let axis = zoom.window(&t, 900.0).unwrap();
        assert_eq!((axis.start, axis.end()), (549, 649));
        assert_eq!(zoom.cut(&t), Some(599));
        zoom.player_runs(true);
        t.playhead_from_player(700);
        assert_eq!(zoom.window(&t, 900.0).unwrap().start, 650);
        assert_eq!(zoom.cut(&t), Some(599), "the cut stays while the window travels");
    }

    #[test]
    fn every_key_and_gesture_in_the_zoom_does_what_it_does_on_the_timeline() {
        for gesture in Gesture::ALL {
            for x in [0.0, 300.0, 450.0, 899.0] {
                let (mut a, mut b) = (shown(), shown());
                a.key("select_mark_in");
                b.key("select_mark_in");
                let zoom = Zoom::new();
                let axis = zoom.window(&a, 900.0).unwrap();
                let on_timeline = a.pointer(gesture, x, axis);
                let in_zoom = zoom.pointer(&mut b, gesture, x, 900.0);
                assert_eq!(on_timeline, in_zoom, "{gesture:?} at {x}");
                assert_eq!(a.marks(), b.marks());
                assert_eq!(a.take_asks(), b.take_asks());
            }
        }
        for action in ["step_back_frame", "activate_focused_item", "clear_focus", "undo_object", "play_pause"] {
            let (mut a, mut b) = (shown(), shown());
            a.key("select_mark_out");
            b.key("select_mark_out");
            assert_eq!(a.key(action), Zoom::new().key(&mut b, action), "{action}");
            assert_eq!(a.marks(), b.marks());
        }
    }
}
