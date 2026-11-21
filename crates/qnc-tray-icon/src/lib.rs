//! The icon in the system tray (copied from the old QNC's `qnc-tray`, its icon and menu): the
//! caller's picture as the icon, a tooltip, a menu in QNC's look drawn by the tray itself (an
//! item may carry a tick box, a line that only tells is grey), and a balloon notice. A click with
//! either button shows the menu; the chosen item comes back as an event. The caller pumps the
//! icon's messages between its own frames; there is no thread. On another system there is no
//! tray yet: starting it says so.

use std::sync::mpsc::Receiver;

#[cfg(windows)]
mod menu_look;
#[cfg(windows)]
mod sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// The id of the chosen menu item.
    Menu(u32),
}

/// One item of the icon's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: u32,
    pub text: String,
    /// A line that only tells (the status): grey, nothing happens on it.
    pub enabled: bool,
    /// A box with a tick drawn before the text: `Some(true)` ticked, `Some(false)` empty, `None`
    /// no box (a choice kept on the computer, as the tray's mode).
    pub check: Option<bool>,
}

/// The picture of the icon at the size the system asks for: straight-alpha RGBA, rows top to bottom.
pub type Picture = fn(u32) -> Vec<u8>;

pub struct Tray {
    #[cfg(windows)]
    inner: sys::Tray,
    events: Receiver<TrayEvent>,
}

impl Tray {
    /// Puts the icon in the tray with its tooltip and menu; without a picture (or when the system
    /// does not take it) the system's own application icon shows.
    pub fn start(tip: &str, menu: Vec<MenuItem>, picture: Option<Picture>) -> Result<Tray, String> {
        #[cfg(windows)]
        {
            let (inner, events) = sys::Tray::start(tip, menu, picture)?;
            Ok(Tray { inner, events })
        }
        #[cfg(not(windows))]
        {
            let _ = (tip, menu, picture);
            Err("the tray is not on this system yet".into())
        }
    }

    /// Handles the messages waiting for the icon (a click shows the menu); call it every frame.
    pub fn pump(&self) {
        #[cfg(windows)]
        self.inner.pump();
    }

    /// What the user chose, when they chose something (never waits).
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

    /// A balloon notice beside the icon.
    pub fn notify(&self, title: &str, text: &str) {
        #[cfg(windows)]
        self.inner.notify(title, text);
        #[cfg(not(windows))]
        let _ = (title, text);
    }
}
