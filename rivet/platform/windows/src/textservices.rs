//! A text field that uses the Windows text engine without a window of its own.
//!
//! `msftedit.dll` offers the engine behind the system's edit controls as `ITextServices`, which
//! asks its host (`ITextHost`) for the few things it needs: a place to draw, the font, the caret
//! and the input method. The setup window is that host. Editing, selection, the clipboard, undo
//! and the Korean input method are done by the engine. This file forwards the window's key and
//! mouse messages to it and draws what it produces.

use std::cell::{Cell, UnsafeCell};
use std::ffi::c_void;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetDeviceCaps, GetSysColor, ReleaseDC,
    SelectObject, SetViewportOrgEx, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, LOGPIXELSY,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Input::Ime::{ImmGetContext, ImmReleaseContext, HIMC};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002_u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
/// The engine did not use a key message, so the window should handle it as usual.
const S_MSG_KEY_IGNORED: i32 = 0x0004_0210;

const TXTBIT_SAVESELECTION: u32 = 0x40;
const TXTBIT_AUTOWORDSEL: u32 = 0x80;
const TXTBIT_CHARFORMATCHANGE: u32 = 0x2_0000;
const TXTBIT_CLIENTRECTCHANGE: u32 = 0x10_0000;
const TXTBACK_TRANSPARENT: u32 = 0;
const DVASPECT_CONTENT: u32 = 1;
const TXTVIEW_ACTIVE: i32 = 0;

const CFM_BOLD: u32 = 0x1;
const CFM_ITALIC: u32 = 0x2;
const CFM_UNDERLINE: u32 = 0x4;
const CFM_STRIKEOUT: u32 = 0x8;
const CFM_CHARSET: u32 = 0x0800_0000;
const CFM_FACE: u32 = 0x2000_0000;
const CFM_COLOR: u32 = 0x4000_0000;
const CFM_SIZE: u32 = 0x8000_0000;
const DEFAULT_CHARSET: u8 = 1;

const WM_SETTEXT: u32 = 0x000C;
const WM_GETTEXT: u32 = 0x000D;
const WM_GETTEXTLENGTH: u32 = 0x000E;
const EM_SETSEL: u32 = 0x00B1;

/// Timers the engine asks for are numbered from here, so the window can tell them from its own.
pub const TIMER_BASE: usize = 0x5300;
pub const TIMER_LAST: usize = TIMER_BASE + 0xff;

const COLOR_WINDOW: i32 = 5;
const COLOR_WINDOWTEXT: i32 = 8;
const COLOR_HIGHLIGHT: i32 = 13;
const COLOR_HIGHLIGHTTEXT: i32 = 14;

fn guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> GUID {
    GUID { data1, data2, data3, data4 }
}

fn same(left: &GUID, right: &GUID) -> bool {
    left.data1 == right.data1 && left.data2 == right.data2 && left.data3 == right.data3 && left.data4 == right.data4
}

#[repr(C)]
struct CharFormat {
    size: u32,
    mask: u32,
    effects: u32,
    height: i32,
    offset: i32,
    color: u32,
    charset: u8,
    pitch_and_family: u8,
    face: [u16; 32],
}

#[repr(C)]
struct ParaFormat {
    size: u32,
    mask: u32,
    numbering: u16,
    reserved: u16,
    start_indent: i32,
    right_indent: i32,
    offset: i32,
    alignment: u16,
    tab_count: i16,
    tabs: [i32; 32],
}

type Unused = *const c_void;

