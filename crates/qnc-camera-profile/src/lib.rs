//! A camera is a text profile, not code: one JSON file per model under the profiles folder
//! (`<manufacturer>/<model>.json`): the record format it writes (`reader`), the card layouts,
//! the rules that recognise it in the card's records, and the procedure (is the card's record
//! final when it states what a probe would, or is there always one probe). A new camera of a
//! known format is one new file. A file with an error is not taken and is named. The profile
//! a clip was read with is stamped into its record (`qnc.camera:profile` = `<id>@<version>`),
//! so a project knows how each clip was read, exactly as the old QNC wrote it.

use std::path::Path;

use qnc_media_model::{ClipMetadata, Fact, StreamDetails};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
/// The tag of a clip record that names the profile it was read with.
pub const RECORD_TAG: &str = "qnc.camera:profile";
/// The reader of the Sony MediaProfile format (`qnc-sony-index` + `qnc-sony-clip`).
pub const SONY_READER: &str = "camera.sony.index.read";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub format_version: u32,
    pub id: String,
    pub version: u32,
    pub manufacturer: String,
    pub model: String,
    /// The profile of a card of this record format whose camera no profile recognises.
    #[serde(default)]
    pub default_for_reader: bool,
    pub reader: String,
    pub layouts: Vec<String>,
    pub recognise: Vec<Rule>,
    pub procedure: Procedure,
    #[serde(default)]
    pub notes: String,
}

/// An element attribute in the card's records that names the camera.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub element: String,
    pub attribute: String,
    #[serde(default)]
    pub equals: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Procedure {
    pub metadata: MetadataRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataRule {
    /// The card record is the final record when it states what a probe would; else one probe.
    DeclaredWhenComplete,
    /// Always one probe.
    ProbeOnce,
}

impl Profile {
    /// `<id>@<version>`, as written into a clip record.
    pub fn stamp(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != FORMAT_VERSION {
            return Err(format!("format_version {} is not {FORMAT_VERSION}", self.format_version));
        }
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(format!("id '{}' must be lowercase letters, digits and '-'", self.id));
        }
        if self.version == 0 || self.manufacturer.trim().is_empty() || self.reader.trim().is_empty() {
            return Err("version, manufacturer and reader are required".into());
        }
        if self.default_for_reader != self.recognise.is_empty() {
            return Err("a default profile recognises nothing; any other needs at least one rule".into());
        }
        if !self.default_for_reader && self.model.trim().is_empty() {
            return Err("model is required".into());
        }
        for rule in &self.recognise {
            if rule.element.trim().is_empty() || rule.attribute.trim().is_empty() || rule.equals.is_some() == rule.prefix.is_some() {
                return Err(format!("rule on {}/@{} needs an element, an attribute and exactly one of equals or prefix", rule.element, rule.attribute));
            }
        }
        Ok(())
    }

    fn matches(&self, records: &[&str]) -> bool {
        self.recognise.iter().any(|rule| {
            records.iter().any(|text| {
                attribute_values(text, &rule.element, &rule.attribute).iter().any(|value| match (&rule.equals, &rule.prefix) {
                    (Some(equals), _) => value == equals,
                    (_, Some(prefix)) => value.starts_with(prefix.as_str()),
                    _ => false,
                })
            })
        })
    }
}

/// Every `*.json` under `dir` (and its folders) that is a valid profile, and what was wrong
/// with each one that is not. Ids must not repeat.
pub fn read_profiles(dir: &Path) -> (Vec<Profile>, Vec<String>) {
    let mut profiles: Vec<Profile> = Vec::new();
    let mut refused = Vec::new();
    let mut files = Vec::new();
    collect(dir, &mut files);
    files.sort();
    for file in files {
        let name = file.display().to_string();
        let profile = std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Profile>(&text).map_err(|e| e.to_string()))
            .and_then(|p| p.validate().map(|_| p));
        match profile {
            Ok(p) if profiles.iter().any(|q| q.id == p.id) => refused.push(format!("{name}: id '{}' is already taken", p.id)),
            Ok(p) => profiles.push(p),
            Err(error) => refused.push(format!("{name}: {error}")),
        }
    }
    (profiles, refused)
}

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            files.push(path);
        }
    }
}

