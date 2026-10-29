//! Edge's vertical tabs for Rivet windows only.
//!
//! Edge builds a container view for its vertical tabs in every browser window and keeps the
//! window's orientation in a state number of that view. The profile's settings decide the state a
//! window starts in, and writing them would change every window. Rivet puts the container of its
//! own windows into the vertical state through the browser view's own function, which never writes
//! a setting, and answers the question "hide the title bar" with yes for those windows only. The
//! toolbar then shares one row with the caption buttons, like Chrome's vertical tab layout.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

use crate::chrome_layout::image;

fn bv() -> Option<&'static crate::builds::EdgeVertical> {
    crate::builds::get().edge_vertical.as_ref()
}

type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;
type SetState = unsafe extern "system" fn(*mut c_void, u32, bool);
type HideTitlebar = unsafe extern "system" fn(*mut c_void) -> bool;

static ORIGINAL_HIDE: AtomicUsize = AtomicUsize::new(0);
/// The browser views of Rivet windows. A view is removed before its window goes away.
static VIEWS: Mutex<Vec<(isize, usize, usize)>> = Mutex::new(Vec::new());
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

fn is_rivet(view: usize) -> bool {
    VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|(_, known, _)| *known == view)
}

fn is_rivet_container(container: usize) -> bool {
    VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|(_, _, known)| *known == container)
}

unsafe extern "system" fn should_hide_titlebar(view: *mut c_void) -> bool {
    if is_rivet(view as usize) && !skipped("edgehide") {
        return true;
    }
    let original: HideTitlebar = std::mem::transmute(ORIGINAL_HIDE.load(Ordering::Acquire));
    original(view)
}

/// Replaces the first instructions of a function with a jump to `hook` and returns the address of
/// a trampoline that runs the original. `prologue` is checked first and ends on an instruction
/// boundary at least twelve bytes in. A stack cookie load among its instructions is relative to
/// the function, so the trampoline loads it from its absolute address instead: `cookie_at` is the
/// offset of that seven byte load.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    cookie_at: Option<usize>,
    hook: usize,
) -> Result<usize, String> {
    use crate::chrome_toolbar::{executable, jump, load_rax, write_code};
    let code = (base + rva) as *mut u8;
    if prologue.len() < 12 || std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    let mut trampoline;
    match cookie_at {
        Some(at) => {
            if prologue.get(at..at + 3) != Some(&[0x48, 0x8B, 0x05]) || prologue.len() < at + 7 {
                return Err(format!("{rva:#x} has no cookie load where recorded"));
            }
            let displacement = i32::from_le_bytes(prologue[at + 3..at + 7].try_into().unwrap());
            let cookie = (base + rva + at + 7).wrapping_add_signed(displacement as isize);
            trampoline = prologue[..at].to_vec();
            // mov rax, [cookie]
            trampoline.extend_from_slice(&[0x48, 0xA1]);
            trampoline.extend_from_slice(&(cookie as u64).to_le_bytes());
            trampoline.extend_from_slice(&prologue[at + 7..]);
        }
        None => trampoline = prologue.to_vec(),
    }
    jump(&mut trampoline, base + rva + prologue.len());
    let original = executable(&trampoline)?;
    let mut patch = Vec::new();
    load_rax(&mut patch, hook);
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(prologue.len(), 0xCC);
    write_code(code, &patch)?;
    Ok(original)
}

/// The preferred size functions of the vtables Rivet reshapes: the vtable and its function.
static SIZED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

unsafe extern "system" fn sized_preferred(view: *mut c_void, out: *mut [i32; 2], bounds: usize) -> *mut c_void {
    type Original = unsafe extern "system" fn(*mut c_void, *mut [i32; 2], usize) -> *mut c_void;
    let table = *(view as *const usize);
    let original = SIZED.lock().unwrap_or_else(|error| error.into_inner()).iter().find(|(vtable, _)| *vtable == table).map(|(_, function)| *function);
    let Some(original) = original else { return out.cast() };
    let original: Original = std::mem::transmute(original);
    let result = original(view, out, bounds);
    let (Some(edge), Some(image)) = (bv(), image::recorded()) else { return result };
    if !skipped("edgesize") && is_rivet_view_tree(view as usize) {
        if table == image.base + edge.toolbar_vtable {
            (*out)[1] = edge.toolbar_height + edge.toolbar_overlap;
        } else {
            *out = [edge.button_size, edge.button_size];
        }
    }
    result
}

static ORIGINAL_SET_VISIBLE: AtomicUsize = AtomicUsize::new(0);

