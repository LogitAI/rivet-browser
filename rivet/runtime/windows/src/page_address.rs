//! Experiment: the address of the page a Rivet window shows, for the top row Rivet paints.
//!
//! Chromium exposes the page's address as the value of the document in its accessibility tree.
//! Reading it goes through UI Automation, which calls back into the browser's UI thread, so it
//! runs on a worker thread and posts the result back to the window.

use std::collections::HashMap;
use std::sync::Mutex;

use windows::core::BSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ControlTypePropertyId,
    UIA_DocumentControlTypeId, UIA_ValueValuePropertyId,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};

static ADDRESSES: Mutex<Option<HashMap<isize, String>>> = Mutex::new(None);
/// Windows whose address is being read, so a window has one reader at a time.
static PENDING: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// The message a window receives once its address has been read.
pub unsafe fn message() -> u32 {
    RegisterWindowMessageW(windows_sys::w!("RivetPageAddress"))
}

/// The last address read for `window`.
pub fn address(window: isize) -> Option<String> {
    ADDRESSES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()?
        .get(&window)
        .cloned()
}

pub fn forget(window: isize) {
    if let Some(map) = ADDRESSES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_mut()
    {
        map.remove(&window);
    }
}

/// Reads the address of `window` in the background and posts `message()` when it changed.
pub fn request(window: isize) {
    {
        let mut pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
        if pending.contains(&window) {
            return;
        }
        pending.push(window);
    }
    std::thread::spawn(move || {
        let read = unsafe { read(window) };
        PENDING
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|known| *known != window);
        let Ok(value) = read else {
            return;
        };
        let changed = {
            let mut guard = ADDRESSES.lock().unwrap_or_else(|error| error.into_inner());
            let map = guard.get_or_insert_with(HashMap::new);
            map.insert(window, value.clone()).as_ref() != Some(&value)
        };
        if changed {
            unsafe { PostMessageW(window as _, message(), 0, 0) };
        }
    });
}

unsafe fn read(window: isize) -> windows::core::Result<String> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
    let root = automation.ElementFromHandle(HWND(window as _))?;
    let condition = automation.CreatePropertyCondition(
        UIA_ControlTypePropertyId,
        &VARIANT::from(UIA_DocumentControlTypeId.0),
    )?;
    let document = root.FindFirst(TreeScope_Descendants, &condition)?;
    let value = document.GetCurrentPropertyValue(UIA_ValueValuePropertyId)?;
    Ok(BSTR::try_from(&value)?.to_string())
}

/// The address as the top row shows it: without the scheme and without a lone trailing slash.
pub fn display(address: &str) -> &str {
    let rest = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    let host = rest.find("://").map_or(0, |scheme| scheme + 3);
    match rest[host..].find('/') {
        Some(slash) if host + slash + 1 == rest.len() => &rest[..host + slash],
        _ => rest,
    }
}

#[cfg(test)]
mod tests {
    use super::display;

    #[test]
    fn top_row_address_drops_the_scheme_and_a_bare_slash() {
        assert_eq!(display("https://example.com/"), "example.com");
        assert_eq!(display("http://a.test/b/c"), "a.test/b/c");
        assert_eq!(display("chrome://newtab/"), "chrome://newtab");
    }
}
