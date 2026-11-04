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

use crate::chromium::image::{self, Image};
use crate::builds::Build;
use crate::feature::Needs;
use crate::targets::Target;

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::Layout {
    &crate::builds::get().layout
}

/// What the client view layout of the frame needs from the browser. These are the entry of the
/// frame table that names this build.
pub(crate) fn needs_layout(build: &Build) -> Needs {
    let entry = crate::builds::frame_rows()
        .iter()
        .find(|entry| entry.module == build.module && parse_version(&entry.version) == Some(build.version));
    match entry {
        Some(entry) => {
            let prologue: &'static [u8] =
                Box::leak(chrome_build::hex(&entry.hwnd_for_view_prologue).unwrap_or_default().into_boxed_slice());
            Needs::targets(vec![
                Target::new("frame view vtable", entry.frame_view_vtable as usize, &[]),
                Target::new("client view bounds", entry.client_view_bounds as usize, &[]),
                Target::new("hwnd for view", entry.hwnd_for_view as usize, prologue),
            ])
        }
        None => Needs { policy: Err("the frame table has no entry for this build"), targets: Vec::new() },
    }
}

/// What the colour of the title bar needs from the browser.
pub(crate) fn needs_colour(build: &Build) -> Needs {
    let row = &build.layout;
    Needs::checked(
        row.provider_verified,
        "the colour provider layout is not verified on this browser build",
        vec![
            Target::new("frame paint", row.on_paint, row.on_paint_prologue),
            Target::new("draw colour", row.draw_color, row.draw_color_prologue),
            Target::new("get colour provider", row.get_color_provider, row.get_color_provider_prologue),
        ],
    )
}

/// What the colours of the caption buttons need from the browser.
pub(crate) fn needs_caption_paint(build: &Build) -> Needs {
    let row = &build.layout;
    Needs::checked(
        row.provider_verified,
        "the colour provider layout is not verified on this browser build",
        vec![
            Target::new("caption button paint", row.caption_paint, row.caption_paint_prologue),
            Target::new("get colour provider", row.get_color_provider, row.get_color_provider_prologue),
        ],
    )
}

/// What the place of the caption buttons needs from the browser.
pub(crate) fn needs_caption_layout(build: &Build) -> Needs {
    let row = &build.layout;
    Needs::targets(vec![
        Target::new("frame view layout", row.layout, row.layout_prologue),
        Target::new("set bounds rect", row.set_bounds_rect, row.set_bounds_rect_prologue),
        Target::new("set paint to layer", row.set_layer, row.set_layer_prologue),
        Target::new("set rounded corners", row.set_radius, row.set_radius_prologue),
        Target::new("set masks", row.set_masks, row.set_masks_prologue),
        Target::new("stack at top", row.stack_at_top, row.stack_at_top_prologue),
        Target::new("set opaque", row.set_opaque, row.set_opaque_prologue),
    ])
}

/// What the height of the caption buttons needs from the browser.
pub(crate) fn needs_caption_height(build: &Build) -> Needs {
    Needs::targets(vec![Target::new("caption button size", build.layout.caption_size, &[])])
}

/// What the space above the top row needs from the browser.
pub(crate) fn needs_top_inset(build: &Build) -> Needs {
    Needs::targets(vec![Target::new("top inset", build.layout.top_inset, &[])])
}

fn recorded_image() -> Result<Image, String> {
    image::recorded().ok_or_else(|| "this browser build is not supported yet".to_owned())
}

