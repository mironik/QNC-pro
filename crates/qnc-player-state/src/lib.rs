//! The shown state of the player. The player owns time; what the interface shows is only what
//! the player confirmed. A `CarrierPositionChanged` is the confirmed frame (a cue sent is not
//! an arrival); `PlaybackReadinessChanged` is only readiness, never a position. Errors stay
//! until the next session. Pure.

use qnc_player_wire::{Event, FrameRange, Timebase, TransportStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The frame the player confirmed, with its range and timebase.
    pub frame: Option<u64>,
    pub range: Option<FrameRange>,
    pub timebase: Option<Timebase>,
    pub status: TransportStatus,
    pub ready: bool,
    /// The frame at which the player reached the end of its range.
    pub boundary: Option<u64>,
    pub error: Option<String>,
    /// Peak of each output, hundredths of dBFS, by track id.
    pub levels: Vec<(String, i32)>,
    /// Commands the player rejected: (name, reason).
    pub rejected: Vec<(String, String)>,
}

impl Default for Shown {
    fn default() -> Shown {
        Shown { frame: None, range: None, timebase: None, status: TransportStatus::Empty, ready: false, boundary: None, error: None, levels: Vec::new(), rejected: Vec::new() }
    }
}

impl Shown {
    pub fn new() -> Shown {
        Shown::default()
    }

    /// Folds the events of one reply in. Returns whether the shown frame changed.
    pub fn fold(&mut self, events: &[Event]) -> bool {
        let before = self.frame;
        for event in events {
            match event {
                Event::CarrierPositionChanged { frame, range, timebase, status, .. } => {
                    self.frame = Some(*frame);
                    self.range = *range;
                    self.timebase = *timebase;
                    self.status = *status;
                    self.boundary = None;
                }
                Event::TransportStatusChanged { status } => self.status = *status,
                Event::PlaybackReadinessChanged { ready, .. } => self.ready = *ready,
                Event::PlaybackBoundaryReached { frame } => self.boundary = Some(*frame),
                Event::SourceFailed { reason, .. } | Event::PlaybackError { reason } => self.error = Some(reason.clone()),
                Event::AudioLevelChanged { track_id, peak_dbfs_x100 } => match self.levels.iter_mut().find(|(id, _)| id == track_id) {
                    Some(level) => level.1 = *peak_dbfs_x100,
                    None => self.levels.push((track_id.clone(), *peak_dbfs_x100)),
                },
                Event::CommandRejected { command_name, reason, .. } => self.rejected.push((command_name.clone(), reason.clone())),
                Event::CommandAccepted { .. } | Event::Other { .. } => {}
            }
        }
        self.frame != before
    }

    pub fn playing(&self) -> bool {
        self.status == TransportStatus::Playing
    }

    /// Takes the rejections seen so far.
    pub fn take_rejected(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_carrier_position_is_a_frame_readiness_is_not_and_errors_stay() {
        let mut shown = Shown::new();
        assert!(!shown.fold(&[Event::PlaybackReadinessChanged { source_id: None, frame: 40, ready: true }]));
        assert_eq!((shown.frame, shown.ready), (None, true), "readiness is not a position");
        assert!(shown.fold(&[Event::CarrierPositionChanged { source_id: None, frame: 40, range: None, timebase: None, status: TransportStatus::Ready }]));
        assert_eq!(shown.frame, Some(40));
        shown.fold(&[Event::PlaybackBoundaryReached { frame: 99 }, Event::PlaybackError { reason: "x".into() }, Event::AudioLevelChanged { track_id: "output_1".into(), peak_dbfs_x100: -1200 }]);
        assert_eq!(shown.boundary, Some(99));
        assert_eq!(shown.error.as_deref(), Some("x"));
        assert_eq!(shown.levels, vec![("output_1".to_string(), -1200)]);
        shown.fold(&[Event::CarrierPositionChanged { source_id: None, frame: 0, range: None, timebase: None, status: TransportStatus::Ready }]);
        assert_eq!(shown.boundary, None, "a new position clears the boundary");
        assert_eq!(shown.error.as_deref(), Some("x"), "the error stays");
    }
}
