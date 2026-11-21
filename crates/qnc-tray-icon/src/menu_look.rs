//! The tray menu in QNC's look (copied from the old QNC's `qnc-tray::menu_look`): the items are
//! drawn by the tray itself (owner-drawn) on the theme's surface, in a larger font, the item under
//! the pointer raised, an item that is not a choice (the status) muted, a tick box before an item
//! that is a choice kept on the computer. The colours come from the theme block, never from here.

use std::cell::RefCell;

use windows_sys::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDC, GetDeviceCaps, GetStockObject, GetTextExtentPoint32W, LineTo, MoveToEx, Rectangle, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, CLEARTYPE_QUALITY, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, HDC, HFONT, LOGPIXELSY, NULL_BRUSH, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_GRAYED, ODS_SELECTED};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetMenuInfo, HMENU, MENUINFO, MIM_BACKGROUND};

/// The font height in points and the room round an item's text, in points too.
const FONT_PT: i32 = 12;
const PAD_X_PT: i32 = 14;
const ROW_PT: i32 = 26;
/// The side of a tick box, in points.
const BOX_PT: i32 = 11;

/// The colours of the menu, from the theme.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Palette {
    pub surface: [u8; 3],
    pub raised: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub tick: [u8; 3],
}

fn rgb([r, g, b]: [u8; 3]) -> COLORREF {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

struct Held {
    items: Vec<(Vec<u16>, Option<bool>)>,
    palette: Palette,
}

thread_local! {
    /// The texts of the menu being shown, by the item data the menu carries (their index).
    static HELD: RefCell<Option<Held>> = const { RefCell::new(None) };
}

/// Points as pixels of the screen.
unsafe fn px(points: i32) -> i32 {
    let screen = GetDC(std::ptr::null_mut());
    let dpi = GetDeviceCaps(screen, LOGPIXELSY as i32);
    ReleaseDC(std::ptr::null_mut(), screen);
    points * dpi / 72
}

unsafe fn font() -> HFONT {
    let face: Vec<u16> = "Segoe UI".encode_utf16().chain([0]).collect();
    CreateFontW(-px(FONT_PT), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, 0, 0, 0, u32::from(CLEARTYPE_QUALITY), 0, face.as_ptr())
}

/// The items about to be shown, each with its box (`MenuItem::check`); item `n` carries `n` as its data.
pub(crate) fn hold(items: Vec<(Vec<u16>, Option<bool>)>, palette: Palette) {
    HELD.with(|held| *held.borrow_mut() = Some(Held { items, palette }));
}

fn item_of(at: usize) -> Option<((Vec<u16>, Option<bool>), Palette)> {
    HELD.with(|held| held.borrow().as_ref().and_then(|held| held.items.get(at).cloned().map(|item| (item, held.palette))))
}

/// The menu's own ground in the surface colour (round the items).
pub(crate) unsafe fn ground(menu: HMENU, palette: Palette) -> isize {
    let brush = CreateSolidBrush(rgb(palette.surface));
    let info = MENUINFO { cbSize: std::mem::size_of::<MENUINFO>() as u32, fMask: MIM_BACKGROUND, dwStyle: 0, cyMax: 0, hbrBack: brush, dwContextHelpID: 0, dwMenuData: 0 };
    SetMenuInfo(menu, &info);
    brush as isize
}

pub(crate) unsafe fn forget_ground(brush: isize) {
    DeleteObject(brush as _);
}

/// WM_MEASUREITEM: the text's width in the menu font and the room round it.
pub(crate) unsafe fn measure(item: *mut MEASUREITEMSTRUCT) {
    let item = &mut *item;
    let Some(((text, check), _)) = item_of(item.itemData) else { return };
    let screen = GetDC(std::ptr::null_mut());
    let font = font();
    let old = SelectObject(screen, font);
    let mut size = SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(screen, text.as_ptr(), text.len() as i32, &mut size);
    SelectObject(screen, old);
    DeleteObject(font);
    ReleaseDC(std::ptr::null_mut(), screen);
    let boxed = if check.is_some() { px(BOX_PT) + px(PAD_X_PT) / 2 } else { 0 };
    item.itemWidth = (size.cx + 2 * px(PAD_X_PT) + boxed) as u32;
    item.itemHeight = px(ROW_PT) as u32;
}

/// WM_DRAWITEM: the row's ground (raised under the pointer), its box, its text (muted when not a choice).
pub(crate) unsafe fn draw(item: *const DRAWITEMSTRUCT) {
    let item = &*item;
    let Some(((text, check), palette)) = item_of(item.itemData) else { return };
    let grey = item.itemState & ODS_GRAYED != 0;
    let lit = item.itemState & ODS_SELECTED != 0 && !grey;
    let dc: HDC = item.hDC;
    let brush = CreateSolidBrush(rgb(if lit { palette.raised } else { palette.surface }));
    FillRect(dc, &item.rcItem, brush);
    DeleteObject(brush);
    let font = font();
    let old = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, rgb(if grey { palette.muted } else { palette.text }));
    let mut left = item.rcItem.left + px(PAD_X_PT);
    if let Some(ticked) = check {
        let side = px(BOX_PT);
        let top = item.rcItem.top + (item.rcItem.bottom - item.rcItem.top - side) / 2;
        let pen = CreatePen(PS_SOLID, (side / 9).max(1), rgb(palette.muted));
        let none = GetStockObject(NULL_BRUSH);
        let (old_pen, old_brush) = (SelectObject(dc, pen), SelectObject(dc, none));
        Rectangle(dc, left, top, left + side, top + side);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        if ticked {
            let pen = CreatePen(PS_SOLID, (side / 6).max(2), rgb(palette.tick));
            let old = SelectObject(dc, pen);
            MoveToEx(dc, left + side / 5, top + side / 2, std::ptr::null_mut());
            LineTo(dc, left + side * 2 / 5, top + side * 3 / 4);
            LineTo(dc, left + side * 4 / 5, top + side / 4);
            SelectObject(dc, old);
            DeleteObject(pen);
        }
        left += side + px(PAD_X_PT) / 2;
    }
    let mut place = RECT { left, ..item.rcItem };
    DrawTextW(dc, text.as_ptr(), text.len() as i32, &mut place, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    SelectObject(dc, old);
    DeleteObject(font);
}
