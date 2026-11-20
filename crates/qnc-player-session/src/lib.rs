//! One session of the Broadcast Player program. The process (`qnc-broadcast-player
//! --monitor-host`, beside the application) serves one session after another: each bootstrap
//! line on its stdin opens a session, its hello line gives the control address, the end of a
//! session is said as `session_ended`. What the session plays (the prepared input and the
//! media bindings) is a JSON object the owner of the catalog prepares; this block adds the
//! session's identity, tokens and frame mailbox and never looks inside it.

mod host;

use std::path::{Path, PathBuf};

use host::Host;
use qnc_player_wire::{Command, CommandEnvelope, Event, EventEnvelope, SessionQuery, SessionReply, SessionRequest, CONTRACT_VERSION};
use serde_json::{Map, Value};

pub const PROGRAM: &str = "qnc-broadcast-player";
const IDLE_TIMEOUT_MS: u64 = 300_000;
const BOOT_LIMIT: usize = 4 * 1024 * 1024;

/// The program's player, kept between sessions.
#[derive(Default)]
pub struct Player {
    host: Option<Host>,
}

#[derive(Debug)]
pub struct Session {
    query: SessionQuery,
    wire_url: String,
    command_token: String,
    sequence: u64,
    last_event: u64,
    frame_map: PathBuf,
    open: bool,
}

impl Player {
    pub fn new() -> Player {
        Player::default()
    }

    /// Opens a session that plays `played` (the prepared input and bindings as the program
    /// reads them). The session before, if any, must have been ended.
    pub fn open(&mut self, played: Map<String, Value>, generation: u64) -> Result<Session, String> {
        let session_id = fresh_id();
        let read_token = fresh_id();
        let command_token = fresh_id();
        let frame_map = std::env::temp_dir().join(format!("qnc-player-frame-{session_id}.map"));
        qnc_frame_map::create(&frame_map, qnc_frame_map::PREVIEW_CAPACITY)?;
        let mut boot = Map::new();
        boot.insert("contract_version".into(), CONTRACT_VERSION.into());
        boot.insert("session_id".into(), session_id.clone().into());
        boot.insert("source_generation".into(), generation.into());
        boot.insert("read_token".into(), read_token.into());
        boot.insert("command_token".into(), command_token.clone().into());
        boot.insert("idle_timeout_ms".into(), IDLE_TIMEOUT_MS.into());
        boot.insert("listen_port".into(), 0.into());
        boot.insert("monitor_frame_map".into(), frame_map.to_string_lossy().into_owned().into());
        boot.extend(played);
        let boot = serde_json::to_vec(&Value::Object(boot)).map_err(|e| e.to_string())?;
        if boot.len() > BOOT_LIMIT {
            return Err(qnc_text::fill("player.startup_failed", &[("error", "bootstrap exceeds limit")]));
        }
        if !self.host.as_mut().is_some_and(|host| host.running()) {
            self.host = Some(Host::start(PROGRAM)?);
        }
        let host = self.host.as_mut().expect("a player process");
        let hello = match host.open_session(&boot, &session_id, generation) {
            Ok(hello) => hello,
            Err(error) => {
                self.host = None; // a process that did not open the session is not trusted again
                let _ = std::fs::remove_file(&frame_map);
                return Err(error);
            }
        };
        let wire_url = hello["wire_url"].as_str().ok_or_else(|| qnc_text::text("player.no_wire").to_string())?.to_string();
        qnc_diagnostics::log(format!("player-session open session={session_id} generation={generation}"));
        Ok(Session { query: SessionQuery::of(&session_id, generation), wire_url, command_token, sequence: 0, last_event: 0, frame_map, open: true })
    }

    /// Ends a session in the process (Shutdown, then the process says it ended); a process that
    /// does not say so in time is dropped.
    pub fn end(&mut self, mut session: Session) {
        if !session.open {
            return;
        }
        let _ = session.command(Command::Shutdown);
        session.open = false;
        let ended = self.host.as_mut().is_some_and(|host| host.session_ended(&session.query.session_id, std::time::Duration::from_secs(2)));
        if !ended {
            self.host = None;
        }
        let _ = std::fs::remove_file(&session.frame_map);
        qnc_diagnostics::log(format!("player-session end session={} ended_in_time={ended}", session.query.session_id));
    }

    /// The process is gone (the application ends).
    pub fn drop_process(&mut self) {
        self.host = None;
    }
}

impl Session {
    pub fn id(&self) -> &str {
        &self.query.session_id
    }

    pub fn generation(&self) -> u64 {
        self.query.source_generation
    }

    /// The mailbox the player posts its pictures into; the monitor opens it.
    pub fn frame_map(&self) -> &Path {
        &self.frame_map
    }

    fn request(&mut self, request: SessionRequest) -> Result<Vec<Event>, String> {
        if !self.open {
            return Err(qnc_text::text("player.closed").to_string());
        }
        let reply: SessionReply = qnc_player_wire::post(&self.wire_url, &self.command_token, &request)?;
        let reply: EventEnvelope = reply?;
        reply.check(&self.query, self.last_event)?;
        self.last_event = reply.sequence;
        Ok(reply.events())
    }

    /// Sends one command; the events the player answers with.
    pub fn command(&mut self, command: Command) -> Result<Vec<Event>, String> {
        self.sequence = self.sequence.checked_add(1).ok_or("player command sequence exhausted")?;
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION.into(),
            session_id: self.query.session_id.clone(),
            request_id: self.sequence.to_string(),
            source_generation: self.query.source_generation,
            sequence: self.sequence,
            command,
        };
        self.request(SessionRequest::Command(envelope))
    }

    /// Asks the player where it is; the events since the last answer.
    pub fn state(&mut self) -> Result<Vec<Event>, String> {
        self.request(SessionRequest::State(self.query.clone()))
    }
}

/// A fresh id: time, process, a counter and a mix; printable, as the wire wants a token.
fn fresh_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let mut x = (nanos as u64) ^ (u64::from(std::process::id()) << 32) ^ count.rotate_left(17);
    let mut out = String::with_capacity(32);
    for _ in 0..32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push(char::from(b"0123456789abcdefghijklmnopqrstuv"[(x & 31) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_printable_distinct_and_a_missing_player_program_is_a_clear_error() {
        let a = fresh_id();
        let b = fresh_id();
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| (33..=126).contains(&c)) && a.len() == 32);
        let mut player = Player::new();
        let error = player.open(Map::new(), 1).unwrap_err();
        assert!(error.contains(PROGRAM), "{error}");
    }
}
