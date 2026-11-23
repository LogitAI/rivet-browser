//! The panel of a Rivet window and the frame view functions it needs from the browser.
//!
//! Rivet leaves Chromium's own frame layout alone. It works out the page panel of each window from
//! the window's client area and the appbar's width, and records it here for every surface that
//! sits on the panel. The frame view's functions are only found, so a view's window can be told,
//! and the frame paint takes the window's colours.
//!
//! The vtable layout is private to each Chromium build. Rivet only attaches to builds whose module
//! contents match an entry of the supported-build table exactly, and leaves every other build
//! untouched.

use std::ffi::c_void;
use std::sync::OnceLock;

use rivet_platform_windows::chrome_build::{self, ChromeBuild};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

use crate::builds::Build;
use crate::chromium::image::{self, Image};
use crate::feature::Needs;
use crate::targets::Target;

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::Layout {
    &crate::builds::get().layout
}

/// What the client view layout of the frame needs from the browser. These are the entry of the
/// frame table that names this build.
pub(crate) fn needs_layout(build: &Build) -> Needs {
    let entry = crate::builds::frame_rows().iter().find(|entry| {
        entry.module == build.module && parse_version(&entry.version) == Some(build.version)
    });
    match entry {
        Some(entry) => {
            let prologue: &'static [u8] = Box::leak(
                chrome_build::hex(&entry.hwnd_for_view_prologue)
                    .unwrap_or_default()
                    .into_boxed_slice(),
            );
            Needs::targets(vec![
                Target::new("frame view vtable", entry.frame_view_vtable as usize, &[]),
                Target::new("client view bounds", entry.client_view_bounds as usize, &[]),
                Target::new("hwnd for view", entry.hwnd_for_view as usize, prologue),
            ])
        }
        None => Needs {
            policy: Err("the frame table has no entry for this build"),
            targets: Vec::new(),
        },
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
            Target::new(
                "get colour provider",
                row.get_color_provider,
                row.get_color_provider_prologue,
            ),
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
            Target::new(
                "caption button paint",
                row.caption_paint,
                row.caption_paint_prologue,
            ),
            Target::new(
                "get colour provider",
                row.get_color_provider,
                row.get_color_provider_prologue,
            ),
        ],
    )
}

/// The frame view's class table: the row's, read again from the window's live frame view.
fn frame_table() -> usize {
    use crate::chromium::live_tables;
    live_tables::table(bv().frame_vtable, live_tables::found().frame)
}

/// The caption buttons' class table, settled with the live buttons' as `frame_table` is.
fn caption_table() -> usize {
    use crate::chromium::live_tables;
    live_tables::table(bv().caption_vtable, live_tables::found().caption)
}

