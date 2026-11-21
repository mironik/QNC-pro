//! The session of one ingest window (copied from the old QNC's `qnc-ai-card-ingest`, its work;
//! the drawing is `qnc-ingest-window-form`): one card into the project the tray made, the
//! progress of its functions and how long each took, Cancel with the question what becomes of the
//! partial import (kept, or the project deleted), the place and description typed for the project
//! (kept as its search text), and the form the computer's mode opens once the card is copied.
//! The form draws the view and hands back what the user did; every rule is here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qnc_card_ingest::{Ingests, Said};
use qnc_card_select::Card;
use qnc_ingest_run::Progress;
use qnc_progress::Counts;

/// How often the counts are read from the project's database.
const READ: Duration = Duration::from_secs(1);

/// The text code of a function's bar.
fn text_of(function: &str) -> &'static str {
    match function {
        "posters" => "ingest.fn.posters",
        "filmstrips" => "ingest.fn.filmstrips",
        "waves" => "ingest.fn.waves",
        "transcripts" => "ingest.fn.transcripts",
        "shots" => "ingest.fn.shots",
        "content" => "ingest.fn.content",
        "captions" => "ingest.fn.captions",
        _ => "ingest.fn.import",
    }
}

/// One bar of the window: the code of its name, what its X stops, how many of all are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub text: &'static str,
    pub function: &'static str,
    pub done: usize,
    pub stopped: bool,
}

/// The function of the one bar the ingest has: the import of the clips.
pub const IMPORT: &str = "import";
/// The form Breaking news opens once the card is copied, and its pool tab.
const NEXT_FORM: &str = "qnc-media-assist-audio-ai";
const NEXT_ARGS: [&str; 2] = ["--pool-tab", "editorial_tab_all"];

/// What the window asks the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Cancel the ingest?
    Cancel,
    /// Keep or delete the partial import?
    Partial,
}

/// Where the ingest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    Cancelled,
}

/// What the form draws.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub phase: Phase,
    /// The card and its camera.
    pub card: String,
    /// How long the ingest ran, or runs.
    pub elapsed: Duration,
    pub project_name: String,
    /// The card is known to an earlier project: the line that says so (and how many clips are new).
    pub known: Option<String>,
    /// What became of the card's eject, once known.
    pub card_said: Option<String>,
    /// The card is out: the notice shown while the window waits to close, with the seconds left.
    pub notice: Option<(String, u64)>,
    pub clips: usize,
    pub bars: Vec<Bar>,
    pub failed: usize,
    pub bytes_copied: u64,
    pub ask: Option<Ask>,
    /// The window ends.
    pub close: bool,
}

/// The place and description typed for the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub location: String,
    pub description: String,
    written: (String, String),
}

pub struct Session {
    root: PathBuf,
    project_id: String,
    name: String,
    camera: String,
    card: Card,
    ingests: Ingests,
    last: Progress,
    started: Instant,
    took: Option<Duration>,

    ask: Option<Ask>,
    phase: Phase,
    close: bool,
    opened_form: bool,
    stopped: bool,
    /// The project is deleted once the ingest thread has let its files go.
    delete_pending: bool,
    pub info: Info,
    /// The project's database: the counts of every function are read from it.
    reader: qnc_db::Reader,
    /// Writes the functions stopped with the X.
    stops: qnc_db::Writer,
    counts: Counts,
    read_at: Option<Instant>,
    /// When each function's bar first became full.
    full_at: BTreeMap<&'static str, Duration>,
    plan: qnc_ingest_plan::Plan,
    /// The waves step, while the plan has it.
    waves: Option<qnc_wave_run::WaveRun>,
    /// The transcripts step, while the plan has it and the project transcribes.
    transcripts: Option<qnc_transcribe_run::TranscriptRun>,
    /// The filmstrips step, while the plan has it.
    filmstrips: Option<qnc_filmstrip_run::FilmstripRun>,
    /// The card is known to an earlier project: what the window says of it.
    known: Option<String>,
    /// The card taken out by itself after the copy or a Cancel.
    card_out: qnc_card_out::CardOut,
    /// The card is out after the copy (the notice was shown).
    card_gone: bool,
    /// Set once the waves ended: the transcripts then queue what the waves did not decide.
    waves_done: qnc_transcribe_run::WavesDone,
}

