//! The filmstrips step of an ingest, in the background of one project: every imported clip
//! without a filmstrip gets its thirteen pictures from its copy in the project (the card is
//! never read again): the intra pictures nearest the places through the MXF index when the
//! original carries one, the key pictures at the places by a seek otherwise; one ffmpeg run a
//! clip. The JPEGs go under the project's filmstrip product folder, the filmstrip through the
//! filmstrip store. The rounds and the stop are those of every step.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use qnc_db::{Reader, Writer};
use qnc_media_model::{single_video, Signal, StreamDetails};

pub const FUNCTION: &str = "filmstrips";
/// The product area of the project settings the pictures go under.
pub const PRODUCT: &str = "filmstrip";

pub struct FilmstripRun(qnc_step_thread::StepThread);

impl FilmstripRun {
    /// Starts the step for the project at `folder` (its `project.db` inside).
    pub fn start(folder: &Path, db: &Path) -> FilmstripRun {
        let (folder, db) = (folder.to_path_buf(), db.to_path_buf());
        FilmstripRun(qnc_step_thread::StepThread::start(FUNCTION, move |stop| run(&folder, &db, stop)))
    }

    /// Tells the step to stop after the clip it is on.
    pub fn stop(&self) {
        self.0.stop();
    }

    pub fn ended(&self) -> bool {
        self.0.ended()
    }
}

fn run(folder: &Path, db: &Path, stop: &AtomicBool) -> Result<(), String> {
    let reader = Reader::open(db)?;
    let product = qnc_project_settings::read(&reader)?.product_path(PRODUCT)?;
    let writer = Writer::start(db, vec![Box::new(qnc_filmstrip_store::FilmstripTables)])?;
    qnc_clip_round::run(&reader, FUNCTION, stop, &qnc_filmstrip_store::made, &mut |clip_id, cancel| make_one(&reader, &writer, folder, &product, clip_id, cancel))
}

/// The filmstrip of one clip from its copy in the project.
fn make_one(reader: &Reader, writer: &Writer, folder: &Path, product: &str, clip_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    let stored = qnc_catalog::read_clip(reader, clip_id)?.ok_or("the clip is not in the catalog")?;
    let (path, timebase) = stored.copy_in(folder)?;
    let original = &stored.clip.snapshot.metadata.original;
    let video = single_video(original).ok_or("the record has no picture stream")?;
    let frames = video.exact_frame_count().ok_or("the record has no exact frame count")?;
    let codec = original.streams.iter().find(|stream| matches!(stream.details, StreamDetails::Video(_))).and_then(|stream| match &stream.codec {
        Some(fact) => match &fact.value {
            Signal::Known(name) => qnc_filmstrip_pictures::stored_picture_codec(name),
            _ => None,
        },
        None => None,
    });
    let places = qnc_filmstrip_pictures::places(frames);
    let fps = timebase.fps();
    let seeks: Vec<f64> = places.iter().map(|frame| *frame as f64 / fps).collect();
    let stored_pictures = match codec {
        Some(_) => qnc_filmstrip_pictures::intra_pictures(&path, &places, cancel)?,
        None => None,
    };
    let jpegs = match (codec, stored_pictures) {
        (Some(codec), Some(pictures)) => qnc_filmstrip_pictures::decode_stored(codec, &pictures, cancel)?,
        _ => qnc_filmstrip_pictures::decode_seeked(&path, &seeks, cancel)?,
    };
    let dir_name = qnc_filmstrip_pictures::folder_name(&stored.clip.name);
    let dir = product.split('/').filter(|p| !p.is_empty()).chain([dir_name.as_str()]).fold(folder.to_path_buf(), |path, part| path.join(part));
    std::fs::create_dir_all(&dir).map_err(|error| format!("filmstrip folder: {error}"))?;
    let mut pictures = Vec::with_capacity(jpegs.len());
    for (index, ((frame, seek_sec), jpeg)) in places.iter().zip(&seeks).zip(&jpegs).enumerate() {
        let name = qnc_filmstrip_pictures::file_name(index, *seek_sec);
        std::fs::write(dir.join(&name), jpeg).map_err(|error| format!("filmstrip picture write: {error}"))?;
        pictures.push(qnc_filmstrip_store::Picture { frame: *frame, seek_sec: *seek_sec, file: format!("{}/{dir_name}/{name}", product.trim_matches('/')) });
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    qnc_filmstrip_store::save(writer, &qnc_filmstrip_store::Saved { clip_id: clip_id.to_string(), pictures, made_at_unix_ms: now })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_run_over_an_empty_project_ends_when_the_filmstrips_are_stopped() {
        let dir = std::env::temp_dir().join(format!("qnc-filmstrip-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let writer = Writer::start(&db, vec![Box::new(qnc_catalog::CatalogTables), Box::new(qnc_stopped_work::StoppedWork), Box::new(qnc_project_settings::SettingsTable)]).unwrap();
        writer.execute(qnc_project_settings::MODULE, serde_json::json!({ "set": { "project_id": "p", "settings": { "products": { "filmstrip": "products/filmstrip" } }, "at": "2026-11-21T00:00:00Z" } })).unwrap();
        qnc_stopped_work::stop(&writer, FUNCTION).unwrap();
        let run = FilmstripRun::start(&dir, &db);
        for _ in 0..40 {
            if run.ended() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(run.ended(), "the X in the database ends the step");
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
