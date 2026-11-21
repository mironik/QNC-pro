//! Windows: a message-only window of the caller's thread owns the shell icon; the caller pumps
//! its messages (`pump`) between its own frames. The menu is modal while it is open and drawn
//! by `menu_look` in the theme's colours.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetSystemMetrics, LoadIconW, PeekMessageW,
    RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, HWND_MESSAGE, ICONINFO, IDI_APPLICATION, MF_GRAYED, MF_OWNERDRAW, MSG, PM_REMOVE, SM_CXSMICON, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_DRAWITEM, WM_LBUTTONUP, WM_MEASUREITEM, WM_RBUTTONUP, WNDCLASSW,
};

use crate::menu_look::Palette;
use crate::{MenuItem, Picture, TrayEvent};

const WM_TRAY: u32 = WM_APP + 1;

struct Shared {
    events: Sender<TrayEvent>,
    menu: Vec<MenuItem>,
    palette: Palette,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

pub(crate) struct Tray {
    window: HWND,
    icon: HICON,
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

/// The menu's colours from the theme.
fn palette() -> Palette {
    let colours = &qnc_theme::theme().colours;
    Palette { surface: colours.panel.0, raised: colours.raised.0, text: colours.text.0, muted: colours.muted.0, tick: colours.done.0 }
}

/// The icon of the caller's picture at the size of a small icon on this screen; null when there
/// is no picture or the system did not make the icon.
///
/// SAFETY: Win32 calls on bitmaps this function makes and frees; the icon is the caller's.
unsafe fn picture_icon(picture: Option<Picture>) -> HICON {
    let Some(picture) = picture else { return std::ptr::null_mut() };
    let size = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
    let rgba = picture(size);
    if rgba.len() != (size * size * 4) as usize {
        return std::ptr::null_mut();
    }
    let mut header: BITMAPINFO = std::mem::zeroed();
    header.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    header.bmiHeader.biWidth = size as i32;
    header.bmiHeader.biHeight = -(size as i32);
    header.bmiHeader.biPlanes = 1;
    header.bmiHeader.biBitCount = 32;
    header.bmiHeader.biCompression = BI_RGB;
    let screen = GetDC(std::ptr::null_mut());
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let colour = CreateDIBSection(screen, &header, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    ReleaseDC(std::ptr::null_mut(), screen);
    if colour.is_null() || bits.is_null() {
        return std::ptr::null_mut();
    }
    let target = std::slice::from_raw_parts_mut(bits.cast::<u8>(), rgba.len());
    for (to, from) in target.chunks_mut(4).zip(rgba.chunks(4)) {
        to.copy_from_slice(&[from[2], from[1], from[0], from[3]]);
    }
    // The mask is all zeros (the colour bitmap's alpha decides); its rows are padded to two bytes.
    let zeros = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
    let mask = CreateBitmap(size as i32, size as i32, 1, 1, zeros.as_ptr().cast());
    let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: colour };
    let made = CreateIconIndirect(&info);
    DeleteObject(colour);
    DeleteObject(mask);
    made
}

impl Tray {
    pub(crate) fn start(tip: &str, menu: Vec<MenuItem>, picture: Option<Picture>) -> Result<(Tray, Receiver<TrayEvent>), String> {
        let (send, events) = mpsc::channel();
        {
            let mut shared = SHARED.lock().map_err(|_| "tray state")?;
            if shared.is_some() {
                return Err("one tray per process".into());
            }
            *shared = Some(Shared { events: send, menu, palette: palette() });
        }
        let class = wide("QncTrayWindow");
        // SAFETY: Win32 calls with buffers that live through each call; the window is this
        // thread's and lives until `Drop`.
        let (window, icon) = unsafe {
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
            let own = picture_icon(picture);
            data.hIcon = if own.is_null() { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) } else { own };
            copy_into(&mut data.szTip, tip);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                if !own.is_null() {
                    DestroyIcon(own);
                }
                DestroyWindow(window);
                *SHARED.lock().unwrap_or_else(|p| p.into_inner()) = None;
                return Err("the shell did not take the tray icon".into());
            }
            (window, own)
        };
        Ok((Tray { window, icon }, events))
    }

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
        // SAFETY: the icon and window are this thread's; the picture icon is this tray's own.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &icon_data(self.window));
            DestroyWindow(self.window);
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
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
        WM_MEASUREITEM => {
            crate::menu_look::measure(lparam as *mut _);
            1
        }
        WM_DRAWITEM => {
            crate::menu_look::draw(lparam as *const _);
            1
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// The menu at the cursor; the id of the chosen item.
unsafe fn menu(window: HWND) -> Option<u32> {
    let (items, palette) = {
        let shared = SHARED.lock().ok()?;
        let shared = shared.as_ref()?;
        (shared.menu.clone(), shared.palette)
    };
    let popup = CreatePopupMenu();
    // Drawn by the tray in QNC's look (`menu_look`): the item's data is its index.
    crate::menu_look::hold(items.iter().map(|item| (item.text.encode_utf16().collect(), item.check)).collect(), palette);
    let ground = crate::menu_look::ground(popup, palette);
    for (at, item) in items.iter().enumerate() {
        let flags = if item.enabled { MF_OWNERDRAW } else { MF_OWNERDRAW | MF_GRAYED };
        AppendMenuW(popup, flags, item.id as usize, at as *const u16);
    }
    let mut at = POINT { x: 0, y: 0 };
    GetCursorPos(&mut at);
    // The menu closes when the user clicks elsewhere only if the window is in front.
    SetForegroundWindow(window);
    let chosen = TrackPopupMenu(popup, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x, at.y, 0, window, std::ptr::null());
    DestroyMenu(popup);
    crate::menu_look::forget_ground(ground);
    (chosen > 0).then_some(chosen as u32)
}
