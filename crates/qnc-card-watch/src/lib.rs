//! When is a card ready. Every look at the attached disks tells what came and what went; a disk
//! that is a card (a known layout on it) is reported ready once it has settled (`qnc-card`), a
//! disk that cannot be read is looked at again a few times and then reported, a card that went
//! out is reported gone. The caller gives the clock and the disks; this module only decides.

use std::collections::BTreeMap;
use std::time::Duration;

use qnc_card::{Found, Layout, Settled, Settling};
use qnc_card_select::Card;
use qnc_disks::{Looks, Volume};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// A card settled and ready to read, with the layout it was found by.
    Ready { card: Card, layout: Layout },
    /// A disk that is not a card; said once.
    NotACard(Volume),
    /// A disk that could not be read after its tries; said once.
    Unreadable { volume: Volume, error: String },
    /// A disk that went out (its serial).
    Gone(String),
}

#[derive(Default)]
pub struct Watch {
    looks: Looks,
    settling: Settling,
    /// Disks already reported ready, not a card or unreadable, by key: said once per stay.
    told: BTreeMap<String, ()>,
}

impl Watch {
    pub fn new() -> Watch {
        Watch::default()
    }

    /// One look at the disks attached `now`.
    pub fn look(&mut self, now: Duration, attached: Vec<Volume>) -> Vec<Seen> {
        let mut seen = Vec::new();
        let changes = self.looks.look(attached);
        for gone in changes.went {
            self.settling.gone(&gone.serial);
            self.told.remove(&gone.key());
            seen.push(Seen::Gone(gone.serial));
        }
        for volume in self.looks.seen().to_vec() {
            if self.told.contains_key(&volume.key()) {
                continue;
            }
            match qnc_card::what_is(volume.clone()) {
                Found::Card { volume, layouts } => {
                    if self.settling.seen(&volume.serial, now) == Settled::Ready {
                        let layout = layouts.into_iter().next().expect("a card has a layout");
                        let card = Card { disk: volume.root.clone(), serial: volume.serial.clone(), volume_name: volume.label.clone().unwrap_or_default(), root_path: layout.root.clone() };
                        self.told.insert(volume.key(), ());
                        seen.push(Seen::Ready { card, layout });
                    }
                }
                Found::Unknown { volume } => {
                    self.told.insert(volume.key(), ());
                    seen.push(Seen::NotACard(volume));
                }
                Found::Unreadable { volume, error } => {
                    if !self.settling.try_again(&volume.serial) {
                        self.told.insert(volume.key(), ());
                        seen.push(Seen::Unreadable { volume, error });
                    }
                }
            }
        }
        seen
    }

    /// Forgets a disk so the next look reports it again (after an ingest, for a second pass).
    pub fn forget(&mut self, volume: &Volume) {
        self.told.remove(&volume.key());
        self.looks.forget(volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qnc_disks::DiskKind;

    #[test]
    fn a_card_is_ready_once_after_it_settled_a_plain_disk_is_said_once_and_a_gone_card_is_told() {
        let dir = std::env::temp_dir().join(format!("qnc-card-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("PRIVATE").join("XDROOT");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("MEDIAPRO.XML"), "x").unwrap();
        let plain = dir.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let card = Volume { root: dir.clone(), serial: "aabbccdd".into(), label: Some("CARD".into()), kind: DiskKind::Removable };
        let disk = Volume { root: plain.clone(), serial: "11223344".into(), label: None, kind: DiskKind::Removable };
        let mut watch = Watch::new();
        let s = Duration::from_secs;
        assert_eq!(watch.look(s(0), vec![card.clone(), disk.clone()]), vec![Seen::NotACard(disk.clone())]);
        assert!(watch.look(s(2), vec![card.clone(), disk.clone()]).is_empty(), "settling, said once");
        let ready = watch.look(s(6), vec![card.clone(), disk.clone()]);
        assert!(matches!(&ready[..], [Seen::Ready { card: c, layout }] if c.root_path == "PRIVATE/XDROOT" && c.serial == "aabbccdd" && layout.id == "sony-xdroot-sd"), "{ready:?}");
        assert!(watch.look(s(7), vec![card.clone(), disk.clone()]).is_empty(), "ready is said once");
        assert_eq!(watch.look(s(8), vec![disk.clone()]), vec![Seen::Gone("aabbccdd".into())]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
