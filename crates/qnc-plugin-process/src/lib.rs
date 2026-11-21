//! A plug-in as its own process: a program beside the application that speaks one versioned
//! contract as JSON lines on stdin and stdout. The application says which contract and version
//! it speaks, the plug-in answers who it is, then each request has one answer (in parts when
//! the contract says so). Replacing a plug-in is replacing its program. A plug-in that does not
//! answer in time, answers too much or speaks another contract is a controlled error; nothing
//! falls back to anything. The process starts at the first request, serves every request of a
//! job and is stopped when it has had nothing to do for a while, or when this handle goes.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Stdio},
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

mod bytes;
mod serve;
pub use bytes::Bytes;
pub use serve::{serve, serve_parts, Cancelled, PartSender};

/// The longest answer line a plug-in may give.
pub const MAX_LINE_BYTES: usize = 64 << 20;
/// A process with nothing to do this long is stopped (it starts again at the next request).
pub const IDLE: Duration = Duration::from_secs(30);

/// The first line the application writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub contract: String,
    pub version: u32,
}

/// The plug-in's first answer: who it is. `about` carries what the contract asks a plug-in to say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    pub contract: String,
    pub version: u32,
    pub plugin_id: String,
    pub plugin_version: String,
    #[serde(default)]
    pub about: Value,
}

/// One request and its answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub op: String,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// A part of the answer that is still coming (a line with a part is not the answer's end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Value>,
}

/// The request the application writes to stop the job of request `body` (a number).
pub const CANCEL: &str = "cancel";
/// How long a plug-in has to end a cancelled job before its process is stopped.
pub const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// How the application reaches one plug-in: its program, the contract it must speak, and how
/// long one answer may take.
#[derive(Debug, Clone)]
pub struct PluginSpec {
    pub executable: PathBuf,
    pub contract: String,
    pub version: u32,
    pub timeout: Duration,
}

impl PluginSpec {
    /// The plug-in named `program` beside the application.
    pub fn beside_application(program: &str, contract: &str, version: u32, timeout: Duration) -> Result<PluginSpec, String> {
        Ok(PluginSpec { executable: qnc_launch::beside_application(program)?, contract: contract.into(), version, timeout })
    }
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Result<String, String>>,
    ready: Ready,
    next_id: u64,
    used: Instant,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One plug-in, started when it is first asked.
pub struct Plugin {
    spec: PluginSpec,
    running: Arc<Mutex<Option<Running>>>,
}

impl Plugin {
    pub fn new(spec: PluginSpec) -> Self {
        let running: Arc<Mutex<Option<Running>>> = Arc::default();
        let idle = Arc::downgrade(&running);
        // Stops the process when it has had nothing to do for a while; ends with the handle.
        std::thread::Builder::new()
            .name("qnc-plugin-idle".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                let Some(running) = idle.upgrade() else { break };
                let mut guard = running.lock().unwrap_or_else(|e| e.into_inner());
                if guard.as_ref().is_some_and(|r| r.used.elapsed() >= IDLE) {
                    *guard = None;
                }
            })
            .ok();
        Self { spec, running }
    }

    /// Who the plug-in is (it is started for this when it is not running).
    pub fn ready(&self) -> Result<Ready, String> {
        let mut guard = self.running.lock().unwrap_or_else(|e| e.into_inner());
        Ok(self.started(&mut guard)?.ready.clone())
    }

    /// Asks `op` with `body`; the plug-in's answer, or its error.
    pub fn call(&self, op: &str, body: Value) -> Result<Value, String> {
        let never = std::sync::atomic::AtomicBool::new(false);
        self.call_parts(op, body, &never, &mut |_| Err("a part where none is asked for".into()))
    }

    /// Asks `op` with `body` and hands every part of the answer to `on_part` as it comes; the
    /// timeout holds between two lines. When `cancel` is set the plug-in is asked to stop the job
    /// and, when it does not end it in [`CANCEL_GRACE`], its process is stopped.
    pub fn call_parts(&self, op: &str, body: Value, cancel: &std::sync::atomic::AtomicBool, on_part: &mut dyn FnMut(Value) -> Result<(), String>) -> Result<Value, String> {
        use std::sync::atomic::Ordering;
        let mut guard = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let running = self.started(&mut guard)?;
        let id = running.next_id;
        running.next_id += 1;
        let program = self.spec.executable.display().to_string();
        if let Err(error) = send(&mut running.stdin, &Request { id, op: op.into(), body }) {
            *guard = None; // a plug-in that broke is started again at the next request
            return Err(format!("plug-in {program}: {error}"));
        }
        let mut cancelled_at: Option<Instant> = None;
        let mut last_line = Instant::now();
        let answer = loop {
            if cancelled_at.is_none() && cancel.load(Ordering::Relaxed) {
                if send(&mut running.stdin, &Request { id: 0, op: CANCEL.into(), body: id.into() }).is_err() {
                    *guard = None;
                    return Err("cancelled".into());
                }
                cancelled_at = Some(Instant::now());
            }
            if cancelled_at.is_some_and(|at| at.elapsed() >= CANCEL_GRACE) {
                *guard = None;
                return Err("cancelled".into());
            }
            let answer = match receive::<Answer>(&running.lines, Duration::from_millis(100)) {
                Ok(answer) => answer,
                Err(error) if error.starts_with(NO_ANSWER) => {
                    if last_line.elapsed() >= self.spec.timeout {
                        *guard = None;
                        return Err(format!("plug-in {program}: no answer in {} ms", self.spec.timeout.as_millis()));
                    }
                    continue;
                }
                Err(error) => {
                    *guard = None;
                    return Err(format!("plug-in {program}: {error}"));
                }
            };
            last_line = Instant::now();
            if answer.id != id {
                *guard = None;
                return Err(format!("plug-in {program} answered request {} for {id}", answer.id));
            }
            match answer.part {
                Some(part) if cancelled_at.is_none() => {
                    if let Err(error) = on_part(part) {
                        // A refused part ends the job as a cancel does.
                        *guard = None;
                        return Err(error);
                    }
                }
                Some(_) => {}
                None => break answer,
            }
        };
        running.used = Instant::now();
        if cancelled_at.is_some() {
            return Err("cancelled".into());
        }
        match (answer.ok, answer.error) {
            (_, Some(error)) => Err(error),
            (Some(ok), None) => Ok(ok),
            (None, None) => Ok(Value::Null),
        }
    }

