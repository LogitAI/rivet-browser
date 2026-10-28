//! Experiment: Chromium's own vertical-tab layout for Rivet windows only.
//!
//! In vertical-tab mode Chromium puts its toolbar and the caption buttons in one top row and the
//! tab strip on the left. Rivet wants exactly that frame, but the mode is a profile setting and
//! would change every window of the profile. Each browser window owns a controller that keeps the
//! mode. Rivet switches the controllers of Rivet windows the way a setting change would, without
//! writing the setting.
//!
//! Chromium asks a controller for its mode during every layout, including layouts that run while
//! the window is still being built. Resolving the window there is not safe, so the controller is
//! only reported, and each Rivet window checks later, on its own thread, whether the controller is
//! its own.
//!
//! The addresses below belong to Google Chrome 156.0.8078.48 only. They move to the supported-build
//! table once the experiment holds.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use crate::chrome_layout::image;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

/// The vtable slot of `VerticalTabStripStateControllerImpl::ShouldDisplayVerticalTabs`.
const SLOT: usize = 0x0FDA_FE00;
const SHOULD_DISPLAY: usize = 0x0369_C6E0;
/// `BrowserView::GetBrowserViewForBrowser`.
const BROWSER_VIEW_FOR_BROWSER: usize = 0x0034_D3E0;
/// `views::HWNDForView`.
const HWND_FOR_VIEW: usize = 0x036A_A950;
/// `VerticalTabStripStateController::NotifyModeChanged`.
const NOTIFY_MODE_CHANGED: usize = 0x0A1D_B330;
/// `VerticalTabStripStateControllerImpl::UpdateCollapseActionItem`.
const UPDATE_COLLAPSE_ACTION: usize = 0x020F_3D90;
/// The vtable slot of the controller's scalar deleting destructor, two words before
/// `ShouldDisplayVerticalTabs`, and the function it holds.
const DESTRUCTOR_SLOT: usize = SLOT - 16;
const DESTRUCTOR: usize = 0x0296_C3F0;
const DESTRUCTOR_PROLOGUE: [u8; 12] =
    [0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x89, 0xd7, 0x48, 0x89, 0xce, 0xe8];
/// Where the controller keeps its browser, its mode and its strip width.
const BROWSER_OFFSET: usize = 0x1B8;
const MODE_OFFSET: usize = 0x1E8;
const WIDTH_OFFSET: usize = 0x1CC;
/// `BrowserView`'s vtable for its `views::View` base. The browser view is reached through another
/// base, so the view base is found by this vtable before asking for the window.
const BROWSER_VIEW_VIEW_VTABLE: usize = 0x0FE7_BF68;

type ShouldDisplay = unsafe extern "system" fn(*const c_void) -> bool;
type ViewForBrowser = unsafe extern "system" fn(*const c_void) -> *const c_void;
type HwndForView = unsafe extern "system" fn(*const c_void) -> HWND;
type ControllerCall = unsafe extern "system" fn(*mut c_void);
type Destructor = unsafe extern "system" fn(*mut c_void, u32) -> *mut c_void;

/// Reports a controller that still shows horizontal tabs, so Rivet windows can check it later.
pub type Report = fn(usize);

struct Hook {
    original: ShouldDisplay,
    view_for_browser: ViewForBrowser,
    hwnd_for_view: HwndForView,
    notify_mode_changed: ControllerCall,
    update_collapse_action: ControllerCall,
    report: Report,
}

static HOOK: OnceLock<Result<Hook, String>> = OnceLock::new();
/// Controllers already reported and not yet destroyed, so each is reported once and a controller
/// that was freed is never dereferenced. Chromium destroys a controller on the UI thread, where
/// the destructor hook removes it, and every use of a controller address checks this list first.
static REPORTED: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static DESTRUCTOR_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The switched controller of each Rivet window.
static SWITCHED: Mutex<Option<HashMap<isize, usize>>> = Mutex::new(None);
static TRACE: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Turns the experiment trace on. The file's presence next to the runtime is the switch.
pub fn set_trace(path: std::path::PathBuf) {
    let _ = TRACE.set(path);
}

pub fn trace(message: &str) {
    use std::io::Write;
    let Some(path) = TRACE.get() else { return };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}

