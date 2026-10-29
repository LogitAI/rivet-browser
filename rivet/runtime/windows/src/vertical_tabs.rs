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

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::VerticalTabs {
    &crate::builds::get().vertical
}

/// The vtable slot of the controller's scalar deleting destructor, two words before
/// `ShouldDisplayVerticalTabs`, and the function it holds.

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
    update_collapse_action: Option<ControllerCall>,
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

/// Diagnostic: logs access violations with their offset in the browser module, so a crash on a
/// new build shows which recorded address was wrong. Only while the trace is on.
pub unsafe fn trace_crashes() {
    #[repr(C)]
    struct Record {
        code: u32,
        flags: u32,
        chained: *mut Record,
        address: *mut c_void,
        parameters: u32,
        information: [usize; 15],
    }
    #[repr(C)]
    struct Pointers {
        record: *mut Record,
        context: *mut c_void,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn RtlCaptureStackBackTrace(skip: u32, count: u32, frames: *mut *mut c_void, hash: *mut u32) -> u16;
        fn GetModuleHandleExW(flags: u32, address: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
        fn AddVectoredExceptionHandler(
            first: u32,
            handler: unsafe extern "system" fn(*mut Pointers) -> i32,
        ) -> *mut c_void;
    }
    unsafe extern "system" fn handler(info: *mut Pointers) -> i32 {
        let record = &*(*info).record;
        // Access violations, other failures (illegal instruction, fast fail, divide by zero) and breakpoints.
        if record.code == 0xC000_0005 || (record.code & 0xFFFF_0000 == 0xC000_0000 && record.code != 0xC000_0194) || record.code == 0x8000_0003 {
            let rip = record.address as usize;
            let base = crate::chrome_layout::image::recorded().map_or(0, |image| image.base);
            trace(&format!(
                "exception {:#x} at {rip:#x} (module offset {:#x}) touching {:#x}",
                record.code,
                rip.wrapping_sub(base),
                record.information[1]
            ));
            let mut frames = [std::ptr::null_mut::<c_void>(); 24];
            let count = RtlCaptureStackBackTrace(0, 24, frames.as_mut_ptr(), std::ptr::null_mut());
            for frame in &frames[..count as usize] {
                let mut module = std::ptr::null_mut();
                // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | UNCHANGED_REFCOUNT
                GetModuleHandleExW(0x4 | 0x2, *frame, &mut module);
                let mut name = [0u16; 260];
                let length = GetModuleFileNameW(module, name.as_mut_ptr(), 260) as usize;
                let name = String::from_utf16_lossy(&name[..length]);
                trace(&format!(
                    "  frame {:#x} {} +{:#x}",
                    *frame as usize,
                    name.rsplit('\\').next().unwrap_or(""),
                    (*frame as usize).wrapping_sub(module as usize)
                ));
            }
        }
        0
    }
    AddVectoredExceptionHandler(1, handler);
    // A panic inside a hook would abort the browser without a word, so it is written down first.
    std::panic::set_hook(Box::new(|info| trace(&format!("panic: {info}"))));
}

pub fn tracing() -> bool {
    TRACE.get().is_some()
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
    if *controller.cast::<u8>().add(bv().mode_offset) == 0 {
        if note_report(controller as usize) {
            trace(&format!("reported controller={controller:?}"));
            (hook.report)(controller as usize);
        }
    }
    (hook.original)(controller)
}

/// The window a controller belongs to. Only safe once the window is built.
unsafe fn controller_window(hook: &Hook, controller: *const c_void) -> Option<HWND> {
    let browser = *(controller.cast::<u8>().add(bv().browser_offset) as *const *const c_void);
    if browser.is_null() {
        return None;
    }
    let view = (hook.view_for_browser)(browser);
    if view.is_null() {
        return None;
    }
    let base = image::recorded()?.base;
    let words = view as *const usize;
    let index = (0..0x100).find(|index| *words.add(*index) == base + bv().browser_view_view_vtable)?;
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
    let mode = pointer.cast::<u8>().add(bv().mode_offset);
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
    if let Some(update) = hook.update_collapse_action {
        update(pointer);
    }
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
        *((controller + bv().width_offset) as *const i32)
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
    if !bv().supported {
        return Err("this browser build has no recorded vertical-tab controller".into());
    }
    let image = image::recorded().ok_or("this browser build has no recorded vertical tabs")?;
    let base = image.base;
    if !image.points_to(bv().slot, bv().should_display) {
        return Err("this browser build has no recorded vertical-tab controller".into());
    }
    if !image.points_to(bv().slot - 16, bv().destructor) || !image.has(bv().destructor, &bv().destructor_prologue) {
        return Err("this browser build has no recorded vertical-tab controller destructor".into());
    }
    // Zero means the build has no separate collapse action to refresh.
    for function in [
        bv().browser_view_for_browser,
        bv().hwnd_for_view,
        bv().notify_mode_changed,
        bv().update_collapse_action,
    ] {
        if function != 0 && image.at(function, 1).is_none() {
            return Err("a recorded vertical-tab function is outside the browser image".into());
        }
    }
    let slot = image.at(bv().slot, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let destructor = image.at(bv().slot - 16, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let hook = Hook {
        original: std::mem::transmute::<usize, ShouldDisplay>(slot.read()),
        view_for_browser: std::mem::transmute::<usize, ViewForBrowser>(
            base + bv().browser_view_for_browser,
        ),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(base + bv().hwnd_for_view),
        notify_mode_changed: std::mem::transmute::<usize, ControllerCall>(
            base + bv().notify_mode_changed,
        ),
        update_collapse_action: (bv().update_collapse_action != 0).then(|| {
            std::mem::transmute::<usize, ControllerCall>(base + bv().update_collapse_action)
        }),
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
static REGION_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

static REGIONS: Mutex<Vec<(isize, usize)>> = Mutex::new(Vec::new());

fn remember_region(window: HWND, view: *mut c_void) {
    let mut regions = locked(&REGIONS);
    match regions.iter_mut().find(|entry| entry.0 == window as isize) {
        Some(entry) => entry.1 = view as usize,
        None => regions.push((window as isize, view as usize)),
    }
}


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
    if !image.has(bv().invalidate_layout, &bv().invalidate_layout_prologue) {
        return;
    }
    let invalidate: unsafe extern "system" fn(*mut c_void, i32) =
        std::mem::transmute(image.base + bv().invalidate_layout);
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

static REGION_MINIMUM_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A Rivet window's strip is never required to be wider than the width Rivet gives it.
unsafe extern "system" fn region_minimum_size(view: *mut c_void, out: *mut [i32; 2]) -> *mut c_void {
    type Original = unsafe extern "system" fn(*mut c_void, *mut [i32; 2]) -> *mut c_void;
    let original: Original = std::mem::transmute(REGION_MINIMUM_ORIGINAL.load(Ordering::Acquire));
    let result = original(view, out);
    if !locked(&STRIP_WIDTHS).is_empty() {
        if let Some(Ok(hook)) = HOOK.get() {
            if let Some(width) = strip_override((hook.hwnd_for_view)(view)) {
                (*out)[0] = (*out)[0].min(width);
            }
        }
    }
    result
}

/// `jmp qword ptr [rip + 0]` followed by the absolute target.
pub(crate) fn absolute_jump(target: usize) -> [u8; 14] {
    let mut code = [0u8; 14];
    code[..6].copy_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
    code[6..].copy_from_slice(&target.to_le_bytes());
    code
}

unsafe fn patch_region_minimum(image: &image::Image) -> Result<(), String> {
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    };
    let length = bv().region_minimum_prologue.len();
    if !image.has(bv().region_minimum_size, &bv().region_minimum_prologue) {
        return Err("the region view's minimum size is not the recorded function".into());
    }
    let function = image.at(bv().region_minimum_size, length).ok_or("function outside the image")? as *mut u8;
    let trampoline = VirtualAlloc(std::ptr::null(), 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
        as *mut u8;
    if trampoline.is_null() {
        return Err("could not allocate the minimum size trampoline".into());
    }
    let resume = absolute_jump(function as usize + bv().region_minimum_prologue.len());
    std::ptr::copy_nonoverlapping(bv().region_minimum_prologue.as_ptr(), trampoline, bv().region_minimum_prologue.len());
    std::ptr::copy_nonoverlapping(resume.as_ptr(), trampoline.add(bv().region_minimum_prologue.len()), resume.len());
    REGION_MINIMUM_ORIGINAL.store(trampoline as usize, Ordering::Release);
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(function.cast(), 19, PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the region view's minimum size".into());
    }
    let jump = absolute_jump(region_minimum_size as *const () as usize);
    std::ptr::copy_nonoverlapping(jump.as_ptr(), function, jump.len());
    // Pad the rest of the replaced instructions with nops.
    std::ptr::write_bytes(function.add(jump.len()), 0x90, bv().region_minimum_prologue.len() - jump.len());
    VirtualProtect(function.cast(), 19, previous, &mut previous);
    windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
        windows_sys::Win32::System::Threading::GetCurrentProcess(),
        function.cast(),
        bv().region_minimum_prologue.len(),
    );
    Ok(())
}

unsafe fn patch_region_size(image: &image::Image) -> Result<(), String> {
    if !image.points_to(bv().region_preferred_size_slot, bv().region_preferred_size) {
        return Err("the region view's size slot is not the recorded function".into());
    }
    let slot = image.at(bv().region_preferred_size_slot, 8).ok_or("slot outside the image")? as *mut usize;
    REGION_ORIGINAL.store(slot.read(), Ordering::Release);
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the region view's vtable".into());
    }
    slot.write(region_preferred_size as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    patch_region_minimum(image)?;
    Ok(())
}

/// Controllers reported so far, for a Rivet window that attaches after its own was reported.
pub fn reported() -> Vec<usize> {
    locked(&REPORTED).clone()
}

/// Experiment for the other direction: makes `BrowserView::ShouldDrawTabStrip` answer no for
/// every window of the process by overwriting its first instructions. Never for production.
pub unsafe fn hide_tab_strip_everywhere() -> Result<(), String> {
    let image = image::recorded().ok_or("this browser build has no recorded tab strip")?;
    if !image.has(bv().should_draw_tab_strip, &bv().should_draw_tab_strip_prologue) {
        return Err("ShouldDrawTabStrip is not the recorded function".into());
    }
    let code = (image.base + bv().should_draw_tab_strip) as *mut u8;
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

    #[test]
    fn fr12_minimum_size_detour_jumps_through_an_absolute_address() {
        let jump = absolute_jump(0x1122_3344_5566_7788);
        assert_eq!(&jump[..6], &[0xff, 0x25, 0, 0, 0, 0]);
        assert_eq!(&jump[6..], &0x1122_3344_5566_7788usize.to_le_bytes());
        assert!(jump.len() <= bv().region_minimum_prologue.len());
    }
}
