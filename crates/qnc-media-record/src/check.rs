//! What is missing or invalid among the facts of a clip's metadata. Only supplied facts are
//! judged; nothing is read, repaired or probed. The paths and messages are the old record's
//! to the letter, because a saved report must equal what these facts give.

use std::collections::{BTreeMap, BTreeSet};

use qnc_media_model::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    Missing,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataIssue {
    pub path: String,
    pub code: IssueCode,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataReport {
    pub issues: Vec<MetadataIssue>,
}

impl MetadataReport {
    pub fn is_complete(&self) -> bool {
        self.issues.is_empty()
    }

    /// The paths of what is missing under `prefix` (a representation).
    pub fn missing_under(&self, prefix: &str) -> Vec<String> {
        self.issues.iter().filter(|i| i.path == prefix || i.path.starts_with(&format!("{prefix}."))).map(|i| i.path.clone()).collect()
    }
}

pub fn inspect(record: &ClipMetadata) -> MetadataReport {
    let mut check = Check { evidence: BTreeMap::new(), report: MetadataReport::default() };
    check.valid("contract_id", record.contract_id == CONTRACT_ID, "unsupported contract");
    check.valid("contract_version", record.contract_version == CONTRACT_VERSION, "unsupported version");
    check.text("clip_id", &record.clip_id);
    for (i, evidence) in record.evidence.iter().enumerate() {
        let path = format!("evidence[{i}]");
        check.text(&format!("{path}.id"), &evidence.id);
        check.uri(&format!("{path}.document_uri"), &evidence.document_uri);
        check.uri(&format!("{path}.media_uri"), &evidence.media_uri);
        let related = evidence.media_uri == record.original.media_uri || record.proxy.as_ref().is_some_and(|p| p.media_uri == evidence.media_uri);
        check.valid(&format!("{path}.media_uri"), related, "evidence refers to an unrelated media representation");
        let duplicate = check.evidence.insert(&evidence.id, evidence).is_some();
        check.valid(&format!("{path}.id"), !duplicate, "duplicate evidence id");
    }
    check.representation("original", &record.original);
    if let Some(proxy) = &record.proxy {
        check.valid("proxy.media_uri", proxy.media_uri != record.original.media_uri, "original and proxy must be separate representations");
        check.representation("proxy", proxy);
        check.proxy_timing(&record.original, proxy);
    }
    check.report
}

struct Check<'a> {
    evidence: BTreeMap<&'a str, &'a Evidence>,
    report: MetadataReport,
}

