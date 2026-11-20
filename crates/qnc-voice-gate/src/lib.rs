//! What is voice. Levels come in per frame as per mille of full scale (0..=1000). The gate of
//! a window is one threshold: voice is at least 6 dB above the quiet fifth of the window,
//! at most 18 dB below its loud tenth, and never below -42 dBFS. Everything that asks "is this
//! voice" (word edges, the end of a reading, the floor of a transcription) asks here, so there
//! is one answer. Pure: reads and writes nothing.

/// The lowest voice, dBFS.
pub const VOICE_FLOOR_DBFS: f32 = -42.0;
/// Voice is this much above the quiet fifth of the window (dB).
pub const ABOVE_QUIET_DB: f32 = 6.0;
/// Voice is at most this much below the loud tenth of the window (dB).
pub const BELOW_LOUD_DB: f32 = 18.0;
/// A rise of this much over the frame before (out of a dip) starts a new onset (dB).
pub const ONSET_RISE_DB: f32 = 9.0;
/// Rises this close (frames) are one onset.
pub const ONSET_MERGE: u64 = 2;
/// A voice goes on through a dip no longer than this (frames).
pub const MAX_DIP: u64 = 5;
/// A transcription hears what is 6 dB above the clip's noise, never above -42 nor below -66 dBFS.
pub const FLOOR_CEILING_DBFS: f32 = -42.0;
pub const FLOOR_BOTTOM_DBFS: f32 = -66.0;

/// A level as dBFS; silence is -100.
pub fn decibels(per_mille: u16) -> f32 {
    if per_mille == 0 {
        -100.0
    } else {
        20.0 * (per_mille as f32 / 1000.0).log10()
    }
}

fn percentile(levels: &[u16], share: f32) -> u16 {
    let mut sorted = levels.to_vec();
    sorted.sort_unstable();
    let at = ((sorted.len() as f32 - 1.0) * share).round() as usize;
    sorted[at.min(sorted.len() - 1)]
}

/// The voice gate of one window of levels.
#[derive(Debug, Clone)]
pub struct Gate {
    levels: Vec<u16>,
    threshold_db: f32,
}

impl Gate {
    pub fn of(levels: &[u16]) -> Gate {
        let threshold_db = if levels.is_empty() {
            VOICE_FLOOR_DBFS
        } else {
            let quiet = decibels(percentile(levels, 0.2)) + ABOVE_QUIET_DB;
            let loud = decibels(percentile(levels, 0.9)) - BELOW_LOUD_DB;
            quiet.max(loud).max(VOICE_FLOOR_DBFS)
        };
        Gate { levels: levels.to_vec(), threshold_db }
    }

    pub fn threshold_db(&self) -> f32 {
        self.threshold_db
    }

