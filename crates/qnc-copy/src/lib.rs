//! A verified copy of one local file. The source is read in large pieces (a card behind USB
//! reads fastest so), two pieces ahead of the writing, so the card never waits for the disk;
//! a source that gives nothing for a minute is an error with its path, not a hang. The copy is
//! the source's only when what was written reads back with the checksum of what was read; the
//! source must not have changed meanwhile. The file is written under its final name (a
//! protected project folder allows no renaming); a failed copy is removed.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use qnc_checksum::XxHash64;

pub const CHUNK: usize = 32 * 1024 * 1024;
pub const AHEAD: usize = 2;
pub const STALL: Duration = Duration::from_secs(60);

/// What a finished copy is proven by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub bytes: u64,
    /// `xxh64:<hex>` of the source as read, equal to the copy as read back.
    pub checksum: String,
}

fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    fs::metadata(path).and_then(|m| Ok((m.len(), m.modified()?))).ok()
}

/// Copies `source` to `destination`, calling `progress(bytes written)` as it goes; `cancel`
/// stops it (the destination is removed).
pub fn copy(source: &Path, destination: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<Copied, String> {
    let before = stamp(source);
    let result = copy_pieces(source, destination, cancel, progress).and_then(|copied| {
        if stamp(source) != before {
            return Err(qnc_text::fill("copy.source_changed", &[("path", &source.display().to_string())]));
        }
        let written = checksum_of(destination)?;
        if written != copied.checksum {
            return Err(qnc_text::fill("copy.differs", &[("copy", &written), ("source", &copied.checksum), ("path", &destination.display().to_string())]));
        }
        Ok(copied)
    });
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

/// The checksum of a file as it reads, `xxh64:<hex>`.
pub fn checksum_of(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut checksum = XxHash64::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer).map_err(|e| format!("{}: {e}", path.display()))?;
        if read == 0 {
            return Ok(checksum.label());
        }
        checksum.update(&buffer[..read]);
    }
}

fn copy_pieces(source: &Path, destination: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<Copied, String> {
    let mut reader = fs::File::open(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let path = source.display().to_string();
    let (tx, rx) = mpsc::sync_channel::<Result<Vec<u8>, String>>(AHEAD);
    let reading = std::thread::spawn(move || loop {
        let mut piece = vec![0u8; CHUNK];
        let mut filled = 0;
        while filled < CHUNK {
            match reader.read(&mut piece[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            }
        }
        piece.truncate(filled);
        let ended = filled < CHUNK;
        if tx.send(Ok(piece)).is_err() || ended {
            return;
        }
    });
    let mut file = fs::File::create(destination).map_err(|e| format!("{}: {e}", destination.display()))?;
    let mut checksum = XxHash64::new();
    let mut bytes = 0u64;
    let outcome = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err(qnc_text::fill("copy.cancelled", &[("path", &path)]));
        }
        let piece = match rx.recv_timeout(STALL) {
            Ok(Ok(piece)) => piece,
            Ok(Err(error)) => break Err(format!("{path}: {error}")),
            Err(mpsc::RecvTimeoutError::Timeout) => break Err(qnc_text::fill("copy.stalled", &[("seconds", &STALL.as_secs().to_string()), ("path", &path)])),
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(()),
        };
        let ended = piece.len() < CHUNK;
        checksum.update(&piece);
        if let Err(e) = file.write_all(&piece) {
            break Err(format!("{}: {e}", destination.display()));
        }
        bytes += piece.len() as u64;
        progress(bytes);
        if ended {
            break Ok(());
        }
    };
    drop(rx);
    let _ = reading.join();
    outcome?;
    file.sync_all().map_err(|e| format!("{}: {e}", destination.display()))?;
    Ok(Copied { bytes, checksum: checksum.label() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_proven_by_its_checksum_and_a_cancelled_one_leaves_nothing() {
        let dir = std::env::temp_dir().join(format!("qnc-copy-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let source = dir.join("source.bin");
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&source, &data).unwrap();
        let destination = dir.join("copy.bin");
        let mut seen = 0;
        let copied = copy(&source, &destination, &AtomicBool::new(false), &mut |b| seen = b).unwrap();
        assert_eq!((copied.bytes, seen), (300_000, 300_000));
        assert_eq!(copied.checksum, checksum_of(&source).unwrap());
        assert_eq!(fs::read(&destination).unwrap(), data);
        let error = copy(&source, &dir.join("none.bin"), &AtomicBool::new(true), &mut |_| {}).unwrap_err();
        assert!(error.contains("none.bin") || error.contains("source.bin"), "{error}");
        assert!(!dir.join("none.bin").exists());
        assert!(copy(&dir.join("missing.bin"), &destination, &AtomicBool::new(false), &mut |_| {}).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
