//! The plug-in's side: says who it is when the application's contract and version are its own,
//! then answers each request until stdin ends; a job may answer in parts and may be cancelled.

use std::io::{BufRead, Write};
use std::sync::{mpsc, Arc};

use serde::Serialize;
use serde_json::Value;

use crate::{Answer, Hello, Ready, Request, CANCEL};

/// Whether the application cancelled the running job of [`serve_parts`].
pub type Cancelled = std::sync::atomic::AtomicBool;
/// Sends a part of the running job's answer before its end.
pub type PartSender<'a> = dyn FnMut(Value) -> Result<(), String> + 'a;

/// Says who it is when the application's contract and version are `ready`'s, then answers each
/// request with `handle` until stdin ends.
pub fn serve(ready: Ready, mut handle: impl FnMut(&str, Value) -> Result<Value, String>) -> Result<(), String> {
    serve_parts(ready, |op, body, _, _| handle(op, body))
}

/// [`serve`] for jobs that answer in parts or can be cancelled: one job at a time; a [`CANCEL`]
/// request for the running job sets the flag `handle` is given, and the job ends early.
pub fn serve_parts(ready: Ready, mut handle: impl FnMut(&str, Value, &Cancelled, &mut PartSender) -> Result<Value, String>) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    let mut stdout = std::io::stdout().lock();
    // Lines are read beside the job, so that a cancel reaches the job that runs.
    let current = Arc::new(AtomicU64::new(0));
    let cancelled = Arc::new(Cancelled::new(false));
    let (send_line, lines) = mpsc::channel::<Result<String, String>>();
    {
        let (current, cancelled) = (current.clone(), cancelled.clone());
        std::thread::Builder::new()
            .name("qnc-plugin-requests".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                for line in stdin.lock().lines() {
                    let line = line.map_err(|e| e.to_string());
                    if let Ok(text) = &line {
                        if let Ok(cancel) = serde_json::from_str::<Request>(text) {
                            if cancel.op == CANCEL {
                                if cancel.body.as_u64() == Some(current.load(Ordering::SeqCst)) {
                                    cancelled.store(true, Ordering::SeqCst);
                                }
                                continue;
                            }
                        }
                    }
                    if send_line.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    let mut lines = lines.into_iter();
    let hello: Hello = match lines.next() {
        Some(line) => serde_json::from_str(&line?).map_err(|e| e.to_string())?,
        None => return Ok(()),
    };
    let mut answer = ready.clone();
    if hello.contract != ready.contract || hello.version != ready.version {
        // It says what it speaks; the application refuses it.
        answer.about = Value::Null;
    }
    write_line(&mut stdout, &answer)?;
    for line in lines {
        let request: Request = serde_json::from_str(&line?).map_err(|e| e.to_string())?;
        cancelled.store(false, Ordering::SeqCst);
        current.store(request.id, Ordering::SeqCst);
        let id = request.id;
        let mut parts_failed: Option<String> = None;
        let result = {
            let out = &mut stdout;
            let failed = &mut parts_failed;
            let mut send_part = |part: Value| {
                let sent = write_line(out, &Answer { id, ok: None, error: None, part: Some(part) });
                if let Err(error) = &sent {
                    *failed = Some(error.clone());
                }
                sent
            };
            handle(&request.op, request.body, &cancelled, &mut send_part)
        };
        current.store(0, Ordering::SeqCst);
        if let Some(error) = parts_failed {
            return Err(error);
        }
        let answer = match result {
            Ok(ok) => Answer { id, ok: Some(ok), error: None, part: None },
            Err(error) => Answer { id, ok: None, error: Some(error), part: None },
        };
        write_line(&mut stdout, &answer)?;
    }
    Ok(())
}

fn write_line(out: &mut impl Write, message: &impl Serialize) -> Result<(), String> {
    let mut line = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    line.push(b'\n');
    out.write_all(&line).and_then(|()| out.flush()).map_err(|e| e.to_string())
}
