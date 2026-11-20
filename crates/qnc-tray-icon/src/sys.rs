//! Windows: a message-only window of the caller's thread owns the shell icon; the caller pumps
//! its messages (`pump`) between its own ticks. The menu is modal while it is open.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, LoadIconW, PeekMessageW, RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HWND_MESSAGE,
    IDI_APPLICATION, MF_CHECKED, MF_GRAYED, MF_STRING, MSG, PM_REMOVE, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
};

use crate::{MenuItem, TrayEvent};

const WM_TRAY: u32 = WM_APP + 1;

struct Shared {
    events: Sender<TrayEvent>,
    menu: Vec<MenuItem>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

pub(crate) struct Tray {
    window: HWND,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_into(target: &mut [u16], text: &str) {
    let units: Vec<u16> = text.encode_utf16().take(target.len() - 1).collect();
    target[..units.len()].copy_from_slice(&units);
    target[units.len()] = 0;
}

fn icon_data(window: HWND) -> NOTIFYICONDATAW {
    // SAFETY: a plain C struct; every field zero is a valid empty value.
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = window;
    data.uID = 1;
    data
}

impl Tray {
    pub(crate) fn start(tip: &str, menu: Vec<MenuItem>) -> Result<(Tray, Receiver<TrayEvent>), String> {
        let (send, events) = mpsc::channel();
        {
            let mut shared = SHARED.lock().map_err(|_| "tray state")?;
            if shared.is_some() {
                return Err("one tray per process".into());
            }
            *shared = Some(Shared { events: send, menu });
        }
        let class = wide("QncTrayWindow");
        // SAFETY: Win32 calls with buffers that live through each call; the window is this
        // thread's and lives until `Drop`.
        let window = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let mut window_class: WNDCLASSW = std::mem::zeroed();
            window_class.lpfnWndProc = Some(procedure);
            window_class.hInstance = instance;
            window_class.lpszClassName = class.as_ptr();
            RegisterClassW(&window_class);
            let window = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), instance, std::ptr::null());
            if window.is_null() {
                *SHARED.lock().unwrap_or_else(|p| p.into_inner()) = None;
                return Err(format!("the tray window was not made ({})", GetLastError()));
            }
            let mut data = icon_data(window);
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.uCallbackMessage = WM_TRAY;
            data.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            copy_into(&mut data.szTip, tip);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                DestroyWindow(window);
                *SHARED.lock().unwrap_or_else(|p| p.into_inner()) = None;
                return Err("the shell did not take the tray icon".into());
            }
            window
        };
        Ok((Tray { window }, events))
    }

    /// Handles the messages waiting for the icon's window (a click shows the menu).
    pub(crate) fn pump(&self) {
        // SAFETY: the window is this thread's; the message struct lives through the calls.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, self.window, 0, 0, PM_REMOVE) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    pub(crate) fn notify(&self, title: &str, text: &str) {
        let mut data = icon_data(self.window);
        data.uFlags = NIF_INFO;
        copy_into(&mut data.szInfoTitle, title);
        copy_into(&mut data.szInfo, text);
        data.dwInfoFlags = NIIF_INFO;
        // SAFETY: the icon is this window's.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    pub(crate) fn set_menu(&self, menu: Vec<MenuItem>) {
        if let Ok(mut shared) = SHARED.lock() {
            if let Some(shared) = shared.as_mut() {
                shared.menu = menu;
            }
        }
    }

    pub(crate) fn set_tip(&self, tip: &str) {
        let mut data = icon_data(self.window);
        data.uFlags = NIF_TIP;
        copy_into(&mut data.szTip, tip);
        // SAFETY: the icon is this window's.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: the icon and window are this thread's.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &icon_data(self.window));
            DestroyWindow(self.window);
        }
        *SHARED.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

fn send(event: TrayEvent) {
    if let Ok(shared) = SHARED.lock() {
        if let Some(shared) = shared.as_ref() {
            let _ = shared.events.send(event);
        }
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TRAY => {
            if matches!(lparam as u32, WM_LBUTTONUP | WM_RBUTTONUP) {
                if let Some(id) = menu(window) {
                    send(TrayEvent::Menu(id));
                }
            }
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// The menu at the cursor; the id of the chosen item.
unsafe fn menu(window: HWND) -> Option<u32> {
    let items = SHARED.lock().ok()?.as_ref()?.menu.clone();
    let popup = CreatePopupMenu();
    let texts: Vec<Vec<u16>> = items.iter().map(|i| wide(&i.text)).collect();
    for (item, text) in items.iter().zip(&texts) {
        let flags = MF_STRING | if item.enabled { 0 } else { MF_GRAYED } | if item.checked { MF_CHECKED } else { 0 };
        AppendMenuW(popup, flags, item.id as usize, text.as_ptr());
    }
    let mut at = POINT { x: 0, y: 0 };
    GetCursorPos(&mut at);
    SetForegroundWindow(window);
    let chosen = TrackPopupMenu(popup, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x, at.y, 0, window, std::ptr::null());
    DestroyMenu(popup);
    (chosen > 0).then_some(chosen as u32)
}