    pub fn len(&self) -> u64 {
        self.levels.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Is frame `at` of the window voice.
    pub fn is_voice(&self, at: u64) -> bool {
        self.levels.get(at as usize).is_some_and(|&level| decibels(level) > self.threshold_db)
    }

    fn db(&self, at: u64) -> f32 {
        self.levels.get(at as usize).map_or(-100.0, |&level| decibels(level))
    }

    /// Voice that lasts: at least 3 of the 4 frames from `at` are voice.
    pub fn sustained(&self, at: u64) -> bool {
        (at..at + 4).filter(|&f| self.is_voice(f)).count() >= 3
    }

    /// Where voice starts: out of quiet, or out of a dip with a rise of `ONSET_RISE_DB`; rises
    /// within `ONSET_MERGE` frames are one onset, its first frame. Only onsets that last.
    pub fn onsets(&self) -> Vec<u64> {
        let mut onsets: Vec<u64> = Vec::new();
        for at in 0..self.len() {
            let rises = self.is_voice(at) && (at == 0 || !self.is_voice(at - 1) || self.db(at) - self.db(at - 1) >= ONSET_RISE_DB);
            if rises && self.sustained(at) && onsets.last().is_none_or(|&last| at - last > ONSET_MERGE) {
                onsets.push(at);
            }
        }
        onsets
    }

    /// The runs of voice `[from, to)`, dips up to `MAX_DIP` frames bridged, shorter than
    /// `least` frames dropped.
    pub fn runs(&self, least: u64) -> Vec<(u64, u64)> {
        let mut runs: Vec<(u64, u64)> = Vec::new();
        let mut open: Option<(u64, u64)> = None;
        for at in 0..self.len() {
            if self.is_voice(at) {
                open = Some(match open {
                    Some((from, to)) if at - to <= MAX_DIP => (from, at + 1),
                    Some(run) => {
                        runs.push(run);
                        (at, at + 1)
                    }
                    None => (at, at + 1),
                });
            }
        }
        runs.extend(open);
        runs.into_iter().filter(|(from, to)| to - from >= least).collect()
    }

    /// Where the voice that is on at `from` ends: the frame after its last voice before a dip
    /// longer than `MAX_DIP`, never past `limit`. `None` when `from` is not voice.
    pub fn voice_end(&self, from: u64, limit: u64) -> Option<u64> {
        if !self.is_voice(from) {
            return None;
        }
        let mut end = from + 1;
        let mut at = from + 1;
        while at < limit.min(self.len()) {
            if self.is_voice(at) {
                end = at + 1;
            } else if at - end >= MAX_DIP {
                break;
            }
            at += 1;
        }
        Some(end)
    }
}

/// The noise of a block of levels: its quiet tenth, dBFS.
pub fn noise_dbfs(levels: &[u16]) -> f32 {
    if levels.is_empty() {
        return -100.0;
    }
    decibels(percentile(levels, 0.1))
}

/// What a transcription of a clip hears: 6 dB above the clip's quietest noise, between
/// `FLOOR_BOTTOM_DBFS` and `FLOOR_CEILING_DBFS`.
pub fn clip_floor_dbfs(quietest_noise_dbfs: f32) -> f32 {
    (quietest_noise_dbfs + ABOVE_QUIET_DB).clamp(FLOOR_BOTTOM_DBFS, FLOOR_CEILING_DBFS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quiet (5 per mille, -46 dB), then a word 10..20 (300, -10 dB), a breath 22..24 (12), a
    /// word 30..45 with a dip at 37..38 (60), quiet to the end.
    fn levels() -> Vec<u16> {
        let mut levels = vec![5u16; 60];
        levels[10..20].fill(300);
        levels[22..24].fill(12);
        levels[30..45].fill(200);
        levels[37..39].fill(100);
        levels
    }

    #[test]
    fn voice_is_above_the_quiet_and_near_the_loud_never_below_the_floor() {
        let gate = Gate::of(&levels());
        assert!(gate.threshold_db() >= VOICE_FLOOR_DBFS);
        assert!(gate.is_voice(15));
        assert!(!gate.is_voice(23), "a breath 1.5 dB over the quiet is not voice");
        assert!(gate.is_voice(37), "a dip still near the loud is voice");
        assert_eq!(Gate::of(&[]).threshold_db(), VOICE_FLOOR_DBFS);
        assert_eq!(decibels(1000), 0.0);
        assert_eq!(clip_floor_dbfs(-80.0), FLOOR_BOTTOM_DBFS);
        assert_eq!(clip_floor_dbfs(-30.0), FLOOR_CEILING_DBFS);
        assert_eq!(clip_floor_dbfs(-60.0), -54.0);
    }

    #[test]
    fn onsets_runs_and_the_end_of_a_voice_follow_the_sound() {
        let gate = Gate::of(&levels());
        assert_eq!(gate.onsets(), vec![10, 30], "a dip inside a word is no onset");
        assert_eq!(gate.runs(3), vec![(10, 20), (30, 45)]);
        assert_eq!(gate.voice_end(12, 60), Some(20));
        assert_eq!(gate.voice_end(31, 60), Some(45), "through the dip");
        assert_eq!(gate.voice_end(31, 40), Some(40), "never past the limit");
        assert_eq!(gate.voice_end(5, 60), None);
        assert!(noise_dbfs(&levels()) < -40.0);
    }
}