/// Whether `view` is the second back or forward button of its toolbar, which is the forward button.
unsafe fn is_forward_button(view: usize) -> bool {
    let (Some(edge), Some(image)) = (bv(), image::recorded()) else { return false };
    if *(view as *const usize) != image.base + edge.button_vtables[0] {
        return false;
    }
    let parent = *((view + 0x198) as *const usize);
    if parent < 0x10000 {
        return false;
    }
    let kids = *((parent + 0x1A0) as *const usize) as *const usize;
    let count = *((parent + 0x1A8) as *const usize);
    if kids.is_null() || count > 64 {
        return false;
    }
    let buttons: Vec<usize> = (0..count)
        .map(|n| *kids.add(n))
        .filter(|child| *child > 0x10000 && *(*child as *const usize) == image.base + edge.button_vtables[0])
        .collect();
    buttons.get(1) == Some(&view)
}

/// Edge hides the forward button until a page can go forward. The design shows it, greyed, so a
/// Rivet window keeps it visible. Showing a view that is already shown changes nothing, so the
/// toolbar does not lay itself out again.
unsafe extern "system" fn set_visible(view: *mut c_void, visible: bool) {
    type Original = unsafe extern "system" fn(*mut c_void, bool);
    let original: Original = std::mem::transmute(ORIGINAL_SET_VISIBLE.load(Ordering::Acquire));
    let keep = !visible && !skipped("edgeforward") && is_rivet_view_tree(view as usize) && is_forward_button(view as usize);
    original(view, visible || keep);
}

static ORIGINAL_PREFERRED: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_MINIMUM: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_SET_BOUNDS: AtomicUsize = AtomicUsize::new(0);

/// The x of a view in window coordinates: its own and every ancestor's.
unsafe fn window_x(view: usize) -> i32 {
    let mut x = 0;
    let mut at = view;
    for _ in 0..16 {
        if at < 0x10000 {
            break;
        }
        x += *((at + 0x1C8) as *const i32);
        at = *((at + 0x198) as *const usize);
    }
    x
}

/// The caption buttons' container of the frame above a view, found by the frame's vtable.
unsafe fn caption_container(view: usize) -> Option<usize> {
    let edge = bv()?;
    let layout = &crate::builds::get().layout;
    let base = image::recorded()?.base;
    let mut at = *((view + 0x198) as *const usize);
    for _ in 0..8 {
        if at < 0x10000 {
            return None;
        }
        if *(at as *const usize) == base + layout.frame_vtable {
            let container = *((at + layout.caption_container_offset) as *const usize);
            return (container > 0x10000).then_some(container);
        }
        at = *((at + 0x198) as *const usize);
    }
    let _ = edge;
    None
}

