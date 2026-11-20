//! The camera's facts of one clip: what the Sony index says of the material (container, codec
//! label, rate, frame count, aspect) and what its `NonRealTimeMeta` sidecar adds (creation
//! time, layout, the exact duration of a normal progressive recording). Every fact carries
//! the evidence it came from and its locator in the document, exactly as the old QNC saved
//! them, so a record made here is the record the old QNC made. Index and sidecar that
//! disagree are a conflict, an error; nothing is preferred silently and nothing is guessed.

mod facts;

use std::collections::BTreeMap;

use qnc_media_model::*;
use qnc_sony_index::{CameraIndex, ClipBinding, IndexedMedia, Material};
use serde::{Deserialize, Serialize};

pub const SIDECAR_NAMESPACE: &str = "urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.20";

/// The M01 sidecar of a material: its path on the card, its URI as a document, its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sidecar {
    pub relative_path: String,
    pub document_uri: String,
    pub text: String,
}

/// Something the camera's documents did not settle; a `conflict` fails the read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    pub code: String,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub metadata: ClipMetadata,
    pub notices: Vec<Notice>,
}

pub(crate) fn fact<T>(id: &str, locator: &str, value: T) -> Fact<T> {
    Fact { value, evidence_id: id.into(), locator: locator.into() }
}

pub(crate) fn notice(notices: &mut Vec<Notice>, code: &str, field: &str, message: &str) {
    notices.push(Notice { code: code.into(), field: field.into(), message: message.into() });
}

fn evidence(id: &str, document_uri: &str, media_uri: &str) -> Evidence {
    Evidence { id: id.into(), kind: EvidenceKind::CameraMetadata, document_uri: document_uri.into(), media_uri: media_uri.into() }
}

fn bound(media: &IndexedMedia, bound: &qnc_sony_index::BoundMedia) -> Result<(), String> {
    qnc_uri::validate_resource_uri(&bound.media_uri)?;
    if qnc_sony_index::relative_reference(&bound.relative_path)? != media.relative_path {
        return Err("binding does not match indexed reference".into());
    }
    Ok(())
}

/// The clip's camera-phase metadata from material `material_index` of the index, bound to the
/// card, with its sidecar when the index relates one and the caller read it.
pub fn read_clip(index: &CameraIndex, material_index: usize, binding: &ClipBinding, sidecar: Option<&Sidecar>) -> Result<Read, String> {
    qnc_uri::validate_resource_uri(&index.document_uri)?;
    qnc_uri::valid_id(&binding.clip_id)?;
    let material: &Material = index.materials.get(material_index).ok_or("material index out of bounds")?;
    bound(&material.original, &binding.original)?;
    if material.proxies.len() > 1 {
        return Err("multiple proxies require an explicit representation contract".into());
    }
    let proxy_pair = match (material.proxies.first(), &binding.proxy) {
        (Some(media), Some(b)) => {
            bound(media, b)?;
            Some((media, b))
        }
        (None, None) => None,
        _ => return Err("proxy binding must match the indexed proxy; unresolved references cannot be dropped".into()),
    };
    if binding.proxy.as_ref().is_some_and(|p| p.media_uri == binding.original.media_uri) {
        return Err("original and proxy cannot share a media identity".into());
    }
    let mut notices = Vec::new();
    let mut evidences = vec![evidence("index-original", &index.document_uri, &binding.original.media_uri)];
    let mut original = facts::representation(&material.original, &binding.original.media_uri, "index-original", "original", &mut notices);
    for (locator, value) in &index.properties {
        original.tags.insert(format!("sony.index:{locator}"), fact("index-original", locator, value.clone()));
    }
    let mut proxy = proxy_pair.map(|(media, b)| {
        evidences.push(evidence("index-proxy", &index.document_uri, &b.media_uri));
        facts::representation(media, &b.media_uri, "index-proxy", "proxy", &mut notices)
    });
    let mut normal_progressive = false;
    if let Some(sidecar) = sidecar {
        let path = qnc_sony_index::relative_reference(&sidecar.relative_path)?;
        if !material.related.iter().any(|r| r.kind == "XML" && r.relative_path == path) {
            return Err("sidecar is not explicitly related to this Material".into());
        }
        qnc_uri::validate_resource_uri(&sidecar.document_uri)?;
        let root = qnc_xml::parse(&sidecar.text, "NonRealTimeMeta", SIDECAR_NAMESPACE)?;
        let target = root.one("TargetMaterial")?.ok_or("missing TargetMaterial")?.required("umidRef")?;
        if !target.eq_ignore_ascii_case(material.original.attributes.get("umid").ok_or("missing original UMID")?) {
            return Err("sidecar UMID does not match the original".into());
        }
        evidences.push(evidence("sidecar-original", &sidecar.document_uri, &binding.original.media_uri));
        normal_progressive = facts::apply_sidecar(&root, &material.original, &mut original, &mut notices)?;
    }
    if normal_progressive {
        if let (Some((camera, _)), Some(proxy)) = (proxy_pair, &mut proxy) {
            if facts::video_ref(proxy).and_then(|v| v.frame_rate.as_ref()).is_some() {
                if let Some(count) = camera.attributes.get("dur").and_then(|d| facts::positive(d, "proxy.frame_count", &mut notices)) {
                    facts::promote_video_count(proxy, "index-proxy", &format!("{}/@dur", camera.locator), count);
                }
            }
        }
    } else {
        notice(&mut notices, "unresolved", "duration", "exact duration requires a supported normal progressive sidecar and consistent index facts");
    }
    // The original only: the XML counts video edit units, and a proxy container is longer
    // than its video (its audio pads it), so its length is not the XML count.
    facts::promote_duration(&mut original, "original.duration_seconds", &mut notices);
    notice(&mut notices, "incomplete", "streams", "XML channel descriptions do not prove container stream indices, timing or complete audio formats");
    if notices.iter().any(|n| n.code == "conflict") {
        return Err("conflicting camera metadata".into());
    }
    Ok(Read {
        metadata: ClipMetadata { contract_id: CONTRACT_ID.into(), contract_version: CONTRACT_VERSION.into(), clip_id: binding.clip_id.clone(), evidence: evidences, original, proxy },
        notices,
    })
}

pub(crate) fn empty_tags() -> BTreeMap<String, Fact<String>> {
    BTreeMap::new()
}

#[cfg(test)]
mod tests;
