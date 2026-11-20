use crate::*;

/// The input the old QNC prepared for a real clip (Sony FX6, original plays, 2 outputs,
/// 48 kHz): the golden answer of this block.
const PREPARED: &str = include_str!("../tests/prepared.json");

fn golden() -> PreparedInput {
    serde_json::from_str(PREPARED).expect("the prepared input reads in the program's shape")
}

fn settings_of(input: &PreparedInput) -> PlaybackSettings {
    PlaybackSettings { workspace_db_uri: input.workspace_db_uri.clone(), playback_input: input.playback_input, audio: input.project_audio.clone() }
}

fn record_of(input: &PreparedInput) -> PlayerClipRecord {
    PlayerClipRecord { name: "Mironik 1483.MXF".into(), snapshot: input.snapshot.clone(), imported_media_uri: Some(input.snapshot.binding.original_uri.clone()), imported_copy_of: None, imported_optimized: None }
}

#[test]
fn the_input_prepared_here_is_the_one_the_old_qnc_prepared_for_the_same_record_and_settings() {
    let golden = golden();
    let prepared = prepare(&settings_of(&golden), &record_of(&golden)).unwrap();
    assert_eq!(prepared, golden);
    prepared.validate_for(&golden.workspace_db_uri, golden.snapshot.clip_id()).unwrap();
    assert_eq!(serde_json::to_value(&prepared).unwrap(), serde_json::from_str::<serde_json::Value>(PREPARED).unwrap(), "written as the program reads it");
}

#[test]
fn the_proxy_plays_the_picture_with_the_originals_sound_and_a_missing_or_unfinished_record_is_refused() {
    let golden = golden();
    let mut settings = settings_of(&golden);
    settings.playback_input = PlaybackInput::ProxyIfAvailable;
    // This project played the original, so its proxy was never probed: asking for the proxy
    // is refused with exactly what is missing, never guessed.
    let Err(InputError::IncompleteMedia(missing)) = prepare(&settings, &record_of(&golden)) else { panic!("an unprobed proxy is incomplete") };
    assert!(missing.iter().all(|p| p.starts_with("proxy")) && missing.contains(&"proxy.streams[0].video.frame_count".to_string()));
    settings.playback_input = PlaybackInput::Original;
    let lead = golden.clone().with_lead_channels(&[2, 3]).unwrap();
    assert_eq!(lead.lead_audio_channels, vec![2, 3]);
    assert!(golden.clone().with_lead_channels(&[9]).is_err());
    let mut other = record_of(&golden);
    other.imported_media_uri = Some("qnc://local/source/project-x/file/original/copy".into());
    assert!(matches!(prepare(&settings, &other), Err(InputError::UnsupportedMedia(_))), "a copy without a binding is refused");
    other.imported_copy_of = Some(Representation::Original);
    let copied = prepare(&settings_of(&golden), &other).unwrap();
    assert_eq!(copied.snapshot.metadata.original.media_uri, "qnc://local/source/project-x/file/original/copy");
    let mut camera = record_of(&golden);
    camera.snapshot.phase = Phase::Camera;
    camera.snapshot.revision = 1;
    assert!(prepare(&settings, &camera).is_err());
    settings.workspace_db_uri = "qnc://local/db/other".into();
    assert_eq!(prepare(&settings, &record_of(&golden)).unwrap_err(), InputError::WrongWorkspace);
}