/// The value after `--name` in the arguments.
fn arg(args: &[String], name: &str) -> Result<String, String> {
    args.iter().position(|a| a == name).and_then(|at| args.get(at + 1)).cloned().ok_or_else(|| qnc_text::fill("ingest.arg_missing", &[("name", name)]))
}

impl Session {
    /// The session of the arguments the tray gave: `--project-id`, `--folder`, `--name`, `--disk`,
    /// `--serial`, `--label`, `--root-path`, `--camera`; `--goes-on` (a card put in again goes on in
    /// its project: what the user stopped runs again), `--only-new-of <folder of the earlier project>` (the clips
    /// that project has are not taken), `--known <text>` (the line said of the earlier project). The
    /// ingest starts at once.
    pub fn start(root: &Path, args: &[String]) -> Result<Session, String> {
        let project_id = arg(args, "--project-id")?;
        let folder = PathBuf::from(arg(args, "--folder")?);
        let name = arg(args, "--name")?;
        let camera = arg(args, "--camera").unwrap_or_default();
        let card = Card { disk: PathBuf::from(arg(args, "--disk")?), serial: arg(args, "--serial")?, volume_name: arg(args, "--label").unwrap_or_default(), root_path: arg(args, "--root-path")? };
        let projects = qnc_project_make::projects_root(root)?;
        let (profiles, refused) = qnc_camera_profile::read_profiles(&root.join("contracts").join("cameras").join("profiles"));
        for why in refused {
            qnc_diagnostics::log(format!("camera profile refused: {why}"));
        }
        let template = { let _writer = qnc_project_templates::writer(root)?; qnc_project_templates::selected(&qnc_registry::reader(root)?)?.settings };
        let db = folder.join(qnc_project_make::PROJECT_DB);
        let reader = qnc_db::Reader::open(&db)?;
        let settings = qnc_project_settings::read(&reader)?;
        let plan = qnc_ingest_plan::Plan::of_settings(settings.saved.get("artifacts").unwrap_or(&serde_json::Value::Null))?;
        let stops = qnc_db::Writer::start(&db, vec![Box::new(qnc_stopped_work::StoppedWork), Box::new(qnc_ingest_cards::IngestCardsTable)])?;
        // The project remembers every ingest of a card: its serial, label, camera, day and time.
        qnc_ingest_cards::record(&stops, &qnc_ingest_cards::IngestedCard { serial: card.serial.clone(), label: card.volume_name.clone(), camera: camera.clone(), started_at_utc: qnc_ingest_cards::now_utc() })?;
        if args.iter().any(|a| a == "--goes-on") {
            qnc_stopped_work::resume_all(&stops)?;
            qnc_diagnostics::log(format!("ingest window: project {project_id} goes on, nothing stopped"));
        }
        let known = arg(args, "--known").ok();
        // The card goes out after the copy only when the project keeps nothing on it.
        let free = match settings.playback_input() {
            Ok(input) => qnc_card_out::free_when_imported(input),
            Err(error) => {
                qnc_diagnostics::log(format!("ingest window: the card stays in, playback input not read: {error}"));
                false
            }
        };
        let card_out = qnc_card_out::CardOut::new(&card.disk, free);
        let mut ingests = Ingests::new(root, &projects, template, profiles);
        ingests.start_into(card.clone(), &project_id, &folder, Default::default())?;

        qnc_diagnostics::log(format!("ingest window: card {} into project {project_id}, plan {}", card.serial, plan.line()));
        // The transcripts first: the waves hand them every clip's first channel (one read of the copy).
        let waves_done = qnc_transcribe_run::WavesDone::default();
        let transcripts = match (plan.has(qnc_ingest_plan::Step::Transcripts) && settings.ai_feature(qnc_transcribe_run::FEATURE)).then(|| qnc_transcribe_run::TranscriptRun::start(root, &folder, &db, waves_done.clone())) {
            Some(Ok(run)) => Some(run),
            Some(Err(error)) => {
                qnc_diagnostics::log(format!("transcripts not started: {error}"));
                None
            }
            None => None,
        };
        let waves = plan.has(qnc_ingest_plan::Step::Waves).then(|| qnc_wave_run::WaveRun::start(&folder, &db, transcripts.as_ref().map(qnc_transcribe_run::TranscriptRun::sink)));
        if waves.is_none() {
            waves_done.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let filmstrips = plan.has(qnc_ingest_plan::Step::Filmstrips).then(|| qnc_filmstrip_run::FilmstripRun::start(&folder, &db));
        Ok(Session { root: root.to_path_buf(), project_id, name, camera, card, ingests, last: Progress::default(), started: Instant::now(), took: None, ask: None, phase: Phase::Running, close: false, opened_form: false, stopped: false, delete_pending: false, info: Info::default(), reader, stops, counts: Counts::default(), read_at: None, full_at: BTreeMap::new(), plan, waves, transcripts, filmstrips, waves_done, known, card_out, card_gone: false })
    }

    /// Nothing of the background work runs any more.
    fn work_ended(&self) -> bool {
        self.ingests.running() == 0 && self.waves.as_ref().is_none_or(qnc_wave_run::WaveRun::ended) && self.transcripts.as_ref().is_none_or(qnc_transcribe_run::TranscriptRun::ended) && self.filmstrips.as_ref().is_none_or(qnc_filmstrip_run::FilmstripRun::ended)
    }

    /// The counts of every function from the project's database, once a second; when a bar
    /// first fills, its time is kept.
    fn read_counts(&mut self) {
        // The waves are done when their step ended, or the copy ended and every imported clip has one.
        let waves_over = self.waves.as_ref().is_none_or(qnc_wave_run::WaveRun::ended) || (self.took.is_some() && self.counts.waves.is_some_and(|made| made >= self.counts.imported));
        if !self.waves_done.load(std::sync::atomic::Ordering::Relaxed) && waves_over {
            self.waves_done.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(transcripts) = &mut self.transcripts {
                transcripts.waves_ended();
            }
        }
        if self.read_at.is_some_and(|at| at.elapsed() < READ) {
            return;
        }
        self.read_at = Some(Instant::now());
        let read = qnc_catalog::counts(&self.reader).and_then(|mut counts| {
            counts.stopped = qnc_stopped_work::bits(&qnc_stopped_work::read(&self.reader)?);
            if self.plan.has(qnc_ingest_plan::Step::Waves) {
                counts.waves = Some(qnc_wave_store::made(&self.reader)?.len());
            }
            if self.transcripts.is_some() {
                counts.transcripts = Some(qnc_transcript_store::heard(&self.reader)?.len());
            }
            if self.filmstrips.is_some() {
                counts.filmstrips = Some(qnc_filmstrip_store::made(&self.reader)?.len());
            }
            Ok(counts)
        });
        match read {
            Ok(counts) => self.counts = counts,
            Err(error) => qnc_diagnostics::log(format!("ingest window: counts not read: {error}")),
        }
        let elapsed = self.started.elapsed();
        for bar in self.counts.bars() {
            if bar.full() && self.counts.clips > 0 {
                self.full_at.entry(bar.function).or_insert(elapsed);
            }
        }
    }

    /// Listens to the ingest, times it, opens the next form when the mode says, deletes the
    /// project when asked; gives what the form draws.
    pub fn poll(&mut self) -> View {
        let mut ended = false;
        for running in self.ingests.running_mut() {
            for said in running.poll() {
                match said {
                    Said::Progress(p) => self.last = p,
                    Said::Done(p) => {
                        self.last = p;
                        ended = true;
                    }
                    Said::Failed(error) => {
                        qnc_diagnostics::log(format!("ingest window: {error}"));
                        ended = true;
                    }
                    Said::ProjectMade { .. } => {}
                }
            }
        }
        if ended && self.took.is_none() {
            self.took = Some(self.started.elapsed());
            if self.stopped {
                self.phase = Phase::Cancelled;
            }
            qnc_diagnostics::log(format!("ingest window: copy ended after {} ms, {:?}", self.started.elapsed().as_millis(), self.last));
        }
        // The card out once the import copied it whole; the window closes when it is out.
        self.card_out.imported(self.last.clips, self.last.finished, self.last.failed);
        let card_said = self.card_out.said();
        // Until the steps run in a program of their own, the window closes only once their work
        // is done too (its threads would end with it).
        if !self.stopped && self.card_out.window_closes() {
            self.card_gone = true;
        }
        if self.card_gone && self.phase == Phase::Done && !self.close {
            qnc_diagnostics::log("ingest window: the card is out and the work done, the window closes");
            self.close = true;
        }
        self.open_next_form();
        self.delete_when_free();
        self.read_counts();
        // Done once the copy ended and every bar is full or stopped; the waves may still run after the copy.
        if self.phase == Phase::Running && self.took.is_some() && self.counts.done() {
            self.phase = Phase::Done;
            if let Some(waves) = &self.waves {
                waves.stop();
            }
            if let Some(transcripts) = &self.transcripts {
                transcripts.stop();
            }
            if let Some(filmstrips) = &self.filmstrips {
                filmstrips.stop();
            }
            qnc_diagnostics::log(format!("ingest window: done after {} ms", self.started.elapsed().as_millis()));
        }
        let elapsed = self.took.unwrap_or_else(|| self.started.elapsed());
        let card = match self.camera.is_empty() {
            true => self.card.volume_name.clone(),
            false => format!("{} ({})", self.card.volume_name, self.camera),
        };
        let bars = self.counts.bars().into_iter().map(|bar| Bar { text: text_of(bar.function), function: bar.function, done: bar.done, stopped: bar.stopped }).collect();
        View {
            phase: self.phase,
            card,
            elapsed,
            project_name: self.name.clone(),
            known: self.known.clone(),
            card_said,
            notice: self.card_out.notice(),
            clips: self.counts.clips.max(self.last.clips),
            bars,
            failed: self.counts.failed.max(self.last.failed),
            bytes_copied: self.last.bytes_copied,
            ask: self.ask,
            close: self.close,
        }
    }

    /// Breaking news on this computer: the next form opens as soon as every clip is copied; the
    /// story step is written. Story: the step says the ingest is done; Net: nothing opens.
    fn open_next_form(&mut self) {
        let copied = self.last.clips > 0 && self.last.finished + self.last.failed >= self.last.clips;
        // After the card is out (or no eject comes): a form opened over it would hold the card.
        if self.opened_form || !copied || self.stopped || !(self.card_out.settled() || self.last.failed > 0) {
            return;
        }
        self.opened_form = true;
        let mode = qnc_db::Reader::open(&qnc_mode::computer_db(&self.root)).and_then(|r| qnc_mode::read_mode(&r)).unwrap_or(None);
        let step = match mode {
            Some(qnc_mode::Mode::BreakingNews) => "ai_media_assist",
            _ => "ingested",
        };
        match qnc_mode::writer(&self.root).and_then(|w| qnc_mode::set_step(&w, &self.project_id, step)) {
            Ok(()) => qnc_diagnostics::log(format!("ingest window: story step {step}")),
            Err(error) => qnc_diagnostics::log(format!("ingest window: story step not written: {error}")),
        }
        if mode == Some(qnc_mode::Mode::BreakingNews) {
            match qnc_launch::start_detached(NEXT_FORM, &NEXT_ARGS, qnc_launch::Priority::Same) {
                Ok(_) => qnc_diagnostics::log(format!("ingest window: breaking news opened {NEXT_FORM}")),
                Err(error) => qnc_diagnostics::log(format!("ingest window: {NEXT_FORM} not opened: {error}")),
            }
        }
    }

    /// The X beside a bar: the function is stopped in the project's database (the work leaves it
    /// alone); the import running stops at once. What is made stays.
    pub fn stop_function(&mut self, function: &str) {
        match qnc_stopped_work::stop(&self.stops, function) {
            Ok(()) => qnc_diagnostics::log(format!("ingest window: {function} stopped")),
            Err(error) => qnc_diagnostics::log(format!("ingest window: {function} not stopped: {error}")),
        }
        self.read_at = None;
        if function == IMPORT {
            self.ingests.stop_all();
            self.stopped = true;
        }
        if function == qnc_wave_run::FUNCTION {
            if let Some(waves) = &self.waves {
                waves.stop();
            }
        }
        if function == qnc_transcribe_run::FUNCTION {
            if let Some(transcripts) = &self.transcripts {
                transcripts.stop();
            }
        }
        if function == qnc_filmstrip_run::FUNCTION {
            if let Some(filmstrips) = &self.filmstrips {
                filmstrips.stop();
            }
        }
    }

    pub fn ask_cancel(&mut self) {
        if self.phase == Phase::Running {
            self.ask = Some(Ask::Cancel);
        }
    }

    /// Cancel confirmed: the work stops and the question of the partial import follows.
    pub fn answer_cancel(&mut self, yes: bool) {
        self.ask = yes.then_some(Ask::Partial);
        if yes {
            qnc_diagnostics::log("ingest window: cancelled by the user");
            self.ingests.stop_all();
            self.card_out.start("cancel");
            if let Some(waves) = &self.waves {
                waves.stop();
            }
            if let Some(transcripts) = &self.transcripts {
                transcripts.stop();
            }
            if let Some(filmstrips) = &self.filmstrips {
                filmstrips.stop();
            }
            self.stopped = true;
        }
    }

    /// Delete the partial import (the project goes once the ingest let its files go) or keep it.
    pub fn answer_partial(&mut self, delete: bool) {
        self.ask = None;
        self.delete_pending = delete;
        self.close = !delete;
        qnc_diagnostics::log(if delete { "ingest window: the partial import is deleted" } else { "ingest window: the partial import is kept" });
    }

    fn delete_when_free(&mut self) {
        if !self.delete_pending || !self.work_ended() {
            return;
        }
        self.delete_pending = false;
        match qnc_project_delete::delete(&self.root, &self.project_id) {
            Ok(deleted) => qnc_diagnostics::log(format!("ingest window: project {} deleted", deleted.name)),
            Err(error) => qnc_diagnostics::log(format!("ingest window: project {} not deleted: {error}", self.project_id)),
        }
        self.close = true;
    }

    /// How long the function of `bar` took (its bar full), or runs.
    pub fn time_of(&self, bar: &Bar) -> Option<Duration> {
        match self.full_at.get(bar.function) {
            Some(took) => Some(*took),
            None => (!bar.stopped).then(|| self.took.unwrap_or_else(|| self.started.elapsed())),
        }
    }

    /// Writes the place and description typed when they differ from what is written: the
    /// project's search text (name, place, description).
    pub fn keep_info(&mut self) {
        let now = (self.info.location.trim().to_string(), self.info.description.trim().to_string());
        if now == self.info.written {
            return;
        }
        let text = format!("{} {} {}", self.name, now.0, now.1).trim().to_string();
        let written = qnc_registry::writer(&self.root).and_then(|w| w.execute(qnc_registry::MODULE, serde_json::json!({ "search": { "project_id": self.project_id, "text": text } })));
        match written {
            Ok(_) => self.info.written = now,
            Err(error) => qnc_diagnostics::log(format!("ingest window: project info not written: {error}")),
        }
    }

    /// The window is being closed by the user: what was typed is kept.
    pub fn closing(&mut self) {
        self.keep_info();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_needs_the_arguments_of_its_card_and_project() {
        let root = std::env::temp_dir().join(format!("qnc-ingest-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("contracts").join("modules")).unwrap();
        let args: Vec<String> = ["--project-id", "p1"].iter().map(|s| s.to_string()).collect();
        let Err(error) = Session::start(&root, &args) else { panic!("an argument is missing") };
        assert!(error.contains("--folder"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