    fn started<'a>(&self, guard: &'a mut Option<Running>) -> Result<&'a mut Running, String> {
        if guard.as_mut().is_some_and(|r| r.child.try_wait().ok().flatten().is_some()) {
            *guard = None;
        }
        if guard.is_none() {
            *guard = Some(start(&self.spec)?);
        }
        Ok(guard.as_mut().expect("started"))
    }
}

fn start(spec: &PluginSpec) -> Result<Running, String> {
    let (send_line, lines) = mpsc::channel();
    // Below the application: a plug-in works in the background, the forms and the player stay above it.
    let mut command = qnc_launch::command(&spec.executable, qnc_launch::Priority::Below);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|e| format!("plug-in {} does not start: {e}", spec.executable.display()))?;
    let mut stdin = child.stdin.take().ok_or("plug-in has no stdin")?;
    let stdout = child.stdout.take().ok_or("plug-in has no stdout")?;
    std::thread::Builder::new().name("qnc-plugin-read".into()).spawn(move || read_lines(stdout, send_line)).map_err(|e| e.to_string())?;
    let hello = Hello { contract: spec.contract.clone(), version: spec.version };
    let ready = send(&mut stdin, &hello).and_then(|()| receive::<Ready>(&lines, spec.timeout));
    let mut running = Running { child, stdin, lines, ready: Ready { contract: String::new(), version: 0, plugin_id: String::new(), plugin_version: String::new(), about: Value::Null }, next_id: 1, used: Instant::now() };
    let ready = ready.map_err(|e| format!("plug-in {} did not say who it is: {e}", spec.executable.display()))?;
    if ready.contract != spec.contract || ready.version != spec.version {
        return Err(format!("plug-in {} speaks {} {}, not {} {}", spec.executable.display(), ready.contract, ready.version, spec.contract, spec.version));
    }
    running.ready = ready;
    Ok(running)
}

fn read_lines(stdout: impl Read, lines: mpsc::Sender<Result<String, String>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let read = (&mut reader).take(MAX_LINE_BYTES as u64 + 1).read_until(b'\n', &mut line);
        let last = matches!(read, Ok(0) | Err(_));
        let sent = match read {
            Ok(0) => lines.send(Err("the plug-in ended".into())),
            Ok(_) if line.len() > MAX_LINE_BYTES => lines.send(Err(format!("an answer longer than {MAX_LINE_BYTES} bytes"))),
            Ok(_) => lines.send(String::from_utf8(line).map_err(|_| "an answer that is not UTF-8".to_string())),
            Err(error) => lines.send(Err(error.to_string())),
        };
        if sent.is_err() || last {
            break;
        }
    }
}

fn send(stdin: &mut dyn Write, message: &impl Serialize) -> Result<(), String> {
    let mut line = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    line.push(b'\n');
    stdin.write_all(&line).and_then(|()| stdin.flush()).map_err(|e| format!("the plug-in does not listen: {e}"))
}

const NO_ANSWER: &str = "no answer in";

fn receive<T: serde::de::DeserializeOwned>(lines: &Receiver<Result<String, String>>, timeout: Duration) -> Result<T, String> {
    let line = match lines.recv_timeout(timeout) {
        Ok(line) => line?,
        Err(RecvTimeoutError::Timeout) => return Err(format!("{NO_ANSWER} {} ms", timeout.as_millis())),
        Err(RecvTimeoutError::Disconnected) => return Err("the plug-in ended".into()),
    };
    serde_json::from_str(line.trim_end()).map_err(|e| format!("an answer outside the contract: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plug_in_that_is_not_there_is_an_error_and_a_name_with_a_path_is_refused() {
        assert!(PluginSpec::beside_application("qnc-nothing-of-the-kind", "qnc.test", 1, Duration::from_secs(1)).is_err());
        assert!(PluginSpec::beside_application("../x", "qnc.test", 1, Duration::from_secs(1)).is_err());
        let plugin = Plugin::new(PluginSpec { executable: PathBuf::from("qnc-nothing-of-the-kind"), contract: "qnc.test".into(), version: 1, timeout: Duration::from_secs(1) });
        assert!(plugin.call("op", Value::Null).unwrap_err().contains("does not start"));
    }
}
