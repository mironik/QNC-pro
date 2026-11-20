//! The clip catalog of a project, in the shape the old QNC saved it (`clips` with
//! `catalog_json`, `clip_sources`, `clip_proxy`, `probe_records`, the `public_clips` view), so a
//! project the old QNC made reads here and the player input is the same. This block owns the
//! tables: it makes them, publishes clips through the database writer and reads a stored clip
//! as the player takes it. The record itself is `qnc-media-record`'s; the status of an import
//! is a column of `clips`.

use qnc_db::{Reader, TableModule};
use qnc_media_model::{MediaRepresentation, StreamDetails};
use qnc_media_record::{Phase, Snapshot};
use qnc_player_input::{PlayerClipRecord, Representation};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MODULE: &str = "qnc.module.catalog";
pub const SCHEMA: &str = include_str!("schema.sql");

/// A clip as Select publishes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogClip {
    pub name: String,
    pub source_uri: String,
    pub source_name: String,
    pub serial_number: String,
    pub volume_name: String,
    pub thumbnail_uri: Option<String>,
    pub media_records_uri: String,
    pub snapshot: Snapshot,
}

impl CatalogClip {
    pub fn id(&self) -> &str {
        self.snapshot.clip_id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Detected,
    Queued,
    Processing,
    Imported,
    Failed,
}

impl ImportStatus {
    fn of(text: &str) -> Option<ImportStatus> {
        Some(match text {
            "detected" => ImportStatus::Detected,
            "queued" => ImportStatus::Queued,
            "processing" => ImportStatus::Processing,
            "imported" => ImportStatus::Imported,
            "failed" => ImportStatus::Failed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClip {
    pub clip: CatalogClip,
    pub selected: bool,
    pub import_status: ImportStatus,
    pub import_error: Option<String>,
    pub imported_media_uri: Option<String>,
    pub imported_copy_of: Option<Representation>,
    pub imported_optimized: Option<MediaRepresentation>,
}

impl StoredClip {
    /// The clip as the player input takes it.
    pub fn for_player(&self) -> PlayerClipRecord {
        PlayerClipRecord {
            name: self.clip.name.clone(),
            snapshot: self.clip.snapshot.clone(),
            imported_media_uri: self.imported_media_uri.clone(),
            imported_copy_of: self.imported_copy_of,
            imported_optimized: self.imported_optimized.clone(),
        }
    }
}

/// A row of the lightweight list (`public_clips`), for a pool.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipSummary {
    pub clip_id: String,
    pub name: String,
    pub source_uri: String,
    pub duration_seconds: Option<f64>,
    pub duration_frames: Option<u64>,
    pub thumbnail_uri: Option<String>,
    pub import_status: ImportStatus,
    pub imported_media_uri: Option<String>,
    pub final_record: bool,
}

pub struct CatalogTables;

impl TableModule for CatalogTables {
    fn id(&self) -> &'static str {
        MODULE
    }

    fn tables(&self) -> &'static [&'static str] {
        &["clips", "clip_sources", "clip_proxy", "probe_records"]
    }

    fn attach(&mut self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())
    }

    /// `{"publish": [CatalogClip, ...]}` or `{"set_import": {"clip_id", "status", "error"?, "media_uri"?, "copy_of"?, "thumbnail_uri"?}}` (a poster given replaces the card's).
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String> {
        if let Some(clips) = request.get("publish") {
            let clips: Vec<CatalogClip> = serde_json::from_value(clips.clone()).map_err(|e| e.to_string())?;
            for clip in &clips {
                publish(conn, clip)?;
            }
            return Ok(json!({ "published": clips.len() }));
        }
        if let Some(set) = request.get("set_import") {
            let clip_id = set["clip_id"].as_str().ok_or("set_import needs a clip_id")?;
            let status = set["status"].as_str().and_then(ImportStatus::of).ok_or("set_import needs a status")?;
            let text = serde_json::to_value(status).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE clips SET import_status=?2, import_error=?3, imported_media_uri=?4, imported_copy_of=?5, thumbnail_uri=COALESCE(?6, thumbnail_uri) WHERE clip_id=?1",
                params![clip_id, text.as_str(), set["error"].as_str(), set["media_uri"].as_str(), set["copy_of"].as_str(), set["thumbnail_uri"].as_str()],
            )
            .map_err(|e| e.to_string())?;
            return Ok(json!({ "clip_id": clip_id }));
        }
        Err("unknown request".into())
    }
}

