//! The rounds of one background step (waves, filmstrips, ...) over a project's clips: once a
//! second the catalog is read for imported clips the step has not made yet, each is made in
//! turn, a clip that fails is said in the log and left alone for this run. The step ends when
//! the user stopped its function (the X, in the project's database) or when it is told to stop.
//! The step gives only what it has made and how one clip is made.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use qnc_db::Reader;

const ROUND: Duration = Duration::from_secs(1);

/// Runs the rounds of `function` (its name in the stopped work and the log): `made` gives the
/// clips already made, `make_one` makes one clip.
pub fn run(reader: &Reader, function: &str, stop: &AtomicBool, made: &dyn Fn(&Reader) -> Result<BTreeSet<String>, String>, make_one: &mut dyn FnMut(&str, &AtomicBool) -> Result<(), String>) -> Result<(), String> {
    let mut failed: BTreeSet<String> = BTreeSet::new();
    while !stop.load(Ordering::Relaxed) {
        if qnc_stopped_work::read(reader)?.contains(function) {
            qnc_diagnostics::log(format!("{function}: stopped by the user"));
            return Ok(());
        }
        let made = made(reader)?;
        let waiting: Vec<String> = qnc_catalog::summaries(reader)?
            .into_iter()
            .filter(|clip| clip.import_status == qnc_catalog::ImportStatus::Imported && !made.contains(&clip.clip_id) && !failed.contains(&clip.clip_id))
            .map(|clip| clip.clip_id)
            .collect();
        for clip_id in waiting {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let started = Instant::now();
            match make_one(&clip_id, stop) {
                Ok(()) => qnc_diagnostics::log(format!("{function}: made {clip_id} in {} ms", started.elapsed().as_millis())),
                Err(error) => {
                    qnc_diagnostics::log(format!("{function}: not made {clip_id}: {error}"));
                    failed.insert(clip_id);
                }
            }
        }
        std::thread::sleep(ROUND);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qnc_db::Writer;

    #[test]
    fn the_rounds_end_on_the_users_x_and_when_told() {
        let dir = std::env::temp_dir().join(format!("qnc-clip-round-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let writer = Writer::start(&db, vec![Box::new(qnc_catalog::CatalogTables), Box::new(qnc_stopped_work::StoppedWork)]).unwrap();
        let reader = Reader::open(&db).unwrap();
        qnc_stopped_work::stop(&writer, "waves").unwrap();
        let stop = AtomicBool::new(false);
        run(&reader, "waves", &stop, &|_| Ok(BTreeSet::new()), &mut |_, _| Ok(())).unwrap();
        stop.store(true, Ordering::Relaxed);
        run(&reader, "filmstrips", &stop, &|_| Ok(BTreeSet::new()), &mut |_, _| panic!("no clip to make")).unwrap();
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