/// `ITextHost`, in the order of its methods.
#[repr(C)]
struct HostVtbl {
    query_interface: unsafe extern "system" fn(*mut Host, *const GUID, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut Host) -> u32,
    release: unsafe extern "system" fn(*mut Host) -> u32,
    get_dc: unsafe extern "system" fn(*mut Host) -> HDC,
    release_dc: unsafe extern "system" fn(*mut Host, HDC) -> i32,
    show_scroll_bar: unsafe extern "system" fn(*mut Host, i32, i32) -> i32,
    enable_scroll_bar: unsafe extern "system" fn(*mut Host, i32, i32) -> i32,
    set_scroll_range: unsafe extern "system" fn(*mut Host, i32, i32, i32, i32) -> i32,
    set_scroll_pos: unsafe extern "system" fn(*mut Host, i32, i32, i32) -> i32,
    invalidate_rect: unsafe extern "system" fn(*mut Host, *const RECT, i32),
    view_change: unsafe extern "system" fn(*mut Host, i32),
    create_caret: unsafe extern "system" fn(*mut Host, Unused, i32, i32) -> i32,
    show_caret: unsafe extern "system" fn(*mut Host, i32) -> i32,
    set_caret_pos: unsafe extern "system" fn(*mut Host, i32, i32) -> i32,
    set_timer: unsafe extern "system" fn(*mut Host, u32, u32) -> i32,
    kill_timer: unsafe extern "system" fn(*mut Host, u32),
    scroll_window_ex: unsafe extern "system" fn(*mut Host, i32, i32, *const RECT, *const RECT, Unused, *mut RECT, u32),
    set_capture: unsafe extern "system" fn(*mut Host, i32),
    set_focus: unsafe extern "system" fn(*mut Host),
    set_cursor: unsafe extern "system" fn(*mut Host, Unused, i32),
    screen_to_client: unsafe extern "system" fn(*mut Host, *mut POINT) -> i32,
    client_to_screen: unsafe extern "system" fn(*mut Host, *mut POINT) -> i32,
    activate: unsafe extern "system" fn(*mut Host, *mut i32) -> i32,
    deactivate: unsafe extern "system" fn(*mut Host, i32) -> i32,
    get_client_rect: unsafe extern "system" fn(*mut Host, *mut RECT) -> i32,
    get_view_inset: unsafe extern "system" fn(*mut Host, *mut RECT) -> i32,
    get_char_format: unsafe extern "system" fn(*mut Host, *mut *const CharFormat) -> i32,
    get_para_format: unsafe extern "system" fn(*mut Host, *mut *const ParaFormat) -> i32,
    get_sys_color: unsafe extern "system" fn(*mut Host, i32) -> u32,
    get_back_style: unsafe extern "system" fn(*mut Host, *mut u32) -> i32,
    get_max_length: unsafe extern "system" fn(*mut Host, *mut u32) -> i32,
    get_scroll_bars: unsafe extern "system" fn(*mut Host, *mut u32) -> i32,
    get_password_char: unsafe extern "system" fn(*mut Host, *mut u16) -> i32,
    get_accelerator_pos: unsafe extern "system" fn(*mut Host, *mut i32) -> i32,
    get_extent: unsafe extern "system" fn(*mut Host, *mut SIZE) -> i32,
    on_char_format_change: unsafe extern "system" fn(*mut Host, *const CharFormat) -> i32,
    on_para_format_change: unsafe extern "system" fn(*mut Host, *const ParaFormat) -> i32,
    get_property_bits: unsafe extern "system" fn(*mut Host, u32, *mut u32) -> i32,
    notify: unsafe extern "system" fn(*mut Host, u32, *mut c_void) -> i32,
    imm_get_context: unsafe extern "system" fn(*mut Host) -> HIMC,
    imm_release_context: unsafe extern "system" fn(*mut Host, HIMC),
    get_selection_bar_width: unsafe extern "system" fn(*mut Host, *mut i32) -> i32,
}

/// What the engine asks of its host. The first field is the table pointer a COM object starts with.
#[repr(C)]
struct Host {
    vtbl: *const HostVtbl,
    window: HWND,
    refs: Cell<u32>,
    /// The field's rectangle in the window's client pixels.
    client: Cell<RECT>,
    format: UnsafeCell<CharFormat>,
    paragraph: UnsafeCell<ParaFormat>,
    caret: Cell<(i32, i32, i32, i32)>,
    caret_shown: Cell<bool>,
    changed: Cell<bool>,
    max_length: Cell<u32>,
}

