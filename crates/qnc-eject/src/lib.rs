//! A card taken out safely. `may_go` is the one rule: the card may go when every clip is
//! copied off it and nothing of it is open (a player, an import). `eject` is the system's own
//! Eject: lock the volume (refused when something has it open), unmount, allow removal, eject
//! the medium, then remove the device as "Safely remove hardware" does, so a reader does not
//! mount the card again at once. Nothing on the card is written. Windows only for now; another
//! system says it has no eject yet.

use std::path::Path;

use qnc_progress::Counts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Clips are still being copied off the card.
    NotCopied { left: usize },
    /// Something still reads the card (a player, an import, a worker).
    InUse,
    /// The system's reason.
    Failed(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NotCopied { left } => write!(f, "{left} clip(s) not copied off the card yet"),
            Refused::InUse => write!(f, "the card is in use"),
            Refused::Failed(reason) => write!(f, "{reason}"),
        }
    }
}

/// Whether the card may go: every clip copied, nothing of it open.
pub fn may_go(counts: &Counts, readers_open: usize) -> Result<(), Refused> {
    if !counts.copied() {
        return Err(Refused::NotCopied { left: counts.clips.saturating_sub(counts.imported) });
    }
    if readers_open > 0 {
        return Err(Refused::InUse);
    }
    Ok(())
}

/// Ejects the disk whose root is `root` (a drive root such as `E:\`).
pub fn eject(root: &Path) -> Result<(), Refused> {
    #[cfg(windows)]
    {
        sys::eject(root)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Err(Refused::Failed("ejecting a card is not made for this system yet".into()))
    }
}

#[cfg(windows)]
mod sys;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_may_go_only_copied_and_not_in_use_and_only_a_drive_root_is_ejected() {
        let mut counts = Counts { clips: 3, imported: 2, ..Counts::default() };
        assert_eq!(may_go(&counts, 0), Err(Refused::NotCopied { left: 1 }));
        counts.imported = 3;
        assert_eq!(may_go(&counts, 1), Err(Refused::InUse));
        assert_eq!(may_go(&counts, 0), Ok(()));
        assert!(matches!(eject(Path::new("C:\\Users")), Err(Refused::Failed(_))));
        assert!(matches!(eject(Path::new("no-such-place")), Err(Refused::Failed(_))));
    }
}
