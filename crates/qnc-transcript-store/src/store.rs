//! The tables: `clip_transcripts` (one row a range of a clip) and `transcript_queue` (one row a
//! range asked), written only through the database writer. The newest transcript is what was
//! said over its range: an older one of the clip loses its lines there, word by word, and goes
//! when its range is inside the new one; a fine one only by another fine one.

use qnc_db::TableModule;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::model::{lines_cut, ClipTranscript, Line, QueueState, Queued};

pub const MODULE: &str = "qnc.module.transcript-store";
pub const TRANSCRIPTS: &str = "clip_transcripts";
pub const QUEUE: &str = "transcript_queue";

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS clip_transcripts (
    clip_id TEXT NOT NULL,
    from_frame INTEGER NOT NULL CHECK (from_frame >= 0),
    to_frame INTEGER NOT NULL CHECK (to_frame > from_frame),
    language TEXT NOT NULL,
    engine TEXT NOT NULL,
    text TEXT NOT NULL,
    lines_json TEXT NOT NULL,
    made_at_utc TEXT NOT NULL,
    fine INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (clip_id, from_frame, to_frame)
);
CREATE VIEW IF NOT EXISTS public_clip_transcripts AS
    SELECT clip_id, from_frame, to_frame, language, engine, text, lines_json, made_at_utc, fine FROM clip_transcripts;
CREATE TABLE IF NOT EXISTS transcript_queue (
    clip_id TEXT NOT NULL,
    from_frame INTEGER NOT NULL CHECK (from_frame >= 0),
    to_frame INTEGER NOT NULL CHECK (to_frame > from_frame),
    quality TEXT NOT NULL CHECK (quality IN ('fast', 'best')),
    state TEXT NOT NULL CHECK (state IN ('waiting', 'running', 'done', 'failed')),
    error TEXT NOT NULL,
    took_ms INTEGER NOT NULL DEFAULT 0,
    asked_at_utc TEXT NOT NULL,
    PRIMARY KEY (clip_id, from_frame, to_frame, quality)
);
CREATE VIEW IF NOT EXISTS public_transcript_queue AS
    SELECT clip_id, from_frame, to_frame, quality, state, error, took_ms, asked_at_utc FROM transcript_queue;";

/// The table module: `{"save": ClipTranscript}`, `{"enqueue": [Queued]}`,
/// `{"set_queued": {clip_id, from_frame, to_frame, quality, state, error, took_ms}}`,
/// `{"clear_done": true}`, `{"forget_clip": "clip"}`.
pub struct TranscriptTables;

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl TableModule for TranscriptTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &[TRANSCRIPTS, QUEUE]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(err)
    }

    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(saved) = request.get("save") {
            let transcript: ClipTranscript = serde_json::from_value(saved.clone()).map_err(err)?;
            save(conn, &transcript)?;
            return Ok(json!({ "clip_id": transcript.clip_id }));
        }
        if let Some(ranges) = request.get("enqueue") {
            let ranges: Vec<Queued> = serde_json::from_value(ranges.clone()).map_err(err)?;
            for queued in &ranges {
                if queued.from_frame >= queued.to_frame {
                    return Err(qnc_text::text("db.transcript_invalid").into());
                }
                conn.execute(
                    "INSERT OR REPLACE INTO transcript_queue (clip_id, from_frame, to_frame, quality, state, error, took_ms, asked_at_utc)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                    params![queued.clip_id, queued.from_frame as i64, queued.to_frame as i64, queued.quality.code(), queued.state.code(), queued.error, queued.took_ms as i64],
                )
                .map_err(err)?;
            }
            return Ok(json!({ "queued": ranges.len() }));
        }
        if let Some(set) = request.get("set_queued") {
            let queued: Queued = serde_json::from_value(set.clone()).map_err(err)?;
            conn.execute(
                "UPDATE transcript_queue SET state = ?5, error = ?6, took_ms = ?7 WHERE clip_id = ?1 AND from_frame = ?2 AND to_frame = ?3 AND quality = ?4",
                params![queued.clip_id, queued.from_frame as i64, queued.to_frame as i64, queued.quality.code(), queued.state.code(), queued.error, queued.took_ms as i64],
            )
            .map_err(err)?;
            return Ok(json!({ "clip_id": queued.clip_id }));
        }
        if request.get("clear_done").is_some() {
            conn.execute("DELETE FROM transcript_queue WHERE state = ?1", [QueueState::Done.code()]).map_err(err)?;
            return Ok(json!({ "cleared": true }));
        }
        if let Some(clip_id) = request.get("forget_clip").and_then(Value::as_str) {
            let running: i64 = conn.query_row("SELECT COUNT(*) FROM transcript_queue WHERE clip_id = ?1 AND state = 'running'", [clip_id], |r| r.get(0)).map_err(err)?;
            if running > 0 {
                return Err(qnc_text::text("db.transcript_running").into());
            }
            conn.execute("DELETE FROM clip_transcripts WHERE clip_id = ?1", [clip_id]).map_err(err)?;
            conn.execute("DELETE FROM transcript_queue WHERE clip_id = ?1", [clip_id]).map_err(err)?;
            return Ok(json!({ "clip_id": clip_id }));
        }
        Err(format!("transcript-store: unknown request {request}"))
    }
}

