//! The statements heard from the sound the waves read: each clip's first channel, handed over
//! once its wave is made, waits here at 16 kHz; as many hearers as the fast engines have
//! programs take them, each hearer with a program of its own. A clip is heard by the statement
//! rule: a short one whole, a long one by its loudest seconds first; a statement's transcript is
//! kept and its queue entry done, a clip that is no statement is marked so in the queue (the AI
//! shots take it, the queue does not hear it again). Nothing is read from the medium again.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use qnc_db::Writer;
use qnc_transcribe_jobs::{Transcriber, WHISPER_RATE};
use qnc_transcript_store::{ClipTranscript, QueueQuality, QueueState, Queued};

use crate::{hear_piece, pieces, Speech};

/// A clip's first channel at 16 kHz and its frames a second.
struct Clip {
    clip_id: String,
    fps: f64,
    samples: Vec<f32>,
}

#[derive(Default)]
struct Waiting {
    /// The clips to hear and whether no more come.
    clips: Mutex<(Vec<Clip>, bool)>,
    ready: Condvar,
    /// How many clips are being heard now.
    hearing: AtomicUsize,
}

impl Waiting {
    /// The next clip to hear; none once no more come.
    fn next(&self) -> Option<Clip> {
        let mut clips = self.clips.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if !clips.0.is_empty() {
                self.hearing.fetch_add(1, Ordering::Relaxed);
                return Some(clips.0.remove(0));
            }
            if clips.1 {
                return None;
            }
            clips = self.ready.wait(clips).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn no_more(&self) {
        let mut clips = self.clips.lock().unwrap_or_else(|e| e.into_inner());
        clips.1 = true;
        self.ready.notify_all();
    }
}

pub struct Hearers {
    waiting: Arc<Waiting>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Hearers {
    /// As many hearers as the fast engines have programs (`parallel` each), each with its own
    /// program, writing to `db`; `stop` makes them drop what waits.
    pub fn start(db: &std::path::Path, speech: &Speech, engines: &[qnc_ai_catalog::Transcriber], stop: Arc<AtomicBool>) -> Result<Hearers, String> {
        let writer = Writer::start(db, vec![Box::new(qnc_transcript_store::TranscriptTables)])?;
        let waiting = Arc::new(Waiting::default());
        let mut threads = Vec::new();
        for engine in engines {
            for _ in 0..engine.parallel.max(1) {
                let program = Transcriber::beside_application(&engine.program)?;
                let (engine, speech, waiting, stop, writer) = (engine.clone(), speech.clone(), waiting.clone(), stop.clone(), writer.clone());
                threads.push(std::thread::spawn(move || {
                    while let Some(clip) = waiting.next() {
                        if !stop.load(Ordering::Relaxed) {
                            if let Err(error) = hear_clip(&writer, &program, &engine, &speech, &clip, &stop) {
                                qnc_diagnostics::log(format!("statement {} not heard: {error}", clip.clip_id));
                            }
                        }
                        waiting.hearing.fetch_sub(1, Ordering::Relaxed);
                    }
                }));
            }
        }
        Ok(Hearers { waiting, threads })
    }

    /// What the wave run hands a clip's first channel to.
    pub fn sink(&self) -> qnc_wave_run::FirstChannelSink {
        let waiting = self.waiting.clone();
        Arc::new(move |first: qnc_wave_run::FirstChannel| match qnc_transcribe_jobs::to_whisper_rate(&first.samples, first.rate) {
            Ok(samples) => {
                let mut clips = waiting.clips.lock().unwrap_or_else(|e| e.into_inner());
                clips.0.push(Clip { clip_id: first.clip_id, fps: first.fps, samples });
                waiting.ready.notify_one();
            }
            Err(error) => qnc_diagnostics::log(format!("statement {} not heard: {error}", first.clip_id)),
        })
    }

    /// Whether a clip waits or is being heard.
    pub fn busy(&self) -> bool {
        !self.waiting.clips.lock().unwrap_or_else(|e| e.into_inner()).0.is_empty() || self.waiting.hearing.load(Ordering::Relaxed) > 0
    }

    /// No more clips come: the hearers hear what waits (nothing once stopped) and end.
    pub fn finish(mut self) {
        self.waiting.no_more();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Hears one clip by the statement rule and writes what it found.
fn hear_clip(writer: &Writer, program: &Transcriber, engine: &qnc_ai_catalog::Transcriber, speech: &Speech, clip: &Clip, cancel: &AtomicBool) -> Result<(), String> {
    let began = std::time::Instant::now();
    let rate = WHISPER_RATE as usize;
    let frame_of = |sample: usize| (sample as f64 / rate as f64 * clip.fps).round() as u64;
    let end = frame_of(clip.samples.len()).max(1);
    let mut hear = |piece: &[f32], start: f64| hear_piece(program, engine, &speech.language, piece, start, clip.fps, (0, end), cancel);
    let whole = |hear: &mut dyn FnMut(&[f32], f64) -> Result<(Vec<qnc_transcript_store::Line>, String, String), String>| pieces::hear_range(&clip.samples, 0, end, clip.fps, cancel, hear);
    let (statement, lines, language, made_by) = if clip.samples.len() <= qnc_statement_rule::WHOLE_SECONDS * rate {
        let (lines, language, made_by) = whole(&mut hear)?;
        let share = qnc_statement_rule::spoken_share(words_of(&lines), 0, end);
        (qnc_statement_rule::is_statement(share), lines, language, made_by)
    } else {
        let window = qnc_statement_rule::loudest_window(&clip.samples, rate, qnc_statement_rule::PROBE_SECONDS * rate);
        let (from, to) = (frame_of(window.start), frame_of(window.end));
        let (probe, _, _) = hear_piece(program, engine, &speech.language, &clip.samples[window.clone()], window.start as f64 / rate as f64, clip.fps, (from, to), cancel)?;
        let share = qnc_statement_rule::spoken_share(words_of(&probe), from, to);
        qnc_diagnostics::log(format!("statement {} probe {from}..{to} share={share:.2}", clip.clip_id));
        if qnc_statement_rule::is_statement(share) {
            let (lines, language, made_by) = whole(&mut hear)?;
            (true, lines, language, made_by)
        } else {
            (false, Vec::new(), String::new(), String::new())
        }
    };
    let entry = Queued { clip_id: clip.clip_id.clone(), from_frame: 0, to_frame: end, quality: QueueQuality::Fast, state: QueueState::Waiting, error: String::new(), took_ms: 0 };
    qnc_transcript_store::enqueue(writer, std::slice::from_ref(&entry))?;
    let took = began.elapsed().as_millis() as u64;
    if statement {
        let language = if language.is_empty() { speech.language.clone() } else { language };
        let count = lines.len();
        qnc_transcript_store::save(writer, &ClipTranscript { clip_id: clip.clip_id.clone(), from_frame: 0, to_frame: end, language, engine: made_by, lines, fine: false })?;
        qnc_transcript_store::set_queued(writer, &entry, QueueState::Done, "", took)?;
        qnc_diagnostics::log(format!("statement {} transcribed: {count} lines in {took} ms ({:.0} s of sound)", clip.clip_id, clip.samples.len() as f64 / rate as f64));
    } else {
        qnc_transcript_store::set_queued(writer, &entry, QueueState::Failed, &qnc_text::text("transcribe.no_statement"), took)?;
        qnc_diagnostics::log(format!("statement {} is no statement after {took} ms", clip.clip_id));
    }
    Ok(())
}

fn words_of(lines: &[qnc_transcript_store::Line]) -> Vec<(u64, u64)> {
    lines.iter().flat_map(|line| line.words.iter().map(|word| (word.from, word.to))).collect()
}
