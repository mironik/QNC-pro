use qnc_db::{create, Reader, Writer};
use qnc_player_input::PlaybackInput;
use serde_json::json;

use crate::*;

/// The settings the old QNC (QNC-AI tray) saved for a real project.
const SAVED: &str = include_str!("../tests/settings.json");

fn fresh(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("qnc-project-settings-{}-{name}.db", std::process::id()));
    for stale in [path.clone(), path.with_extension("db-wal"), path.with_extension("db-shm")] {
        let _ = std::fs::remove_file(stale);
    }
    create(&path).unwrap();
    path
}

#[test]
fn the_settings_the_old_qnc_saved_read_strictly_into_what_the_work_needs() {
    let path = fresh("real");
    let writer = Writer::start(&path, vec![Box::new(SettingsTable)]).unwrap();
    let saved: Value = serde_json::from_str(SAVED).unwrap();
    writer.execute(MODULE, json!({ "set": { "project_id": "p-2026", "template_id": "news", "settings": saved, "at": "2026-11-20T20:00:00Z" } })).unwrap();
    let settings = read(&Reader::open(&path).unwrap()).unwrap();
    assert_eq!(settings.project_id, "p-2026");
    assert_eq!(settings.template_id.as_deref(), Some("news"));
    let playback = settings.playback().unwrap();
    assert_eq!(playback.playback_input, PlaybackInput::Original);
    assert_eq!((playback.audio.channels, playback.audio.sample_rate_hz), (2, 48000));
    assert_eq!(playback.workspace_db_uri, "qnc://local/db/project_workspace/p-2026");
    assert_eq!(settings.project_media_source_uri().unwrap(), "qnc://local/source/project-p-2026");
    assert_eq!(settings.ingest_media().unwrap(), "original");
    assert_eq!(settings.product_path("thumbnails").unwrap(), "products/thumbnails");
    assert!(settings.ai_feature("transcription") && !settings.ai_feature("nothing"));
    // Set again: the one row changes, it does not double.
    writer.execute(MODULE, json!({ "set": { "project_id": "p-2026", "settings": { "playback": { "input": "proxy" } } } })).unwrap();
    let reader = Reader::open(&path).unwrap();
    let again = read(&reader).unwrap();
    assert_eq!(again.playback_input().unwrap(), PlaybackInput::Proxy);
    let rows: i64 = reader.connection().query_row("SELECT COUNT(*) FROM public_project_settings", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn a_missing_or_unusable_value_is_an_error_that_names_its_key_never_a_substitute() {
    let settings = Settings { project_id: "p".into(), template_id: None, saved: json!({ "audio": { "channels": 0, "sample_rate": 48000 }, "products": { "bad": "../x" } }) };
    assert_eq!(settings.playback_input().unwrap_err(), "settings_json has no usable playback.input");
    assert!(settings.audio().is_err());
    assert_eq!(settings.ingest_media().unwrap_err(), "settings_json has no usable storage.ingest_media");
    assert!(settings.product_path("bad").is_err());
    assert!(settings.playback().is_err());
    let path = fresh("empty");
    Writer::start(&path, vec![Box::new(SettingsTable)]).unwrap();
    assert!(read(&Reader::open(&path).unwrap()).unwrap_err().contains("no settings row"));
}
