//! One background step of an ingest (waves, transcripts, ...) as its own thread: started with a
//! name and the work, told to stop through a flag the work reads, asked whether it ended. The
//! work's error, if any, goes to the log under the step's name.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct StepThread {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StepThread {
    /// Starts `work` on a thread named `name`; the flag it gets turns true when the step is told to stop.
    pub fn start(name: &str, work: impl FnOnce(&AtomicBool) -> Result<(), String> + Send + 'static) -> StepThread {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let step = name.to_string();
        let thread = std::thread::Builder::new()
            .name(format!("qnc-{name}"))
            .spawn(move || {
                if let Err(error) = work(&flag) {
                    qnc_diagnostics::log(format!("{step}: {error}"));
                }
            })
            .expect("a thread for the step");
        StepThread { stop, thread: Some(thread) }
    }

    /// Tells the step to stop after the piece of work it is on.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn ended(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_runs_until_told_to_stop_and_then_says_it_ended() {
        let step = StepThread::start("test", |stop| {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(())
        });
        assert!(!step.ended());
        step.stop();
        for _ in 0..200 {
            if step.ended() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the step did not end");
    }
}
