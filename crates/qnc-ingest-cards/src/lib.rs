//! The cards a project was ingested from, in its own database: one row per ingest of a card
//! (the same card put in again is one more row): the card's volume serial and label, the camera
//! that wrote it, and the day and time the ingest started (UTC, `YYYY-MM-DD HH:MM:SS`). The
//! one owner of the table; the ingest window writes a row as it starts, the forms read them.

use qnc_db::{Reader, TableModule, Writer};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.ingest-cards";
pub const TABLE: &str = "ingest_cards";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ingest_cards (
    serial TEXT NOT NULL, label TEXT NOT NULL, camera TEXT NOT NULL, started_at_utc TEXT NOT NULL,
    PRIMARY KEY (serial, started_at_utc)
)";

/// One ingest of a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestedCard {
    pub serial: String,
    pub label: String,
    pub camera: String,
    /// `YYYY-MM-DD HH:MM:SS` UTC.
    pub started_at_utc: String,
}

pub struct IngestCardsTable;

impl TableModule for IngestCardsTable {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &[TABLE]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| format!("ingest-cards: {e}"))
    }

    /// `{"record": IngestedCard}`.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        let card: IngestedCard = serde_json::from_value(request.get("record").cloned().ok_or("ingest-cards: unknown request")?).map_err(|e| format!("ingest-cards: {e}"))?;
        if card.serial.trim().is_empty() || card.started_at_utc.len() != 19 {
            return Err("ingest-cards: a card needs its serial and the time its ingest started".into());
        }
        conn.execute("INSERT OR REPLACE INTO ingest_cards (serial, label, camera, started_at_utc) VALUES (?1, ?2, ?3, ?4)", params![card.serial.to_ascii_lowercase(), card.label, card.camera, card.started_at_utc]).map_err(|e| e.to_string())?;
        Ok(json!({ "serial": card.serial }))
    }
}

/// Writes that the ingest of `card` started.
pub fn record(writer: &Writer, card: &IngestedCard) -> Result<(), String> {
    writer.execute(MODULE, json!({ "record": card })).map(|_| ())
}

/// The day and time now, as the table keeps it.
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = secs / 86_400;
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

/// The cards the project was ingested from, newest first; none before the table exists.
pub fn read(reader: &Reader) -> Result<Vec<IngestedCard>, String> {
    let mut statement = match reader.connection().prepare("SELECT serial, label, camera, started_at_utc FROM ingest_cards ORDER BY started_at_utc DESC") {
        Ok(statement) => statement,
        Err(rusqlite::Error::SqliteFailure(_, Some(message))) if message.contains("no such table") => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let rows = statement.query_map([], |r| Ok(IngestedCard { serial: r.get(0)?, label: r.get(1)?, camera: r.get(2)?, started_at_utc: r.get(3)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ingest_of_a_card_is_one_row_with_its_camera_and_time() {
        let dir = std::env::temp_dir().join(format!("qnc-ingest-cards-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(read(&reader).unwrap().is_empty(), "no table yet is no card");
        let writer = Writer::start(&db, vec![Box::new(IngestCardsTable)]).unwrap();
        let first = IngestedCard { serial: "67870D0C".into(), label: "SDXC".into(), camera: "Sony XAVC (SD card)".into(), started_at_utc: "2026-11-21 21:46:00".into() };
        record(&writer, &first).unwrap();
        record(&writer, &IngestedCard { started_at_utc: "2026-11-21 21:59:10".into(), ..first.clone() }).unwrap();
        assert!(record(&writer, &IngestedCard { started_at_utc: "today".into(), ..first.clone() }).is_err());
        let cards = read(&reader).unwrap();
        assert_eq!(cards.len(), 2, "the card put in again is one more row");
        assert_eq!((cards[0].serial.as_str(), cards[0].started_at_utc.as_str()), ("67870d0c", "2026-11-21 21:59:10"));
        assert_eq!(now_utc().len(), 19);
        assert!(now_utc().starts_with("20"));
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
