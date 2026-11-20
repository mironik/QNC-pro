//! The stream layout the player plays: the one video stream of the chosen representation and
//! the audio channel inventory of the original, as the saved facts give them.

use qnc_media_model::{FrameRateMode, FrameTimebase, MediaRepresentation, StreamDetails};
use qnc_media_record::Snapshot;
use serde::{Deserialize, Serialize};

use crate::{InputError, Representation, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoInput {
    pub stream_index: u32,
    pub timebase: FrameTimebase,
    pub duration_frames: u64,
    pub frame_rate_mode: FrameRateMode,
}

/// Native encoded stream index and zero-based channel within that stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioChannel {
    pub stream_index: u32,
    pub channel_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamLayout {
    pub video: Option<VideoInput>,
    /// Always the original's channels, whatever plays the picture.
    pub audio_channels: Vec<AudioChannel>,
}

pub(crate) fn layout(snapshot: &Snapshot, optimized: Option<&MediaRepresentation>, representation: Representation) -> Result<StreamLayout> {
    let original = representation_layout(snapshot, optimized, Representation::Original)?;
    if representation == Representation::Original {
        return Ok(original);
    }
    let mut selected = representation_layout(snapshot, optimized, representation)?;
    let timing_matches = match (&original.video, &selected.video) {
        (Some(a), Some(b)) => a.timebase == b.timebase && a.duration_frames == b.duration_frames && a.frame_rate_mode != FrameRateMode::Variable && b.frame_rate_mode != FrameRateMode::Variable,
        _ => false,
    };
    if !timing_matches {
        return Err(InputError::UnsupportedMedia("Proxy picture and original audio require matching saved video timing.".into()));
    }
    selected.audio_channels = original.audio_channels;
    Ok(selected)
}

fn representation_layout(snapshot: &Snapshot, optimized: Option<&MediaRepresentation>, representation: Representation) -> Result<StreamLayout> {
    let (prefix, media) = match representation {
        Representation::Original => ("original", &snapshot.metadata.original),
        Representation::Proxy => ("proxy", snapshot.metadata.proxy.as_ref().ok_or(InputError::MissingProxy)?),
        Representation::Optimized => ("optimized", optimized.ok_or(InputError::InvalidDescriptor)?),
    };
    let missing = snapshot.report.missing_under(prefix);
    if !missing.is_empty() {
        return Err(InputError::IncompleteMedia(missing));
    }
    let mut result = StreamLayout { video: None, audio_channels: Vec::new() };
    let mut streams: Vec<_> = media.streams.iter().collect();
    streams.sort_by_key(|s| s.index.as_ref().map(|i| i.value));
    for stream in streams {
        let index = stream.index.as_ref().ok_or(InputError::InvalidDescriptor)?.value;
        match &stream.details {
            StreamDetails::Video(video) => {
                if result.video.is_some() {
                    return Err(InputError::UnsupportedMedia("Multiple video streams require an explicit stream selection contract.".into()));
                }
                let duration_frames = video.exact_frame_count().ok_or_else(|| InputError::UnsupportedMedia("Frame-accurate playback requires a saved exact frame count.".into()))?;
                result.video = Some(VideoInput {
                    stream_index: index,
                    timebase: video.frame_rate.as_ref().ok_or(InputError::InvalidDescriptor)?.value,
                    duration_frames,
                    frame_rate_mode: video.frame_rate_mode.as_ref().ok_or(InputError::InvalidDescriptor)?.value,
                });
            }
            StreamDetails::Audio(audio) => {
                let count = audio.channels.as_ref().ok_or(InputError::InvalidDescriptor)?.value;
                if result.audio_channels.len() as u64 + u64::from(count) > u64::from(u16::MAX) {
                    return Err(InputError::UnsupportedMedia("Audio channel inventory exceeds the player contract limit.".into()));
                }
                result.audio_channels.extend((0..count).map(|channel_index| AudioChannel { stream_index: index, channel_index }));
            }
            StreamDetails::Other { .. } => {}
        }
    }
    Ok(result)
}
