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
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
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
/// Controllers already reported, so each is reported once.
static REPORTED: Mutex<Vec<usize>> = Mutex::new(Vec::new());
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

unsafe extern "system" fn should_display(controller: *const c_void) -> bool {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    if *controller.cast::<u8>().add(MODE_OFFSET) == 0 {
        let fresh = {
            let mut reported = locked(&REPORTED);
            let fresh = !reported.contains(&(controller as usize));
            if fresh {
                reported.push(controller as usize);
            }
            fresh
        };
        if fresh {
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
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
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

/// The vertical tab strip width Chromium lays out under the appbar of `window`, in DIPs, or
/// `None` while the window shows horizontal tabs.
pub fn strip_width(window: HWND) -> Option<i32> {
    let controller = *locked(&SWITCHED).as_ref()?.get(&(window as isize))?;
    Some(unsafe {
        let width = *((controller + WIDTH_OFFSET) as *const i32);
        if width == 0 {
            125
        } else {
            width
        }
    })
}

/// Forgets a window that is going away, so its controller is never read again.
pub fn forget(window: HWND) {
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
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    if base == 0 {
        return Err("this browser process has no chrome.dll".into());
    }
    let slot = (base + SLOT) as *mut usize;
    if slot.read() != base + SHOULD_DISPLAY {
        return Err("this browser build has no recorded vertical-tab controller".into());
    }
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
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the vertical-tab controller vtable".into());
    }
    slot.write(should_display as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    trace("installed");
    Ok(hook)
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
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    let code = (base + SHOULD_DRAW_TAB_STRIP) as *mut u8;
    if std::slice::from_raw_parts(code, 3) != PROLOGUE {
        return Err("ShouldDrawTabStrip is not the recorded function".into());
    }
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