/// Points the vtable slot at `rva` to `hook` after checking that the slot lies inside the image
/// and held `expected`.
unsafe fn replace_vtable_slot(
    image: &Image,
    rva: usize,
    expected: usize,
    hook: usize,
    what: &str,
) -> Result<(), String> {
    if !image.points_to(rva, expected) {
        return Err(format!("this browser build has no recorded {what}"));
    }
    let slot = image.at(rva, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err(format!("could not unprotect the {what}"));
    }
    slot.write(hook);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

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

/// The panel of a Rivet window from the client view bounds the browser chose. The browser keeps a
/// title bar above its client view, which a Rivet window does not have: Rivet's bar is the
/// panel's own top row. So the client view reaches the window's top edge before the gap is taken.
fn panel_from_client(bounds: Rect) -> Rect {
    inset_panel(lifted_client(bounds))
}

/// The client view's bounds lifted to the window's top edge, as `panel_from_client` places it.
fn lifted_client(bounds: Rect) -> Rect {
    Rect { y: 0, height: bounds.height + bounds.y, ..bounds }
}

/// The one offset in `fields` that holds `wanted`, read as rects at 4 byte steps. None when no
/// field or more than one holds it, since then the cached area cannot be told apart.
fn sole_rect_offset(fields: &[i32], wanted: Rect) -> Option<usize> {
    let wanted = [wanted.x, wanted.y, wanted.width, wanted.height];
    let mut found = (0..fields.len().saturating_sub(3)).filter(|at| fields[*at..*at + 4] == wanted);
    let first = found.next()?;
    found.next().is_none().then_some(first * 4)
}

/// Chromium's frame view keeps the client area it laid out and hands it to the browser view's
/// layout as the visual client area, which must contain the browser view. Rivet lifts the client
/// view to the window's top edge, so the kept area is lifted with it, or the browser stops on that
/// check. The area is the frame view's one field equal to the bounds it reported.
unsafe fn lift_cached_client_area(view: *const c_void, browser_chose: Rect) {
    const SPAN: usize = 0x500;
    if browser_chose.y <= 0 || !crate::chromium::memory::readable(view as usize, SPAN) {
        return;
    }
    let fields = std::slice::from_raw_parts(view.cast::<i32>(), SPAN / 4);
    if let Some(offset) = sole_rect_offset(fields, browser_chose) {
        *(view.cast::<u8>().add(offset) as *mut Rect) = lifted_client(browser_chose);
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

type OnPaint = unsafe extern "system" fn(*const c_void, *const c_void);
type GetProvider = unsafe extern "system" fn(*const c_void) -> *const u8;
type DrawColor = unsafe extern "system" fn(*const c_void, u32);
static PAINT: OnceLock<(OnPaint, GetProvider, DrawColor, Paint)> = OnceLock::new();
/// Whether `length` bytes at `address` are committed, readable and writable memory, and the address
/// is pointer aligned.
fn readable(address: usize, length: usize) -> bool {
    use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT};
    if address < 0x10000 || address % 8 != 0 || address > 0x7FFF_FFFF_0000 {
        return false;
    }
    let Some(last) = address.checked_add(length) else { return false };
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let got = unsafe { VirtualQuery(address as *const c_void, &mut info, std::mem::size_of_val(&info)) };
    // Readable or writable, and neither a guard page nor no-access.
    got != 0
        && info.State == MEM_COMMIT
        && info.Protect & 0xEE != 0
        && info.Protect & 0x101 == 0
        && last <= info.BaseAddress as usize + info.RegionSize
}

/// Finds the node for colour `id` in the provider's tree, reading nothing that `valid` rejects.
/// A node whose pointer is bad ends the walk with `None`, and so does a tree deeper than any real
/// one, which a cycle would be.
unsafe fn find_slot(provider: *const u8, id: i32, valid: &dyn Fn(usize, usize) -> bool) -> Option<*mut u32> {
    if !valid(provider as usize + 0x18, 8) {
        return None;
    }
    let mut node = *(provider.add(0x18) as *const *mut u8);
    for _ in 0..512 {
        if node.is_null() {
            return None;
        }
        if !valid(node as usize, 0x28) {
            return None;
        }
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

unsafe fn colour_slot(provider: *const u8, id: i32) -> Option<*mut u32> {
    if !bv().provider_verified {
        return None;
    }
    find_slot(provider, id, &readable)
}

/// The provider's slots for the colours the frame paints with. A provider makes a colour the first
/// time it is asked for it, so one that is missing is asked for before its slot is read. A build
/// whose provider layout is not verified yields no slots, so nothing is created or swapped.
unsafe fn frame_slots(provider: *const u8) -> Vec<(*mut u32, u32)> {
    if !bv().provider_verified {
        return Vec::new();
    }
    let get_color = (crate::builds::get().toolbar.get_color != crate::builds::ABSENT)
        .then(|| image::recorded())
        .flatten()
        .map(|image| {
            std::mem::transmute::<usize, unsafe extern "system" fn(*const u8, i32) -> u32>(image.base + crate::builds::get().toolbar.get_color)
        });
    bv().frame_ids
        .iter()
        .filter_map(|id| {
            let mut slot = colour_slot(provider, *id);
            if slot.is_none() {
                if let Some(get_color) = get_color {
                    get_color(provider, *id);
                    slot = colour_slot(provider, *id);
                }
            }
            slot
        })
        .map(|slot| (slot, slot.read()))
        .collect()
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
    let saved = frame_slots(provider);
    for (slot, _) in &saved {
        slot.write(colour);
    }
    original(view, canvas);
    for (slot, value) in saved {
        slot.write(value);
    }
}

static CORNER_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe extern "system" fn corner_paint(view: *const c_void, canvas: *const c_void) {
    let original: OnPaint = std::mem::transmute(CORNER_ORIGINAL.load(std::sync::atomic::Ordering::Relaxed));
    let colour = match (HOOK.get(), PAINT.get()) {
        (Some(Ok(hook)), Some((_, provider_of, _, paint))) => {
            paint((hook.hwnd_for_view)(view)).map(|colour| (colour, provider_of(view)))
        }
        _ => None,
    };
    let Some((colour, provider)) = colour.filter(|(_, provider)| !provider.is_null()) else {
        return original(view, canvas);
    };
    let saved = frame_slots(provider);
    for (slot, _) in &saved {
        slot.write(colour);
    }
    original(view, canvas);
    for (slot, value) in saved {
        slot.write(value);
    }
}

unsafe fn install_corner_paint(image: &Image) -> Result<(), String> {
    let slot_rva = bv().corner_vtable + bv().on_paint_slot * 8;
    if !image.has(bv().corner_paint, bv().corner_paint_prologue) {
        return Err("this browser build has no recorded corner paint".into());
    }
    CORNER_ORIGINAL.store(image.base + bv().corner_paint, std::sync::atomic::Ordering::Relaxed);
    replace_vtable_slot(image, slot_rva, bv().corner_paint, corner_paint as *const () as usize, "corner view vtable")
}

/// Experiment: lets Rivet choose the title bar colour of its own windows only.
pub unsafe fn install_frame_paint(paint: Paint) -> Result<(), String> {
    if PAINT.get().is_some() {
        return Ok(());
    }
    if !bv().provider_verified {
        return Err("this browser build has no verified colour provider layout".into());
    }
    let image = recorded_image()?;
    let slot_rva = bv().frame_vtable + bv().on_paint_slot * 8;
    if !image.points_to(slot_rva, bv().on_paint) {
        return Err("this browser build has no recorded frame paint".into());
    }
    if !image.has(bv().on_paint, bv().on_paint_prologue)
        || !image.has(bv().draw_color, bv().draw_color_prologue)
        || !image.has(bv().get_color_provider, bv().get_color_provider_prologue)
    {
        return Err("this browser build has no recorded frame paint helpers".into());
    }
    let base = image.base;
    let _ = PAINT.set((
        std::mem::transmute::<usize, OnPaint>(base + bv().on_paint),
        std::mem::transmute::<usize, GetProvider>(base + bv().get_color_provider),
        std::mem::transmute::<usize, DrawColor>(base + bv().draw_color),
        paint,
    ));
    replace_vtable_slot(&image, slot_rva, bv().on_paint, on_paint as *const () as usize, "frame view vtable")?;
    // The corner wedges are cosmetic. A build whose wedge paint is unmapped keeps the browser's.
    if let Err(error) = install_corner_paint(&image) {
        log(&format!("rivet: {error}"));
    }
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
        let browser_chose = *out;
        *out = reserve_leading(*out, reserved);
        if PANEL.load(std::sync::atomic::Ordering::Relaxed) {
            *out = panel_from_client(*out);
            lift_cached_client_area(view, browser_chose);
        }
        record_panel((hook.hwnd_for_view)(view), *out);
    }
    result
}

/// The page panel each window's browser last laid out, in device-independent pixels of the frame
/// view. Rivet cuts its ground's hole to exactly this.
static PANELS: std::sync::Mutex<Vec<(isize, Rect)>> = std::sync::Mutex::new(Vec::new());
/// Called with a window whose panel changed, from inside the browser's layout.
static PANEL_CHANGED: OnceLock<fn(HWND)> = OnceLock::new();

pub fn on_panel_change(notify: fn(HWND)) {
    let _ = PANEL_CHANGED.set(notify);
}

fn record_panel(window: HWND, panel: Rect) {
    let changed = {
        let mut panels = PANELS.lock().unwrap_or_else(|error| error.into_inner());
        match panels.iter_mut().find(|entry| entry.0 == window as isize) {
            Some(entry) => std::mem::replace(&mut entry.1, panel) != panel,
            None => {
                panels.push((window as isize, panel));
                true
            }
        }
    };
    if let (true, Some(notify)) = (changed, PANEL_CHANGED.get()) {
        notify(window);
    }
}

/// The page panel the browser laid out for a window, once it has laid the window out.
pub fn panel_bounds(window: HWND) -> Option<Rect> {
    PANELS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
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
    log(&format!("rivet: {}", crate::builds::table_note()));
    crate::trace::trace(&format!("build table: {}", crate::builds::table_note()));
    let (build, image) = crate::builds::frame_rows()
        .iter()
        .find_map(|build| {
            let module = wide(&build.module);
            let base = GetModuleHandleW(module.as_ptr()) as usize;
            let version = parse_version(&build.version)?;
            let image = image::verify(base, version)?;
            matches(&image, build).then_some((build, image))
        })
        .ok_or("this browser build is not supported yet")?;
    let base = image.base;
    let slot_rva = build.frame_view_vtable as usize + build.client_view_slot as usize * 8;
    let slot = image.at(slot_rva, 8).ok_or("recorded slot outside the image")? as *mut usize;
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
unsafe fn matches(image: &Image, build: &ChromeBuild) -> bool {
    let Some(expected) = chrome_build::hex(&build.hwnd_for_view_prologue) else {
        return false;
    };
    let slot = build.frame_view_vtable as usize + build.client_view_slot as usize * 8;
    image.points_to(slot, build.client_view_bounds as usize)
        && image.has(build.hwnd_for_view as usize, &expected)
}

/// `156.0.8078.48` as the four numbers of a file version.
fn parse_version(text: &str) -> Option<[u16; 4]> {
    let parts: Vec<u16> = text
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_native_caption_buttons_give_way_to_rivets_own() {
        // The container keeps its right edge but has no size, so nothing of it paints or answers.
        assert_eq!(hidden_caption_bounds([900, 0, 138, 32]), [1038, 0, 0, 0]);
    }

    #[test]
    fn fr12_top_inset_is_the_panel_gap_for_rivet_windows_only() {
        // Chrome, Brave, Whale, Samsung and Edge each answer differently; a Rivet window always
        // gets the panel's top gap, so the bar sits at the same place on every browser.
        for original in [6, 26, 32, 41] {
            assert_eq!(panel_top_inset(original, Some(PANEL_GAP)), PANEL_GAP);
        }
        // Another window of the same browser keeps the browser's answer.
        assert_eq!(panel_top_inset(6, None), 6);
    }

    #[test]
    fn fr12_lifted_panel_keeps_the_browser_view_inside_the_kept_client_area() {
        // Chromium checks that its kept client area contains the browser view, so the area is
        // lifted with the panel and found only where a single field holds it.
        let chose = Rect { x: 0, y: 30, width: 1600, height: 970 };
        let mut fields = vec![0i32; 64];
        fields[20..24].copy_from_slice(&[0, 30, 1600, 970]);
        assert_eq!(sole_rect_offset(&fields, chose), Some(80));
        let lifted = lifted_client(chose);
        let panel = panel_from_client(reserve_leading(chose, 268));
        assert!(lifted.y <= panel.y && panel.y + panel.height <= lifted.y + lifted.height);
        fields[40..44].copy_from_slice(&[0, 30, 1600, 970]);
        assert_eq!(sole_rect_offset(&fields, chose), None);
    }

    #[test]
    fn fr12_panel_starts_at_the_mockup_top_inset() {
        // Whatever top the browser kept for its title bar, the panel starts one gap below the
        // window's top edge, as the study's `.main { padding: 8px }`.
        for top in [0, 32, 38, 47] {
            let panel = panel_from_client(Rect { x: 268, y: top, width: 1000, height: 800 - top });
            assert_eq!(panel, Rect { x: 268, y: PANEL_GAP, width: 992, height: 800 - 2 * PANEL_GAP });
        }
    }

    #[test]
    fn fr12_caption_ink_is_independent_of_window_activation() {
        assert_eq!(caption_ink(), 0xff1a_1a1a);
    }

    #[test]
    fn fr2_panel_clips_each_corner_at_twelve_dips() {
        assert_eq!(panel_radii(), [12.0; 4]);
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

type PreferredSize =
    unsafe extern "system" fn(*const c_void, *mut [i32; 2], *const c_void) -> *mut [i32; 2];
/// Decides the caption button height of a window in DIPs, or `None` to keep the browser's.
pub type CaptionHeight = fn(HWND) -> Option<i32>;
static CAPTION: OnceLock<(PreferredSize, CaptionHeight)> = OnceLock::new();

static CAPTION_PAINT_HOOK: OnceLock<(OnPaint, GetProvider)> = OnceLock::new();

fn caption_ink() -> u32 {
    0xff1a_1a1a
}

unsafe extern "system" fn caption_paint(button: *const c_void, canvas: *const c_void) {
    let (original, provider_of) = CAPTION_PAINT_HOOK
        .get()
        .expect("patched after it is stored");
    let rivet = matches!((HOOK.get(), PAINT.get()), (Some(Ok(hook)), Some((_, _, _, paint))) if paint((hook.hwnd_for_view)(button)).is_some());
    if !rivet {
        return original(button, canvas);
    }
    let provider = provider_of(button);
    if provider.is_null() {
        return original(button, canvas);
    }
    let saved: Vec<_> = bv().caption_ink_ids
        .iter().copied()
        .filter_map(|id| colour_slot(provider, id))
        .map(|slot| (slot, slot.read()))
        .collect();
    // The glyphs are white on the dark row of a page in developer mode.
    let ink = match HOOK.get() {
        Some(Ok(hook)) if crate::features::dev_page::window_in_dev_mode((hook.hwnd_for_view)(button)) => 0xffff_ffff,
        _ => caption_ink(),
    };
    for (slot, _) in &saved {
        slot.write(ink);
    }
    original(button, canvas);
    for (slot, colour) in saved {
        slot.write(colour);
    }
}

pub unsafe fn install_caption_paint() -> Result<(), String> {
    if CAPTION_PAINT_HOOK.get().is_some() {
        return Ok(());
    }
    if !bv().provider_verified {
        return Err("this browser build has no verified colour provider layout".into());
    }
    let image = recorded_image()?;
    let slot_rva = bv().caption_vtable + bv().caption_paint_slot * 8;
    if !image.points_to(slot_rva, bv().caption_paint)
        || !image.has(bv().caption_paint, bv().caption_paint_prologue)
        || !image.has(bv().get_color_provider, bv().get_color_provider_prologue)
    {
        return Err("this browser build has no recorded caption paint".into());
    }
    let _ = CAPTION_PAINT_HOOK.set((
        std::mem::transmute::<usize, OnPaint>(image.base + bv().caption_paint),
        std::mem::transmute::<usize, GetProvider>(image.base + bv().get_color_provider),
    ));
    replace_vtable_slot(&image, slot_rva, bv().caption_paint, caption_paint as *const () as usize, "caption paint slot")
}

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
    let image = recorded_image()?;
    let slot_rva = bv().caption_vtable + bv().caption_size_slot * 8;
    if !image.points_to(slot_rva, bv().caption_size) {
        return Err("this browser build has no recorded caption button size".into());
    }
    let _ = CAPTION.set((
        std::mem::transmute::<usize, PreferredSize>(image.base + bv().caption_size),
        height,
    ));
    replace_vtable_slot(&image, slot_rva, bv().caption_size, caption_size as *const () as usize, "caption button vtable")
}

type TopInset = unsafe extern "system" fn(*const c_void, bool) -> i32;
/// Extra title bar height in DIPs for a window, or `None` to keep the browser's.
pub type ExtraTop = fn(HWND) -> Option<i32>;
static TOP: OnceLock<(TopInset, ExtraTop)> = OnceLock::new();

unsafe extern "system" fn top_inset(view: *const c_void, restored: bool) -> i32 {
    let (original, extra) = TOP.get().expect("patched after it is stored");
    let inset = original(view, restored);
    if crate::trace::tracing() {
        crate::trace::trace(&format!("top inset original {inset} restored {restored}"));
    }
    match HOOK.get() {
        Some(Ok(hook)) => panel_top_inset(inset, extra((hook.hwnd_for_view)(view))),
        _ => inset,
    }
}

/// The top inset of a window: the browser's own, or for a Rivet window the top of its panel. Each
/// browser keeps a title bar of its own height, so a Rivet window takes a fixed value instead of
/// adding to it, and the browser's top elements begin at the panel's top edge.
fn panel_top_inset(original: i32, target: Option<i32>) -> i32 {
    target.unwrap_or(original)
}

pub unsafe fn install_top_inset(extra: ExtraTop) -> Result<(), String> {
    if TOP.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = bv().frame_vtable + bv().top_inset_slot * 8;
    if !image.points_to(slot_rva, bv().top_inset) {
        return Err("this browser build has no recorded top inset".into());
    }
    let _ = TOP.set((std::mem::transmute::<usize, TopInset>(image.base + bv().top_inset), extra));
    replace_vtable_slot(&image, slot_rva, bv().top_inset, top_inset as *const () as usize, "frame view vtable")
}

type Layout = unsafe extern "system" fn(*mut c_void);
type SetBoundsRect = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);
static LAYOUT_HOOK: OnceLock<(Layout, SetBoundsRect, CaptionHeight)> = OnceLock::new();
type SetPaintToLayer = unsafe extern "system" fn(*mut c_void, i32);
type SetRadius = unsafe extern "system" fn(*mut c_void, *const [f32; 4]);
type SetMasks = unsafe extern "system" fn(*mut c_void, bool);
type StackAtTop = unsafe extern "system" fn(*mut c_void, *mut c_void);
type SetOpaque = unsafe extern "system" fn(*mut c_void, bool);
static SET_OPAQUE: OnceLock<SetOpaque> = OnceLock::new();
static STACK_AT_TOP: OnceLock<StackAtTop> = OnceLock::new();
static PANEL_LAYER: OnceLock<(SetPaintToLayer, SetRadius, SetMasks)> = OnceLock::new();

fn panel_radii() -> [f32; 4] {
    [12.0; 4]
}


unsafe fn clip_panel(frame: *mut c_void) {
    let Some((set_layer, set_radius, set_masks)) = PANEL_LAYER.get() else { return };
    // BrowserFrameViewWin holds BrowserView's BrowserWindow subobject at +0x340.
    let browser_window = *(frame.cast::<u8>().add(bv().frame_browser_window_offset) as *const *mut u8);
    if browser_window.is_null() {
        return;
    }
    let browser = browser_window.sub(bv().browser_window_in_view).cast::<usize>();
    let Some(image) = image::recorded() else { return };
    let base = image.base;
    let Some(index) = (0..0x100).find(|index| *browser.add(*index) == base + bv().browser_view_view_vtable)
    else {
        return;
    };
    let view = browser.add(index).cast::<c_void>();
    // `views::View` keeps its `ui::Layer` at +0x20.
    let layer_of = || *(view.cast::<u8>().add(0x20) as *const *mut c_void);
    if layer_of().is_null() {
        set_layer(view, 1);
    }
    let layer = layer_of();
    if !layer.is_null() {
        set_masks(layer, true);
        set_radius(layer, &panel_radii());
    }
}

/// Gives `view` its own layer and stacks it above its siblings' layers, so it paints over a
/// sibling it overlaps, such as the page. Views order layers by tree position, so a later layout
/// can undo it and the caller repeats it after each layout.
pub unsafe fn raise_view(view: *mut c_void) {
    let Some((set_layer, _, _)) = PANEL_LAYER.get() else { return };
    let Some(stack_at_top) = STACK_AT_TOP.get() else { return };
    let layer_of = || *(view.cast::<u8>().add(0x20) as *const *mut c_void);
    if layer_of().is_null() {
        set_layer(view, 1);
    }
    let layer = layer_of();
    if layer.is_null() {
        return;
    }
    let parent = *(layer.cast::<u8>().add(0x20) as *const *mut c_void);
    if !parent.is_null() {
        stack_at_top(parent, layer);
    }
}

/// The native caption container of a Rivet window: no size, at its right edge. Rivet draws its own
/// minimize, maximize and close buttons in its bar, so the browser's must neither paint over the
/// panel nor take clicks.
fn hidden_caption_bounds(bounds: [i32; 4]) -> [i32; 4] {
    [bounds[0] + bounds[2], 0, 0, 0]
}

/// The frame view each window last laid out, so Rivet can ask for a layout when the space it
/// reserves changes. Chromium only lays out again on its own when the window's size changes, and
/// a maximized window cannot be resized to make it.
static FRAME_VIEWS: std::sync::Mutex<Vec<(isize, usize)>> = std::sync::Mutex::new(Vec::new());

fn remember_view(window: HWND, view: *mut c_void) {
    let mut views = FRAME_VIEWS.lock().unwrap_or_else(|error| error.into_inner());
    match views.iter_mut().find(|entry| entry.0 == window as isize) {
        Some(entry) => entry.1 = view as usize,
        None => views.push((window as isize, view as usize)),
    }
}

/// The window a view belongs to, once the layout hook is installed.
pub unsafe fn hwnd_of_view(view: *const c_void) -> Option<HWND> {
    match HOOK.get() {
        Some(Ok(hook)) => Some((hook.hwnd_for_view)(view)),
        _ => None,
    }
}

/// The frame view a window last laid out, if any.
pub fn frame_view(window: HWND) -> Option<usize> {
    FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
}

/// Forgets a window that is going away, so its view is never used again.
pub fn forget_view(window: HWND) {
    PANELS.lock().unwrap_or_else(|error| error.into_inner()).retain(|entry| entry.0 != window as isize);
    FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.0 != window as isize);
}

/// Lays the window's frame out again. Returns false when the window has not been laid out yet.
pub unsafe fn relayout(window: HWND) -> bool {
    let view = FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1);
    match view {
        Some(view) => {
            let Some(image) = image::recorded() else { return false };
            let base = image.base;
            // `views::View::DeprecatedLayoutImmediately` does the bookkeeping around a layout
            // and then runs the view's own `Layout`, which is this hook. The wanted bytes are
            // its first instructions in this build.
            if !image.has(bv().layout_immediately, &bv().layout_immediately_prologue) {
                return false;
            }
            let layout_immediately: unsafe extern "system" fn(*mut c_void) =
                std::mem::transmute(base + bv().layout_immediately);
            layout_immediately(view as *mut c_void);
            true
        }
        None => false,
    }
}

unsafe extern "system" fn frame_layout(view: *mut c_void) {
    let (original, set_bounds, height) = LAYOUT_HOOK.get().expect("patched after it is stored");
    original(view);
    let Some(Ok(hook)) = HOOK.get() else { return };
    remember_view((hook.hwnd_for_view)(view), view);
    if height((hook.hwnd_for_view)(view)).is_none() {
        return;
    }
    clip_panel(view);
    let container = *(view.cast::<u8>().add(bv().caption_container_offset) as *const *mut c_void);
    if container.is_null() {
        return;
    }
    let bounds = *(container.cast::<u8>().add(bv().view_bounds_offset) as *const [i32; 4]);
    let wanted = hidden_caption_bounds(bounds);
    if wanted != bounds {
        set_bounds(container, &wanted);
    }
}

/// Makes the caption button container of a window reach `height` DIPs from the top of the frame.
pub unsafe fn install_caption_layout(height: CaptionHeight) -> Result<(), String> {
    if LAYOUT_HOOK.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = bv().frame_vtable + bv().layout_slot * 8;
    if !image.points_to(slot_rva, bv().layout) {
        return Err("this browser build has no recorded frame layout".into());
    }
    if !image.has(bv().layout, bv().layout_prologue)
        || !image.has(bv().set_bounds_rect, bv().set_bounds_rect_prologue)
    {
        return Err("this browser build has no recorded caption layout helpers".into());
    }
    if !image.has(bv().set_layer, bv().set_layer_prologue)
        || !image.has(bv().set_radius, bv().set_radius_prologue)
        || !image.has(bv().set_masks, bv().set_masks_prologue)
        || !image.has(bv().stack_at_top, bv().stack_at_top_prologue)
        || !image.has(bv().set_opaque, bv().set_opaque_prologue)
    {
        return Err("this browser build has no recorded panel layer helpers".into());
    }
    let base = image.base;
    let _ = SET_OPAQUE.set(std::mem::transmute::<usize, SetOpaque>(base + bv().set_opaque));
    let _ = STACK_AT_TOP.set(std::mem::transmute::<usize, StackAtTop>(base + bv().stack_at_top));
    let _ = PANEL_LAYER.set((
        std::mem::transmute::<usize, SetPaintToLayer>(base + bv().set_layer),
        std::mem::transmute::<usize, SetRadius>(base + bv().set_radius),
        std::mem::transmute::<usize, SetMasks>(base + bv().set_masks),
    ));
    let _ = LAYOUT_HOOK.set((
        std::mem::transmute::<usize, Layout>(base + bv().layout),
        std::mem::transmute::<usize, SetBoundsRect>(base + bv().set_bounds_rect),
        height,
    ));
    replace_vtable_slot(&image, slot_rva, bv().layout, frame_layout as *const () as usize, "frame view vtable")
}


#[cfg(test)]
mod provider_guard_tests {
    use super::*;

    /// A node: left, right, 8 bytes of padding, then the id and the colour at +0x20 and +0x24.
    #[repr(C)]
    struct Node {
        left: *mut Node,
        right: *mut Node,
        pad: [u64; 2],
        id: i32,
        colour: u32,
        tail: u64,
    }

    fn node(id: i32, colour: u32) -> Node {
        Node { left: std::ptr::null_mut(), right: std::ptr::null_mut(), pad: [0; 2], id, colour, tail: 0 }
    }

    #[test]
    fn nfr3_tree_walk_finds_a_colour_and_refuses_bad_pointers() {
        let mut leaf = node(5, 0xff00_ff00);
        let mut root = node(3, 1);
        root.right = &mut leaf;
        let mut provider = [0u64; 4];
        provider[3] = &mut root as *mut Node as u64;
        let any = |_: usize, _: usize| true;
        unsafe {
            let found = find_slot(provider.as_ptr() as *const u8, 5, &any).expect("slot");
            assert_eq!(found.read(), 0xff00_ff00);
            assert!(find_slot(provider.as_ptr() as *const u8, 9, &any).is_none());
            // A validator that rejects the node ends the walk without reading it.
            let reject_nodes = |address: usize, _: usize| address != &root as *const Node as usize;
            assert!(find_slot(provider.as_ptr() as *const u8, 5, &reject_nodes).is_none());
            // A node pointing at itself is cut off.
            root.right = &mut root;
            assert!(find_slot(provider.as_ptr() as *const u8, 9, &any).is_none());
        }
    }

    #[test]
    fn nfr3_readable_rejects_bad_addresses() {
        assert!(!readable(0, 8));
        assert!(!readable(usize::MAX, 8));
        assert!(!readable(0xffff_ffff_ffff_fff8, 8));
        assert!(!readable(0x1234_5678, 8));
        let value = Box::new(0u64);
        assert!(readable(&*value as *const u64 as usize, 8));
        assert!(!readable(&*value as *const u64 as usize + 1, 8));
    }

    #[test]
    fn nfr3_only_builds_with_a_recorded_get_color_may_create_colours() {
        for build in crate::builds::all() {
            let creates = build.toolbar.get_color != crate::builds::ABSENT;
            if build.layout.frame_ids.is_empty() {
                assert!(!build.layout.provider_verified, "{}", build.browser);
            }
            assert!(!creates || build.layout.provider_verified, "{}", build.browser);
        }
    }
}
