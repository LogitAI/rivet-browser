//! Makes Chromium's own frame layout leave room for the Rivet appbar.
//!
//! Chromium's Windows browser frame asks its frame view where the client view goes. Rivet
//! replaces that one virtual function in the frame view's vtable. For windows that carry a Rivet
//! appbar it moves the client view right by the appbar width. Chromium then lays out, paints,
//! hit-tests and positions menus, IME windows and accessibility bounds for the narrower view
//! itself, so nothing has to be translated after the fact.
//!
//! The vtable layout is private to each Chromium build. Rivet only patches builds whose module
//! contents match an entry of the supported-build table exactly, and leaves every other build
//! untouched.

use std::ffi::c_void;
use std::sync::OnceLock;

use rivet_platform_windows::chrome_build::{self, ChromeBuild};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

/// `gfx::Rect`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Moves the client view right by `reserved` device-independent pixels. Views coordinates are
/// already scaled for the monitor, so the reservation does not depend on DPI.
pub fn reserve_leading(bounds: Rect, reserved: i32) -> Rect {
    let reserved = reserved.clamp(0, (bounds.width - 1).max(0));
    Rect {
        x: bounds.x + reserved,
        width: bounds.width - reserved,
        ..bounds
    }
}

/// Experiment: whether the page is laid out as the shell study's panel, below a taller top row
/// and inside a gap on the right and the bottom.
pub static PANEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// How much taller the top row is than the browser's title bar, and the gap, in DIPs.
pub const PANEL_TOP: i32 = 12;
pub const PANEL_GAP: i32 = 8;

fn inset_panel(bounds: Rect) -> Rect {
    Rect {
        x: bounds.x,
        y: bounds.y + PANEL_GAP,
        width: (bounds.width - PANEL_GAP).max(1),
        height: (bounds.height - 2 * PANEL_GAP).max(1),
    }
}

type ClientViewBounds = unsafe extern "system" fn(*const c_void, *mut Rect) -> *mut Rect;
type HwndForView = unsafe extern "system" fn(*const c_void) -> HWND;

/// Decides the width to reserve for a window from the client-view bounds the browser chose, or
/// returns `None` when the window carries no Rivet appbar.
pub type Reserve = fn(HWND, Rect) -> Option<i32>;

struct Hook {
    original: ClientViewBounds,
    hwnd_for_view: HwndForView,
    reserved: Reserve,
}

static HOOK: OnceLock<Result<Hook, String>> = OnceLock::new();

/// Experiment, Chrome 156.0.8078.48 only. The frame view paints its title bar with colours from
/// a colour provider that other windows of the profile share. Rivet wraps the frame view's
/// `OnPaint` and swaps the frame colours in that provider only while a Rivet window paints.
///
/// `views::View::GetColorProvider` returns the provider. It keeps generated colours in a tree
/// whose nodes hold the colour id at `+0x20` and the colour at `+0x24`.
const ON_PAINT_SLOT: usize = 89;
const ON_PAINT: usize = 0x011B_5610;
const GET_COLOR_PROVIDER: usize = 0x04AA_27D0;
const DRAW_COLOR: usize = 0x036B_AD80;
/// The active and inactive frame colour ids.
const FRAME_IDS: [i32; 2] = [0x13C, 0x140];
type OnPaint = unsafe extern "system" fn(*const c_void, *const c_void);
type GetProvider = unsafe extern "system" fn(*const c_void) -> *const u8;
type DrawColor = unsafe extern "system" fn(*const c_void, u32);
static PAINT: OnceLock<(OnPaint, GetProvider, DrawColor, Paint)> = OnceLock::new();

unsafe fn colour_slot(provider: *const u8, id: i32) -> Option<*mut u32> {
    let mut node = *(provider.add(0x18) as *const *mut u8);
    while !node.is_null() {
        let key = *(node.add(0x20) as *const i32);
        if key == id {
            return Some(node.add(0x24) as *mut u32);
        }
        node = if id < key {
            *(node as *const *mut u8)
        } else {
            *(node.add(8) as *const *mut u8)
        };
    }
    None
}

