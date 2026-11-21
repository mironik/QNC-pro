//! The waves step of an ingest, in the background of one project: every imported clip without a
//! wave gets one from its copy in the project (the card is never read again), through the MXF
//! sound reader and the wave maker, written through the wave store. The catalog is read once a
//! second for new clips; the step ends when the user stopped the waves (the X, in the project's
//! database) or when it is told to stop. A clip whose wave cannot be made is said in the log and
//! left alone for this run.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use qnc_db::{Reader, Writer};
use qnc_media_model::StreamDetails;

pub const FUNCTION: &str = "waves";

pub struct WaveRun(qnc_step_thread::StepThread);

impl WaveRun {
    /// Starts the step for the project at `folder` (its `project.db` inside).
    pub fn start(folder: &Path, db: &Path) -> WaveRun {
        let (folder, db) = (folder.to_path_buf(), db.to_path_buf());
        WaveRun(qnc_step_thread::StepThread::start(FUNCTION, move |stop| run(&folder, &db, stop)))
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
    let writer = Writer::start(db, vec![Box::new(qnc_wave_store::WaveTables)])?;
    qnc_clip_round::run(&reader, FUNCTION, stop, &qnc_wave_store::made, &mut |clip_id, cancel| make_one(&reader, &writer, folder, clip_id, cancel))
}

/// The wave of one clip from its copy in the project.
fn make_one(reader: &Reader, writer: &Writer, folder: &Path, clip_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    let stored = qnc_catalog::read_clip(reader, clip_id)?.ok_or("the clip is not in the catalog")?;
    let (path, timebase) = stored.copy_in(folder)?;
    let original = &stored.clip.snapshot.metadata.original;
    let mut audio: Vec<(u32, u32)> = original
        .streams
        .iter()
        .filter_map(|stream| match &stream.details {
            StreamDetails::Audio(details) => Some((stream.index.as_ref()?.value, details.sample_rate_hz.as_ref()?.value)),
            _ => None,
        })
        .collect();
    audio.sort();
    let sample_rate = audio.first().map(|(_, rate)| *rate).ok_or("the record has no sound stream")?;
    if audio.iter().any(|(_, rate)| *rate != sample_rate) {
        return Err("the sound streams have different sample rates".into());
    }
    let indices: Vec<u32> = audio.iter().map(|(index, _)| *index).collect();
    let (elements, sample_bytes) = qnc_mxf_sound::sound_elements(original, &indices).ok_or("the copy is not an MXF with mono PCM sound")?;
    let mut maker = qnc_wave_make::Maker::new(sample_rate, elements.len(), timebase)?;
    let mut samples = Vec::new();
    qnc_mxf_sound::read_sound_elements(&path, cancel, &mut |number, value| {
        let Some(slot) = elements.iter().position(|element| *element == number) else { return Ok(()) };
        samples.clear();
        match sample_bytes {
            2 => qnc_mxf_sound::s16le_to_f32(value, &mut samples),
            _ => qnc_mxf_sound::s24le_to_f32(value, &mut samples),
        }
        maker.push_channel(slot, &samples)
    })?;
    let made = maker.finish();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    qnc_wave_store::save(writer, &qnc_wave_store::Saved { clip_id: clip_id.to_string(), frames_per_second: made.frames_per_second, lanes: made.lanes, loudness: made.loudness, made_at_unix_ms: now })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_run_over_an_empty_project_ends_when_told_and_when_the_waves_are_stopped() {
        let dir = std::env::temp_dir().join(format!("qnc-wave-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let catalog = Writer::start(&db, vec![Box::new(qnc_catalog::CatalogTables), Box::new(qnc_stopped_work::StoppedWork)]).unwrap();
        let run = WaveRun::start(&dir, &db);
        std::thread::sleep(Duration::from_millis(300));
        assert!(!run.ended(), "it keeps looking");
        qnc_stopped_work::stop(&catalog, FUNCTION).unwrap();
        for _ in 0..40 {
            if run.ended() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(run.ended(), "the X in the database ends the step");
        let run = WaveRun::start(&dir, &db);
        run.stop();
        for _ in 0..40 {
            if run.ended() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(run.ended());
        drop(catalog);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
