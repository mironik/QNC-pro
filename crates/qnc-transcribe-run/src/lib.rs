//! The transcripts step of an ingest, in the background of one project: every imported clip is
//! queued whole (long ones in pieces of about a minute) with the fast engine of this computer and
//! the project's speech channel and language; each piece's sound is read from the clip's copy in
//! the project (never the card), brought to 16 kHz and heard piece by piece (`pieces`); what is
//! heard is kept in the transcript store as it comes. The step ends when the user stopped the
//! transcripts (the X, in the project's database) or when it is told to stop.

mod pieces;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use qnc_db::{Reader, Writer};
use qnc_media_model::{single_video, StreamDetails};
use qnc_transcribe_jobs::{TranscribeRequest, Transcriber, WHISPER_RATE};
use qnc_transcript_store::{ClipTranscript, Line, QueueQuality, QueueState, Queued, Word};

pub const FUNCTION: &str = "transcripts";
/// The feature of the project's AI that turns the step on.
pub const FEATURE: &str = "transcription";
/// Seconds of one queued piece of a long clip.
const QUEUED_SECONDS: f64 = 60.0;
const ROUND: Duration = Duration::from_secs(1);

pub struct TranscriptRun(qnc_step_thread::StepThread);

impl TranscriptRun {
    /// Starts the step for the project at `folder` (its `project.db`), with the engines of the QNC `root`.
    pub fn start(root: &Path, folder: &Path, db: &Path) -> TranscriptRun {
        let (root, folder, db) = (root.to_path_buf(), folder.to_path_buf(), db.to_path_buf());
        TranscriptRun(qnc_step_thread::StepThread::start(FUNCTION, move |stop| run(&root, &folder, &db, stop)))
    }

    /// Tells the step to stop after the piece it is on.
    pub fn stop(&self) {
        self.0.stop();
    }

    pub fn ended(&self) -> bool {
        self.0.ended()
    }
}

/// What the project says about its speech.
struct Speech {
    language: String,
    /// The index among the clip's sound streams (`CH1` is 0).
    channel: usize,
}

fn speech(reader: &Reader) -> Result<Speech, String> {
    let settings = qnc_project_settings::read(reader)?;
    if !settings.ai_feature(FEATURE) {
        return Err(qnc_text::text("transcribe.off_in_project").into());
    }
    let language = settings.saved["ai"]["language"].as_str().unwrap_or("auto").to_string();
    let channel = settings.saved["audio"]["transcribe_channel"].as_str().and_then(|ch| ch.strip_prefix("CH")).and_then(|n| n.parse::<usize>().ok()).and_then(|n| n.checked_sub(1)).ok_or_else(|| qnc_text::text("transcribe.no_channel").to_string())?;
    Ok(Speech { language, channel })
}

fn run(root: &Path, folder: &Path, db: &Path, stop: &AtomicBool) -> Result<(), String> {
    let reader = Reader::open(db)?;
    let speech = speech(&reader)?;
    let engine = qnc_ai_catalog::transcriber_for(root, qnc_ai_catalog::Quality::Fast)?;
    let program = Transcriber::beside_application(&engine.program)?;
    let writer = Writer::start(db, vec![Box::new(qnc_transcript_store::TranscriptTables)])?;
    qnc_diagnostics::log(format!("transcripts: engine {} language {} channel {}", engine.id, speech.language, speech.channel + 1));
    let mut failed: BTreeSet<String> = BTreeSet::new();
    while !stop.load(Ordering::Relaxed) {
        if qnc_stopped_work::read(&reader)?.contains(FUNCTION) {
            qnc_diagnostics::log("transcripts: stopped by the user");
            return Ok(());
        }
        queue_imported(&reader, &writer, &failed)?;
        let waiting: Vec<Queued> = qnc_transcript_store::read_queue(&reader)?.into_iter().filter(|entry| entry.state == QueueState::Waiting && !failed.contains(&entry.clip_id)).collect();
        for entry in waiting {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            qnc_transcript_store::set_queued(&writer, &entry, QueueState::Running, "", 0)?;
            let started = Instant::now();
            match hear_one(&reader, &writer, folder, &entry, &speech, &engine, &program, stop) {
                Ok(lines) => {
                    qnc_transcript_store::set_queued(&writer, &entry, QueueState::Done, "", started.elapsed().as_millis() as u64)?;
                    qnc_diagnostics::log(format!("transcript {} {}..{} {lines} lines in {} ms", entry.clip_id, entry.from_frame, entry.to_frame, started.elapsed().as_millis()));
                }
                Err(error) => {
                    qnc_transcript_store::set_queued(&writer, &entry, QueueState::Failed, &error, 0)?;
                    qnc_diagnostics::log(format!("transcript not made {} {}..{}: {error}", entry.clip_id, entry.from_frame, entry.to_frame));
                    failed.insert(entry.clip_id.clone());
                }
            }
        }
        std::thread::sleep(ROUND);
    }
    Ok(())
}

