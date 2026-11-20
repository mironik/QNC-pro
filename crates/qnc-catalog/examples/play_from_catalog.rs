//! The live proof of the catalog against a project the old QNC made: reads the project's
//! `project.db` (a copy, read-only), takes the playback settings from `public_project_settings`,
//! the stored clip from the catalog, prepares the player input and opens a session with the
//! real player program using the bindings of a `played` JSON file. Writes nothing.
//!
//! Arguments: <project.db> <clip_id> <played.json> (the bindings come from the third).

use std::time::{Duration, SystemTime};

use qnc_db::Reader;
use qnc_player_input::{PlaybackInput, PlaybackSettings, ProjectAudio};
use qnc_player_session::Player;
use qnc_player_wire::{Command, Event};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let db = args.next().ok_or("project.db")?;
    let clip_id = args.next().ok_or("clip id")?;
    let played_file = args.next().ok_or("played.json with the bindings")?;
    let reader = Reader::open(std::path::Path::new(&db))?;

    let (project_id, settings_json): (String, String) = reader
        .connection()
        .query_row("SELECT project_id, settings_json FROM public_project_settings LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?;
    let settings: serde_json::Value = serde_json::from_str(&settings_json).map_err(|e| e.to_string())?;
    let playback_input: PlaybackInput = serde_json::from_value(settings["playback"]["input"].clone()).map_err(|e| e.to_string())?;
    let audio = ProjectAudio {
        channels: settings["audio"]["channels"].as_u64().ok_or("audio.channels")? as u16,
        sample_rate_hz: settings["audio"]["sample_rate"].as_u64().ok_or("audio.sample_rate")? as u32,
    };
    let playback = PlaybackSettings { workspace_db_uri: format!("qnc://local/db/project_workspace/{project_id}"), playback_input, audio };
    println!("project {project_id}: plays {playback_input:?}, {} outputs at {} Hz", playback.audio.channels, playback.audio.sample_rate_hz);

    let stored = qnc_catalog::read_clip(&reader, &clip_id)?.ok_or("no such clip in the catalog")?;
    println!("clip {} {:?} imported at {:?}", stored.clip.name, stored.import_status, stored.imported_media_uri);
    let prepared = qnc_player_input::prepare(&playback, &stored.for_player()).map_err(|e| e.to_string())?;
    println!("prepared: {:?}, {} frames", prepared.representation, prepared.layout.video.as_ref().map(|v| v.duration_frames).unwrap_or(0));

    let text = std::fs::read_to_string(&played_file).map_err(|e| e.to_string())?;
    let mut played: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    played.insert("input".into(), serde_json::to_value(&prepared).map_err(|e| e.to_string())?);

    let mut player = Player::new();
    let started = SystemTime::now();
    let mut session = player.open(played, 1)?;
    let mut ready = false;
    for _ in 0..200 {
        for event in session.state()? {
            match event {
                Event::PlaybackReadinessChanged { ready: true, .. } => ready = true,
                Event::SourceFailed { reason, .. } | Event::PlaybackError { reason } => return Err(reason),
                _ => {}
            }
        }
        if ready {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("ready after {:?}: {ready}", started.elapsed().unwrap_or_default());
    if ready {
        session.command(Command::Play)?;
        std::thread::sleep(Duration::from_secs(2));
        let mut last = None;
        for event in session.state()? {
            if let Event::CarrierPositionChanged { frame, .. } = event {
                last = Some(frame);
            }
        }
        println!("after 2 s of play the carrier is at {last:?}");
        session.command(Command::Pause)?;
    }
    player.end(session);
    Ok(())
}
