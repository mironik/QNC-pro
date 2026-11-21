//! The waves step of an ingest, in the background of one project: every imported clip without a
//! wave gets one from its copy in the project (the card is never read again), through the MXF
//! sound reader and the wave maker, written through the wave store. The catalog is read once a
//! second for new clips; the step ends when the user stopped the waves (the X, in the project's
//! database) or when it is told to stop. A clip whose wave cannot be made is said in the log and
//! left alone for this run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use qnc_db::{Reader, Writer};
use qnc_media_model::{single_video, StreamDetails};

pub const FUNCTION: &str = "waves";
const ROUND: Duration = Duration::from_secs(1);

pub struct WaveRun {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WaveRun {
    /// Starts the step for the project at `folder` (its `project.db` inside).
    pub fn start(folder: &Path, db: &Path) -> WaveRun {
        let stop = Arc::new(AtomicBool::new(false));
        let (folder, db, flag) = (folder.to_path_buf(), db.to_path_buf(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("qnc-wave-run".into())
            .spawn(move || {
                if let Err(error) = run(&folder, &db, &flag) {
                    qnc_diagnostics::log(format!("waves: {error}"));
                }
            })
            .expect("a thread for the waves");
        WaveRun { stop, thread: Some(thread) }
    }

    /// Tells the step to stop after the clip it is on.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn ended(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

fn run(folder: &Path, db: &Path, stop: &AtomicBool) -> Result<(), String> {
    let reader = Reader::open(db)?;
    let writer = Writer::start(db, vec![Box::new(qnc_wave_store::WaveTables)])?;
    let mut failed: BTreeSet<String> = BTreeSet::new();
    while !stop.load(Ordering::Relaxed) {
        if qnc_stopped_work::read(&reader)?.contains(FUNCTION) {
            qnc_diagnostics::log("waves: stopped by the user");
            return Ok(());
        }
        let made = qnc_wave_store::made(&reader)?;
        let waiting: Vec<String> = qnc_catalog::summaries(&reader)?
            .into_iter()
            .filter(|clip| clip.import_status == qnc_catalog::ImportStatus::Imported && !made.contains(&clip.clip_id) && !failed.contains(&clip.clip_id))
            .map(|clip| clip.clip_id)
            .collect();
        for clip_id in waiting {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let started = std::time::Instant::now();
            match make_one(&reader, &writer, folder, &clip_id, stop) {
                Ok(()) => qnc_diagnostics::log(format!("wave made {clip_id} in {} ms", started.elapsed().as_millis())),
                Err(error) => {
                    qnc_diagnostics::log(format!("wave not made {clip_id}: {error}"));
                    failed.insert(clip_id);
                }
            }
        }
        std::thread::sleep(ROUND);
    }
    Ok(())
}

fn under(folder: &Path, relative: &str) -> PathBuf {
    relative.split('/').filter(|p| !p.is_empty()).fold(folder.to_path_buf(), |path, part| path.join(part))
}

/// The wave of one clip from its copy in the project.
fn make_one(reader: &Reader, writer: &Writer, folder: &Path, clip_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    let stored = qnc_catalog::read_clip(reader, clip_id)?.ok_or("the clip is not in the catalog")?;
    let media_uri = stored.imported_media_uri.ok_or("the clip has no copy in the project")?;
    let (_, relative) = qnc_uri::split_file_uri(&media_uri)?;
    let path = under(folder, &relative);
    if !path.starts_with(folder) || !path.is_file() {
        return Err(format!("the copy {} is not in the project", path.display()));
    }
    let original = &stored.clip.snapshot.metadata.original;
    let timebase = single_video(original).and_then(|video| video.frame_rate.as_ref()).map(|rate| rate.value).ok_or("the record has no frame rate")?;
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
