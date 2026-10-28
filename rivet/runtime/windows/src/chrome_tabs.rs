//! Chrome 156 tab operations on the browser UI thread. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
const CONTENT_WINDOW_FOR_HWND: usize = 0x00D5_64A0;
const BROWSER_VIEW_FOR_WINDOW: usize = 0x0BC8_1E10;
const ACTIVE_WEB_CONTENTS: usize = 0x0BC8_2340;
type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;
/// Tab strip and browser actions for Chrome 156.0.8078.48.
const GET_TAB_MODEL: usize = 0x04FB_01F0;
const TAB_COUNT: usize = 0x0369_CBE0;
const TAB_AT: usize = 0x0311_22A0;
const ACTIVATE_TAB: usize = 0x0311_2220;
const CLOSE_TAB: usize = 0x0A20_11B0;
const TITLE: usize = 0x00D7_8440;
const NEW_TAB: usize = 0x0A0B_8F50;
type Count = unsafe extern "system" fn(*mut c_void) -> i32;
type TabAt = unsafe extern "system" fn(*mut c_void, i32) -> *mut c_void;
type Activate = unsafe extern "system" fn(*mut c_void, i32, *const [u8; 16]);
type Close = unsafe extern "system" fn(*mut c_void, i32, u32);
type NewTab = unsafe extern "system" fn(*mut c_void, i32);

static BASE: OnceLock<Option<usize>> = OnceLock::new();
fn base() -> Option<usize> {
    *BASE.get_or_init(|| unsafe {
        let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
        if base == 0 {
            return None;
        }
        let signatures: &[(usize, &[u8])] = &[
            (
                CONTENT_WINDOW_FOR_HWND,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
            ),
            (BROWSER_VIEW_FOR_WINDOW, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (ACTIVE_WEB_CONTENTS, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
            (GET_TAB_MODEL, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
            (TAB_COUNT, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
            (TAB_AT, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
            (ACTIVATE_TAB, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
            (CLOSE_TAB, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (TITLE, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
            (NEW_TAB, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
        ];
        for (rva, bytes) in signatures {
            if std::slice::from_raw_parts((base + rva) as *const u8, bytes.len()) != *bytes {
                crate::vertical_tabs::trace(&format!("tabs: signature mismatch {rva:#x}"));
                return None;
            }
        }
        Some(base)
    })
}
unsafe fn function<T: Copy>(base: usize, rva: usize) -> T {
    std::mem::transmute_copy(&(base + rva))
}
unsafe fn web_contents(base: usize, window: HWND) -> Option<*mut c_void> {
    let content = function::<Pointer>(base, CONTENT_WINDOW_FOR_HWND)(window);
    if content.is_null() {
        return None;
    }
    let view = function::<Pointer>(base, BROWSER_VIEW_FOR_WINDOW)(content);
    if view.is_null() {
        return None;
    }
    let result = function::<Pointer>(base, ACTIVE_WEB_CONTENTS)(view);
    (!result.is_null()).then_some(result)
}
unsafe fn browser(base: usize, window: HWND) -> Option<*mut c_void> {
    let content = function::<Pointer>(base, CONTENT_WINDOW_FOR_HWND)(window);
    if content.is_null() {
        return None;
    }
    let view = function::<Pointer>(base, BROWSER_VIEW_FOR_WINDOW)(content);
    if view.is_null() {
        return None;
    }
    let holder = *(view.add(0x560) as *const *mut u8);
    if holder.is_null() {
        return None;
    }
    Some(holder.cast())
}

unsafe fn tab_model(base: usize, window: HWND) -> Option<*mut c_void> {
    let browser = browser(base, window)?;
    let model = function::<Pointer>(base, GET_TAB_MODEL)(browser);
    (!model.is_null()).then_some(model)
}

fn chromium_string(value: *const u8) -> Option<String> {
    unsafe {
        let last = *value.add(23);
        let (data, size) = if last & 0x80 != 0 {
            let words = value as *const usize;
            (*words as *const u16, *words.add(1))
        } else {
            (value as *const u16, last as usize)
        };
        if size > 1 << 16 {
            return None;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            data, size,
        )))
    }
}

/// Returns the browser's current tab titles and active index.
pub unsafe fn tabs(window: HWND) -> Option<(Vec<String>, usize)> {
    let base = base()?;
    let model = tab_model(base, window)?;
    let count = function::<Count>(base, TAB_COUNT)(model);
    if !(0..=128).contains(&count) {
        return None;
    }
    let mut titles = Vec::with_capacity(count as usize);
    for index in 0..count {
        let contents = function::<TabAt>(base, TAB_AT)(model, index);
        if contents.is_null() {
            titles.push(String::from("New tab"));
            continue;
        }
        let title = function::<Pointer>(base, TITLE)(contents).cast::<u8>();
        titles.push(chromium_string(title).unwrap_or_else(|| String::from("New tab")));
    }
    let active = web_contents(base, window).unwrap_or(std::ptr::null_mut());
    let active_index = (0..count)
        .find(|index| function::<TabAt>(base, TAB_AT)(model, *index) == active)
        .unwrap_or(0) as usize;
    Some((titles, active_index))
}

pub unsafe fn activate_tab(window: HWND, index: usize) -> bool {
    let Some(base) = base() else { return false };
    let Some(model) = tab_model(base, window) else {
        return false;
    };
    if index >= function::<Count>(base, TAB_COUNT)(model).max(0) as usize {
        return false;
    }
    let selection = [0u8; 16];
    function::<Activate>(base, ACTIVATE_TAB)(model, index as i32, &selection);
    true
}

pub unsafe fn close_tab(window: HWND, index: usize) -> bool {
    let Some(base) = base() else { return false };
    let Some(model) = tab_model(base, window) else {
        return false;
    };
    if index >= function::<Count>(base, TAB_COUNT)(model).max(0) as usize {
        return false;
    }
    function::<Close>(base, CLOSE_TAB)(model, index as i32, 0);
    true
}

pub unsafe fn new_tab(window: HWND) -> bool {
    let Some(base) = base() else { return false };
    let Some(browser) = browser(base, window) else {
        return false;
    };
    // Chrome accepts the new-tab command source value 5.
    function::<NewTab>(base, NEW_TAB)(browser, 5);
    true
}
