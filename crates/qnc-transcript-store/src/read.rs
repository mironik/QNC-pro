//! Reading: the transcripts of a clip (what an older one says over a newer one's range is not
//! read, word by word; a fine one counts as newer than any that is not), the queue, and which
//! clips have a transcript or are heard out.

use std::collections::BTreeSet;

use qnc_db::Reader;
use rusqlite::params;

use crate::model::{lines_cut, ClipTranscript, QueueQuality, QueueState, Queued};

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn no_table(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("no such table"))
}

/// The transcripts of a clip, in the order of their ranges.
pub fn read_clip(reader: &Reader, clip_id: &str) -> Result<Vec<ClipTranscript>, String> {
    let conn = reader.connection();
    let mut statement = match conn.prepare("SELECT from_frame, to_frame, language, engine, lines_json, fine FROM clip_transcripts WHERE clip_id = ?1 ORDER BY from_frame, to_frame") {
        Ok(statement) => statement,
        Err(e) if no_table(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement
        .query_map(params![clip_id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)? != 0)))
        .map_err(err)?;
    let mut transcripts = Vec::new();
    for row in rows {
        let (from, to, language, engine, lines, fine) = row.map_err(err)?;
        let lines = serde_json::from_str(&lines).map_err(err)?;
        transcripts.push(ClipTranscript { clip_id: clip_id.to_string(), from_frame: u64::try_from(from).unwrap_or(0), to_frame: u64::try_from(to).unwrap_or(0), language, engine, lines, fine });
    }
    // The same rule as a save: what an older transcript says over a newer one's range is not read.
    let mut order = conn.prepare("SELECT from_frame, to_frame FROM clip_transcripts WHERE clip_id = ?1 ORDER BY fine, made_at_utc, from_frame, to_frame").map_err(err)?;
    let newer: Vec<(u64, u64)> = order
        .query_map(params![clip_id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .map_err(err)?
        .map(|row| row.map(|(from, to)| (u64::try_from(from).unwrap_or(0), u64::try_from(to).unwrap_or(0))).map_err(err))
        .collect::<Result<_, _>>()?;
    let made = |t: &ClipTranscript| newer.iter().position(|(from, to)| (*from, *to) == (t.from_frame, t.to_frame)).unwrap_or(0);
    let ranges: Vec<(usize, u64, u64)> = transcripts.iter().map(|t| (made(t), t.from_frame, t.to_frame)).collect();
    for transcript in &mut transcripts {
        let mine = made(transcript);
        for &(order, from, to) in &ranges {
            if order > mine {
                transcript.lines = lines_cut(&transcript.lines, from, to, false);
            }
        }
    }
    Ok(transcripts)
}

/// The queue, in the order it was asked.
pub fn read_queue(reader: &Reader) -> Result<Vec<Queued>, String> {
    let mut statement = match reader.connection().prepare("SELECT clip_id, from_frame, to_frame, quality, state, error, took_ms FROM transcript_queue ORDER BY asked_at_utc, clip_id, from_frame") {
        Ok(statement) => statement,
        Err(e) if no_table(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement
        .query_map([], |row| {
            Ok(Queued {
                clip_id: row.get(0)?,
                from_frame: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                to_frame: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                quality: QueueQuality::of(&row.get::<_, String>(3)?).unwrap_or(QueueQuality::Fast),
                state: QueueState::of(&row.get::<_, String>(4)?).unwrap_or(QueueState::Waiting),
                error: row.get(5)?,
                took_ms: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
            })
        })
        .map_err(err)?;
    rows.collect::<Result<_, _>>().map_err(err)
}

/// The clips that have a kept transcript.
pub fn transcribed(reader: &Reader) -> Result<BTreeSet<String>, String> {
    let mut statement = match reader.connection().prepare("SELECT DISTINCT clip_id FROM clip_transcripts") {
        Ok(statement) => statement,
        Err(e) if no_table(&e) => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(err)?;
    rows.collect::<Result<_, _>>().map_err(err)
}

/// The clips heard out: every queued range of the clip done or failed (a clip queued in pieces
/// is heard when every piece is).
pub fn heard(reader: &Reader) -> Result<BTreeSet<String>, String> {
    let mut over: BTreeSet<String> = BTreeSet::new();
    let mut open: BTreeSet<String> = BTreeSet::new();
    for entry in read_queue(reader)? {
        if entry.state.is_over() {
            over.insert(entry.clip_id);
        } else {
            open.insert(entry.clip_id);
        }
    }
    Ok(over.difference(&open).cloned().collect())
}