unsafe extern "system" fn on_paint(view: *const c_void, canvas: *const c_void) {
    let (original, provider_of, draw_color, paint) =
        PAINT.get().expect("patched after it is stored");
    let colour = match HOOK.get() {
        Some(Ok(hook)) => paint((hook.hwnd_for_view)(view)),
        _ => None,
    };
    let provider = provider_of(view);
    let Some(colour) = colour.filter(|_| !provider.is_null()) else {
        return original(view, canvas);
    };
    draw_color(canvas, colour);
    let saved: Vec<(*mut u32, u32)> = FRAME_IDS
        .iter()
        .filter_map(|id| colour_slot(provider, *id))
        .map(|slot| (slot, slot.read()))
        .collect();
    for (slot, _) in &saved {
        slot.write(colour);
    }
    original(view, canvas);
    for (slot, value) in saved {
        slot.write(value);
    }
}

/// Experiment: lets Rivet choose the title bar colour of its own windows only.
pub unsafe fn install_frame_paint(paint: Paint) -> Result<(), String> {
    if PAINT.get().is_some() {
        return Ok(());
    }
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    let slot = (base + 0x0FE7_3E28 + ON_PAINT_SLOT * 8) as *mut usize;
    if slot.read() != base + ON_PAINT {
        return Err("this browser build has no recorded frame paint".into());
    }
    if std::slice::from_raw_parts((base + ON_PAINT) as *const u8, 8)
        != [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]
        || std::slice::from_raw_parts((base + DRAW_COLOR) as *const u8, 6)
            != [0x48, 0x83, 0xec, 0x38, 0x48, 0x8b]
        || std::slice::from_raw_parts((base + GET_COLOR_PROVIDER) as *const u8, 6)
            != [0x56, 0x57, 0x48, 0x83, 0xec, 0x78]
    {
        return Err("this browser build has no recorded frame paint helpers".into());
    }
    let _ = PAINT.set((
        std::mem::transmute::<usize, OnPaint>(slot.read()),
        std::mem::transmute::<usize, GetProvider>(base + GET_COLOR_PROVIDER),
        std::mem::transmute::<usize, DrawColor>(base + DRAW_COLOR),
        paint,
    ));
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(on_paint as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// Decides the title bar colour of a window as `0xAARRGGBB`, or `None` to keep the browser's.
pub type Paint = fn(HWND) -> Option<u32>;

unsafe extern "system" fn client_view_bounds(view: *const c_void, out: *mut Rect) -> *mut Rect {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    let result = (hook.original)(view, out);
    if let Some(reserved) = (hook.reserved)((hook.hwnd_for_view)(view), *out) {
        *out = reserve_leading(*out, reserved);
        if PANEL.load(std::sync::atomic::Ordering::Relaxed) {
            *out = inset_panel(*out);
        }
    }
    result
}

/// Installs the layout hook once per browser process. Returns why it cannot when the build is
/// not recognised, so Rivet refuses to attach rather than guess.
pub unsafe fn install(reserved: Reserve) -> Result<(), String> {
    HOOK.get_or_init(|| patch(reserved))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

unsafe fn patch(reserved: Reserve) -> Result<Hook, String> {
    let builds = chrome_build::builds(chrome_build::TABLE);
    let (build, base) = builds
        .iter()
        .find_map(|build| {
            let module = wide(&build.module);
            let base = GetModuleHandleW(module.as_ptr()) as usize;
            (base != 0 && matches(base, build)).then_some((build, base))
        })
        .ok_or("this browser build is not supported yet")?;
    let slot = (base + build.frame_view_vtable as usize + build.client_view_slot as usize * 8)
        as *mut usize;
    let hook = Hook {
        original: std::mem::transmute::<usize, ClientViewBounds>(slot.read()),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(
            base + build.hwnd_for_view as usize,
        ),
        reserved,
    };
    // Layout runs on this UI thread only, so no call can reach the new slot before HOOK holds
    // this value.
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(client_view_bounds as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    log(&format!(
        "rivet: laying out {} {}",
        build.browser, build.version
    ));
    Ok(hook)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(text.as_ptr()) };
}

/// Checks that the loaded module is the recorded build: the vtable slot must still point at the
/// recorded function, and the helper must start with the recorded instructions.
unsafe fn matches(base: usize, build: &ChromeBuild) -> bool {
    let Some(expected) = chrome_build::hex(&build.hwnd_for_view_prologue) else {
        return false;
    };
    let slot = (base + build.frame_view_vtable as usize + build.client_view_slot as usize * 8)
        as *const usize;
    let prologue = std::slice::from_raw_parts(
        (base + build.hwnd_for_view as usize) as *const u8,
        expected.len(),
    );
    slot.read() == base + build.client_view_bounds as usize && prologue == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_caption_fills_the_panel_top_row() {
        assert_eq!(panel_caption_bounds([900, 0, 138, 32], 44), [900, 8, 138, 44]);
    }

    #[test]
    fn fr12_panel_reserves_the_frame_gap() {
        assert_eq!(
            inset_panel(Rect {
                x: 220,
                y: 0,
                width: 980,
                height: 800
            }),
            Rect {
                x: 220,
                y: 8,
                width: 972,
                height: 784
            }
        );
    }

    #[test]
    fn fr2_client_view_moves_right_by_the_appbar_width() {
        let bounds = Rect {
            x: 0,
            y: 32,
            width: 1200,
            height: 800,
        };
        assert_eq!(
            reserve_leading(bounds, 292),
            Rect {
                x: 292,
                y: 32,
                width: 908,
                height: 800
            }
        );
    }

    #[test]
    fn fr2_narrow_client_view_keeps_one_pixel() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        assert_eq!(reserve_leading(bounds, 292).width, 1);
    }
}

/// Experiment, Chrome 156.0.8078.48 only: `WindowsCaptionButton::CalculatePreferredSize` in the
/// caption button's vtable. Rivet windows get caption buttons as tall as the shell study's top
/// row, so the native buttons sit centred in it.
const CAPTION_VTABLE: usize = 0x0FF0_25C8;
const CAPTION_SIZE_SLOT: usize = 74;
const CAPTION_SIZE: usize = 0x0313_2D90;
type PreferredSize =
    unsafe extern "system" fn(*const c_void, *mut [i32; 2], *const c_void) -> *mut [i32; 2];
/// Decides the caption button height of a window in DIPs, or `None` to keep the browser's.
pub type CaptionHeight = fn(HWND) -> Option<i32>;
static CAPTION: OnceLock<(PreferredSize, CaptionHeight)> = OnceLock::new();

unsafe extern "system" fn caption_size(
    button: *const c_void,
    out: *mut [i32; 2],
    bounds: *const c_void,
) -> *mut [i32; 2] {
    let (original, height) = CAPTION.get().expect("patched after it is stored");
    let result = original(button, out, bounds);
    if let Some(Ok(hook)) = HOOK.get() {
        if let Some(height) = height((hook.hwnd_for_view)(button)) {
            (*out)[1] = height;
        }
    }
    result
}

pub unsafe fn install_caption_height(height: CaptionHeight) -> Result<(), String> {
    if CAPTION.get().is_some() {
        return Ok(());
    }
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    let slot = (base + CAPTION_VTABLE + CAPTION_SIZE_SLOT * 8) as *mut usize;
    if slot.read() != base + CAPTION_SIZE {
        return Err("this browser build has no recorded caption button size".into());
    }
    let _ = CAPTION.set((
        std::mem::transmute::<usize, PreferredSize>(slot.read()),
        height,
    ));
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the caption button vtable".into());
    }
    slot.write(caption_size as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::GetTopInset` in the frame view's
/// vtable. It is the height of the browser's own title bar, which also sizes the caption buttons.
const TOP_INSET_SLOT: usize = 127;
const TOP_INSET: usize = 0x0A40_33E0;
type TopInset = unsafe extern "system" fn(*const c_void, bool) -> i32;
/// Extra title bar height in DIPs for a window, or `None` to keep the browser's.
pub type ExtraTop = fn(HWND) -> Option<i32>;
static TOP: OnceLock<(TopInset, ExtraTop)> = OnceLock::new();

unsafe extern "system" fn top_inset(view: *const c_void, restored: bool) -> i32 {
    let (original, extra) = TOP.get().expect("patched after it is stored");
    let inset = original(view, restored);
    match HOOK.get() {
        Some(Ok(hook)) => inset + extra((hook.hwnd_for_view)(view)).unwrap_or(0),
        _ => inset,
    }
}

pub unsafe fn install_top_inset(extra: ExtraTop) -> Result<(), String> {
    if TOP.get().is_some() {
        return Ok(());
    }
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    let slot = (base + 0x0FE7_3E28 + TOP_INSET_SLOT * 8) as *mut usize;
    if slot.read() != base + TOP_INSET {
        return Err("this browser build has no recorded top inset".into());
    }
    let _ = TOP.set((std::mem::transmute::<usize, TopInset>(slot.read()), extra));
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(top_inset as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::Layout` in the frame view's
/// vtable. It sizes the caption button container from the tab strip, not from the title bar
/// height, so the container is resized once the browser's own layout is done.
const LAYOUT_SLOT: usize = 23;
const LAYOUT: usize = 0x017A_E5E0;
/// `views::View::SetBoundsRect`, and where a frame view keeps its caption button container and a
/// view keeps its bounds.
const SET_BOUNDS_RECT: usize = 0x04F4_2540;
const CAPTION_CONTAINER_OFFSET: usize = 0x3C8;
const VIEW_BOUNDS_OFFSET: usize = 0x1C8;
type Layout = unsafe extern "system" fn(*mut c_void);
type SetBoundsRect = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);
static LAYOUT_HOOK: OnceLock<(Layout, SetBoundsRect, CaptionHeight)> = OnceLock::new();

fn panel_caption_bounds(bounds: [i32; 4], height: i32) -> [i32; 4] {
    [bounds[0], PANEL_GAP, bounds[2], height.max(0)]
}

unsafe extern "system" fn frame_layout(view: *mut c_void) {
    let (original, set_bounds, height) = LAYOUT_HOOK.get().expect("patched after it is stored");
    original(view);
    let Some(Ok(hook)) = HOOK.get() else { return };
    let Some(bottom) = height((hook.hwnd_for_view)(view)) else {
        return;
    };
    let container = *(view.cast::<u8>().add(CAPTION_CONTAINER_OFFSET) as *const *mut c_void);
    if container.is_null() {
        return;
    }
    let bounds = *(container.cast::<u8>().add(VIEW_BOUNDS_OFFSET) as *const [i32; 4]);
    let wanted = panel_caption_bounds(bounds, bottom);
    if wanted != bounds {
        set_bounds(container, &wanted);
    }
}

/// Makes the caption button container of a window reach `height` DIPs from the top of the frame.
pub unsafe fn install_caption_layout(height: CaptionHeight) -> Result<(), String> {
    if LAYOUT_HOOK.get().is_some() {
        return Ok(());
    }
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    let slot = (base + 0x0FE7_3E28 + LAYOUT_SLOT * 8) as *mut usize;
    if slot.read() != base + LAYOUT {
        return Err("this browser build has no recorded frame layout".into());
    }
    if std::slice::from_raw_parts((base + LAYOUT) as *const u8, 6)
        != [0x41, 0x56, 0x56, 0x57, 0x55, 0x53]
        || std::slice::from_raw_parts((base + SET_BOUNDS_RECT) as *const u8, 8)
            != [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]
    {
        return Err("this browser build has no recorded caption layout helpers".into());
    }
    let _ = LAYOUT_HOOK.set((
        std::mem::transmute::<usize, Layout>(slot.read()),
        std::mem::transmute::<usize, SetBoundsRect>(base + SET_BOUNDS_RECT),
        height,
    ));
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(frame_layout as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}