/// Every imported clip not queued or transcribed yet goes in, whole; a long one in pieces of
/// about a minute.
fn queue_imported(reader: &Reader, writer: &Writer, failed: &BTreeSet<String>) -> Result<(), String> {
    let queued: BTreeSet<String> = qnc_transcript_store::read_queue(reader)?.into_iter().map(|e| e.clip_id).collect();
    let transcribed = qnc_transcript_store::transcribed(reader)?;
    let mut new = Vec::new();
    for clip in qnc_catalog::summaries(reader)? {
        if clip.import_status != qnc_catalog::ImportStatus::Imported || queued.contains(&clip.clip_id) || transcribed.contains(&clip.clip_id) || failed.contains(&clip.clip_id) {
            continue;
        }
        let (Some(frames), Some(seconds)) = (clip.duration_frames, clip.duration_seconds) else { continue };
        if frames == 0 || seconds <= 0.0 {
            continue;
        }
        let most = ((QUEUED_SECONDS * frames as f64 / seconds).round() as u64).max(1);
        let pieces = frames.div_ceil(most).max(1);
        for n in 0..pieces {
            new.push(Queued { clip_id: clip.clip_id.clone(), from_frame: frames * n / pieces, to_frame: frames * (n + 1) / pieces, quality: QueueQuality::Fast, state: QueueState::Waiting, error: String::new(), took_ms: 0 });
        }
    }
    if !new.is_empty() {
        qnc_transcript_store::enqueue(writer, &new)?;
    }
    Ok(())
}

/// The speech channel of a queued range, from the clip's copy, heard; the lines kept.
#[allow(clippy::too_many_arguments)]
fn hear_one(reader: &Reader, writer: &Writer, folder: &Path, entry: &Queued, speech: &Speech, engine: &qnc_ai_catalog::Transcriber, program: &Transcriber, cancel: &AtomicBool) -> Result<usize, String> {
    let stored = qnc_catalog::read_clip(reader, &entry.clip_id)?.ok_or("the clip is not in the catalog")?;
    let (path, timebase) = stored.copy_in(folder)?;
    let original = &stored.clip.snapshot.metadata.original;
    let fps = timebase.fps();
    let mut audio: Vec<(u32, u32)> = original
        .streams
        .iter()
        .filter_map(|stream| match &stream.details {
            StreamDetails::Audio(details) => Some((stream.index.as_ref()?.value, details.sample_rate_hz.as_ref()?.value)),
            _ => None,
        })
        .collect();
    audio.sort();
    let (index, rate) = audio.get(speech.channel).copied().ok_or_else(|| qnc_text::text("transcribe.no_channel").to_string())?;
    let (elements, sample_bytes) = qnc_mxf_sound::sound_elements(original, &[index]).ok_or("the copy is not an MXF with mono PCM sound")?;
    let element = elements.first().copied().ok_or("no sound element")?;
    // A second either side, within the clip: the engine hears the words at the edges whole.
    let second = fps.round() as u64;
    let frames = single_video(original).and_then(|video| video.exact_frame_count());
    let from = entry.from_frame.saturating_sub(second);
    let to = frames.map_or(entry.to_frame + second, |frames| (entry.to_frame + second).min(frames).max(entry.to_frame));
    let sound = qnc_mxf_sound::read_element_frames(&path, element, sample_bytes, from, to, cancel)?;
    let samples = qnc_transcribe_jobs::to_whisper_rate(&sound, rate)?;
    let (lines, language, made_by) = pieces::hear_range(&samples, entry.from_frame, entry.to_frame, fps, cancel, &mut |piece, start| hear_piece(program, engine, &speech.language, piece, start, fps, (entry.from_frame, entry.to_frame), cancel))?;
    let language = if language.is_empty() { speech.language.clone() } else { language };
    let count = lines.len();
    let transcript = ClipTranscript { clip_id: entry.clip_id.clone(), from_frame: entry.from_frame, to_frame: entry.to_frame, language, engine: made_by, lines, fine: false };
    qnc_transcript_store::save(writer, &transcript)?;
    Ok(count)
}

/// One piece (16 kHz, from `start` seconds of the clip) through the engine, at the level of
/// speech; its lines as clip frames within `range`.
fn hear_piece(program: &Transcriber, engine: &qnc_ai_catalog::Transcriber, language: &str, piece: &[f32], start: f64, fps: f64, range: (u64, u64), cancel: &AtomicBool) -> Result<(Vec<Line>, String, String), String> {
    let heard = qnc_transcribe_jobs::at_speech_level(piece, WHISPER_RATE as usize);
    let request = TranscribeRequest::new(&engine.id, &heard, language)?;
    let frame = |seconds: f64| (((start + seconds) * fps).round().max(0.0) as u64).clamp(range.0, range.1);
    let to_line = |segment: &qnc_transcribe_jobs::Segment| Line {
        from: frame(segment.start),
        to: frame(segment.end),
        text: segment.text.clone(),
        words: segment.words.iter().map(|word| Word { from: frame(word.start), to: frame(word.end).max(frame(word.start)), text: word.text.clone() }).collect(),
    };
    let transcript = program.transcribe(&request, cancel, &mut |_| {})?;
    let made_by = format!("{} {}", transcript.engine, qnc_ai_catalog::model_name(&engine.model));
    Ok((transcript.segments.iter().map(to_line).collect(), transcript.language, made_by))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_whose_ai_has_no_transcription_does_not_start_the_step() {
        let dir = std::env::temp_dir().join(format!("qnc-transcribe-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let writer = Writer::start(&db, vec![Box::new(qnc_project_settings::SettingsTable)]).unwrap();
        writer.execute(qnc_project_settings::MODULE, serde_json::json!({ "set": { "project_id": "p", "settings": { "ai": { "features": [] } }, "at": "2026-11-21T00:00:00Z" } })).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(speech(&reader).is_err(), "transcription off is no step");
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