/// A Rivet window's toolbar starts at the page's edge and stops short of the caption buttons, as
/// Chrome's does, and its strip is moved out of the window with its children.
unsafe extern "system" fn set_bounds_rect(view: *mut c_void, rect: *const [i32; 4]) {
    type Original = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);
    let original: Original = std::mem::transmute(ORIGINAL_SET_BOUNDS.load(Ordering::Acquire));
    let (Some(edge), Some(image)) = (bv(), image::recorded()) else { return original(view, rect) };
    let mut moved = *rect;
    // A hidden view that the browser places itself, such as the chat button inside the address field,
    // is put out of the window: its layout asks it for no room, but a view paints its children and
    // layers wherever it is, so size alone does not hide it.
    if !skipped("edgebounds") && crate::chrome_toolbar::is_hidden_view(view) {
        // Moved out of the window, with its children and any layer they paint into.
        moved[0] -= 20000;
        return original(view, &moved);
    }
    if is_rivet_container(view as usize) && !skipped("edgebounds") {
        moved[0] -= 20000;
        return original(view, &moved);
    }
    if *(view as *const usize) == image.base + edge.toolbar_vtable && !skipped("edgebounds") {
        let parent = *((view as usize + 0x198) as *const usize);
        if parent > 0x10000 && is_rivet_view_tree(view as usize) {
            // The toolbar keeps running to the container's right edge, under the caption buttons, as
            // Edge's own does: the window's backdrop shows through whatever it leaves bare.
            let left = edge.toolbar_leading;
            let right = *((parent + 0x1C8 + 8) as *const i32);
            if left >= 0 && right > left {
                // The browser lifts the toolbar by the overlap and makes its container as much shorter.
                // The toolbar's own height is what the design gives the row, from the container's top.
                moved = [left, 0, right - left, edge.toolbar_height];
            }
        }
    }
    // The toolbar's own children keep the design's positions. Their order in the toolbar is back,
    // forward, reload.
    if !skipped("edgechildren") && is_rivet_view_tree(view as usize) {
        let table = *(view as *const usize);
        let parent = *((view as usize + 0x198) as *const usize);
        if parent > 0x10000 && *(parent as *const usize) == image.base + edge.toolbar_vtable {
            if edge.button_vtables.iter().any(|vtable| table == image.base + vtable) {
                let kids = *((parent + 0x1A0) as *const usize) as *const usize;
                let count = *((parent + 0x1A8) as *const usize);
                let mut index = 0;
                if !kids.is_null() && count <= 64 {
                    for n in 0..count {
                        let child = *kids.add(n);
                        if child == view as usize {
                            break;
                        }
                        if child > 0x10000 && edge.button_vtables.iter().any(|vtable| *(child as *const usize) == image.base + vtable) {
                            index += 1;
                        }
                    }
                }
                moved = [edge.button_x + index * edge.button_pitch, edge.button_y, edge.button_size, edge.button_size];
            } else if table == image.base + edge.field_vtable {
                let toolbar_width = *((parent + 0x1C8 + 8) as *const i32);
                let mut width = toolbar_width - edge.field_x - edge.field_margin;
                if let Some(container) = caption_container(view as usize) {
                    // The field stops short of the caption buttons by the design's gap.
                    let caption_x = *((container + 0x1C8) as *const i32);
                    width = caption_x - edge.caption_gap - window_x(parent) - edge.field_x;
                }
                moved = [edge.field_x, edge.field_y, width, edge.field_height];
            }
        }
    }
    // Bounds changes lay a view out at once, outside any widget layout or paint, so the layout of a
    // Rivet window's views runs in the window's scope.
    if !skipped("edgescope") && !crate::chrome_toolbar::active() && is_rivet_view_tree(view as usize) {
        let widget = crate::chrome_toolbar::widget_holding(view);
        if crate::chrome_toolbar::is_rivet(widget) {
            return crate::chrome_toolbar::scoped(widget, || original(view, &moved));
        }
    }
    original(view, &moved)
}

/// Whether a view lies in the browser view of a Rivet window.
unsafe fn is_rivet_view_tree(view: usize) -> bool {
    let mut at = view;
    for _ in 0..16 {
        if at < 0x10000 {
            return false;
        }
        if VIEWS.lock().unwrap_or_else(|error| error.into_inner()).iter().any(|(_, known, _)| at >= *known && at < *known + 0x400) {
            return true;
        }
        at = *((at + 0x198) as *const usize);
    }
    false
}


/// A Rivet window's strip asks for no width: the appbar takes its place, and the browser lays the
/// toolbar and the page out from the appbar's edge.
unsafe extern "system" fn preferred_size(container: *mut c_void, out: *mut [i32; 2]) -> *mut [i32; 2] {
    type Original = unsafe extern "system" fn(*mut c_void, *mut [i32; 2]) -> *mut [i32; 2];
    let original: Original = std::mem::transmute(ORIGINAL_PREFERRED.load(Ordering::Acquire));
    let result = original(container, out);
    if is_rivet_container(container as usize) && !skipped("edgewidth") {
        (*out)[0] = 0;
    }
    result
}

unsafe extern "system" fn minimum_size(container: *mut c_void, out: *mut [i32; 2]) -> *mut [i32; 2] {
    type Original = unsafe extern "system" fn(*mut c_void, *mut [i32; 2]) -> *mut [i32; 2];
    if is_rivet_container(container as usize) && !skipped("edgewidth") {
        *out = [0, 0];
        return out;
    }
    let original: Original = std::mem::transmute(ORIGINAL_MINIMUM.load(Ordering::Acquire));
    original(container, out)
}

/// Trace-only switches in `rivet-toolbar-skip.txt`, which let one change be left out while a
/// capture is compared.
pub(crate) fn skipped(token: &str) -> bool {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        unsafe { crate::frame::runtime_directory() }
            .and_then(|dir| std::fs::read_to_string(dir.join("rivet-toolbar-skip.txt")).ok())
            .unwrap_or_default()
    })
    .contains(token)
}

