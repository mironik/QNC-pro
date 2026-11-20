use qnc_media_model::*;

use crate::*;

/// ffprobe's report of the original of Mironik 1483 as it lies in the project (the copy of
/// the card; `format.filename` is the media URI, as the probe writes it).
const REPORT: &str = include_str!("../tests/probe_1483.json");
const MEDIA: &str = "qnc://local/source/project-ingest_sony_ilme-fx6v_2026-11-20_13_28_0_2945922201e04bc8b69d791611a4b98a/file/original/PRIVATE/XDROOT/Clip/Mironik%201483%2EMXF";

#[test]
fn the_report_of_a_real_mxf_reads_into_exact_frames_four_sound_streams_and_tags() {
    let parsed = read(REPORT, MEDIA, "qnc://local/artifact/probe-1", "ffprobe-0").unwrap();
    assert_eq!(parsed.evidence.kind, EvidenceKind::Ffprobe);
    let video = single_video(&parsed.media).unwrap();
    assert_eq!(video.exact_frame_count(), Some(362));
    assert_eq!(video.frame_rate.as_ref().map(|f| (f.value.fps_num, f.value.fps_den)), Some((50, 1)));
    assert_eq!((video.width.as_ref().unwrap().value, video.height.as_ref().unwrap().value), (1920, 1080));
    assert_eq!(video.scan_mode.as_ref().map(|s| s.value), Some(ScanMode::Progressive));
    assert_eq!(parsed.media.streams.iter().filter(|s| matches!(s.details, StreamDetails::Audio(_))).count(), 4);
    assert!(parsed.media.tags.contains_key("ffprobe:/format/tags/company_name"));
    assert_eq!(parsed.media.container.as_ref().map(|c| c.value.as_str()), Some("mxf"));
    assert!(!summary_lacks_picture_facts(&serde_json::from_str(REPORT).unwrap()));
    assert_eq!(read(REPORT, "qnc://local/source/other/file/x", "qnc://local/artifact/probe-1", "ffprobe-0").unwrap_err(), Error::InvalidBinding);
    assert_eq!(read("{\"error\":{}}", MEDIA, "qnc://local/artifact/probe-1", "ffprobe-0").unwrap_err(), Error::InvalidDocument);
}
