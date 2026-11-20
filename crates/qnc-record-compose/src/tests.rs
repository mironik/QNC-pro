use qnc_media_record::{Binding, Completeness, Phase, Snapshot};
use qnc_media_model::Fact;

use crate::*;

const INDEX: &str = include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML");
const SIDECAR: &str = include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML");
const REPORT: &str = include_str!("../../qnc-ffprobe-read/tests/probe_1483.json");
/// The record the old QNC saved for the same clip from the same documents and the same medium.
const SAVED: &str = include_str!("../../qnc-media-record/tests/snapshot.json");

/// The camera record of Mironik 1483 as the tray would make it: the index and sidecar of
/// the card, the original bound to its copy in the project, the proxy to the card.
fn camera_record(saved: &Snapshot) -> Snapshot {
    let index = qnc_sony_index::read_index(INDEX, &saved.metadata.evidence[0].document_uri).unwrap();
    let i = index.materials.iter().position(|m| m.original.relative_path == "Clip/Mironik 1483.MXF").unwrap();
    let mut binding = qnc_sony_index::bind(&index.materials[i], saved.clip_id(), "qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT").unwrap();
    binding.original.media_uri = saved.binding.original_uri.clone();
    let sidecar = qnc_sony_clip::Sidecar { relative_path: "Clip/Mironik 1483M01.XML".into(), document_uri: saved.metadata.evidence[2].document_uri.clone(), text: SIDECAR.into() };
    let metadata = qnc_sony_clip::read_clip(&index, i, &binding, Some(&sidecar)).unwrap().metadata;
    let report = inspect_phase(&metadata, Phase::Camera).unwrap();
    Snapshot { binding: Binding { source_index_uri: saved.binding.source_index_uri.clone(), source_record_id: saved.binding.source_record_id.clone(), original_uri: binding.original.media_uri, proxy_uri: binding.proxy.map(|p| p.media_uri) }, revision: 1, phase: Phase::Camera, completeness: Completeness::Partial, metadata, report, recorded_at_unix_ms: 1 }
}

#[test]
fn the_camera_record_probed_once_is_the_final_record_the_old_qnc_saved() {
    let saved: Snapshot = serde_json::from_str(SAVED).unwrap();
    let mut camera = camera_record(&saved);
    // The camera profile block (not here) names the profile the clip was read with; the
    // saved record carries it as a tag of the original.
    camera.metadata.original.tags.insert("qnc.camera:profile".into(), Fact { value: "sony-ilme-fx6v@1".into(), evidence_id: "sidecar-original".into(), locator: "camera-profile:recognised".into() });
    camera.report = inspect_phase(&camera.metadata, Phase::Camera).unwrap();
    camera.validate().unwrap();
    // Both media lack something; this project plays the original, so only it is probed.
    let needed = required_probes(&camera).unwrap();
    assert_eq!(needed, vec![camera.binding.original_uri.clone(), camera.binding.proxy_uri.clone().unwrap()]);
    let probe_uri = &saved.metadata.evidence[3].document_uri;
    let probe = qnc_ffprobe_read::read(REPORT, &needed[0], probe_uri, "ffprobe-0").unwrap();
    let done = finalize(&camera, &[probe.clone()], saved.recorded_at_unix_ms).unwrap();
    assert_eq!(done.phase, Phase::Final);
    assert_eq!(done.revision, 2);
    assert_eq!(done.completeness, Completeness::Partial, "the proxy was not probed, as in the saved record");
    assert_eq!(done, saved, "fact for fact the record the old QNC saved");
    assert_eq!(required_probes(&done).unwrap_err(), Error::Finalized);
    // The same probe twice, a probe of a medium the clip has not, a differing fact: errors.
    assert_eq!(compose(&camera.metadata, &[probe.clone(), probe.clone()]).unwrap_err(), Error::DuplicateProbe);
    let mut stranger = probe.clone();
    stranger.media.media_uri = "qnc://local/source/x/file/y".into();
    stranger.evidence.media_uri = stranger.media.media_uri.clone();
    assert_eq!(compose(&camera.metadata, &[stranger]).unwrap_err(), Error::UnrelatedMedia);
    let mut other = probe;
    for stream in &mut other.media.streams {
        if let StreamDetails::Video(v) = &mut stream.details {
            if let Some(w) = v.width.as_mut() {
                w.value = 1280;
            }
        }
    }
    assert!(matches!(compose(&camera.metadata, &[other]), Err(Error::Conflicts(_))));
}
