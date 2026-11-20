//! The card rule. A disk is a card when the root folder of a layout QNC knows is on it
//! (`contracts/cameras/layouts.json`, built in); the card is only looked at, never written. A
//! card put in settles: a reader still waking gives half a card, so a card is ready for its
//! ingest only once it has been seen for `SETTLE` without a gap; out in between, it settles
//! anew. A disk that could not be read is looked at again a few times, then told. Time comes
//! from the caller, so the rule is a pure function of looks.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use qnc_disks::Volume;
use serde::Deserialize;

pub const LAYOUTS: &str = include_str!("../../../contracts/cameras/layouts.json");
/// How long a card put in settles before it is read.
pub const SETTLE: Duration = Duration::from_secs(5);
/// A card not seen for longer than this was out: it settles anew.
pub const GAP: Duration = Duration::from_secs(4);
/// How many times an unreadable disk is looked at again.
pub const READ_TRIES: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Layout {
    pub id: String,
    pub manufacturer: String,
    pub family: String,
    /// The card's root folder, relative to the disk.
    pub root: String,
    pub index: String,
}

#[derive(Debug, Deserialize)]
struct Layouts {
    layouts: Vec<Layout>,
}

/// The card layouts QNC knows.
pub fn layouts() -> Vec<Layout> {
    serde_json::from_str::<Layouts>(LAYOUTS).expect("the built-in layouts are valid; the test says so").layouts
}

/// What a disk is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A card: the layouts found on it (their roots exist).
    Card { volume: Volume, layouts: Vec<Layout> },
    Unknown { volume: Volume },
    /// The disk could not be looked at.
    Unreadable { volume: Volume, error: String },
}

/// What the disk is, by the layouts whose root folder and index are on it.
pub fn what_is(volume: Volume) -> Found {
    match std::fs::read_dir(&volume.root) {
        Err(error) => Found::Unreadable { volume, error: error.to_string() },
        Ok(_) => {
            let found: Vec<Layout> = layouts().into_iter().filter(|l| is_card_root(&volume.root.join(&l.root), &l.index)).collect();
            if found.is_empty() {
                Found::Unknown { volume }
            } else {
                Found::Card { volume, layouts: found }
            }
        }
    }
}

fn is_card_root(root: &Path, index: &str) -> bool {
    root.is_dir() && root.join(index).is_file()
}

/// Since when each card is seen, by serial: (first seen, last seen), as the caller's clock.
#[derive(Debug, Default)]
pub struct Settling {
    seen: BTreeMap<String, (Duration, Duration)>,
    tries: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// Not yet: look again.
    Wait,
    /// Seen long enough: read it now.
    Ready,
}

impl Settling {
    pub fn new() -> Settling {
        Settling::default()
    }

    /// A card seen at `now`: ready once it was seen for `SETTLE` without a gap longer than `GAP`.
    pub fn seen(&mut self, serial: &str, now: Duration) -> Settled {
        let (since, last) = self.seen.entry(serial.to_string()).or_insert((now, now));
        if now.saturating_sub(*last) > GAP {
            *since = now;
        }
        *last = now;
        if now.saturating_sub(*since) < SETTLE {
            return Settled::Wait;
        }
        self.seen.remove(serial);
        Settled::Ready
    }

    /// The card went out.
    pub fn gone(&mut self, serial: &str) {
        self.seen.remove(serial);
        self.tries.remove(serial);
    }

    /// An unreadable disk: whether to look again (`true`) or to give up and tell.
    pub fn try_again(&mut self, serial: &str) -> bool {
        let count = self.tries.entry(serial.to_string()).or_default();
        *count += 1;
        *count < READ_TRIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as D;

    #[test]
    fn the_built_in_layouts_parse_and_a_folder_with_a_layout_root_is_a_card() {
        let layouts = layouts();
        assert!(layouts.iter().any(|l| l.root == "XDROOT"));
        let dir = std::env::temp_dir().join(format!("qnc-card-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("PRIVATE").join("M4ROOT")).unwrap();
        let volume = Volume { root: dir.clone(), serial: "1".into(), label: None, kind: qnc_disks::DiskKind::Removable };
        assert!(matches!(what_is(volume.clone()), Found::Unknown { .. }), "a root without its index is no card");
        std::fs::write(dir.join("PRIVATE").join("M4ROOT").join("MEDIAPRO.XML"), "<x/>").unwrap();
        let Found::Card { layouts, .. } = what_is(volume) else { panic!("a card") };
        assert_eq!(layouts[0].id, "sony-m4root-sd");
        let nowhere = Volume { root: dir.join("nowhere"), serial: "2".into(), label: None, kind: qnc_disks::DiskKind::Removable };
        assert!(matches!(what_is(nowhere), Found::Unreadable { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_card_settles_five_seconds_without_a_gap_and_an_unreadable_disk_gets_five_tries() {
        let mut s = Settling::new();
        assert_eq!(s.seen("a", D::from_secs(0)), Settled::Wait);
        assert_eq!(s.seen("a", D::from_secs(2)), Settled::Wait);
        assert_eq!(s.seen("a", D::from_secs(4)), Settled::Wait);
        assert_eq!(s.seen("a", D::from_secs(5)), Settled::Ready);
        assert_eq!(s.seen("a", D::from_secs(7)), Settled::Wait, "ready once; a card seen again settles anew");
        assert_eq!(s.seen("a", D::from_secs(9)), Settled::Wait);
        assert_eq!(s.seen("a", D::from_secs(20)), Settled::Wait, "a gap of 11 s: out meanwhile");
        assert_eq!(s.seen("a", D::from_secs(24)), Settled::Wait);
        assert_eq!(s.seen("a", D::from_secs(25)), Settled::Ready);
        assert!((0..4).all(|_| s.try_again("b")));
        assert!(!s.try_again("b"));
        s.gone("b");
        assert!(s.try_again("b"));
    }
}
