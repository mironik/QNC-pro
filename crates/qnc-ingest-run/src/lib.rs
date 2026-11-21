//! The ingest of one card into one project, as the old QNC did it, block by block: the card is
//! selected (read only) and its clips published in camera phase; each original is copied into
//! `original/<its path on the card>` of the project and proven by its checksum; the poster the
//! card has is copied beside the thumbnails; the copy is bound as the original of the record,
//! probed once, and the final record is published. A clip whose copy or probe fails is marked
//! failed with the reason and the others go on. Progress is reported as it happens. The card is
//! never written; the project is written only through its database writer and under its folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use qnc_card_select::{Card, Selected};
use qnc_catalog::{CatalogClip, CatalogTables, ImportStatus};
use qnc_db::{Reader, Writer};
use qnc_media_record::Snapshot;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const ORIGINAL_FOLDER: &str = "original";
pub const POSTER_FILE: &str = "poster.jpg";
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(40);

/// Where the ingest stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub clips: usize,
    pub copied: usize,
    pub finished: usize,
    pub failed: usize,
    pub bytes_copied: u64,
}

/// The project an ingest writes into.
pub struct Project {
    pub id: String,
    pub folder: PathBuf,
    pub writer: Writer,
    /// `products.thumbnails` of its settings.
    pub thumbnails: String,
}

impl Project {
    /// Opens the project at `folder` (its settings say where the posters go).
    pub fn open(folder: &Path) -> Result<Project, String> {
        let db = folder.join("project.db");
        let settings = qnc_project_settings::read(&Reader::open(&db)?)?;
        let thumbnails = settings.product_path("thumbnails")?;
        let writer = Writer::start(&db, vec![Box::new(qnc_project_settings::SettingsTable), Box::new(CatalogTables)])?;
        Ok(Project { id: settings.project_id, folder: folder.to_path_buf(), writer, thumbnails })
    }

    pub fn media_source_uri(&self) -> String {
        format!("qnc://local/source/project-{}", self.id)
    }
}

/// Selects the card and publishes its clips in camera phase; gives what was selected.
/// `leave_out`: originals (URIs) not taken at all (another project has them); a clip whose original
/// the project already has keeps its clip id.
pub fn select_into(card: &Card, project: &Project, profiles: &[qnc_camera_profile::Profile], now_unix_ms: u64, leave_out: &std::collections::BTreeSet<String>) -> Result<Selected, String> {
    let reader = Reader::open(&project.folder.join("project.db"))?;
    let keep_ids: std::collections::BTreeMap<String, String> = qnc_catalog::originals(&reader)?.into_iter().map(|o| (o.original_uri, o.clip_id)).collect();
    let mut selected = qnc_card_select::select(card, profiles, now_unix_ms, &keep_ids)?;
    selected.clips.retain(|clip| !leave_out.contains(&clip.snapshot.binding.original_uri));

    for (path, why) in &selected.skipped {
        qnc_diagnostics::log(format!("ingest skipped {path}: {why}"));
    }
    project.writer.execute(qnc_catalog::MODULE, json!({ "publish": selected.clips }))?;
    Ok(selected)
}

fn under(folder: &Path, relative: &str) -> PathBuf {
    relative.split('/').filter(|p| !p.is_empty()).fold(folder.to_path_buf(), |path, part| path.join(part))
}

fn inside(project: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(project) && path != project {
        Ok(())
    } else {
        Err(format!("{} is not inside the project", path.display()))
    }
}

/// The card file of a URI of this card.
fn card_file(card: &Card, uri: &str) -> Result<(PathBuf, String), String> {
    let (source, relative) = qnc_uri::split_file_uri(uri)?;
    if source != card.source_uri() {
        return Err(format!("{uri} is not on this card"));
    }
    Ok((under(&card.disk, &relative), relative))
}

/// Everything a record says of the original at `from` now says it at `to`.
fn rebind(snapshot: &mut Snapshot, from: &str, to: &str) {
    let metadata = &mut snapshot.metadata;
    let uris = [Some(&mut snapshot.binding.original_uri), Some(&mut metadata.original.media_uri)];
    for uri in uris.into_iter().flatten().chain(metadata.evidence.iter_mut().map(|e| &mut e.media_uri)) {
        if *uri == from {
            *uri = to.to_string();
        }
    }
}

