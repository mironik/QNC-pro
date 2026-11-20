//! The player process. Started beside the application through the launch block, it reads a
//! bootstrap per session on stdin and says on stdout the hello of the session, a refusal, or
//! the end of a session. Its stdout is read by a thread so a slow line never holds the caller.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

use qnc_player_wire::CONTRACT_VERSION;
use serde_json::Value;

const HELLO_WAIT: Duration = Duration::from_secs(10);
const LINE_LIMIT: u64 = 8192;

pub(crate) struct Host {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<Vec<u8>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Host {
    pub(crate) fn start(program: &str) -> Result<Host, String> {
        let mut child = qnc_launch::start(program, &["--monitor-host"], qnc_launch::Priority::Same)?;
        let stdout = child.stdout.take().ok_or("missing player startup pipe")?;
        let stdin = child.stdin.take().ok_or("missing player input pipe")?;
        let (send, lines) = mpsc::channel();
        thread::Builder::new()
            .name("qnc-player-host-stdout".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    match (&mut reader).take(LINE_LIMIT + 1).read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            if send.send(line).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Host { child, stdin, lines })
    }

    pub(crate) fn running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    fn failed(error: &str) -> String {
        qnc_text::fill("player.startup_failed", &[("error", error)])
    }

    /// Hands the process the bootstrap of a session and waits for its hello.
    pub(crate) fn open_session(&mut self, boot: &[u8], session: &str, generation: u64) -> Result<Value, String> {
        self.stdin
            .write_all(boot)
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| Host::failed(&e.to_string()))?;
        let until = SystemTime::now() + HELLO_WAIT;
        loop {
            let left = until.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO);
            let line = self.lines.recv_timeout(left).map_err(|_| qnc_text::fill("player.startup_timed_out", &[("seconds", &HELLO_WAIT.as_secs().to_string())]))?;
            if line.is_empty() || line.len() as u64 > LINE_LIMIT {
                return Err(Host::failed("the player said nothing readable"));
            }
            let said: Value = serde_json::from_slice(&line).map_err(|e| Host::failed(&e.to_string()))?;
            if let Some(why) = said.get("session_refused") {
                return Err(Host::failed(why.as_str().unwrap_or_default()));
            }
            if said["session_ended"] == session {
                return Err(Host::failed("the session ended before it said hello"));
            }
            if said.get("session_ended").is_some() {
                continue; // the end of an earlier session
            }
            if said["contract_version"] != CONTRACT_VERSION || said["session_id"] != session || said["source_generation"] != generation {
                return Err(qnc_text::text("player.identity_mismatch").to_string());
            }
            return Ok(said);
        }
    }

    /// Waits until the process says `session` ended; `false` when it did not within `within`.
    pub(crate) fn session_ended(&mut self, session: &str, within: Duration) -> bool {
        let until = SystemTime::now() + within;
        loop {
            let left = until.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO);
            let Ok(line) = self.lines.recv_timeout(left) else { return false };
            let said: Value = serde_json::from_slice(&line).unwrap_or_default();
            if said["session_ended"] == session {
                return true;
            }
        }
    }
}
