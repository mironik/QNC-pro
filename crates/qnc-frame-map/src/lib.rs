//! The frame mailbox of a player session (format `QNCFRM03`, as the player program writes it).
//! The client makes the file before the session starts and names it in the bootstrap; the
//! player posts each picture into one of 8 slots and raises a wake; the monitor reads the
//! newest posted record. A header of 64 bytes, then 8 slots of a 32-byte prefix, 8192 bytes
//! of JSON metadata (the picture's header, or none for a clear) and the RGBA payload.

mod wake;

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use memmap2::{Mmap, MmapMut, MmapOptions};
use qnc_player_wire::Timebase;
use serde::{Deserialize, Serialize};
use wake::Wake;

pub const MAGIC: &[u8; 8] = b"QNCFRM03";
pub const VERSION: u8 = 1;
pub const SLOT_COUNT: usize = 8;
pub const HEADER_BYTES: usize = 64;
pub const SLOT_PREFIX_BYTES: usize = 32;
pub const METADATA_BYTES: usize = 8192;
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;
/// What the client makes for the preview monitor.
pub const PREVIEW_CAPACITY: usize = 8 * 1024 * 1024;

const VERSION_OFFSET: usize = 8;
const SLOT_COUNT_OFFSET: usize = 9;
const CAPACITY_OFFSET: usize = 16;
const PUBLISHED_OFFSET: usize = 24;

/// The player's description of a posted picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PictureHeader {
    pub contract_version: String,
    pub session_id: String,
    pub source_generation: u64,
    pub output_generation: u64,
    pub sequence: u64,
    pub source_id: String,
    pub frame: u64,
    pub timebase: Timebase,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub header: PictureHeader,
    pub rgba: Arc<[u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Posted {
    Clear,
    Picture(Picture),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    header: Option<PictureHeader>,
}

/// Makes the mailbox file of a session (empty, no picture posted).
pub fn create(path: &Path, capacity: usize) -> Result<(), String> {
    check_capacity(capacity)?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).map_err(|e| e.to_string())?;
    file.set_len(file_len(capacity)? as u64).map_err(|e| e.to_string())?;
    // SAFETY: the file is ours and sized; nobody else maps it before the player starts.
    let mut mmap = unsafe { MmapOptions::new().map_mut(&file) }.map_err(|e| e.to_string())?;
    mmap[..HEADER_BYTES].fill(0);
    mmap[..8].copy_from_slice(MAGIC);
    mmap[VERSION_OFFSET] = VERSION;
    mmap[SLOT_COUNT_OFFSET] = SLOT_COUNT as u8;
    mmap[CAPACITY_OFFSET..CAPACITY_OFFSET + 8].copy_from_slice(&(capacity as u64).to_le_bytes());
    mmap[PUBLISHED_OFFSET..PUBLISHED_OFFSET + 8].copy_from_slice(&0u64.to_le_bytes());
    mmap.flush().map_err(|e| e.to_string())?;
    Wake::create(path).map(|_| ())
}

/// The monitor's side of a mailbox.
pub struct Reader {
    mmap: Mmap,
    capacity: usize,
    observed: u64,
    wake: Wake,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Reader, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        // SAFETY: the player only appends records; a torn record is caught by its begin/end marks.
        let mmap = unsafe { MmapOptions::new().map(&file) }.map_err(|e| e.to_string())?;
        let capacity = check_header(&mmap)?;
        Ok(Reader { mmap, capacity, observed: 0, wake: Wake::open(path)? })
    }

    fn published(&self) -> u64 {
        read_u64(&self.mmap[PUBLISHED_OFFSET..PUBLISHED_OFFSET + 8])
    }

    /// Records posted that this reader has not taken.
    pub fn backlog(&self) -> u64 {
        self.published().saturating_sub(self.observed)
    }

    /// The newest posted record, once; `None` while nothing newer than the last take exists.
    pub fn read_newest(&mut self) -> Result<Option<Posted>, String> {
        let published = self.published();
        if published == 0 || published <= self.observed {
            return Ok(None);
        }
        self.read_generation(published)
    }

    /// Waits up to `timeout` for a new record and takes the newest.
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Posted>, String> {
        if let Some(posted) = self.read_newest()? {
            return Ok(Some(posted));
        }
        let signaled = self.wake.wait(timeout)?;
        for _ in 0..8 {
            if let Some(posted) = self.read_newest()? {
                return Ok(Some(posted));
            }
            if !signaled {
                break;
            }
            std::thread::yield_now();
        }
        Ok(None)
    }

    fn read_generation(&mut self, generation: u64) -> Result<Option<Posted>, String> {
        let offset = slot_offset(self.capacity, (generation as usize) % SLOT_COUNT);
        let begin = read_u64(&self.mmap[offset..offset + 8]);
        let metadata_len = read_u32(&self.mmap[offset + 8..offset + 12]) as usize;
        let payload_len = read_u32(&self.mmap[offset + 12..offset + 16]) as usize;
        let end = read_u64(&self.mmap[offset + 16..offset + 24]);
        if begin != generation || end != generation {
            return Ok(None); // the player is still filling it
        }
        if metadata_len == 0 || metadata_len > METADATA_BYTES || payload_len > self.capacity {
            return Err("frame map slot is invalid".into());
        }
        let metadata_offset = offset + SLOT_PREFIX_BYTES;
        let payload_offset = metadata_offset + METADATA_BYTES;
        let metadata: Metadata = serde_json::from_slice(&self.mmap[metadata_offset..metadata_offset + metadata_len]).map_err(|e| e.to_string())?;
        self.observed = generation;
        let Some(header) = metadata.header else { return Ok(Some(Posted::Clear)) };
        let expected = u64::from(header.width) * u64::from(header.height) * 4;
        if header.width == 0 || header.height == 0 || expected != payload_len as u64 || header.timebase.fps_num <= 0 || header.timebase.fps_den <= 0 {
            return Err("invalid frame map picture".into());
        }
        let rgba: Arc<[u8]> = self.mmap[payload_offset..payload_offset + payload_len].to_vec().into();
        Ok(Some(Posted::Picture(Picture { header, rgba })))
    }
}

