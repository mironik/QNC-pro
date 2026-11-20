use std::path::PathBuf;

use qnc_db::{create, Reader, Writer};
use qnc_player_input::{PlaybackInput, PlaybackSettings, ProjectAudio};
use serde_json::json;

use crate::*;

/// The real saved record of Mironik 1483 (Sony FX6, final, complete).
const SNAPSHOT: &str = include_str!("../../qnc-media-record/tests/snapshot.json");

fn fresh(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("qnc-catalog-{}-{name}.db", std::process::id()));
    for stale in [path.clone(), path.with_extension("db-wal"), path.with_extension("db-shm")] {
        let _ = std::fs::remove_file(stale);
    }
    create(&path).unwrap();
    path
}

fn clip() -> CatalogClip {
    let snapshot: Snapshot = serde_json::from_str(SNAPSHOT).unwrap();
    CatalogClip {
        name: "Mironik 1483.MXF".into(),
        source_uri: "qnc://local/source/volume-67870d0c".into(),
        source_name: "Sony FX6 card".into(),
        serial_number: "67870d0c".into(),
        volume_name: "UNTITLED".into(),
        thumbnail_uri: None,
        media_records_uri: "qnc://local/db/media_records".into(),
        snapshot,
    }
}

#[test]
fn a_published_clip_reads_back_whole_and_is_what_the_player_input_takes() {
    let path = fresh("publish");
    let writer = Writer::start(&path, vec![Box::new(CatalogTables)]).unwrap();
    let clip = clip();
    let reply = writer.execute(MODULE, json!({ "publish": [clip] })).unwrap();
    assert_eq!(reply["published"], 1);

    let reader = Reader::open(&path).unwrap();
    let stored = read_clip(&reader, clip.id()).unwrap().expect("the clip is there");
    assert_eq!(stored.clip, clip);
    assert_eq!(stored.import_status, ImportStatus::Detected);
    assert!(!stored.selected && stored.imported_media_uri.is_none());
    assert!(read_clip(&reader, "clip-nobody").unwrap().is_none());

    let list = summaries(&reader).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Mironik 1483.MXF");
    assert_eq!(list[0].duration_frames, Some(362));
    assert!(list[0].final_record);

    // Imported where it is: the player input is prepared from the stored clip alone.
    writer
        .execute(MODULE, json!({ "set_import": { "clip_id": clip.id(), "status": "imported", "media_uri": clip.snapshot.binding.original_uri } }))
        .unwrap();
    let stored = read_clip(&reader, clip.id()).unwrap().unwrap();
    assert_eq!(stored.import_status, ImportStatus::Imported);
    let settings = PlaybackSettings {
        workspace_db_uri: "qnc://local/db/project_workspace/p1".into(),
        playback_input: PlaybackInput::Original,
        audio: ProjectAudio { channels: 2, sample_rate_hz: 48000 },
    };
    let prepared = qnc_player_input::prepare(&settings, &stored.for_player()).unwrap();
    assert_eq!(prepared.snapshot, clip.snapshot);

    // Published again (a new Select of the same card) the import status and selection stay.
    writer.execute(MODULE, json!({ "publish": [clip] })).unwrap();
    let again = read_clip(&reader, clip.id()).unwrap().unwrap();
    assert_eq!(again.import_status, ImportStatus::Imported);
    assert_eq!(again.imported_media_uri, stored.imported_media_uri);
}

#[test]
fn a_record_that_is_not_sound_is_not_published() {
    let path = fresh("refuse");
    let writer = Writer::start(&path, vec![Box::new(CatalogTables)]).unwrap();
    let mut broken = clip();
    broken.snapshot.revision = 7;
    assert!(writer.execute(MODULE, json!({ "publish": [broken] })).is_err());
    let reader = Reader::open(&path).unwrap();
    assert!(summaries(&reader).unwrap().is_empty());
}

#[test]
fn a_catalog_the_old_qnc_wrote_without_the_later_columns_still_reads() {
    let path = fresh("old");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE clips (clip_id TEXT PRIMARY KEY, source_uri TEXT NOT NULL, original_uri TEXT NOT NULL, name TEXT NOT NULL,
             created_at_utc TEXT, duration_seconds REAL, duration_frames INTEGER, fps_num INTEGER, fps_den INTEGER, thumbnail_uri TEXT,
             catalog_json TEXT NOT NULL, revision INTEGER NOT NULL, final INTEGER NOT NULL, selected INTEGER NOT NULL DEFAULT 0,
             import_status TEXT NOT NULL DEFAULT 'detected', import_error TEXT, imported_media_uri TEXT);",
        )
        .unwrap();
        let clip = clip();
        conn.execute(
            "INSERT INTO clips(clip_id,source_uri,original_uri,name,catalog_json,revision,final) VALUES(?1,?2,?3,?4,?5,2,1)",
            rusqlite::params![clip.id(), clip.source_uri, clip.snapshot.binding.original_uri, clip.name, serde_json::to_string(&clip).unwrap()],
        )
        .unwrap();
    }
    let reader = Reader::open(&path).unwrap();
    let stored = read_clip(&reader, clip().id()).unwrap().unwrap();
    assert_eq!(stored.imported_copy_of, None);
    assert_eq!(stored.clip.name, "Mironik 1483.MXF");
}