unsafe extern "system" fn host_query(this: *mut Host, id: *const GUID, out: *mut *mut c_void) -> i32 {
    let unknown = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    let text_host = guid(0x13E6_70F4, 0x1A5A, 0x11cf, [0xAB, 0xEB, 0x00, 0xAA, 0x00, 0xB6, 0x5E, 0xA1]);
    if same(&*id, &unknown) || same(&*id, &text_host) {
        (*this).refs.set((*this).refs.get() + 1);
        *out = this.cast();
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "system" fn host_add_ref(this: *mut Host) -> u32 {
    (*this).refs.set((*this).refs.get() + 1);
    (*this).refs.get()
}
unsafe extern "system" fn host_release(this: *mut Host) -> u32 {
    let left = (*this).refs.get().saturating_sub(1);
    (*this).refs.set(left);
    left
}
unsafe extern "system" fn host_get_dc(this: *mut Host) -> HDC {
    GetDC((*this).window)
}
unsafe extern "system" fn host_release_dc(this: *mut Host, dc: HDC) -> i32 {
    ReleaseDC((*this).window, dc)
}
unsafe extern "system" fn host_false2(_: *mut Host, _: i32, _: i32) -> i32 {
    0
}
unsafe extern "system" fn host_false4(_: *mut Host, _: i32, _: i32, _: i32, _: i32) -> i32 {
    0
}
unsafe extern "system" fn host_false3(_: *mut Host, _: i32, _: i32, _: i32) -> i32 {
    0
}
unsafe extern "system" fn host_invalidate(this: *mut Host, _: *const RECT, _: i32) {
    (*this).changed.set(true);
}
unsafe extern "system" fn host_view_change(this: *mut Host, _: i32) {
    (*this).changed.set(true);
}
unsafe extern "system" fn host_create_caret(this: *mut Host, _: Unused, width: i32, height: i32) -> i32 {
    let (x, y, _, _) = (*this).caret.get();
    (*this).caret.set((x, y, width, height));
    1
}
unsafe extern "system" fn host_show_caret(this: *mut Host, show: i32) -> i32 {
    (*this).caret_shown.set(show != 0);
    (*this).changed.set(true);
    1
}
unsafe extern "system" fn host_set_caret_pos(this: *mut Host, x: i32, y: i32) -> i32 {
    let (_, _, width, height) = (*this).caret.get();
    (*this).caret.set((x, y, width, height));
    (*this).changed.set(true);
    1
}
unsafe extern "system" fn host_set_timer(this: *mut Host, id: u32, timeout: u32) -> i32 {
    SetTimer((*this).window, TIMER_BASE + (id as usize & 0xff), timeout, None);
    1
}
unsafe extern "system" fn host_kill_timer(this: *mut Host, id: u32) {
    KillTimer((*this).window, TIMER_BASE + (id as usize & 0xff));
}
unsafe extern "system" fn host_scroll(_: *mut Host, _: i32, _: i32, _: *const RECT, _: *const RECT, _: Unused, _: *mut RECT, _: u32) {}
unsafe extern "system" fn host_set_capture(this: *mut Host, capture: i32) {
    if capture != 0 {
        SetCapture((*this).window);
    } else {
        ReleaseCapture();
    }
}
unsafe extern "system" fn host_set_focus(_: *mut Host) {}
unsafe extern "system" fn host_set_cursor(_: *mut Host, _: Unused, _: i32) {}
unsafe extern "system" fn host_screen_to_client(this: *mut Host, point: *mut POINT) -> i32 {
    ScreenToClient((*this).window, point)
}
unsafe extern "system" fn host_client_to_screen(this: *mut Host, point: *mut POINT) -> i32 {
    ClientToScreen((*this).window, point)
}
unsafe extern "system" fn host_activate(_: *mut Host, old: *mut i32) -> i32 {
    if !old.is_null() {
        *old = 0;
    }
    S_OK
}
unsafe extern "system" fn host_deactivate(_: *mut Host, _: i32) -> i32 {
    S_OK
}
unsafe extern "system" fn host_client_rect(this: *mut Host, out: *mut RECT) -> i32 {
    *out = (*this).client.get();
    S_OK
}
unsafe extern "system" fn host_view_inset(_: *mut Host, out: *mut RECT) -> i32 {
    *out = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    S_OK
}
unsafe extern "system" fn host_char_format(this: *mut Host, out: *mut *const CharFormat) -> i32 {
    *out = (*this).format.get();
    S_OK
}
unsafe extern "system" fn host_para_format(this: *mut Host, out: *mut *const ParaFormat) -> i32 {
    *out = (*this).paragraph.get();
    S_OK
}
unsafe extern "system" fn host_sys_color(_: *mut Host, index: i32) -> u32 {
    // Rivet's own colours for the selection and the text, as COLORREF (blue, green, red).
    match index {
        COLOR_WINDOW => 0x00ff_ffff,
        COLOR_WINDOWTEXT => 0x0024_201f,
        COLOR_HIGHLIGHT => 0x0052_8ad0,
        COLOR_HIGHLIGHTTEXT => 0x00ff_ffff,
        _ => GetSysColor(index),
    }
}
unsafe extern "system" fn host_back_style(_: *mut Host, out: *mut u32) -> i32 {
    *out = TXTBACK_TRANSPARENT;
    S_OK
}
unsafe extern "system" fn host_max_length(this: *mut Host, out: *mut u32) -> i32 {
    *out = (*this).max_length.get();
    S_OK
}
unsafe extern "system" fn host_scroll_bars(_: *mut Host, out: *mut u32) -> i32 {
    *out = 0;
    S_OK
}
unsafe extern "system" fn host_password(_: *mut Host, _: *mut u16) -> i32 {
    S_FALSE
}
unsafe extern "system" fn host_accelerator(_: *mut Host, out: *mut i32) -> i32 {
    *out = -1;
    S_OK
}
unsafe extern "system" fn host_extent(_: *mut Host, _: *mut SIZE) -> i32 {
    E_NOTIMPL
}
unsafe extern "system" fn host_char_changed(_: *mut Host, _: *const CharFormat) -> i32 {
    S_OK
}
unsafe extern "system" fn host_para_changed(_: *mut Host, _: *const ParaFormat) -> i32 {
    S_OK
}
unsafe extern "system" fn host_property_bits(_: *mut Host, mask: u32, out: *mut u32) -> i32 {
    // Plain text on one line, with no wrapping, beeping or scroll bars.
    *out = (TXTBIT_SAVESELECTION | TXTBIT_AUTOWORDSEL) & mask;
    S_OK
}
unsafe extern "system" fn host_notify(this: *mut Host, _: u32, _: *mut c_void) -> i32 {
    (*this).changed.set(true);
    S_OK
}
unsafe extern "system" fn host_imm_get_context(this: *mut Host) -> HIMC {
    ImmGetContext((*this).window)
}
unsafe extern "system" fn host_imm_release_context(this: *mut Host, context: HIMC) {
    ImmReleaseContext((*this).window, context);
}
unsafe extern "system" fn host_selection_bar(_: *mut Host, out: *mut i32) -> i32 {
    *out = 0;
    S_OK
}

static HOST_VTBL: HostVtbl = HostVtbl {
    query_interface: host_query,
    add_ref: host_add_ref,
    release: host_release,
    get_dc: host_get_dc,
    release_dc: host_release_dc,
    show_scroll_bar: host_false2,
    enable_scroll_bar: host_false2,
    set_scroll_range: host_false4,
    set_scroll_pos: host_false3,
    invalidate_rect: host_invalidate,
    view_change: host_view_change,
    create_caret: host_create_caret,
    show_caret: host_show_caret,
    set_caret_pos: host_set_caret_pos,
    set_timer: host_set_timer,
    kill_timer: host_kill_timer,
    scroll_window_ex: host_scroll,
    set_capture: host_set_capture,
    set_focus: host_set_focus,
    set_cursor: host_set_cursor,
    screen_to_client: host_screen_to_client,
    client_to_screen: host_client_to_screen,
    activate: host_activate,
    deactivate: host_deactivate,
    get_client_rect: host_client_rect,
    get_view_inset: host_view_inset,
    get_char_format: host_char_format,
    get_para_format: host_para_format,
    get_sys_color: host_sys_color,
    get_back_style: host_back_style,
    get_max_length: host_max_length,
    get_scroll_bars: host_scroll_bars,
    get_password_char: host_password,
    get_accelerator_pos: host_accelerator,
    get_extent: host_extent,
    on_char_format_change: host_char_changed,
    on_para_format_change: host_para_changed,
    get_property_bits: host_property_bits,
    notify: host_notify,
    imm_get_context: host_imm_get_context,
    imm_release_context: host_imm_release_context,
    get_selection_bar_width: host_selection_bar,
};

/// `ITextServices`, in the order of its methods.
#[repr(C)]
struct ServicesVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    send_message: unsafe extern "system" fn(*mut c_void, u32, usize, isize, *mut isize) -> i32,
    draw: unsafe extern "system" fn(
        *mut c_void,
        u32,
        i32,
        *mut c_void,
        *mut c_void,
        HDC,
        HDC,
        *const RECT,
        *const RECT,
        *mut RECT,
        Unused,
        u32,
        i32,
    ) -> i32,
    get_hscroll: Unused,
    get_vscroll: Unused,
    on_set_cursor: Unused,
    query_hit_point: Unused,
    in_place_activate: unsafe extern "system" fn(*mut c_void, *const RECT) -> i32,
    in_place_deactivate: unsafe extern "system" fn(*mut c_void) -> i32,
    ui_activate: unsafe extern "system" fn(*mut c_void) -> i32,
    ui_deactivate: unsafe extern "system" fn(*mut c_void) -> i32,
    get_text: Unused,
    set_text: Unused,
    cur_target_x: Unused,
    baseline_pos: Unused,
    natural_size: Unused,
    drop_target: Unused,
    property_bits_change: unsafe extern "system" fn(*mut c_void, u32, u32) -> i32,
    cached_size: Unused,
}

#[repr(C)]
struct Services {
    vtbl: *const ServicesVtbl,
}

/// The pixels of the field drawn at its place: blue, green, red and an opaque alpha.
pub struct Pixels<'a> {
    pub width: i32,
    pub height: i32,
    pub bytes: &'a [u8],
}