fn locked<T>(mutex: &'static Mutex<T>) -> MutexGuard<'static, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Records a controller. Returns true the first time it is seen.
fn note_report(controller: usize) -> bool {
    let mut reported = locked(&REPORTED);
    let fresh = !reported.contains(&controller);
    if fresh {
        reported.push(controller);
    }
    fresh
}

/// Whether the controller has been reported and not destroyed since.
fn is_live(controller: usize) -> bool {
    locked(&REPORTED).contains(&controller)
}

/// Forgets a controller that is being destroyed, so nothing keeps its address.
fn note_destroyed(controller: usize) {
    locked(&REPORTED).retain(|known| *known != controller);
    if let Some(map) = locked(&SWITCHED).as_mut() {
        map.retain(|_, switched| *switched != controller);
    }
}

unsafe extern "system" fn controller_destroyed(controller: *mut c_void, flags: u32) -> *mut c_void {
    note_destroyed(controller as usize);
    let original: Destructor = std::mem::transmute(DESTRUCTOR_ORIGINAL.load(Ordering::Acquire));
    original(controller, flags)
}

unsafe extern "system" fn should_display(controller: *const c_void) -> bool {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    if *controller.cast::<u8>().add(MODE_OFFSET) == 0 {
        if note_report(controller as usize) {
            trace(&format!("reported controller={controller:?}"));
            (hook.report)(controller as usize);
        }
    }
    (hook.original)(controller)
}

/// The window a controller belongs to. Only safe once the window is built.
unsafe fn controller_window(hook: &Hook, controller: *const c_void) -> Option<HWND> {
    let browser = *(controller.cast::<u8>().add(BROWSER_OFFSET) as *const *const c_void);
    if browser.is_null() {
        return None;
    }
    let view = (hook.view_for_browser)(browser);
    if view.is_null() {
        return None;
    }
    let base = image::recorded()?.base;
    let words = view as *const usize;
    let index = (0..0x100).find(|index| *words.add(*index) == base + BROWSER_VIEW_VIEW_VTABLE)?;
    trace(&format!("view base at +{:#x}", index * 8));
    let window = (hook.hwnd_for_view)(words.add(index).cast());
    (!window.is_null()).then_some(window)
}

/// Switches a reported controller into vertical tabs if it belongs to `window`, the way a setting
/// change does: store the mode, tell the window to rebuild, refresh the collapse action. Runs on
/// the browser UI thread, outside layout.
pub unsafe fn offer(window: HWND, controller: usize) {
    let Some(Ok(hook)) = HOOK.get() else { return };
    // The address may have been posted before the controller was destroyed.
    if !is_live(controller) {
        return;
    }
    let pointer = controller as *mut c_void;
    let owner = controller_window(hook, pointer);
    trace(&format!(
        "offer window={window:?} controller={pointer:?} owner={owner:?}"
    ));
    if owner != Some(window) {
        return;
    }
    let mode = pointer.cast::<u8>().add(MODE_OFFSET);
    if *mode != 0 {
        return;
    }
    locked(&SWITCHED)
        .get_or_insert_with(HashMap::new)
        .insert(window as isize, controller);
    *mode = 1;
    // Keep Chromium's vertical-tab mode for its toolbar composition. The frame layout hook
    // accounts for the strip when it moves the page to the appbar edge.
    (hook.notify_mode_changed)(pointer);
    (hook.update_collapse_action)(pointer);
}

/// The width Rivet gives the vertical tab strip of each window, in DIPs. Chromium keeps the strip
/// in its layout even though a Rivet window hides it, so the strip is made exactly as wide as the
/// appbar that covers it, or nothing while the appbar is hidden. Nothing is written to the
/// browser's own state or preferences.
static STRIP_WIDTHS: Mutex<Vec<(isize, i32)>> = Mutex::new(Vec::new());

pub fn set_strip_override(window: HWND, width: i32) {
    let mut widths = locked(&STRIP_WIDTHS);
    match widths.iter_mut().find(|entry| entry.0 == window as isize) {
        Some(entry) => entry.1 = width,
        None => widths.push((window as isize, width)),
    }
}

fn strip_override(window: HWND) -> Option<i32> {
    locked(&STRIP_WIDTHS)
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
}

/// The width the strip takes in the layout of `window`, or `None` while it shows horizontal tabs.
pub fn layout_width(window: HWND) -> Option<i32> {
    let controller = *locked(&SWITCHED).as_ref()?.get(&(window as isize))?;
    Some(strip_override(window).unwrap_or_else(|| unsafe {
        *((controller + WIDTH_OFFSET) as *const i32)
    }))
}

/// Forgets a window that is going away, so its controller is never read again.
pub fn forget(window: HWND) {
    locked(&STRIP_WIDTHS).retain(|entry| entry.0 != window as isize);
    locked(&REGIONS).retain(|entry| entry.0 != window as isize);
    let removed = locked(&SWITCHED)
        .as_mut()
        .and_then(|map| map.remove(&(window as isize)));
    if let Some(controller) = removed {
        locked(&REPORTED).retain(|known| *known != controller);
    }
}

pub unsafe fn install(report: Report) -> Result<(), String> {
    HOOK.get_or_init(|| patch(report))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

unsafe fn patch(report: Report) -> Result<Hook, String> {
    let image = image::recorded().ok_or("this browser build has no recorded vertical tabs")?;
    let base = image.base;
    if !image.points_to(SLOT, SHOULD_DISPLAY) {
        return Err("this browser build has no recorded vertical-tab controller".into());
    }
    if !image.points_to(DESTRUCTOR_SLOT, DESTRUCTOR) || !image.has(DESTRUCTOR, &DESTRUCTOR_PROLOGUE) {
        return Err("this browser build has no recorded vertical-tab controller destructor".into());
    }
    for function in [
        BROWSER_VIEW_FOR_BROWSER,
        HWND_FOR_VIEW,
        NOTIFY_MODE_CHANGED,
        UPDATE_COLLAPSE_ACTION,
    ] {
        if image.at(function, 1).is_none() {
            return Err("a recorded vertical-tab function is outside the browser image".into());
        }
    }
    let slot = image.at(SLOT, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let destructor = image.at(DESTRUCTOR_SLOT, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let hook = Hook {
        original: std::mem::transmute::<usize, ShouldDisplay>(slot.read()),
        view_for_browser: std::mem::transmute::<usize, ViewForBrowser>(
            base + BROWSER_VIEW_FOR_BROWSER,
        ),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(base + HWND_FOR_VIEW),
        notify_mode_changed: std::mem::transmute::<usize, ControllerCall>(
            base + NOTIFY_MODE_CHANGED,
        ),
        update_collapse_action: std::mem::transmute::<usize, ControllerCall>(
            base + UPDATE_COLLAPSE_ACTION,
        ),
        report,
    };
    // Both slots are replaced or neither is: a controller must never be reported without the
    // hook that forgets it.
    DESTRUCTOR_ORIGINAL.store(destructor.read(), Ordering::Release);
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(destructor.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the vertical-tab controller vtable".into());
    }
    destructor.write(controller_destroyed as *const () as usize);
    VirtualProtect(destructor.cast(), 8, previous, &mut previous);
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        destructor.write(DESTRUCTOR_ORIGINAL.load(Ordering::Acquire));
        return Err("could not unprotect the vertical-tab controller vtable".into());
    }
    slot.write(should_display as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    match patch_region_size(&image) {
        Ok(()) => trace("region size hook installed"),
        Err(error) => trace(&format!("region size hook unavailable: {error}")),
    }
    trace("installed");
    Ok(hook)
}
/// `VerticalTabStripRegionView::CalculatePreferredSize`, and the vtable slot that holds it.
const REGION_PREFERRED_SIZE: usize = 0x0BC7_FA80;
const REGION_PREFERRED_SIZE_SLOT: usize = 0x0FF1_7CF8;
static REGION_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

static REGIONS: Mutex<Vec<(isize, usize)>> = Mutex::new(Vec::new());

fn remember_region(window: HWND, view: *mut c_void) {
    let mut regions = locked(&REGIONS);
    match regions.iter_mut().find(|entry| entry.0 == window as isize) {
        Some(entry) => entry.1 = view as usize,
        None => regions.push((window as isize, view as usize)),
    }
}

/// `views::View::InvalidateLayout`, with the first bytes that identify it in this build.
const INVALIDATE_LAYOUT: usize = 0x0411_6480;
const INVALIDATE_LAYOUT_PROLOGUE: [u8; 12] =
    [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];

/// Makes the strip's view ask for its width again. Chromium caches the size it asked for, so a
/// width Rivet changed is only seen after this.
pub unsafe fn invalidate_strip(window: HWND) {
    let Some(view) = locked(&REGIONS)
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
    else {
        return;
    };
    let Some(image) = image::recorded() else { return };
    if !image.has(INVALIDATE_LAYOUT, &INVALIDATE_LAYOUT_PROLOGUE) {
        return;
    }
    let invalidate: unsafe extern "system" fn(*mut c_void, i32) =
        std::mem::transmute(image.base + INVALIDATE_LAYOUT);
    invalidate(view as *mut c_void, 0);
}

/// A Rivet window's strip asks for the width Rivet gives it instead of Chromium's own.
unsafe extern "system" fn region_preferred_size(
    view: *mut c_void,
    out: *mut [i32; 2],
    bounds: usize,
) -> *mut c_void {
    type Original = unsafe extern "system" fn(*mut c_void, *mut [i32; 2], usize) -> *mut c_void;
    let original: Original = std::mem::transmute(REGION_ORIGINAL.load(Ordering::Acquire));
    let result = original(view, out, bounds);
    if !locked(&STRIP_WIDTHS).is_empty() {
        if let Some(Ok(hook)) = HOOK.get() {
            let window = (hook.hwnd_for_view)(view);
            if let Some(width) = strip_override(window) {
                (*out)[0] = width;
                remember_region(window, view);
            }
        }
    }
    result
}

unsafe fn patch_region_size(image: &image::Image) -> Result<(), String> {
    if !image.points_to(REGION_PREFERRED_SIZE_SLOT, REGION_PREFERRED_SIZE) {
        return Err("the region view's size slot is not the recorded function".into());
    }
    let slot = image.at(REGION_PREFERRED_SIZE_SLOT, 8).ok_or("slot outside the image")? as *mut usize;
    REGION_ORIGINAL.store(slot.read(), Ordering::Release);
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the region view's vtable".into());
    }
    slot.write(region_preferred_size as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// Controllers reported so far, for a Rivet window that attaches after its own was reported.
pub fn reported() -> Vec<usize> {
    locked(&REPORTED).clone()
}

/// Experiment for the other direction: makes `BrowserView::ShouldDrawTabStrip` answer no for
/// every window of the process by overwriting its first instructions. Never for production.
pub unsafe fn hide_tab_strip_everywhere() -> Result<(), String> {
    const SHOULD_DRAW_TAB_STRIP: usize = 0x0528_6C00;
    const PROLOGUE: [u8; 3] = [0x56, 0x57, 0x53];
    let image = image::recorded().ok_or("this browser build has no recorded tab strip")?;
    if !image.has(SHOULD_DRAW_TAB_STRIP, &PROLOGUE) {
        return Err("ShouldDrawTabStrip is not the recorded function".into());
    }
    let code = (image.base + SHOULD_DRAW_TAB_STRIP) as *mut u8;
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    let execute_write = windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE;
    if VirtualProtect(code.cast(), 3, execute_write, &mut previous) == 0 {
        return Err("could not unprotect ShouldDrawTabStrip".into());
    }
    // xor eax, eax; ret
    std::ptr::copy_nonoverlapping([0x31u8, 0xC0, 0xC3].as_ptr(), code, 3);
    VirtualProtect(code.cast(), 3, previous, &mut previous);
    windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
        windows_sys::Win32::System::Threading::GetCurrentProcess(),
        code.cast(),
        3,
    );
    trace("tab strip hidden everywhere");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_destroyed_controller_is_no_longer_live() {
        let controller = 0x7000_1000;
        assert!(note_report(controller));
        assert!(!note_report(controller));
        assert!(is_live(controller));
        note_destroyed(controller);
        assert!(!is_live(controller));
        assert!(note_report(controller));
        note_destroyed(controller);
    }

    #[test]
    fn fr12_destroyed_controller_leaves_the_switched_windows() {
        let controller = 0x7000_2000;
        note_report(controller);
        locked(&SWITCHED)
            .get_or_insert_with(HashMap::new)
            .insert(0x77, controller);
        note_destroyed(controller);
        assert!(locked(&SWITCHED).as_ref().unwrap().get(&0x77).is_none());
        assert_eq!(layout_width(0x77 as HWND), None);
    }
}
