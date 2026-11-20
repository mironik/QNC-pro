//! The icon in the system tray. A click on it shows the menu it was given; the chosen item comes
//! back as an event. The tip and a balloon notice are set from outside. On another system there
//! is no tray: starting it says so.

use std::sync::mpsc::Receiver;

#[cfg(windows)]
mod sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// The id of the chosen menu item.
    Menu(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: u32,
    pub text: String,
    pub enabled: bool,
    pub checked: bool,
}

pub struct Tray {
    #[cfg(windows)]
    inner: sys::Tray,
    events: Receiver<TrayEvent>,
}

impl Tray {
    pub fn start(tip: &str, menu: Vec<MenuItem>) -> Result<Tray, String> {
        #[cfg(windows)]
        {
            let (inner, events) = sys::Tray::start(tip, menu)?;
            Ok(Tray { inner, events })
        }
        #[cfg(not(windows))]
        {
            let _ = (tip, menu);
            Err("the tray is not on this system yet".into())
        }
    }

    /// Handles what the user did with the icon since the last call; the events follow.
    pub fn pump(&self) {
        #[cfg(windows)]
        self.inner.pump();
    }

    /// The next event the user made, when there is one.
    pub fn try_event(&self) -> Option<TrayEvent> {
        self.events.try_recv().ok()
    }

    pub fn set_menu(&self, menu: Vec<MenuItem>) {
        #[cfg(windows)]
        self.inner.set_menu(menu);
        #[cfg(not(windows))]
        let _ = menu;
    }

    pub fn set_tip(&self, tip: &str) {
        #[cfg(windows)]
        self.inner.set_tip(tip);
        #[cfg(not(windows))]
        let _ = tip;
    }

    /// A balloon notice by the icon.
    pub fn notify(&self, title: &str, text: &str) {
        #[cfg(windows)]
        self.inner.notify(title, text);
        #[cfg(not(windows))]
        let _ = (title, text);
    }
}
