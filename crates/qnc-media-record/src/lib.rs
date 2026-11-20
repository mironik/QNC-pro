//! The saved record of a clip's media, exactly as QNC stores it (`qnc.media.records` 0.2.0):
//! the binding to the source record and the original/proxy media, the phase (what the camera
//! said, or final after the one probe), the completeness, the metadata with its facts and the
//! report of what is missing. A record reads back only when its report is what the facts
//! give: nothing in it is taken on trust.

mod check;

use qnc_media_model::{ClipMetadata, EvidenceKind};
use serde::{Deserialize, Serialize};

pub use check::{inspect, IssueCode, MetadataIssue, MetadataReport};

pub const VERSION: &str = "0.2.0";
pub const CONTRACT_ID: &str = "qnc.media.records";
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    InvalidRequest,
    InvalidMetadata,
    TooLarge,
    Protocol,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "media record: {self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// What the card's documents said; the probe is still to come.
    Camera,
    /// After the one probe; never changes again.
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Partial,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source_index_uri: String,
    pub source_record_id: String,
    pub original_uri: String,
    pub proxy_uri: Option<String>,
}

impl Binding {
    pub fn validate(&self, metadata: &ClipMetadata) -> Result<(), Error> {
        let index = qnc_uri::parse(&self.source_index_uri).map_err(|_| Error::InvalidRequest)?;
        if index.resource_kind != "db" || index.resource_id != "source_index" {
            return Err(Error::InvalidRequest);
        }
        qnc_uri::valid_id(&self.source_record_id).map_err(|_| Error::InvalidRequest)?;
        if self.original_uri != metadata.original.media_uri || self.proxy_uri.as_deref() != metadata.proxy.as_ref().map(|p| p.media_uri.as_str()) {
            return Err(Error::InvalidMetadata);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub binding: Binding,
    pub revision: u32,
    pub phase: Phase,
    pub completeness: Completeness,
    pub metadata: ClipMetadata,
    pub report: MetadataReport,
    pub recorded_at_unix_ms: u64,
}

impl Snapshot {
    /// The record is sound: the binding matches, the revision fits the phase, and the report
    /// and completeness are exactly what the facts give.
    pub fn validate(&self) -> Result<(), Error> {
        self.binding.validate(&self.metadata)?;
        if !matches!(self.revision, 1 | 2) || (self.revision == 2 && self.phase != Phase::Final) {
            return Err(Error::Protocol);
        }
        let report = inspect_phase(&self.metadata, self.phase)?;
        if self.report != report || self.completeness != completeness(&report) {
            return Err(Error::Protocol);
        }
        Ok(())
    }

    pub fn clip_id(&self) -> &str {
        &self.metadata.clip_id
    }
}

/// The report of the facts in a phase: a camera record may not carry a probe; invalid facts
/// are refused, missing ones are reported.
pub fn inspect_phase(metadata: &ClipMetadata, phase: Phase) -> Result<MetadataReport, Error> {
    qnc_uri::valid_id(&metadata.clip_id).map_err(|_| Error::InvalidMetadata)?;
    if metadata.evidence.len() > 64 || std::iter::once(&metadata.original).chain(&metadata.proxy).any(|r| r.streams.len() > 128 || r.tags.len() > 512) {
        return Err(Error::TooLarge);
    }
    if serde_json::to_vec(metadata).map_err(|_| Error::InvalidRequest)?.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    if phase == Phase::Camera && metadata.evidence.iter().any(|e| e.kind == EvidenceKind::Ffprobe) {
        return Err(Error::InvalidMetadata);
    }
    let report = inspect(metadata);
    if report.issues.iter().any(|i| i.code == IssueCode::Invalid) {
        return Err(Error::InvalidMetadata);
    }
    Ok(report)
}

pub fn completeness(report: &MetadataReport) -> Completeness {
    if report.is_complete() {
        Completeness::Complete
    } else {
        Completeness::Partial
    }
}

#[cfg(test)]
mod tests;