/// The camera of a card's `records` (XML texts) for the format `reader`: the recognised
/// profile, else the format's default (`recognised` false), else none.
pub fn recognise<'a>(profiles: &'a [Profile], reader: &str, records: &[&str]) -> Option<(&'a Profile, bool)> {
    let of_reader = || profiles.iter().filter(move |p| p.reader == reader);
    of_reader().find(|p| !p.default_for_reader && p.matches(records)).map(|p| (p, true)).or_else(|| of_reader().find(|p| p.default_for_reader).map(|p| (p, false)))
}

/// Stamps the profile into the clip record, on the evidence of its sidecar (else the first).
pub fn stamp(metadata: &mut ClipMetadata, profile: &Profile, recognised: bool) {
    let evidence = metadata.evidence.iter().find(|e| e.id.starts_with("sidecar")).or(metadata.evidence.first()).map(|e| e.id.clone());
    if let Some(evidence_id) = evidence {
        let locator = if recognised { "camera-profile:recognised" } else { "camera-profile:default" };
        metadata.original.tags.insert(RECORD_TAG.into(), Fact { value: profile.stamp(), evidence_id, locator: locator.into() });
    }
}

/// The profile a record was read with, by its stamp.
pub fn profile_of<'a>(profiles: &'a [Profile], metadata: &ClipMetadata) -> Option<&'a Profile> {
    let stamp = &metadata.original.tags.get(RECORD_TAG)?.value;
    profiles.iter().find(|p| &p.stamp() == stamp)
}

/// Whether the card's record states what a probe would (size, rate, exact count of the
/// picture; or length and sound format when there is no picture).
pub fn has_probe_facts(metadata: &ClipMetadata) -> bool {
    let original = &metadata.original;
    let videos: Vec<_> = original.streams.iter().filter_map(|s| match &s.details {
        StreamDetails::Video(video) => Some(video),
        _ => None,
    }).collect();
    if !videos.is_empty() {
        return videos.iter().any(|v| v.width.is_some() && v.height.is_some() && v.frame_rate.is_some() && v.exact_frame_count().is_some());
    }
    original.duration_seconds.is_some()
        && original.streams.iter().any(|s| match &s.details {
            StreamDetails::Audio(audio) => audio.sample_rate_hz.is_some() && audio.channels.is_some(),
            _ => false,
        })
}

/// Whether the record needs the one probe before it is final, by its profile's procedure.
pub fn needs_probe(profile: &Profile, metadata: &ClipMetadata) -> bool {
    match profile.procedure.metadata {
        MetadataRule::DeclaredWhenComplete => !has_probe_facts(metadata),
        MetadataRule::ProbeOnce => true,
    }
}

/// The values of `attribute` on every `element` of an XML text (local names; a text that is
/// not well formed gives what was read before the error).
fn attribute_values(text: &str, element: &str, attribute: &str) -> Vec<String> {
    use xml::reader::{EventReader, XmlEvent};
    let mut values = Vec::new();
    for event in EventReader::from_str(text) {
        match event {
            Ok(XmlEvent::StartElement { name, attributes, .. }) if name.local_name == element => {
                values.extend(attributes.into_iter().filter(|a| a.name.local_name == attribute).map(|a| a.value));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDECAR: &str = include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML");

    #[test]
    fn the_profiles_shipped_read_the_fx6_is_recognised_by_its_sidecar_and_a_bad_file_is_named() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/cameras/profiles");
        let (profiles, refused) = read_profiles(&dir);
        assert!(refused.is_empty(), "{refused:?}");
        assert!(profiles.len() >= 5);
        let (profile, recognised) = recognise(&profiles, SONY_READER, &[SIDECAR]).unwrap();
        assert_eq!((profile.stamp().as_str(), recognised), ("sony-ilme-fx6v@1", true));
        let (fallback, recognised) = recognise(&profiles, SONY_READER, &["<x/>"]).unwrap();
        assert!(fallback.default_for_reader && !recognised);
        assert!(recognise(&profiles, "other.reader", &[SIDECAR]).is_none());
        let bad = std::env::temp_dir().join(format!("qnc-camera-profile-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&bad);
        std::fs::write(bad.join("x.json"), r#"{"format_version":1,"id":"X","version":1,"manufacturer":"a","model":"m","reader":"r","layouts":[],"recognise":[{"element":"a","attribute":"b","equals":"c"}],"procedure":{"metadata":"probe_once"}}"#).unwrap();
        let (none, refused) = read_profiles(&bad);
        assert!(none.is_empty() && refused[0].contains("lowercase"));
        let _ = std::fs::remove_dir_all(&bad);
    }
}