fn save(conn: &Connection, transcript: &ClipTranscript) -> Result<(), String> {
    let in_range = |from: u64, to: u64| transcript.from_frame <= from && from <= to && to <= transcript.to_frame;
    let lines_fit = transcript.lines.iter().all(|line| in_range(line.from, line.to) && line.words.iter().all(|w| in_range(w.from, w.to)));
    if transcript.from_frame >= transcript.to_frame || !lines_fit || transcript.language.is_empty() {
        return Err(qnc_text::text("db.transcript_invalid").into());
    }
    // Every transcript of the clip that overlaps the new one: (from, to, lines, fine).
    let overlapping: Vec<(i64, i64, String, bool)> = {
        let mut statement = conn.prepare("SELECT from_frame, to_frame, lines_json, fine FROM clip_transcripts WHERE clip_id = ?1 AND from_frame < ?3 AND to_frame > ?2").map_err(err)?;
        let rows = statement.query_map(params![transcript.clip_id, transcript.from_frame as i64, transcript.to_frame as i64], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, i64>(3)? != 0))).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)?
    };
    let same = |from: i64, to: i64| (u64::try_from(from).unwrap_or(0), u64::try_from(to).unwrap_or(0)) == (transcript.from_frame, transcript.to_frame);
    // A fast transcript never says over a fine one: it keeps only its words outside the fine
    // ones; one of the very range of a fine one is not kept at all.
    let mut new_lines = transcript.lines.clone();
    if !transcript.fine {
        for (from, to, _, fine) in &overlapping {
            if *fine {
                if same(*from, *to) {
                    return Ok(());
                }
                new_lines = lines_cut(&new_lines, u64::try_from(*from).unwrap_or(0), u64::try_from(*to).unwrap_or(0), false);
            }
        }
    }
    let lines = serde_json::to_string(&new_lines).map_err(err)?;
    let text = new_lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join(" ");
    conn.execute(
        "INSERT INTO clip_transcripts (clip_id, from_frame, to_frame, language, engine, text, lines_json, made_at_utc, fine)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%fZ','now'), ?8)
         ON CONFLICT(clip_id, from_frame, to_frame) DO UPDATE SET language=excluded.language, engine=excluded.engine,
           text=excluded.text, lines_json=excluded.lines_json, made_at_utc=excluded.made_at_utc, fine=excluded.fine",
        params![transcript.clip_id, transcript.from_frame as i64, transcript.to_frame as i64, transcript.language, transcript.engine, text, lines, i64::from(transcript.fine)],
    )
    .map_err(err)?;
    for (from, to, lines_json, fine) in overlapping {
        if same(from, to) || (fine && !transcript.fine) {
            continue;
        }
        if transcript.from_frame <= u64::try_from(from).unwrap_or(0) && u64::try_from(to).unwrap_or(0) <= transcript.to_frame {
            conn.execute("DELETE FROM clip_transcripts WHERE clip_id = ?1 AND from_frame = ?2 AND to_frame = ?3", params![transcript.clip_id, from, to]).map_err(err)?;
            continue;
        }
        let lines: Vec<Line> = serde_json::from_str(&lines_json).map_err(err)?;
        let kept = lines_cut(&lines, transcript.from_frame, transcript.to_frame, false);
        let text = kept.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join(" ");
        conn.execute("UPDATE clip_transcripts SET lines_json = ?4, text = ?5 WHERE clip_id = ?1 AND from_frame = ?2 AND to_frame = ?3", params![transcript.clip_id, from, to, serde_json::to_string(&kept).map_err(err)?, text]).map_err(err)?;
    }
    Ok(())
}
