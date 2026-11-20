//! The saved description of a medium, exactly as QNC records store it (contract
//! `qnc.media.metadata` 0.2.0): every fact carries the evidence it came from (a camera
//! document, a probe, or what QNC itself wrote) and a locator inside it. A field nobody
//! supplied is `None`, never a guess. Data only; the rules of what is complete are the
//! record's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CONTRACT_ID: &str = "qnc.media.metadata";
pub const CONTRACT_VERSION: &str = "0.2.0";

/// A frame rate or timebase as a positive rational.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameTimebase {
    pub fps_num: i64,
    pub fps_den: i64,
}

impl FrameTimebase {
    pub fn new(fps_num: i64, fps_den: i64) -> Result<FrameTimebase, String> {
        if fps_num <= 0 || fps_den <= 0 {
            return Err("frame timebase must come from saved metadata and cannot be zero".into());
        }
        Ok(FrameTimebase { fps_num, fps_den })
    }

    pub fn fps(self) -> f64 {
        self.fps_num as f64 / self.fps_den as f64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipMetadata {
    pub contract_id: String,
    pub contract_version: String,
    pub clip_id: String,
    pub evidence: Vec<Evidence>,
    pub original: MediaRepresentation,
    pub proxy: Option<MediaRepresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub kind: EvidenceKind,
    pub document_uri: String,
    pub media_uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    CameraMetadata,
    Ffprobe,
    /// A file QNC wrote itself: its facts are what QNC asked the encoder for.
    QncWritten,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact<T> {
    pub value: T,
    pub evidence_id: String,
    pub locator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signal<T> {
    Known(T),
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRepresentation {
    pub media_uri: String,
    pub container: Option<Fact<String>>,
    pub duration_seconds: Option<Fact<Rational>>,
    pub streams_complete: Option<Fact<bool>>,
    pub streams: Vec<MediaStream>,
    pub tags: BTreeMap<String, Fact<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaStream {
    pub index: Option<Fact<u32>>,
    pub codec: Option<Fact<Signal<String>>>,
    pub profile: Option<Fact<String>>,
    pub time_base: Option<Fact<Rational>>,
    pub start_pts: Option<Fact<i64>>,
    pub duration_ts: Option<Fact<i64>>,
    pub details: StreamDetails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "metadata", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamDetails {
    Video(Box<VideoMetadata>),
    Audio(Box<AudioMetadata>),
    Other { stream_type: Fact<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoMetadata {
    pub width: Option<Fact<u32>>,
    pub height: Option<Fact<u32>>,
    pub frame_rate: Option<Fact<FrameTimebase>>,
    pub frame_rate_mode: Option<Fact<FrameRateMode>>,
    pub frame_count: Option<Fact<FrameCount>>,
    pub scan_mode: Option<Fact<ScanMode>>,
    pub pixel_format: Option<Fact<String>>,
    pub sample_aspect_ratio: Option<Fact<Rational>>,
    pub rotation_degrees: Option<Fact<Signal<i32>>>,
    pub color: ColorMetadata,
}

impl VideoMetadata {
    /// An estimated count never silently becomes a frame-accurate boundary.
    pub fn exact_frame_count(&self) -> Option<u64> {
        match self.frame_count.as_ref()?.value {
            FrameCount::Exact(count) if count > 0 && count <= i64::MAX as u64 => Some(count),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "accuracy", content = "frames", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrameCount {
    Exact(u64),
    Estimated(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameRateMode {
    Constant,
    Variable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    Progressive,
    InterlacedTopFieldFirst,
    InterlacedBottomFieldFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorMetadata {
    pub primaries: Option<Fact<Signal<String>>>,
    pub transfer: Option<Fact<Signal<String>>>,
    pub matrix: Option<Fact<Signal<String>>>,
    pub range: Option<Fact<Signal<String>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioMetadata {
    pub sample_rate_hz: Option<Fact<u32>>,
    pub channels: Option<Fact<u32>>,
    pub sample_format: Option<Fact<Signal<String>>>,
    pub channel_layout: Option<Fact<Signal<String>>>,
    pub bits_per_sample: Option<Fact<u16>>,
}

/// The one video stream of a representation; `None` with none or several.
pub fn single_video(media: &MediaRepresentation) -> Option<&VideoMetadata> {
    let mut videos = media.streams.iter().filter_map(|s| match &s.details {
        StreamDetails::Video(v) => Some(v.as_ref()),
        _ => None,
    });
    let first = videos.next()?;
    videos.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_representation_reads_back_as_written_and_an_estimated_count_is_not_exact() {
        let json = r#"{"media_uri":"qnc://local/source/v/file/a","container":{"value":"mxf","evidence_id":"e1","locator":"/format"},"duration_seconds":null,"streams_complete":{"value":true,"evidence_id":"e1","locator":"/"},"streams":[{"index":{"value":0,"evidence_id":"e1","locator":"/s/0"},"codec":{"value":{"state":"known","value":"h264"},"evidence_id":"e1","locator":"/s/0"},"profile":null,"time_base":null,"start_pts":null,"duration_ts":null,"details":{"kind":"video","metadata":{"width":null,"height":null,"frame_rate":{"value":{"fps_num":50,"fps_den":1},"evidence_id":"e1","locator":"/"},"frame_rate_mode":null,"frame_count":{"value":{"accuracy":"estimated","frames":100},"evidence_id":"e1","locator":"/"},"scan_mode":null,"pixel_format":null,"sample_aspect_ratio":null,"rotation_degrees":null,"color":{"primaries":null,"transfer":null,"matrix":null,"range":null}}}}],"tags":{}}"#;
        let media: MediaRepresentation = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&media).unwrap(), json);
        assert_eq!(single_video(&media).unwrap().exact_frame_count(), None);
        assert!(FrameTimebase::new(0, 1).is_err());
    }
}