/// Checks the recorded functions and replaces the title bar question. Safe to call again.
pub unsafe fn install() -> Result<(), String> {
    INSTALLED
        .get_or_init(|| {
            let edge = bv().ok_or("this browser build has no Edge vertical tabs")?;
            let image = image::recorded().ok_or("this browser build has no recorded image")?;
            for (rva, bytes) in [
                (edge.content_window_for_hwnd, edge.content_window_for_hwnd_prologue),
                (edge.browser_view_for_window, edge.browser_view_for_window_prologue),
                (edge.set_state, edge.set_state_prologue),
            ] {
                if !image.has(rva, bytes) {
                    return Err(format!("{rva:#x} is not the recorded Edge function"));
                }
            }
            let hook = should_hide_titlebar as *const () as usize;
            ORIGINAL_HIDE.store(
                detour(image.base, edge.hide_titlebar, edge.hide_titlebar_prologue, Some(edge.cookie_at), hook)?,
                Ordering::Release,
            );
            ORIGINAL_PREFERRED.store(
                detour(image.base, edge.container_preferred_size, edge.container_preferred_size_prologue, None, preferred_size as *const () as usize)?,
                Ordering::Release,
            );
            ORIGINAL_MINIMUM.store(
                detour(image.base, edge.container_minimum_size, edge.container_minimum_size_prologue, Some(edge.container_minimum_cookie_at), minimum_size as *const () as usize)?,
                Ordering::Release,
            );
            ORIGINAL_SET_BOUNDS.store(
                detour(image.base, edge.set_bounds_rect, edge.set_bounds_rect_prologue, None, set_bounds_rect as *const () as usize)?,
                Ordering::Release,
            );
            let toolbar = &crate::builds::get().toolbar;
            if image.has(toolbar.set_visible, toolbar.set_visible_prologue) {
                ORIGINAL_SET_VISIBLE.store(
                    detour(image.base, toolbar.set_visible, toolbar.set_visible_prologue, None, set_visible as *const () as usize)?,
                    Ordering::Release,
                );
            }
            for vtable in edge.button_vtables.iter().copied().chain([edge.toolbar_vtable]) {
                let slot = ((image.base + vtable) as *mut usize).add(edge.size_slot);
                let held = *slot;
                SIZED.lock().unwrap_or_else(|error| error.into_inner()).push((image.base + vtable, held));
                crate::chrome_toolbar::replace_slot(slot, sized_preferred as *const () as usize)?;
            }
            Ok(())
        })
        .clone()
}

/// The browser view of a window, found through the navigation functions both builds record.
unsafe fn browser_view(window: HWND) -> Option<*mut c_void> {
    let navigation = &crate::builds::get().navigation;
    let base = image::recorded()?.base;
    let content: Pointer = std::mem::transmute(base + navigation.content_window_for_hwnd);
    let content = content(window as *const c_void);
    if content.is_null() {
        return None;
    }
    let view: Pointer = std::mem::transmute(base + navigation.browser_view_for_window);
    let view = view(content);
    (!view.is_null()).then_some(view)
}

/// Trace only: prints the view tree of a window's browser view, for any build.
pub unsafe fn dump_window(window: HWND) {
    let Some(image) = image::recorded() else { return };
    let Some(view) = browser_view(window) else { return };
    let words = view as *const usize;
    let table = image.base + crate::builds::get().layout.browser_view_view_vtable;
    match (0..0x100).find(|index| *words.add(*index) == table) {
        Some(index) => {
            let mut ancestor = words.add(index) as usize;
            while ancestor > 0x10000 {
                let bounds = *((ancestor + 0x1C8) as *const [i32; 4]);
                let table = (*(ancestor as *const usize)).wrapping_sub(image.base);
                crate::vertical_tabs::trace(&format!("ancestor {ancestor:#x} vtable {table:#x} bounds {bounds:?}"));
                ancestor = *((ancestor + 0x198) as *const usize);
            }
            crate::vertical_tabs::trace("browser view children");
            dump_children(words.add(index) as usize, 0);
            let frame = *((words.add(index) as usize + 0x198) as *const usize);
            crate::vertical_tabs::trace("frame view children");
            dump_children(frame, 0);
        }
        None => crate::vertical_tabs::trace("no view base found"),
    }
}

