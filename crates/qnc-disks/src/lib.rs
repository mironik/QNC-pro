//! The disks of this computer. A disk is known by its volume serial (the same card on another
//! letter is the same disk), with its label and the kind the system gives it. `Looks` remembers
//! the last look and says what came and what went; the caller decides how often to look. Reads
//! nothing from the disk but its identity.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The root, e.g. `E:\`.
    pub root: PathBuf,
    /// Eight hex digits.
    pub serial: String,
    pub label: Option<String>,
    pub kind: DiskKind,
}

impl Volume {
    /// The QNC source URI of the disk: one per serial, whatever letter it got.
    pub fn source_uri(&self) -> String {
        format!("qnc://local/source/volume-{}", self.serial)
    }

    /// What tells it apart over looks: letter and serial.
    pub fn key(&self) -> String {
        format!("{}|{}", self.root.display(), self.serial)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskKind {
    /// A card in a reader, a USB stick.
    Removable,
    Fixed,
    Network,
    Optical,
    Other,
}

/// The disks attached now.
pub fn attached() -> Vec<Volume> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z')
            .filter_map(|letter| {
                let root = PathBuf::from(format!("{}:\\", letter as char));
                if !root.is_dir() {
                    return None;
                }
                let (serial, label) = sys::identity(&root)?;
                Some(Volume { kind: sys::kind(&root), root, serial, label })
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The size of the disk at `root`, bytes, when the system says it.
pub fn size_bytes(root: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        sys::size(root)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        None
    }
}

/// What changed between two looks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub came: Vec<Volume>,
    pub went: Vec<Volume>,
}

/// The disks of the last look.
#[derive(Debug, Default)]
pub struct Looks {
    seen: Option<Vec<Volume>>,
}

impl Looks {
    pub fn new() -> Looks {
        Looks::default()
    }

    /// Takes a look: what came since the last one (everything at the first) and what went.
    pub fn look(&mut self, now: Vec<Volume>) -> Changes {
        let keys = |volumes: &[Volume]| volumes.iter().map(Volume::key).collect::<BTreeSet<_>>();
        let changes = match &self.seen {
            None => Changes { came: now.clone(), went: Vec::new() },
            Some(seen) => {
                let (before, after) = (keys(seen), keys(&now));
                Changes {
                    came: now.iter().filter(|v| !before.contains(&v.key())).cloned().collect(),
                    went: seen.iter().filter(|v| !after.contains(&v.key())).cloned().collect(),
                }
            }
        };
        self.seen = Some(now);
        changes
    }

    /// Forgets a disk, so the next look has it come again (a card not readable yet).
    pub fn forget(&mut self, volume: &Volume) {
        if let Some(seen) = &mut self.seen {
            seen.retain(|known| known.key() != volume.key());
        }
    }

    pub fn seen(&self) -> &[Volume] {
        self.seen.as_deref().unwrap_or(&[])
    }
}

#[cfg(windows)]
mod sys {
    use super::DiskKind;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW};

    fn wide(root: &Path) -> Vec<u16> {
        root.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    pub(super) fn kind(root: &Path) -> DiskKind {
        let wide = wide(root);
        // SAFETY: a nul-terminated wide path living through the call.
        match unsafe { GetDriveTypeW(wide.as_ptr()) } {
            2 => DiskKind::Removable,
            3 => DiskKind::Fixed,
            4 => DiskKind::Network,
            5 => DiskKind::Optical,
            _ => DiskKind::Other,
        }
    }

    pub(super) fn size(root: &Path) -> Option<u64> {
        let wide = wide(root);
        let mut total = 0u64;
        // SAFETY: as above; the output lives through the call.
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), std::ptr::null_mut(), &mut total, std::ptr::null_mut()) };
        (ok != 0 && total > 0).then_some(total)
    }

    pub(super) fn identity(root: &Path) -> Option<(String, Option<String>)> {
        let wide = wide(root);
        let mut name = vec![0u16; 260];
        let mut serial = 0u32;
        // SAFETY: the buffers live through the call and their lengths are given.
        let ok = unsafe {
            GetVolumeInformationW(wide.as_ptr(), name.as_mut_ptr(), name.len() as u32, &mut serial, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0)
        };
        if ok == 0 {
            return None;
        }
        let end = name.iter().position(|v| *v == 0).unwrap_or(name.len());
        let label = String::from_utf16_lossy(&name[..end]);
        Some((format!("{serial:08x}"), (!label.trim().is_empty()).then_some(label)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(letter: &str, serial: &str) -> Volume {
        Volume { root: PathBuf::from(format!("{letter}:\\")), serial: serial.into(), label: None, kind: DiskKind::Removable }
    }

    #[test]
    fn a_look_says_what_came_and_went_and_a_forgotten_disk_comes_again() {
        let mut looks = Looks::new();
        let first = looks.look(vec![disk("C", "1"), disk("E", "2")]);
        assert_eq!(first.came.len(), 2);
        let second = looks.look(vec![disk("C", "1"), disk("F", "2")]);
        assert_eq!((second.came.len(), second.went.len()), (1, 1), "the same serial on another letter is a new disk");
        looks.forget(&disk("F", "2"));
        assert_eq!(looks.look(vec![disk("C", "1"), disk("F", "2")]).came, vec![disk("F", "2")]);
        assert_eq!(disk("E", "7e80f1fb").source_uri(), "qnc://local/source/volume-7e80f1fb");
    }

    #[cfg(windows)]
    #[test]
    fn the_system_disk_is_attached_fixed_and_has_a_size() {
        let c = attached().into_iter().find(|v| v.root.to_string_lossy().starts_with("C:")).expect("C:");
        assert_eq!((c.serial.len(), c.kind), (8, DiskKind::Fixed));
        assert!(size_bytes(&c.root).is_some_and(|b| b > 1 << 30));
    }
}
