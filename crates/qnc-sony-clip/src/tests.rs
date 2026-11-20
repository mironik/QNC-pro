use qnc_media_model::*;
use qnc_media_record::{inspect_phase, Phase, Snapshot};

use crate::*;

const INDEX: &str = include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML");
const SIDECAR: &str = include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML");
/// The record the old QNC saved for the same clip (final: the probe added to these facts).
const SAVED: &str = include_str!("../../qnc-media-record/tests/snapshot.json");

const INDEX_URI: &str = "qnc://local/artifact/camera-document/293e22eee5fede3ab5c18c3d54776320aefcadd89fe0764fd8c2ce129d106120/volume-67870d0c/file/PRIVATE/XDROOT/MEDIAPRO%2EXML";
const SIDECAR_URI: &str = "qnc://local/artifact/camera-document/87975909116ecdba51b9a2971999d92bb25abb62fbac67a41aa16c15092880b1/volume-67870d0c/file/PRIVATE/XDROOT/Clip/Mironik%201483M01%2EXML";

fn read_1483(sidecar: bool) -> Read {
    let index = qnc_sony_index::read_index(INDEX, INDEX_URI).unwrap();
    let i = index.materials.iter().position(|m| m.original.relative_path == "Clip/Mironik 1483.MXF").unwrap();
    let binding = qnc_sony_index::bind(&index.materials[i], "clip-1df27557-752e-4a3e-894e-f96e0ea70c60", "qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT").unwrap();
    let sidecar = sidecar.then(|| Sidecar { relative_path: "Clip/Mironik 1483M01.XML".into(), document_uri: SIDECAR_URI.into(), text: SIDECAR.into() });
    read_clip(&index, i, &binding, sidecar.as_ref()).unwrap()
}

fn video(media: &MediaRepresentation) -> &VideoMetadata {
    qnc_media_model::single_video(media).unwrap()
}

#[test]
fn the_camera_facts_read_here_are_the_ones_in_the_record_the_old_qnc_saved() {
    let read = read_1483(true);
    let saved: Snapshot = serde_json::from_str(SAVED).unwrap();
    let (ours, theirs) = (&read.metadata.original, &saved.metadata.original);
    let (v, w) = (video(ours), video(theirs));
    assert_eq!(v.frame_count, w.frame_count);
    assert_eq!(v.frame_rate, w.frame_rate);
    assert_eq!(v.frame_rate_mode, w.frame_rate_mode);
    assert_eq!(v.scan_mode, w.scan_mode);
    assert_eq!((v.width.clone(), v.height.clone()), (w.width.clone(), w.height.clone()));
    assert_eq!(v.sample_aspect_ratio, w.sample_aspect_ratio);
    assert_eq!(ours.duration_seconds, theirs.duration_seconds);
    assert_eq!(ours.container, theirs.container);
    assert_eq!(ours.tags.get("creation_time"), theirs.tags.get("creation_time"));
    // Every camera tag the old record carries is here, with the same evidence and locator.
    for (key, value) in theirs.tags.iter().filter(|(k, _)| k.starts_with("sony.")) {
        assert_eq!(ours.tags.get(key), Some(value), "{key}");
    }
    // The proxy's facts as well (its media URI stays on the card in the saved record).
    let (p, q) = (read.metadata.proxy.as_ref().unwrap(), saved.metadata.proxy.as_ref().unwrap());
    assert_eq!(p.media_uri, q.media_uri);
    assert_eq!((video(p).width.clone(), video(p).frame_rate.clone()), (video(q).width.clone(), video(q).frame_rate.clone()));
    assert_eq!(read.metadata.evidence.iter().map(|e| (e.id.as_str(), e.document_uri.as_str())).collect::<Vec<_>>(), saved.metadata.evidence.iter().filter(|e| e.kind == EvidenceKind::CameraMetadata).map(|e| (e.id.as_str(), e.document_uri.as_str())).collect::<Vec<_>>());
    // A camera-phase record is sound, and it says what the probe must still add.
    let report = inspect_phase(&read.metadata, Phase::Camera).unwrap();
    assert!(!report.is_complete());
}

#[test]
fn without_the_sidecar_the_length_stays_unresolved_and_a_disagreeing_sidecar_is_a_conflict() {
    let read = read_1483(false);
    assert!(video(&read.metadata.original).frame_count.is_none() && read.metadata.original.duration_seconds.is_none());
    assert!(read.notices.iter().any(|n| n.code == "unresolved" && n.field == "duration"));
    let index = qnc_sony_index::read_index(INDEX, INDEX_URI).unwrap();
    let i = index.materials.iter().position(|m| m.original.relative_path == "Clip/Mironik 1483.MXF").unwrap();
    let binding = qnc_sony_index::bind(&index.materials[i], "clip-1", "qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT").unwrap();
    let wrong = Sidecar { relative_path: "Clip/Mironik 1483M01.XML".into(), document_uri: SIDECAR_URI.into(), text: SIDECAR.replace("<Duration value=\"362\"/>", "<Duration value=\"361\"/>") };
    assert_eq!(read_clip(&index, i, &binding, Some(&wrong)).unwrap_err(), "conflicting camera metadata");
    let other = Sidecar { relative_path: "Clip/Mironik 1484M01.XML".into(), ..wrong };
    assert!(read_clip(&index, i, &binding, Some(&other)).is_err());
}
