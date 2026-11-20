//! Writes the final record of Mironik 1483 composed from the test fixtures as sorted JSON to
//! the file given, for comparing with the record the old QNC saved. Reads only fixtures.
fn main() {
    let out = std::env::args().nth(1).expect("output file");
    let saved: serde_json::Value = serde_json::from_str(include_str!("../../qnc-media-record/tests/snapshot.json")).unwrap();
    let snapshot: qnc_media_record::Snapshot = serde_json::from_value(saved.clone()).unwrap();
    let index = qnc_sony_index::read_index(include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML"), &snapshot.metadata.evidence[0].document_uri).unwrap();
    let i = index.materials.iter().position(|m| m.original.relative_path == "Clip/Mironik 1483.MXF").unwrap();
    let mut binding = qnc_sony_index::bind(&index.materials[i], snapshot.clip_id(), "qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT").unwrap();
    binding.original.media_uri = snapshot.binding.original_uri.clone();
    let sidecar = qnc_sony_clip::Sidecar { relative_path: "Clip/Mironik 1483M01.XML".into(), document_uri: snapshot.metadata.evidence[2].document_uri.clone(), text: include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML").into() };
    let metadata = qnc_sony_clip::read_clip(&index, i, &binding, Some(&sidecar)).unwrap().metadata;
    let report = qnc_media_record::inspect_phase(&metadata, qnc_media_record::Phase::Camera).unwrap();
    let camera = qnc_media_record::Snapshot { binding: snapshot.binding.clone(), revision: 1, phase: qnc_media_record::Phase::Camera, completeness: qnc_media_record::Completeness::Partial, metadata, report, recorded_at_unix_ms: 1 };
    let probe = qnc_ffprobe_read::read(include_str!("../../qnc-ffprobe-read/tests/probe_1483.json"), &snapshot.binding.original_uri, &snapshot.metadata.evidence[3].document_uri, "ffprobe-0").unwrap();
    let done = qnc_record_compose::finalize(&camera, &[probe], snapshot.recorded_at_unix_ms).unwrap();
    let ours: serde_json::Value = serde_json::to_value(&done).unwrap();
    std::fs::write(&out, serde_json::to_string_pretty(&ours).unwrap()).unwrap();
    std::fs::write(format!("{out}.saved"), serde_json::to_string_pretty(&saved).unwrap()).unwrap();
}