fn publish(conn: &Connection, clip: &CatalogClip) -> Result<(), String> {
    clip.snapshot.validate().map_err(|e| e.to_string())?;
    qnc_uri::validate_resource_uri(&clip.source_uri)?;
    let original = &clip.snapshot.metadata.original;
    let video = qnc_media_model::single_video(original);
    let duration = original.duration_seconds.as_ref().map(|d| d.value.numerator as f64 / d.value.denominator as f64);
    let fps = video.and_then(|v| v.frame_rate.as_ref()).map(|f| f.value);
    let json = serde_json::to_string(clip).map_err(|e| e.to_string())?;
    let created = original.tags.get("creation_time").map(|f| f.value.clone());
    let frames = video.and_then(|v| v.exact_frame_count());
    let is_final = clip.snapshot.phase == Phase::Final;
    let fail = |e: rusqlite::Error| e.to_string();
    // A clip already there keeps its selection, import status and thumbnail (as the old QNC's
    // ON CONFLICT did); a new one is inserted.
    let changed = conn
        .execute(
            "UPDATE clips SET name=?2,catalog_json=?3,revision=?4,final=?5,duration_seconds=?6,duration_frames=?7,fps_num=?8,fps_den=?9,created_at_utc=?10,thumbnail_uri=COALESCE(thumbnail_uri,?11) WHERE clip_id=?1",
            params![clip.id(), clip.name, json, clip.snapshot.revision, is_final, duration, frames, fps.map(|f| f.fps_num), fps.map(|f| f.fps_den), created, clip.thumbnail_uri],
        )
        .map_err(fail)?;
    if changed == 0 {
        conn.execute(
            "INSERT INTO clips(clip_id,source_uri,original_uri,name,created_at_utc,duration_seconds,duration_frames,fps_num,fps_den,thumbnail_uri,catalog_json,revision,final) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![clip.id(), clip.source_uri, original.media_uri, clip.name, created, duration, frames, fps.map(|f| f.fps_num), fps.map(|f| f.fps_den), clip.thumbnail_uri, json, clip.snapshot.revision, is_final],
        )
        .map_err(fail)?;
    }
    conn.execute(
        "INSERT OR REPLACE INTO clip_sources VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![clip.id(), clip.source_uri, original.media_uri, original.container.as_ref().map(|v| v.value.clone()), codec(original), clip.serial_number, clip.volume_name, clip.source_name],
    )
    .map_err(fail)?;
    if let Some(proxy) = &clip.snapshot.metadata.proxy {
        conn.execute(
            "INSERT OR REPLACE INTO clip_proxy VALUES(?1,?2,?3,?4)",
            params![clip.id(), proxy.media_uri, proxy.container.as_ref().map(|v| v.value.clone()), codec(proxy)],
        )
        .map_err(fail)?;
    }
    conn.execute(
        "INSERT OR REPLACE INTO probe_records VALUES(?1,?2,?3,?4,?5)",
        params![clip.id(), serde_json::to_string(&clip.snapshot.metadata).map_err(|e| e.to_string())?, format!("unix_ms:{}", clip.snapshot.recorded_at_unix_ms), clip.media_records_uri, clip.snapshot.revision],
    )
    .map_err(fail)?;
    Ok(())
}

fn codec(media: &MediaRepresentation) -> Option<String> {
    media.streams.iter().find(|s| matches!(s.details, StreamDetails::Video(_))).and_then(|s| s.codec.as_ref()).and_then(|c| match &c.value {
        qnc_media_model::Signal::Known(name) => Some(name.clone()),
        qnc_media_model::Signal::Unspecified => None,
    })
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))
        .and_then(|mut s| s.exists([column]))
        .unwrap_or(false)
}

/// One stored clip, as the old QNC wrote it (columns added later read as absent).
pub fn read_clip(reader: &Reader, clip_id: &str) -> Result<Option<StoredClip>, String> {
    qnc_uri::valid_id(clip_id)?;
    let conn = reader.connection();
    let copy = if has_column(conn, "clips", "imported_copy_of") { "imported_copy_of,optimized_json" } else { "NULL,NULL" };
    let sql = format!("SELECT catalog_json,selected,import_status,import_error,imported_media_uri,thumbnail_uri,{copy} FROM clips WHERE clip_id=?1");
    let row = conn
        .query_row(&sql, [clip_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((json, selected, status, error, media, thumbnail, copy_of, optimized)) = row else { return Ok(None) };
    let mut clip: CatalogClip = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if thumbnail.is_some() {
        clip.thumbnail_uri = thumbnail;
    }
    if clip.id() != clip_id {
        return Err("the stored clip does not belong to the requested id".into());
    }
    let imported_copy_of = match copy_of.as_deref() {
        None => None,
        Some("original") => Some(Representation::Original),
        Some("proxy") => Some(Representation::Proxy),
        Some(other) => return Err(format!("unknown imported_copy_of {other}")),
    };
    let imported_optimized = optimized.map(|o| serde_json::from_str(&o)).transpose().map_err(|e| e.to_string())?;
    Ok(Some(StoredClip {
        clip,
        selected,
        import_status: ImportStatus::of(&status).ok_or_else(|| format!("unknown import status {status}"))?,
        import_error: error,
        imported_media_uri: media,
        imported_copy_of,
        imported_optimized,
    }))
}

/// The lightweight list of the catalog, by name.
pub fn summaries(reader: &Reader) -> Result<Vec<ClipSummary>, String> {
    let conn = reader.connection();
    let mut statement = conn
        .prepare("SELECT clip_id,name,source_uri,duration_seconds,duration_frames,thumbnail_uri,import_status,imported_media_uri,final FROM public_clips ORDER BY name")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok(ClipSummary {
                clip_id: r.get(0)?,
                name: r.get(1)?,
                source_uri: r.get(2)?,
                duration_seconds: r.get(3)?,
                duration_frames: r.get::<_, Option<i64>>(4)?.and_then(|f| u64::try_from(f).ok()),
                thumbnail_uri: r.get(5)?,
                import_status: ImportStatus::of(&r.get::<_, String>(6)?).unwrap_or(ImportStatus::Detected),
                imported_media_uri: r.get(7)?,
                final_record: r.get::<_, i64>(8)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