/// The corner wedges' class table, settled with the live wedges' as `frame_table` is.
fn corner_table() -> usize {
    use crate::chromium::live_tables;
    live_tables::table(bv().corner_vtable, live_tables::found().corner)
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

/// The panel of a Rivet window from the client view bounds. The browser keeps a title bar above its
/// client view, which a Rivet window does not have: Rivet's bar is the panel's own top row. So the
/// client view reaches the window's top edge before the gap is taken.
fn panel_from_client(bounds: Rect) -> Rect {
    inset_panel(lifted_client(bounds))
}

/// The client view's bounds lifted to the window's top edge, as `panel_from_client` places it.
fn lifted_client(bounds: Rect) -> Rect {
    Rect {
        y: 0,
        height: bounds.height + bounds.y,
        ..bounds
    }
}

type HwndForView = unsafe extern "system" fn(*const c_void) -> HWND;

struct Hook {
    hwnd_for_view: HwndForView,
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
    let Some(last) = address.checked_add(length) else {
        return false;
    };
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let got = unsafe {
        VirtualQuery(
            address as *const c_void,
            &mut info,
            std::mem::size_of_val(&info),
        )
    };
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
unsafe fn find_slot(
    provider: *const u8,
    id: i32,
    valid: &dyn Fn(usize, usize) -> bool,
) -> Option<*mut u32> {
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
        .then(image::recorded)
        .flatten()
        .map(|image| {
            std::mem::transmute::<usize, unsafe extern "system" fn(*const u8, i32) -> u32>(
                image.base + crate::builds::get().toolbar.get_color,
            )
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
    let original: OnPaint =
        std::mem::transmute(CORNER_ORIGINAL.load(std::sync::atomic::Ordering::Relaxed));
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
    let slot_rva = corner_table() + bv().on_paint_slot * 8;
    if !image.has(bv().corner_paint, bv().corner_paint_prologue) {
        return Err("this browser build has no recorded corner paint".into());
    }
    CORNER_ORIGINAL.store(
        image.base + bv().corner_paint,
        std::sync::atomic::Ordering::Relaxed,
    );
    replace_vtable_slot(
        image,
        slot_rva,
        bv().corner_paint,
        corner_paint as *const () as usize,
        "corner view vtable",
    )
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
    let slot_rva = frame_table() + bv().on_paint_slot * 8;
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
    replace_vtable_slot(
        &image,
        slot_rva,
        bv().on_paint,
        on_paint as *const () as usize,
        "frame view vtable",
    )?;
    // The corner wedges are cosmetic. A build whose wedge paint is unmapped keeps the browser's.
    if let Err(error) = install_corner_paint(&image) {
        log(&format!("rivet: {error}"));
    }
    Ok(())
}

/// Decides the title bar colour of a window as `0xAARRGGBB`, or `None` to keep the browser's.
pub type Paint = fn(HWND) -> Option<u32>;

/// The page panel Rivet last placed for each window, in device-independent pixels of the frame
/// view. Rivet cuts its ground's hole to exactly this.
static PANELS: std::sync::Mutex<Vec<(isize, Rect)>> = std::sync::Mutex::new(Vec::new());
/// Called with a window whose panel changed.
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

/// The panel of a Rivet window from its client area alone: `reserved` DIPs at the left, then the
/// panel's gap.
pub fn rivet_panel(client: Rect, reserved: i32) -> Rect {
    panel_from_client(reserve_leading(client, reserved))
}

/// Records a panel Rivet placed.
pub fn place_panel(window: HWND, panel: Rect) {
    record_panel(window, panel);
}

/// The page panel Rivet placed for a window, once it has placed one.
pub fn panel_bounds(window: HWND) -> Option<Rect> {
    PANELS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
}

/// Finds the build's frame view functions once per browser process, so a view's window can be
/// told. The client view placement stays the browser's. Returns why it cannot when the build is
/// not recognised, so Rivet refuses to attach rather than guess.
pub unsafe fn install() -> Result<(), String> {
    HOOK.get_or_init(|| find())
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

unsafe fn find() -> Result<Hook, String> {
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
    log(&format!(
        "rivet: attaching to {} {}, which keeps its own client view placement",
        build.browser, build.version
    ));
    Ok(Hook {
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(
            image.base + build.hwnd_for_view as usize,
        ),
    })
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(text.as_ptr()) };
}

/// Checks that the loaded module is the recorded build: the frame view's client view slot must
/// still point at the recorded function, and the helper must start with the recorded
/// instructions.
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
    fn fr12_panel_starts_at_the_mockup_top_inset() {
        // Whatever top the browser kept for its title bar, the panel starts one gap below the
        // window's top edge, as the study's `.main { padding: 8px }`.
        for top in [0, 32, 38, 47] {
            let panel = panel_from_client(Rect {
                x: 268,
                y: top,
                width: 1000,
                height: 800 - top,
            });
            assert_eq!(
                panel,
                Rect {
                    x: 268,
                    y: PANEL_GAP,
                    width: 992,
                    height: 800 - 2 * PANEL_GAP
                }
            );
        }
    }

    #[test]
    fn fr12_panel_placed_by_rivet_ignores_the_browsers_title_row() {
        // The client view reaches the window's top edge once lifted, so the panel Rivet works out
        // from the client area is the same for any title row height the browser keeps.
        let client = Rect {
            x: 0,
            y: 0,
            width: 1600,
            height: 1000,
        };
        for top in [0, 32, 41] {
            let chose = Rect {
                y: top,
                height: 1000 - top,
                ..client
            };
            assert_eq!(
                rivet_panel(client, 268),
                panel_from_client(reserve_leading(chose, 268))
            );
        }
        assert_eq!(
            rivet_panel(client, 268),
            Rect {
                x: 268,
                y: PANEL_GAP,
                width: 1324,
                height: 1000 - 2 * PANEL_GAP
            }
        );
    }

    #[test]
    fn fr12_caption_ink_is_independent_of_window_activation() {
        assert_eq!(caption_ink(), 0xff1a_1a1a);
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
    let saved: Vec<_> = bv()
        .caption_ink_ids
        .iter()
        .copied()
        .filter_map(|id| colour_slot(provider, id))
        .map(|slot| (slot, slot.read()))
        .collect();
    // The glyphs are white on the dark row of a page in developer mode.
    let ink = match HOOK.get() {
        Some(Ok(hook))
            if crate::features::dev_page::window_in_dev_mode((hook.hwnd_for_view)(button)) =>
        {
            0xffff_ffff
        }
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
    let slot_rva = caption_table() + bv().caption_paint_slot * 8;
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
    replace_vtable_slot(
        &image,
        slot_rva,
        bv().caption_paint,
        caption_paint as *const () as usize,
        "caption paint slot",
    )
}

/// The window a view belongs to, once the frame view's functions are found.
pub unsafe fn hwnd_of_view(view: *const c_void) -> Option<HWND> {
    match HOOK.get() {
        Some(Ok(hook)) => Some((hook.hwnd_for_view)(view)),
        _ => None,
    }
}

/// Forgets a window that is going away, so its panel is never used again.
pub fn forget_view(window: HWND) {
    PANELS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.0 != window as isize);
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
        Node {
            left: std::ptr::null_mut(),
            right: std::ptr::null_mut(),
            pad: [0; 2],
            id,
            colour,
            tail: 0,
        }
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
            let root_node = provider[3] as *mut Node;
            (*root_node).right = root_node;
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
            assert!(
                !creates || build.layout.provider_verified,
                "{}",
                build.browser
            );
        }
    }
}