impl Check<'_> {
    fn issue(&mut self, path: &str, code: IssueCode, message: &str) {
        self.report.issues.push(MetadataIssue { path: path.into(), code, message: message.into() });
    }

    fn valid(&mut self, path: &str, valid: bool, message: &str) {
        if !valid {
            self.issue(path, IssueCode::Invalid, message);
        }
    }

    fn text(&mut self, path: &str, text: &str) {
        self.valid(path, !text.trim().is_empty(), "empty value");
    }

    fn known_text(&mut self, path: &str, text: &str) {
        self.text(path, text);
        let placeholder = matches!(text.trim().to_ascii_lowercase().as_str(), "unknown" | "unspecified" | "n/a" | "none");
        self.valid(path, !placeholder, "missing value cannot be a known string");
    }

    fn uri(&mut self, path: &str, uri: &str) {
        self.valid(path, qnc_uri::validate_resource_uri(uri).is_ok() && uri.len() <= 8192, "expected an OS-neutral QNC resource URI");
    }

    fn fact<'b, T>(&mut self, path: &str, fact: &'b Fact<T>, media_uri: &str) -> &'b T {
        self.text(&format!("{path}.locator"), &fact.locator);
        match self.evidence.get(fact.evidence_id.as_str()) {
            Some(evidence) if evidence.media_uri == media_uri => {}
            Some(_) => self.issue(path, IssueCode::Invalid, "fact evidence belongs to another representation"),
            None => self.issue(path, IssueCode::Invalid, "fact references missing evidence"),
        }
        &fact.value
    }

    fn field<'b, T>(&mut self, path: &str, fact: &'b Option<Fact<T>>, media_uri: &str, required: bool) -> Option<&'b T> {
        match fact {
            Some(fact) => Some(self.fact(path, fact, media_uri)),
            None => {
                if required {
                    self.issue(path, IssueCode::Missing, "required metadata is missing");
                }
                None
            }
        }
    }

    fn positive<T: Copy + From<u8> + PartialOrd>(&mut self, path: &str, fact: &Option<Fact<T>>, uri: &str, required: bool) {
        if let Some(value) = self.field(path, fact, uri, required) {
            self.valid(path, *value > T::from(0), "value must be positive");
        }
    }

    fn string(&mut self, path: &str, fact: &Option<Fact<String>>, uri: &str, required: bool) {
        if let Some(value) = self.field(path, fact, uri, required) {
            self.known_text(path, value);
        }
    }

    fn signal(&mut self, path: &str, fact: &Option<Fact<Signal<String>>>, uri: &str) {
        if let Some(Signal::Known(value)) = self.field(path, fact, uri, true) {
            self.known_text(path, value);
        }
    }

    fn rational(&mut self, path: &str, fact: &Option<Fact<Rational>>, uri: &str) {
        if let Some(value) = self.field(path, fact, uri, true) {
            self.valid(path, value.numerator > 0 && value.denominator > 0, "ratio must have positive numerator and denominator");
        }
    }

    fn representation(&mut self, path: &str, media: &MediaRepresentation) {
        let uri = &media.media_uri;
        self.uri(&format!("{path}.media_uri"), uri);
        self.string(&format!("{path}.container"), &media.container, uri, true);
        self.rational(&format!("{path}.duration_seconds"), &media.duration_seconds, uri);
        if let Some(false) = self.field(&format!("{path}.streams_complete"), &media.streams_complete, uri, true) {
            self.issue(&format!("{path}.streams_complete"), IssueCode::Missing, "stream inventory is not complete");
        }
        if media.streams.is_empty() {
            self.issue(&format!("{path}.streams"), IssueCode::Missing, "stream map is not known yet");
        } else {
            let av = media.streams.iter().any(|s| matches!(s.details, StreamDetails::Audio(_) | StreamDetails::Video(_)));
            self.valid(&format!("{path}.streams"), av, "clip must contain audio or video");
        }
        let mut indices = BTreeSet::new();
        for (i, stream) in media.streams.iter().enumerate() {
            let prefix = format!("{path}.streams[{i}]");
            if let Some(index) = self.field(&format!("{prefix}.index"), &stream.index, uri, true) {
                self.valid(&format!("{prefix}.index"), indices.insert(*index), "duplicate stream index");
            }
            let av = !matches!(stream.details, StreamDetails::Other { .. });
            let codec_path = format!("{prefix}.codec");
            match self.field(&codec_path, &stream.codec, uri, true) {
                Some(Signal::Known(value)) => self.known_text(&codec_path, value),
                Some(Signal::Unspecified) if av => self.issue(&codec_path, IssueCode::Missing, "audio/video requires a known codec"),
                _ => {}
            }
            self.string(&format!("{prefix}.profile"), &stream.profile, uri, false);
            if av || stream.time_base.is_some() {
                self.rational(&format!("{prefix}.time_base"), &stream.time_base, uri);
            }
            self.field(&format!("{prefix}.start_pts"), &stream.start_pts, uri, av);
            self.positive(&format!("{prefix}.duration_ts"), &stream.duration_ts, uri, av);
            match &stream.details {
                StreamDetails::Video(video) => self.video(&format!("{prefix}.video"), video, uri),
                StreamDetails::Audio(audio) => {
                    let prefix = format!("{prefix}.audio");
                    self.positive(&format!("{prefix}.sample_rate_hz"), &audio.sample_rate_hz, uri, true);
                    self.positive(&format!("{prefix}.channels"), &audio.channels, uri, true);
                    self.positive(&format!("{prefix}.bits_per_sample"), &audio.bits_per_sample, uri, false);
                    self.signal(&format!("{prefix}.sample_format"), &audio.sample_format, uri);
                    self.signal(&format!("{prefix}.channel_layout"), &audio.channel_layout, uri);
                }
                StreamDetails::Other { stream_type } => {
                    let path = format!("{prefix}.other.stream_type");
                    let value = self.fact(&path, stream_type, uri);
                    self.known_text(&path, value);
                    let typed = !matches!(value.trim().to_ascii_lowercase().as_str(), "video" | "audio");
                    self.valid(&path, typed, "audio/video cannot bypass typed stream metadata");
                }
            }
        }
        for (name, fact) in &media.tags {
            let path = format!("{path}.tags[{name}]");
            self.text(&path, name);
            self.fact(&path, fact, uri);
        }
    }

    fn video(&mut self, path: &str, video: &VideoMetadata, uri: &str) {
        self.positive(&format!("{path}.width"), &video.width, uri, true);
        self.positive(&format!("{path}.height"), &video.height, uri, true);
        if let Some(rate) = self.field(&format!("{path}.frame_rate"), &video.frame_rate, uri, true) {
            self.valid(&format!("{path}.frame_rate"), FrameTimebase::new(rate.fps_num, rate.fps_den).is_ok(), "invalid rational frame rate");
        }
        self.field(&format!("{path}.frame_rate_mode"), &video.frame_rate_mode, uri, true);
        if let Some(FrameCount::Exact(count) | FrameCount::Estimated(count)) = self.field(&format!("{path}.frame_count"), &video.frame_count, uri, true) {
            self.valid(&format!("{path}.frame_count"), *count > 0 && *count <= i64::MAX as u64, "frame count must fit a positive signed 64-bit frame position");
        }
        self.field(&format!("{path}.scan_mode"), &video.scan_mode, uri, true);
        self.string(&format!("{path}.pixel_format"), &video.pixel_format, uri, true);
        self.rational(&format!("{path}.sample_aspect_ratio"), &video.sample_aspect_ratio, uri);
        self.field(&format!("{path}.rotation_degrees"), &video.rotation_degrees, uri, true);
        self.signal(&format!("{path}.color.primaries"), &video.color.primaries, uri);
        self.signal(&format!("{path}.color.transfer"), &video.color.transfer, uri);
        self.signal(&format!("{path}.color.matrix"), &video.color.matrix, uri);
        self.signal(&format!("{path}.color.range"), &video.color.range, uri);
    }

    fn proxy_timing(&mut self, original: &MediaRepresentation, proxy: &MediaRepresentation) {
        let Some((a, b)) = single_video(original).zip(single_video(proxy)) else { return };
        if let Some((a_count, b_count)) = a.exact_frame_count().zip(b.exact_frame_count()) {
            self.valid("proxy.video.frame_count", a_count == b_count, "proxy and original exact frame counts differ");
        }
        let constant = |v: &VideoMetadata| v.frame_rate_mode.as_ref().map(|f| f.value) == Some(FrameRateMode::Constant);
        if constant(a) && constant(b) {
            if let Some((a, b)) = a.frame_rate.as_ref().zip(b.frame_rate.as_ref()) {
                let (a, b) = (a.value, b.value);
                if FrameTimebase::new(a.fps_num, a.fps_den).is_ok() && FrameTimebase::new(b.fps_num, b.fps_den).is_ok() {
                    let same = i128::from(a.fps_num) * i128::from(b.fps_den) == i128::from(b.fps_num) * i128::from(a.fps_den);
                    self.valid("proxy.video.frame_rate", same, "proxy and original constant frame rates differ");
                }
            }
        }
    }
}