struct Surface {
    dc: HDC,
    bitmap: *mut c_void,
    previous: *mut c_void,
    bits: *mut u8,
    width: i32,
    height: i32,
}

pub struct Field {
    host: *mut Host,
    services: *mut Services,
    surface: Option<Surface>,
}

type CreateTextServices = unsafe extern "system" fn(*mut c_void, *mut Host, *mut *mut c_void) -> i32;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

impl Field {
    /// Starts the engine for `window`. `face` is the family GDI knows, and `rgb` the text colour.
    pub unsafe fn new(window: HWND, face: &str, rgb: u32) -> Result<Self, String> {
        let create: CreateTextServices = ["msftedit.dll", "riched20.dll"]
            .iter()
            .find_map(|library| {
                let module = LoadLibraryW(wide(library).as_ptr());
                if module.is_null() {
                    return None;
                }
                let found = GetProcAddress(module, c"CreateTextServices".as_ptr().cast())?;
                Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateTextServices>(found))
            })
            .ok_or("the Windows text engine is not available")?;
        let mut name = [0u16; 32];
        for (slot, unit) in name.iter_mut().zip(face.encode_utf16().take(31)) {
            *slot = unit;
        }
        let host = Box::into_raw(Box::new(Host {
            vtbl: &HOST_VTBL,
            window,
            refs: Cell::new(1),
            client: Cell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }),
            format: UnsafeCell::new(CharFormat {
                size: std::mem::size_of::<CharFormat>() as u32,
                mask: CFM_SIZE | CFM_COLOR | CFM_FACE | CFM_CHARSET | CFM_BOLD | CFM_ITALIC | CFM_UNDERLINE | CFM_STRIKEOUT,
                effects: 0,
                height: 200,
                offset: 0,
                color: ((rgb & 0xff) << 16) | (rgb & 0xff00) | (rgb >> 16),
                charset: DEFAULT_CHARSET,
                pitch_and_family: 0,
                face: name,
            }),
            paragraph: UnsafeCell::new(ParaFormat {
                size: std::mem::size_of::<ParaFormat>() as u32,
                mask: 0,
                numbering: 0,
                reserved: 0,
                start_indent: 0,
                right_indent: 0,
                offset: 0,
                alignment: 1,
                tab_count: 0,
                tabs: [0; 32],
            }),
            caret: Cell::new((0, 0, 1, 16)),
            caret_shown: Cell::new(false),
            changed: Cell::new(false),
            max_length: Cell::new(64),
        }));
        let mut unknown: *mut c_void = std::ptr::null_mut();
        let status = create(std::ptr::null_mut(), host, &mut unknown);
        if status < 0 || unknown.is_null() {
            drop(Box::from_raw(host));
            return Err(format!("the Windows text engine did not start ({status:#x})"));
        }
        let id = guid(0x8d33_f740, 0xcf58, 0x11ce, [0xa8, 0x9d, 0x00, 0xaa, 0x00, 0x6c, 0xad, 0xc5]);
        let unknown_table = *(unknown as *mut *const ServicesVtbl);
        let mut services: *mut c_void = std::ptr::null_mut();
        let status = ((*unknown_table).query_interface)(unknown, &id, &mut services);
        ((*unknown_table).release)(unknown);
        if status < 0 || services.is_null() {
            drop(Box::from_raw(host));
            return Err(format!("the Windows text engine has no text services ({status:#x})"));
        }
        Ok(Self { host, services: services.cast(), surface: None })
    }

    fn table(&self) -> &ServicesVtbl {
        unsafe { &*(*self.services).vtbl }
    }

    /// Puts the field at `client`, in the window's client pixels, with text of `size_dip` at `dpi`.
    pub unsafe fn place(&mut self, client: RECT, size_dip: f32, dpi: u32) {
        let host = &*self.host;
        let changed = {
            let old = host.client.get();
            old.left != client.left || old.top != client.top || old.right != client.right || old.bottom != client.bottom
        };
        host.client.set(client);
        // Twips are device independent, but the engine turns them into pixels with the logical
        // resolution of the device it draws to, which is not the monitor's.
        let device = self.surface_dc();
        let logical = GetDeviceCaps(device, LOGPIXELSY as i32).max(1) as f32;
        let wanted = (size_dip * dpi as f32 / 96.0 * 1440.0 / logical).round() as i32;
        let format = &mut *host.format.get();
        let size_changed = format.height != wanted;
        format.height = wanted;
        let mut mask = 0;
        if changed {
            mask |= TXTBIT_CLIENTRECTCHANGE;
        }
        if size_changed {
            mask |= TXTBIT_CHARFORMATCHANGE;
        }
        if mask != 0 {
            (self.table().property_bits_change)(self.services.cast(), mask, mask);
        }
    }

    /// Makes the field live: the engine starts showing the caret and selection.
    pub unsafe fn activate(&mut self) {
        let client = (*self.host).client.get();
        (self.table().in_place_activate)(self.services.cast(), &client);
        (self.table().ui_activate)(self.services.cast());
        self.send(0x0007, 0, 0);
    }

    /// Hands a window message to the engine. `None` means it did not use a key message, so the
    /// window should treat the message as usual.
    pub fn send(&self, message: u32, wparam: usize, lparam: isize) -> Option<isize> {
        let mut result = 0isize;
        let status = unsafe { (self.table().send_message)(self.services.cast(), message, wparam, lparam, &mut result) };
        (status != S_MSG_KEY_IGNORED).then_some(result)
    }

    pub fn text(&self) -> String {
        let length = self.send(WM_GETTEXTLENGTH, 0, 0).unwrap_or(0).max(0) as usize;
        let mut buffer = vec![0u16; length + 2];
        let copied = self.send(WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize).unwrap_or(0).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied.min(buffer.len())])
    }

    pub fn set_text(&self, text: &str) {
        let text = wide(text);
        self.send(WM_SETTEXT, 0, text.as_ptr() as isize);
    }

    /// The most characters the field holds. An address can be much longer than a name.
    pub fn set_max_length(&self, length: u32) {
        unsafe { (*self.host).max_length.set(length) };
        const TXTBIT_MAXLENGTHCHANGE: u32 = 0x8000;
        unsafe { (self.table().property_bits_change)(self.services.cast(), TXTBIT_MAXLENGTHCHANGE, TXTBIT_MAXLENGTHCHANGE) };
    }

    pub fn select_all(&self) {
        self.send(EM_SETSEL, 0, -1);
    }

    /// Whether the engine asked for a redraw since the last call.
    pub fn take_changed(&self) -> bool {
        unsafe { (*self.host).changed.replace(false) }
    }

    /// The caret in the window's client pixels, when the engine shows one.
    pub fn caret(&self) -> Option<RECT> {
        let host = unsafe { &*self.host };
        let (x, y, width, height) = host.caret.get();
        host.caret_shown.get().then_some(RECT { left: x, top: y, right: x + width.max(1), bottom: y + height })
    }

    unsafe fn surface_dc(&mut self) -> HDC {
        if self.surface.is_none() {
            self.surface = Some(Self::make_surface(1, 1));
        }
        self.surface.as_ref().map_or(std::ptr::null_mut(), |surface| surface.dc)
    }

    unsafe fn make_surface(width: i32, height: i32) -> Surface {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let previous = SelectObject(dc, bitmap);
        Surface { dc, bitmap, previous, bits: bits.cast(), width, height }
    }

    /// Draws the field over `ground` (a colour as 0xRRGGBB) and returns its pixels.
    pub unsafe fn render(&mut self, ground: u32) -> Option<Pixels<'_>> {
        let client = (*self.host).client.get();
        let (width, height) = (client.right - client.left, client.bottom - client.top);
        if width <= 0 || height <= 0 {
            return None;
        }
        if self.surface.as_ref().map_or(true, |surface| surface.width != width || surface.height != height) {
            if let Some(old) = self.surface.take() {
                Self::free_surface(old);
            }
            self.surface = Some(Self::make_surface(width, height));
        }
        let surface = self.surface.as_ref()?;
        if surface.bits.is_null() {
            return None;
        }
        let count = (width * height) as usize;
        let pixels = std::slice::from_raw_parts_mut(surface.bits as *mut u32, count);
        pixels.fill(0xff00_0000 | ground);
        SetViewportOrgEx(surface.dc, -client.left, -client.top, std::ptr::null_mut());
        (self.table().draw)(
            self.services.cast(),
            DVASPECT_CONTENT,
            -1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            surface.dc,
            std::ptr::null_mut(),
            &client,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
            TXTVIEW_ACTIVE,
        );
        for pixel in pixels.iter_mut() {
            *pixel |= 0xff00_0000;
        }
        Some(Pixels { width, height, bytes: std::slice::from_raw_parts(surface.bits, count * 4) })
    }

    unsafe fn free_surface(surface: Surface) {
        SelectObject(surface.dc, surface.previous);
        DeleteObject(surface.bitmap);
        DeleteDC(surface.dc);
    }
}

