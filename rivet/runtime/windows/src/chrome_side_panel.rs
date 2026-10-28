//! Rivet's Sidebar, built on Chrome's own side panel.
//!
//! Chrome 156 keeps one `SidePanel` view in each browser window. It sits beside the page and
//! hosts extension side panels, the reading list and the bookmarks. Rivet restyles that view in
//! its own windows only and opens its entries through Chrome's own side panel coordinator, so
//! extension panels keep working.
//!
//! Every address here belongs to Chrome 156.0.8078.48. Each one is checked against recorded
//! instruction bytes before it is used.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT};

use crate::chrome_layout;

/// `SidePanel`'s own vtable, which its `views::View` base shares as the object's first word.
const SIDE_PANEL_VTABLE: usize = 0x0FE6_7CA8;
const BROWSER_OFFSET: usize = 0x560;
const FEATURES_OFFSET: usize = 0xd0;
/// Where `BrowserWindowFeatures` keeps its `SidePanelCoordinator`.
const FEATURES_COORDINATOR: usize = 0x330;
/// `SidePanelCoordinator`'s vtable.
const COORDINATOR_VTABLE: usize = 0x0FE5_9440;
/// `SidePanelUIBase::Show(this, SidePanelEntryId, optional<SidePanelOpenTrigger>, bool)`.
const SHOW: usize = 0x0997_6C50;
const SHOW_PROLOGUE: [u8; 9] = [0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58];
/// `SidePanelEntryIdToString(std::string*, SidePanelEntryId)`.
const ENTRY_ID_TO_STRING: usize = 0x01E7_18C0;
const ENTRY_ID_TO_STRING_PROLOGUE: [u8; 8] = [0x56, 0x48, 0x83, 0xec, 0x30, 0x48, 0x89, 0xce];
/// How many entry ids Chrome 156 defines.
const ENTRY_COUNT: u32 = 0x18;

type Show = unsafe extern "system" fn(*mut c_void, u32, u64, u32);
type EntryIdToString = unsafe extern "system" fn(*mut [u8; 24], u32) -> *mut [u8; 24];

fn trace(message: &str) {
    crate::vertical_tabs::trace(&format!("side panel: {message}"));
}