/// Copies, probes and finishes one clip; the error says why it did not.
fn ingest_clip(card: &Card, project: &Project, clip: &CatalogClip, cancel: &AtomicBool, progress: &mut Progress, now_unix_ms: u64, report: &mut dyn FnMut(Progress)) -> Result<(), String> {
    let (source, relative) = card_file(card, &clip.snapshot.binding.original_uri)?;
    let destination = under(&project.folder.join(ORIGINAL_FOLDER), &relative);
    inside(&project.folder, &destination)?;
    std::fs::create_dir_all(destination.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    let before = progress.bytes_copied;
    let copied = qnc_copy::copy(&source, &destination, cancel, &mut |bytes| {
        progress.bytes_copied = before + bytes;
        report(*progress);
    })?;
    progress.bytes_copied = before + copied.bytes;
    progress.copied += 1;
    let copy_uri = qnc_uri::file_uri(&project.media_source_uri(), &format!("{ORIGINAL_FOLDER}/{relative}"))?;
    qnc_diagnostics::log(format!("ingest copied {relative} {} MB {}", copied.bytes / 1_000_000, copied.checksum));
    let mut thumbnail = clip.thumbnail_uri.clone();
    if let Some(uri) = &clip.thumbnail_uri {
        if let Ok((poster, _)) = card_file(card, uri) {
            let to = under(&project.folder, &format!("{}/{}/{POSTER_FILE}", project.thumbnails, clip.id()));
            if inside(&project.folder, &to).is_ok() && std::fs::create_dir_all(to.parent().ok_or("no folder")?).is_ok() && std::fs::copy(&poster, &to).is_ok() {
                thumbnail = Some(qnc_uri::file_uri(&project.media_source_uri(), &format!("{}/{}/{POSTER_FILE}", project.thumbnails, clip.id()))?);
            }
        }
    }
    report(*progress);
    // The copy is the original of the record from here on; it is probed once, where it lies.
    let mut camera = clip.snapshot.clone();
    rebind(&mut camera, &clip.snapshot.binding.original_uri, &copy_uri);
    let probes = qnc_record_compose::required_probes(&camera).map_err(|e| e.to_string())?;
    let mut parsed = Vec::new();
    for (i, media_uri) in probes.iter().filter(|u| *u == &copy_uri).enumerate() {
        let document_uri = format!("qnc://local/artifact/probe-{}", qnc_ids_fresh());
        let report = qnc_probe::probe(&destination, media_uri, PROBE_TIMEOUT)?;
        parsed.push(qnc_ffprobe_read::read(&report.json, media_uri, &document_uri, &format!("ffprobe-{i}")).map_err(|e| e.to_string())?);
    }
    let done = qnc_record_compose::finalize(&camera, &parsed, now_unix_ms).map_err(|e| e.to_string())?;
    let published = CatalogClip { snapshot: done, ..clip.clone() };
    project.writer.execute(qnc_catalog::MODULE, json!({ "publish": [published] }))?;
    project.writer.execute(qnc_catalog::MODULE, json!({ "set_import": { "clip_id": clip.id(), "status": "imported", "media_uri": copy_uri, "copy_of": "original", "thumbnail_uri": thumbnail } }))?;
    Ok(())
}

fn qnc_ids_fresh() -> String {
    qnc_ids::fresh()
}

/// Ingests every selected clip: copies, probes, finishes; `report` hears each step. A clip that
/// fails is marked failed with the reason. Returns the final progress.
pub fn run(card: &Card, project: &Project, selected: &Selected, cancel: &AtomicBool, now_unix_ms: u64, report: &mut dyn FnMut(Progress)) -> Result<Progress, String> {
    let mut progress = Progress { clips: selected.clips.len(), ..Progress::default() };
    report(progress);
    let reader = Reader::open(&project.folder.join("project.db"))?;
    for clip in &selected.clips {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        // A clip already copied and final (the card put in again): nothing to do.
        if is_done(&reader, clip.id())? {
            progress.finished += 1;
            report(progress);
            continue;
        }
        match ingest_clip
(card, project, clip, cancel, &mut progress, now_unix_ms, report) {
            Ok(()) => progress.finished += 1,
            Err(error) => {
                progress.failed += 1;
                qnc_diagnostics::log(format!("ingest failed {}: {error}", clip.name));
                let _ = project.writer.execute(qnc_catalog::MODULE, json!({ "set_import": { "clip_id": clip.id(), "status": "failed", "error": error } }));
            }
        }
        report(progress);
    }
    Ok(progress)
}

/// Whether a clip of the project is imported and final.
pub fn is_done(reader: &Reader, clip_id: &str) -> Result<bool, String> {
    Ok(qnc_catalog::read_clip(reader, clip_id)?.is_some_and(|c| c.import_status == ImportStatus::Imported && c.clip.snapshot.phase == qnc_media_record::Phase::Final))
}

#[cfg(test)]
mod tests;
