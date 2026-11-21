//! The wave of a clip from its sound: one lane per channel, one peak per picture frame (per
//! mille of full scale), and the loudness of every channel measured in the same pass
//! (`qnc-loudness`). Samples are pushed as they are read, channel by channel or interleaved;
//! nothing is read or written here.

use qnc_loudness::{ChannelLoudness, Meter};
use qnc_media_model::FrameTimebase;

/// What a pass made: the lanes of peaks and the loudness of their channels.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub frames_per_second: f64,
    /// One lane per channel, one peak per frame, 0..=1000.
    pub lanes: Vec<Vec<u16>>,
    pub loudness: Vec<ChannelLoudness>,
}

struct Lane {
    peaks: Vec<u16>,
    /// The peak of the frame being filled, as the sample's absolute value.
    peak: f32,
    /// Samples pushed to this lane.
    samples: u64,
    meter: Meter,
}

/// Makes the wave of one clip whose sound is `sample_rate_hz` on `channels` channels at
/// `timebase` pictures a second.
pub struct Maker {
    sample_rate_hz: u32,
    timebase: FrameTimebase,
    lanes: Vec<Lane>,
}

impl Maker {
    pub fn new(sample_rate_hz: u32, channels: usize, timebase: FrameTimebase) -> Result<Maker, String> {
        if channels == 0 {
            return Err("a wave needs at least one channel".into());
        }
        let mut lanes = Vec::with_capacity(channels);
        for _ in 0..channels {
            lanes.push(Lane { peaks: Vec::new(), peak: 0.0, samples: 0, meter: Meter::new(sample_rate_hz)? });
        }
        Ok(Maker { sample_rate_hz, timebase, lanes })
    }

    /// Samples of one channel, in order.
    pub fn push_channel(&mut self, channel: usize, samples: &[f32]) -> Result<(), String> {
        let Some(lane) = self.lanes.get_mut(channel) else {
            return Err(format!("no channel {channel} in the wave"));
        };
        let (rate, timebase) = (self.sample_rate_hz, self.timebase);
        for sample in samples {
            let at = frame_of(rate, timebase, lane.samples);
            while (lane.peaks.len() as u64) < at {
                let peak = lane.peak;
                lane.peaks.push(per_mille(peak));
                lane.peak = 0.0;
            }
            lane.peak = lane.peak.max(sample.abs());
            lane.meter.push(*sample);
            lane.samples += 1;
        }
        Ok(())
    }

    /// Interleaved samples of every channel (one sample frame is one sample per channel).
    pub fn push_interleaved(&mut self, samples: &[f32]) -> Result<(), String> {
        let channels = self.lanes.len();
        if !samples.len().is_multiple_of(channels) {
            return Err("interleaved samples do not fill every channel".into());
        }
        for channel in 0..channels {
            let own: Vec<f32> = samples.iter().skip(channel).step_by(channels).copied().collect();
            self.push_channel(channel, &own)?;
        }
        Ok(())
    }

    /// The wave; the last, partial frame counts too.
    pub fn finish(self) -> Made {
        let frames_per_second = self.timebase.fps();
        let mut lanes = Vec::with_capacity(self.lanes.len());
        let mut loudness = Vec::with_capacity(self.lanes.len());
        for mut lane in self.lanes {
            if lane.samples > 0 {
                lane.peaks.push(per_mille(lane.peak));
            }
            lanes.push(lane.peaks);
            loudness.push(lane.meter.finish());
        }
        Made { frames_per_second, lanes, loudness }
    }
}

/// The picture frame a sample at `index` belongs to (no overflow for hours of sound).
fn frame_of(sample_rate_hz: u32, timebase: FrameTimebase, index: u64) -> u64 {
    let num = u128::from(index) * timebase.fps_num as u128;
    let den = u128::from(sample_rate_hz) * timebase.fps_den as u128;
    (num / den) as u64
}

fn per_mille(peak: f32) -> u16 {
    (peak.clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_peak_a_frame_per_channel_and_the_loudness_of_each() {
        let timebase = FrameTimebase::new(50, 1).unwrap();
        let mut maker = Maker::new(48_000, 2, timebase).unwrap();
        // One second: channel 0 a tone at half scale, channel 1 silence.
        let tone: Vec<f32> = (0..48_000).map(|i| 0.5 * (i as f32 * 0.05).sin()).collect();
        maker.push_channel(0, &tone[..24_000]).unwrap();
        maker.push_channel(0, &tone[24_000..]).unwrap();
        maker.push_channel(1, &vec![0.0; 48_000]).unwrap();
        let made = maker.finish();
        assert_eq!(made.frames_per_second, 50.0);
        assert_eq!(made.lanes.len(), 2);
        assert_eq!(made.lanes[0].len(), 50, "one peak a frame");
        assert!(made.lanes[0].iter().all(|peak| (490..=500).contains(peak)), "{:?}", made.lanes[0]);
        assert!(made.lanes[1].iter().all(|peak| *peak == 0));
        assert_eq!(made.loudness.len(), 2);
        assert_eq!(made.loudness[0].energies.len(), 10, "ten segments of 100 ms");
    }

    #[test]
    fn interleaved_samples_fill_every_channel() {
        let mut maker = Maker::new(48_000, 2, FrameTimebase::new(25, 1).unwrap()).unwrap();
        maker.push_interleaved(&[1.0, 0.0, 1.0, 0.0]).unwrap();
        assert!(maker.push_interleaved(&[1.0]).is_err());
        let made = maker.finish();
        assert_eq!(made.lanes, vec![vec![1000], vec![0]]);
    }
}