/// The player's side; here only so a test can post as the player does.
pub struct Writer {
    mmap: MmapMut,
    capacity: usize,
    published: u64,
    wake: Wake,
}

impl Writer {
    pub fn open(path: &Path) -> Result<Writer, String> {
        let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
        // SAFETY: one writer per mailbox, as the player.
        let mmap = unsafe { MmapOptions::new().map_mut(&file) }.map_err(|e| e.to_string())?;
        let capacity = check_header(&mmap)?;
        Ok(Writer { published: read_u64(&mmap[PUBLISHED_OFFSET..PUBLISHED_OFFSET + 8]), mmap, capacity, wake: Wake::open(path)? })
    }

    pub fn post(&mut self, header: Option<&PictureHeader>, rgba: &[u8]) -> Result<u64, String> {
        if rgba.len() > self.capacity {
            return Err("frame map payload exceeds capacity".into());
        }
        let metadata = serde_json::to_vec(&Metadata { header: header.cloned() }).map_err(|e| e.to_string())?;
        if metadata.len() > METADATA_BYTES {
            return Err("frame map metadata exceeds capacity".into());
        }
        let generation = self.published + 1;
        let offset = slot_offset(self.capacity, (generation as usize) % SLOT_COUNT);
        self.mmap[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
        self.mmap[offset + 8..offset + 12].copy_from_slice(&(metadata.len() as u32).to_le_bytes());
        self.mmap[offset + 12..offset + 16].copy_from_slice(&(rgba.len() as u32).to_le_bytes());
        self.mmap[offset + 16..offset + 24].copy_from_slice(&generation.to_le_bytes());
        let metadata_offset = offset + SLOT_PREFIX_BYTES;
        let payload_offset = metadata_offset + METADATA_BYTES;
        self.mmap[metadata_offset..metadata_offset + METADATA_BYTES].fill(0);
        self.mmap[metadata_offset..metadata_offset + metadata.len()].copy_from_slice(&metadata);
        self.mmap[payload_offset..payload_offset + rgba.len()].copy_from_slice(rgba);
        self.mmap[offset..offset + 8].copy_from_slice(&generation.to_le_bytes());
        self.mmap[PUBLISHED_OFFSET..PUBLISHED_OFFSET + 8].copy_from_slice(&generation.to_le_bytes());
        self.published = generation;
        self.wake.signal()?;
        Ok(generation)
    }
}

fn check_capacity(capacity: usize) -> Result<(), String> {
    if capacity == 0 || capacity > MAX_FRAME_BYTES {
        return Err("invalid frame map capacity".into());
    }
    Ok(())
}

fn check_header(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < HEADER_BYTES || &bytes[..8] != MAGIC || bytes[VERSION_OFFSET] != VERSION || bytes[SLOT_COUNT_OFFSET] != SLOT_COUNT as u8 {
        return Err("frame map header mismatch".into());
    }
    let capacity = read_u64(&bytes[CAPACITY_OFFSET..CAPACITY_OFFSET + 8]) as usize;
    check_capacity(capacity)?;
    if bytes.len() != file_len(capacity)? {
        return Err("frame map size mismatch".into());
    }
    Ok(capacity)
}

fn file_len(capacity: usize) -> Result<usize, String> {
    SLOT_PREFIX_BYTES
        .checked_add(METADATA_BYTES)
        .and_then(|slot| slot.checked_add(capacity))
        .and_then(|slot| slot.checked_mul(SLOT_COUNT))
        .and_then(|slots| slots.checked_add(HEADER_BYTES))
        .ok_or_else(|| "frame map size overflow".into())
}

fn slot_offset(capacity: usize, slot: usize) -> usize {
    HEADER_BYTES + slot * (SLOT_PREFIX_BYTES + METADATA_BYTES + capacity)
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("4 bytes"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("8 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reader_takes_the_newest_complete_record_once_and_sees_a_clear() {
        let path = std::env::temp_dir().join(format!("qnc-frame-map-{}.map", std::process::id()));
        create(&path, 16).unwrap();
        let mut reader = Reader::open(&path).unwrap();
        assert!(reader.read_newest().unwrap().is_none());
        let mut writer = Writer::open(&path).unwrap();
        let header = PictureHeader { contract_version: "0.3.0".into(), session_id: "s".into(), source_generation: 1, output_generation: 2, sequence: 1, source_id: "c".into(), frame: 9, timebase: Timebase { fps_num: 50, fps_den: 1 }, width: 2, height: 1 };
        writer.post(Some(&header), &[1; 8]).unwrap();
        writer.post(Some(&PictureHeader { frame: 10, sequence: 2, ..header.clone() }), &[2; 8]).unwrap();
        assert_eq!(reader.backlog(), 2);
        let Some(Posted::Picture(picture)) = reader.recv(Duration::from_millis(10)).unwrap() else { panic!("a picture") };
        assert_eq!(picture.header.frame, 10, "the newest, not the first");
        assert_eq!(&picture.rgba[..], &[2; 8]);
        assert!(reader.read_newest().unwrap().is_none(), "taken once");
        writer.post(None, &[]).unwrap();
        assert_eq!(reader.read_newest().unwrap(), Some(Posted::Clear));
        drop(writer);
        drop(reader);
        let _ = std::fs::remove_file(&path);
    }
}
