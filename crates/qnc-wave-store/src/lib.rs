//! The waves of a project's clips, in its own database: one row per clip with the peaks of
//! every channel (one per picture frame, per mille) and each channel's loudness segments. The
//! one owner of the table; whoever makes a wave writes it through the database writer, the
//! forms read it as their `WaveSource`, the bars count the clips that have one.

use std::collections::BTreeSet;

use qnc_db::{Reader, TableModule, Writer};
use qnc_loudness::ChannelLoudness;
use qnc_wave_read::{Wave, WaveSource};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.wave-store";
pub const TABLE: &str = "waves";
/// Raised when the shape of a stored wave changes: older rows are made again.
pub const VERSION: i64 = 1;
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS waves (
    clip_id TEXT PRIMARY KEY, version INTEGER NOT NULL, frames_per_second REAL NOT NULL,
    lanes_json TEXT NOT NULL, loudness_json TEXT NOT NULL, made_at_unix_ms INTEGER NOT NULL
)";

/// A wave as it is written: `{"save": Saved}`; `{"forget": "clip"}` removes one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub clip_id: String,
    pub frames_per_second: f64,
    pub lanes: Vec<Vec<u16>>,
    pub loudness: Vec<ChannelLoudness>,
    pub made_at_unix_ms: u64,
}

pub struct WaveTables;

impl TableModule for WaveTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &[TABLE]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
    }

    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(saved) = request.get("save") {
            let saved: Saved = serde_json::from_value(saved.clone()).map_err(|e| format!("wave-store: {e}"))?;
            if saved.lanes.is_empty() || saved.frames_per_second <= 0.0 {
                return Err(format!("wave-store: the wave of {} is empty", saved.clip_id));
            }
            conn.execute(
                "INSERT OR REPLACE INTO waves (clip_id, version, frames_per_second, lanes_json, loudness_json, made_at_unix_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![saved.clip_id, VERSION, saved.frames_per_second, serde_json::to_string(&saved.lanes).map_err(|e| e.to_string())?, serde_json::to_string(&saved.loudness).map_err(|e| e.to_string())?, saved.made_at_unix_ms as i64],
            )
            .map_err(|e| e.to_string())?;
            return Ok(json!({ "clip_id": saved.clip_id }));
        }
        if let Some(clip_id) = request.get("forget").and_then(Value::as_str) {
            conn.execute("DELETE FROM waves WHERE clip_id = ?1", [clip_id]).map_err(|e| e.to_string())?;
            return Ok(json!({ "clip_id": clip_id }));
        }
        Err(format!("wave-store: unknown request {request}"))
    }
}

/// Writes a wave.
pub fn save(writer: &Writer, saved: &Saved) -> Result<(), String> {
    writer.execute(MODULE, json!({ "save": saved })).map(|_| ())
}

fn no_table(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("no such table"))
}

/// The wave of a clip of this version; none before the table exists or when the clip has none.
pub fn read(reader: &Reader, clip_id: &str) -> Result<Option<Wave>, String> {
    let row = reader
        .connection()
        .query_row("SELECT frames_per_second, lanes_json FROM waves WHERE clip_id = ?1 AND version = ?2", params![clip_id, VERSION], |r| Ok((r.get::<_, f64>(0)?, r.get::<_, String>(1)?)))
        .optional();
    match row {
        Ok(Some((frames_per_second, lanes))) => {
            let lanes: Vec<Vec<u16>> = serde_json::from_str(&lanes).map_err(|e| format!("wave-store: {e}"))?;
            Ok(Some(Wave { frames_per_second, lanes }))
        }
        Ok(None) => Ok(None),
        Err(e) if no_table(&e) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The loudness of every channel of a clip's wave, as measured.
pub fn loudness(reader: &Reader, clip_id: &str) -> Result<Option<Vec<ChannelLoudness>>, String> {
    let row = reader.connection().query_row("SELECT loudness_json FROM waves WHERE clip_id = ?1 AND version = ?2", params![clip_id, VERSION], |r| r.get::<_, String>(0)).optional();
    match row {
        Ok(Some(text)) => serde_json::from_str(&text).map(Some).map_err(|e| format!("wave-store: {e}")),
        Ok(None) => Ok(None),
        Err(e) if no_table(&e) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The clips that have a wave of this version.
pub fn made(reader: &Reader) -> Result<BTreeSet<String>, String> {
    let mut statement = match reader.connection().prepare("SELECT clip_id FROM waves WHERE version = ?1") {
        Ok(statement) => statement,
        Err(e) if no_table(&e) => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement.query_map([VERSION], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// The wave source of a form: the waves of one project's database.
pub struct Stored {
    reader: Reader,
}

impl Stored {
    pub fn open(db: &std::path::Path) -> Result<Stored, String> {
        Ok(Stored { reader: Reader::open(db)? })
    }
}

impl WaveSource for Stored {
    fn read(&mut self, clip_id: &str) -> Result<Option<Wave>, String> {
        read(&self.reader, clip_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_wave_is_read_back_counted_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("qnc-wave-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(read(&reader, "c").unwrap().is_none(), "no table yet is no wave");
        assert!(made(&reader).unwrap().is_empty());
        let writer = Writer::start(&db, vec![Box::new(WaveTables)]).unwrap();
        let wave = Saved { clip_id: "c".into(), frames_per_second: 50.0, lanes: vec![vec![1000, 500], vec![0, 0]], loudness: vec![ChannelLoudness { segment_ms: 100, energies: vec![0.1], true_peaks: vec![1.0] }], made_at_unix_ms: 7 };
        save(&writer, &wave).unwrap();
        assert!(save(&writer, &Saved { lanes: Vec::new(), ..wave.clone() }).is_err(), "an empty wave is refused");
        let read_back = read(&reader, "c").unwrap().unwrap();
        assert_eq!(read_back.lanes, wave.lanes);
        assert_eq!(read_back.frames(), 2);
        assert_eq!(loudness(&reader, "c").unwrap().unwrap().len(), 1);
        assert_eq!(made(&reader).unwrap().len(), 1);
        writer.execute(MODULE, json!({ "forget": "c" })).unwrap();
        assert!(made(&reader).unwrap().is_empty());
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