thread_local! {
    /// The last region found readable, so a scan over one object does not query every word.
    static REGION: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

fn readable(address: usize, length: usize) -> bool {
    if address == 0 || address % 8 != 0 {
        return false;
    }
    let Some(last) = address.checked_add(length) else { return false };
    let (start, end) = REGION.with(|region| region.get());
    if address >= start && last <= end {
        return true;
    }
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let got = unsafe { VirtualQuery(address as *const c_void, &mut info, std::mem::size_of_val(&info)) };
    let ok = got != 0 && info.State == MEM_COMMIT && info.Protect & 0xEE != 0 && info.Protect & 0x101 == 0;
    if !ok {
        return false;
    }
    let region = (info.BaseAddress as usize, info.BaseAddress as usize + info.RegionSize);
    REGION.with(|cell| cell.set(region));
    last <= region.1
}

fn chrome_base() -> usize {
    unsafe { GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize }
}

/// The objects of one browser window that the Sidebar works with.
#[derive(Debug, Clone, Copy)]
pub struct Objects {
    pub browser_view: usize,
    pub coordinator: usize,
    pub side_panel: usize,
}

/// Finds a window's `BrowserView`, its side panel coordinator and its `SidePanel` from the frame
/// view the browser laid out last.
pub unsafe fn objects(window: HWND) -> Option<Objects> {
    let base = chrome_base();
    let frame = chrome_layout::frame_view(window)?;
    // The frame view holds the `BrowserWindow` subobject of `BrowserView` at +0x340.
    let browser_window = *((frame + 0x340) as *const usize);
    if !readable(browser_window, 8) {
        return None;
    }
    let browser_view = browser_window - 0x1f0;
    let find = |vtable: usize| {
        (0..0x400)
            .map(|index| browser_view + index * 8)
            .filter(|slot| readable(*slot, 8))
            .map(|slot| *(slot as *const usize))
            .find(|candidate| readable(*candidate, 8) && *(*candidate as *const usize) == base + vtable)
            .unwrap_or(0)
    };
    // `BrowserView` points to its `Browser` at +0x560. The browser owns the window's
    // `BrowserWindowFeatures` at +0xd0, and the features own the coordinator.
    let step = |from: usize, offset: usize| {
        let at = from.wrapping_add(offset);
        if readable(at, 8) { *(at as *const usize) } else { 0 }
    };
    let coordinator = step(step(step(browser_view, BROWSER_OFFSET), FEATURES_OFFSET), FEATURES_COORDINATOR);
    let coordinator = if readable(coordinator, 8) && *(coordinator as *const usize) == base + COORDINATOR_VTABLE {
        coordinator
    } else {
        0
    };
    let side_panel = find(SIDE_PANEL_VTABLE);
    Some(Objects { browser_view, coordinator, side_panel })
}

fn prologue_is(address: usize, expected: &[u8]) -> bool {
    readable(address & !7, 8) && unsafe { std::slice::from_raw_parts(address as *const u8, expected.len()) } == expected
}

/// Opens one of Chrome's side panel entries by its id through the coordinator.
pub unsafe fn open_entry(window: HWND, entry: u32) -> bool {
    let (Some(objects), base) = (objects(window), chrome_base()) else { return false };
    if objects.coordinator == 0 || !prologue_is(base + SHOW, &SHOW_PROLOGUE) {
        return false;
    }
    let show: Show = std::mem::transmute(base + SHOW);
    show(objects.coordinator as *mut c_void, entry, 0, 0);
    true
}

/// Names of the entry ids, for the trace.
unsafe fn entry_names() -> Vec<String> {
    let base = chrome_base();
    if !prologue_is(base + ENTRY_ID_TO_STRING, &ENTRY_ID_TO_STRING_PROLOGUE) {
        return Vec::new();
    }
    let to_string: EntryIdToString = std::mem::transmute(base + ENTRY_ID_TO_STRING);
    (0..ENTRY_COUNT)
        .map(|id| {
            let mut text = [0u8; 24];
            to_string(&mut text, id);
            // Short strings only: the last byte holds the length.
            let length = (text[23] as usize).min(22);
            format!("{id}={}", String::from_utf8_lossy(&text[..length]))
        })
        .collect()
}

/// What a test flag file asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Dump,
    Open(u32),
}

impl Command {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text == "dump" {
            return Some(Self::Dump);
        }
        text.strip_prefix("open ")?.trim().parse().ok().map(Self::Open)
    }
}

/// Reads the test flag file and acts on it. A flag holds `dump` or `open <entry id>`. The file is
/// how a checked build is driven without moving the pointer or focusing the window.
pub unsafe fn tick(window: HWND, directory: Option<&std::path::Path>) {
    let Some(flag) = directory.map(|dir| dir.join("rivet-sidepanel.flag")) else { return };
    let Ok(text) = std::fs::read_to_string(&flag) else { return };
    let _ = std::fs::remove_file(&flag);
    trace(&format!("command {:?}", text.trim()));
    match Command::parse(&text) {
        Some(Command::Dump) => {
            trace(&format!("ids {}", entry_names().join(" ")));
            let found = objects(window);
            trace(&format!("objects {found:x?}"));
            if let Some(found) = found.filter(|found| found.side_panel != 0) {
                let bounds = *((found.side_panel + 0x1c8) as *const [i32; 4]);
                trace(&format!("side panel bounds {bounds:?}"));
            }
        }
        Some(Command::Open(id)) => trace(&format!("open {id} -> {}", open_entry(window, id))),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr10_test_flag_opens_an_entry_by_id() {
        assert_eq!(Command::parse("open 1
"), Some(Command::Open(1)));
        assert_eq!(Command::parse("dump"), Some(Command::Dump));
        assert_eq!(Command::parse("open x"), None);
    }
}