/// Puts the window into Edge's vertical tabs, on the browser UI thread.
pub unsafe fn adopt(window: HWND) -> Result<(), String> {
    install()?;
    let edge = bv().ok_or("this browser build has no Edge vertical tabs")?;
    let base = image::recorded().ok_or("no image")?.base;
    let view = browser_view(window).ok_or("the window has no browser view")?;
    VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push((window as isize, view as usize, 0));
    let container = *(view.cast::<u8>().add(edge.container_offset) as *const usize);
    if container == 0 {
        return Err("the browser view has no vertical tab container".into());
    }
    if let Some(entry) = VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter_mut()
        .find(|entry| entry.0 == window as isize)
    {
        entry.2 = container;
    }
    let set: SetState = std::mem::transmute(base + edge.set_state);
    // The state change lays the window out at once, so it runs in the window's scope.
    crate::chrome_toolbar::register(window);
    crate::chrome_toolbar::for_window(window, || set(view, edge.vertical_state, false));
    // The strip has no width, so its children would spill over the page. The appbar replaces it.
    let toolbar = &crate::builds::get().toolbar;
    if image::recorded().is_some_and(|image| image.has(toolbar.set_visible, toolbar.set_visible_prologue)) {
        let visible: unsafe extern "system" fn(*mut c_void, bool) = std::mem::transmute(base + toolbar.set_visible);
        visible(container as *mut c_void, false);
    }
    crate::vertical_tabs::trace(&format!(
        "edge vertical tabs: view={view:?} container={container:#x} state={}",
        *((container + edge.state_offset) as *const u32)
    ));
    if crate::vertical_tabs::tracing() && skipped("dump") {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(4));
            crate::vertical_tabs::trace("browser view children after 4 s");
            unsafe {
                let parent = *((container + 0x198) as *const usize);
                crate::vertical_tabs::trace(&format!("parent of the strip {parent:#x}"));
                let raw: Vec<String> = (0x190 / 8..0x1E8 / 8).map(|index| format!("{:#x}", *((container + index * 8) as *const usize))).collect();
                crate::vertical_tabs::trace(&format!("strip words 0x190.. {}", raw.join(" ")));
                let base = image::recorded().map_or(0, |image| image.base);
                crate::vertical_tabs::trace(&format!("parent vtable {:#x} container vtable {:#x}", (*(parent as *const usize)).wrapping_sub(base), (*(container as *const usize)).wrapping_sub(base)));
                dump_children(parent, 0);
            }
        });
    }
    Ok(())
}

/// Makes every view of a Rivet window's browser view ask for its size again, in the window's scope.
/// The browser sized its toolbar before the window was a Rivet window, and keeps what it asked for.
pub unsafe fn relayout(window: HWND) {
    if bv().is_none() || skipped("edgerelayout") {
        return;
    }
    let Some(view) = browser_view(window) else { return };
    let Some(image) = image::recorded() else { return };
    let words = view as *const usize;
    let table = image.base + crate::builds::get().layout.browser_view_view_vtable;
    let Some(index) = (0..0x100).find(|index| *words.add(*index) == table) else { return };
    let root = words.add(index) as usize;
    unsafe fn walk(view: usize, depth: usize) {
        if depth > 6 {
            return;
        }
        let kids = *((view + 0x1A0) as *const usize) as *const usize;
        let count = *((view + 0x1A8) as *const usize);
        if kids.is_null() || count > 64 {
            return;
        }
        for n in 0..count {
            let child = *kids.add(n);
            if child > 0x10000 {
                walk(child, depth + 1);
            }
        }
        if !skipped("edgeforward") && ORIGINAL_SET_VISIBLE.load(Ordering::Acquire) != 0 && is_forward_button(view) {
            let show: unsafe extern "system" fn(*mut c_void, bool) = std::mem::transmute(ORIGINAL_SET_VISIBLE.load(Ordering::Acquire));
            show(view as *mut c_void, true);
        }
        crate::chrome_toolbar::invalidate_layout_of(view as *mut c_void);
    }
    crate::chrome_toolbar::for_window(window, || walk(root, 0));
}

/// Forgets a window that is going away.
pub fn forget(window: HWND) {
    VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|(known, _, _)| *known != window as isize);
}

/// Trace only: prints the children of a view with their vtable offsets and bounds, so a capture
/// can be compared with the browser's own layout. A view keeps its children as a data pointer at
/// `+0x1A0` and a count at `+0x1A8`.
pub unsafe fn dump_children(view: usize, depth: usize) {
    let Some(image) = image::recorded() else { return };
    let kids = *((view + 0x1A0) as *const usize) as *const usize;
    let count = *((view + 0x1A8) as *const usize);
    if kids.is_null() || count > 64 {
        return;
    }
    for n in 0..count {
        let child = *kids.add(n);
        if child < 0x10000 {
            continue;
        }
        let table = (*(child as *const usize)).wrapping_sub(image.base);
        let bounds = *((child + 0x1C8) as *const [i32; 4]);
        let visible = *((child + 0x1D8) as *const u8);
        crate::vertical_tabs::trace(&format!(
            "{}child {child:#x} vtable {table:#x} bounds {bounds:?} visible {visible} windowx {}",
            "  ".repeat(depth),
            window_x(child)
        ));
        if depth < 4 {
            dump_children(child, depth + 1);
        }
    }
}