impl Drop for Field {
    fn drop(&mut self) {
        unsafe {
            (self.table().in_place_deactivate)(self.services.cast());
            (self.table().release)(self.services.cast());
            if let Some(surface) = self.surface.take() {
                Self::free_surface(surface);
            }
            drop(Box::from_raw(self.host));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};

    /// A window nobody sees, which the engine needs only as the owner of its input method context.
    unsafe fn hidden_window() -> HWND {
        CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            windows_sys::w!(""),
            WS_POPUP,
            0,
            0,
            10,
            10,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    }

    #[test]
    fn nfr5_the_windows_text_engine_edits_korean_text_without_rivet_editing_keys() {
        unsafe {
            let window = hidden_window();
            let mut field = Field::new(window, "Segoe UI", 0x1f2024).expect("the text engine starts");
            field.place(RECT { left: 0, top: 0, right: 200, bottom: 24 }, 14.0, 96);
            field.activate();
            field.set_text("Default");
            assert_eq!(field.text(), "Default");
            // Typing replaces a selection, as in any edit control: the engine does it, not Rivet.
            field.select_all();
            for unit in "작업 Work".encode_utf16() {
                field.send(0x0102, unit as usize, 1);
            }
            assert_eq!(field.text(), "작업 Work");
            // Backspace is a key the engine edits with.
            field.send(0x0100, 0x08, 1);
            assert_eq!(field.text(), "작업 Wor");
            let pixels = field.render(0xffffff).expect("the field draws");
            assert_eq!((pixels.width, pixels.height), (200, 24));
            assert!(pixels.bytes.chunks_exact(4).any(|pixel| pixel[..3] != [0xff, 0xff, 0xff]), "the text is drawn");
            drop(field);
            DestroyWindow(window);
        }
    }
}
