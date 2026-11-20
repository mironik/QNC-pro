//! The report ffprobe wrote (`-show_format -show_streams ... -of json`, and the first decoded
//! pictures when the summary did not know the colour or scan) as the saved description of the
//! medium: every value a fact with the evidence and a JSON pointer into the report. Kept to
//! the letter of the old QNC so a record made here is the record it made. Nothing runs here.

mod fields;

use std::collections::{BTreeMap, BTreeSet};

use qnc_media_model::*;
use qnc_media_record::{inspect, IssueCode};
use serde_json::Value;

pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidDocument,
    InvalidBinding,
    InvalidField(String),
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ffprobe report: {self:?}")
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub evidence: Evidence,
    pub media: MediaRepresentation,
}

/// The report `text` of `media_uri`, kept as document `document_uri`, as evidence `evidence_id`.
/// The report's `format.filename` must be the media URI (the probe wrote it so).
pub fn read(text: &str, media_uri: &str, document_uri: &str, evidence_id: &str) -> Result<Parsed> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::TooLarge);
    }
    qnc_uri::validate_resource_uri(media_uri).map_err(|_| Error::InvalidBinding)?;
    qnc_uri::validate_resource_uri(document_uri).map_err(|_| Error::InvalidBinding)?;
    if evidence_id.is_empty() || evidence_id.len() > 128 {
        return Err(Error::InvalidBinding);
    }
    let root: Value = serde_json::from_str(text).map_err(|_| Error::InvalidDocument)?;
    if root.get("error").is_some() {
        return Err(Error::InvalidDocument);
    }
    let format = root.get("format").filter(|f| f.is_object()).ok_or(Error::InvalidDocument)?;
    if format.get("filename").and_then(Value::as_str) != Some(media_uri) {
        return Err(Error::InvalidBinding);
    }
    let streams = root.get("streams").and_then(Value::as_array).ok_or(Error::InvalidDocument)?;
    if streams.is_empty() || streams.len() > 128 {
        return Err(Error::InvalidDocument);
    }
    let frames = match root.get("frames") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(frames)) if frames.len() <= 256 => frames.as_slice(),
        _ => return Err(Error::InvalidDocument),
    };
    let reader = fields::Reader { id: evidence_id };
    let mut media = MediaRepresentation {
        media_uri: media_uri.into(),
        container: reader.string(format, "format_name", "/format")?,
        duration_seconds: reader.ratio(format, "duration", "/format")?,
        streams_complete: Some(reader.fact("/streams", true)),
        streams: Vec::new(),
        tags: BTreeMap::new(),
    };
    reader.tags(format, "/format", &mut media.tags)?;
    let mut indices = BTreeSet::new();
    for (i, stream) in streams.iter().enumerate() {
        let path = format!("/streams/{i}");
        let index = reader.integer::<u32>(stream, "index", &path)?.ok_or_else(|| Error::InvalidField(path.clone()))?;
        if !indices.insert(index.value) {
            return Err(Error::InvalidField(format!("{path}/index")));
        }
        let kind = reader.string(stream, "codec_type", &path)?.ok_or_else(|| Error::InvalidField(path.clone()))?;
        let details = match kind.value.as_str() {
            "video" => StreamDetails::Video(Box::new(reader.video(stream, &path, index.value, frames)?)),
            "audio" => StreamDetails::Audio(Box::new(AudioMetadata {
                sample_rate_hz: reader.integer(stream, "sample_rate", &path)?,
                channels: reader.integer(stream, "channels", &path)?,
                sample_format: reader.signal(stream, "sample_fmt", &path)?,
                channel_layout: reader.signal(stream, "channel_layout", &path)?,
                bits_per_sample: reader.integer::<u16>(stream, "bits_per_raw_sample", &path)?.filter(|v| v.value > 0).or(reader.integer::<u16>(stream, "bits_per_sample", &path)?.filter(|v| v.value > 0)),
            })),
            _ => StreamDetails::Other { stream_type: kind },
        };
        media.streams.push(MediaStream {
            index: Some(index),
            codec: reader.signal(stream, "codec_name", &path)?,
            profile: reader.string(stream, "profile", &path)?,
            time_base: reader.ratio(stream, "time_base", &path)?,
            start_pts: reader.integer(stream, "start_pts", &path)?,
            duration_ts: reader.integer(stream, "duration_ts", &path)?,
            details,
        });
        reader.tags(stream, &path, &mut media.tags)?;
        for key in ["codec_tag_string", "codec_tag"] {
            if let Some(value) = reader.raw(stream, key, &path)? {
                let pointer = format!("{path}/{key}");
                media.tags.insert(format!("ffprobe:{pointer}"), reader.fact(&pointer, value));
            }
        }
        if media.tags.len() > 512 {
            return Err(Error::TooLarge);
        }
    }
    let evidence = Evidence { id: evidence_id.into(), kind: EvidenceKind::Ffprobe, document_uri: document_uri.into(), media_uri: media_uri.into() };
    let clip = ClipMetadata { contract_id: CONTRACT_ID.into(), contract_version: CONTRACT_VERSION.into(), clip_id: "validation".into(), evidence: vec![evidence.clone()], original: media.clone(), proxy: None };
    if let Some(issue) = inspect(&clip).issues.into_iter().find(|i| i.code == IssueCode::Invalid) {
        return Err(Error::InvalidField(issue.path));
    }
    Ok(Parsed { evidence, media })
}

/// Whether a picture stream of a summary leaves its colour or field order unknown: then the
/// probe decodes the first pictures too.
pub fn summary_lacks_picture_facts(json: &Value) -> bool {
    let known = |stream: &Value, field: &str| stream.get(field).and_then(|v| v.as_str()).is_some_and(|v| !v.is_empty() && v != "unknown");
    json.get("streams").and_then(|v| v.as_array()).is_some_and(|streams| {
        streams
            .iter()
            .filter(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("video"))
            .any(|stream| !known(stream, "color_primaries") || !known(stream, "color_space") || !known(stream, "field_order"))
    })
}

#[cfg(test)]
mod tests;
