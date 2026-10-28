//! Rivet's Sidebar, built on Chrome's own side panel.
//!
//! Chrome 156 keeps one `SidePanel` view in each browser window. It sits beside the page and
//! hosts extension side panels, the reading list and the bookmarks. Rivet restyles that view in
//! its own windows only and opens its entries through Chrome's own side panel coordinator, so
//! extension panels keep working.
//!
//! Every address here belongs to Chrome 156.0.8078.48. Each one is checked against recorded
//! instruction bytes before it is used.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    VirtualProtect, VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_PROTECTION_FLAGS,
    PAGE_READWRITE,
};

use crate::chrome_layout;

/// `SidePanel`'s own vtable, which its `views::View` base shares as the object's first word.
const SIDE_PANEL_VTABLE: usize = 0x0FE6_7CA8;
const BROWSER_OFFSET: usize = 0x560;
const FEATURES_OFFSET: usize = 0xd0;
/// Where `BrowserWindowFeatures` keeps its `SidePanelCoordinator`.
const FEATURES_COORDINATOR: usize = 0x330;
/// The vtable of the `views::View` part of `BrowserView`.
const BROWSER_VIEW_VIEW_VTABLE: usize = 0x0FE7_BF68;
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
    /// The `views::View` part of `BrowserView`, which the browser's layout manager works on.
    pub view: usize,
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
    let view = (0..0x400)
        .map(|index| browser_view + index * 8)
        .find(|slot| readable(*slot, 8) && *(*slot as *const usize) == base + BROWSER_VIEW_VIEW_VTABLE)
        .unwrap_or(0);
    Some(Objects { browser_view, coordinator, side_panel, view })
}

fn prologue_is(address: usize, expected: &[u8]) -> bool {
    readable(address & !7, 8) && unsafe { std::slice::from_raw_parts(address as *const u8, expected.len()) } == expected
}

/// `SidePanelCoordinator::Toggle(this, key*, trigger)` opens an entry or closes it when it is
/// showing, and `SidePanelUIBase::IsSidePanelEntryShowing(this, key*)` tells which.
const TOGGLE: usize = 0x0997_F450;
const TOGGLE_PROLOGUE: [u8; 11] = [0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60];
const ENTRY_SHOWING: usize = 0x0369_1A10;
const ENTRY_SHOWING_PROLOGUE: [u8; 8] = [0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01];
/// `SidePanelRegistry::From(BrowserWindowInterface*)` returns the window's global registry. It
/// keeps its entries as a pointer at +0x38 and a count at +0x40, and each entry keeps its
/// `SidePanelEntryKey` at +0x30.
const REGISTRY_FROM: usize = 0x030B_E3C0;
const REGISTRY_FROM_PROLOGUE: [u8; 7] = [0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00];

/// A `SidePanelEntryKey`: the entry id, and the extension id of an extension's panel.
pub type EntryKey = [u8; 0x28];

/// The id of a key.
pub fn entry_id(key: &EntryKey) -> u32 {
    u32::from_le_bytes([key[0], key[1], key[2], key[3]])
}

/// The entries the rail lists: the ones a person uses from a side panel, which are the reading
/// list, bookmarks, history, the reading mode and the extensions' own panels. Chrome's other
/// entries belong to features that open them themselves.
fn listed(id: u32) -> bool {
    matches!(id, 0..=4 | 0x17)
}

/// The entries Chrome registered for the window's side panel, in its own order.
pub unsafe fn listed_entries(window: HWND) -> Vec<EntryKey> {
    let base = chrome_base();
    let Some(found) = objects(window).filter(|found| found.coordinator != 0) else { return Vec::new() };
    if !prologue_is(base + REGISTRY_FROM, &REGISTRY_FROM_PROLOGUE) {
        return Vec::new();
    }
    let browser = *((found.coordinator + 8) as *const usize);
    let from: unsafe extern "system" fn(usize) -> usize = std::mem::transmute(base + REGISTRY_FROM);
    let registry = from(browser);
    if !readable(registry.wrapping_add(0x38), 0x10) {
        return Vec::new();
    }
    let data = *((registry + 0x38) as *const usize);
    let count = *((registry + 0x40) as *const usize);
    (0..count.min(32))
        .filter_map(|index| {
            let entry = *((data.wrapping_add(index * 8)) as *const usize);
            readable(entry.wrapping_add(0x30), 0x28).then(|| *((entry + 0x30) as *const EntryKey))
        })
        .filter(|key| listed(entry_id(key)))
        .collect()
}

/// Whether Chrome's side panel shows this entry in the window.
pub unsafe fn entry_showing(window: HWND, key: &EntryKey) -> bool {
    let base = chrome_base();
    let Some(objects) = objects(window).filter(|found| found.coordinator != 0) else { return false };
    if !prologue_is(base + ENTRY_SHOWING, &ENTRY_SHOWING_PROLOGUE) {
        return false;
    }
    let showing: unsafe extern "system" fn(*mut c_void, *const u8) -> bool =
        std::mem::transmute(base + ENTRY_SHOWING);
    showing(objects.coordinator as *mut c_void, key.as_ptr())
}

/// Opens an entry, or closes it when it is the one showing.
pub unsafe fn toggle_entry(window: HWND, key: &EntryKey) -> bool {
    let base = chrome_base();
    let Some(objects) = objects(window).filter(|found| found.coordinator != 0) else { return false };
    if !prologue_is(base + TOGGLE, &TOGGLE_PROLOGUE) {
        return false;
    }
    let toggle: unsafe extern "system" fn(*mut c_void, *const u8, u32) =
        std::mem::transmute(base + TOGGLE);
    toggle(objects.coordinator as *mut c_void, key.as_ptr(), 0);
    true
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

/// How wide the rail is, in device-independent pixels. The mock-up's rail is 40 DIPs.
pub const RAIL_WIDTH: i32 = 40;

thread_local! {
    /// Windows whose Sidebar is unfolded: the rail column is reserved beside the page.
    static UNFOLDED: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    /// The host view each window's browser layout works on, so Rivet can ask for a layout.
    static HOST_VIEWS: RefCell<HashMap<isize, usize>> = RefCell::new(HashMap::new());
    /// The vertical extent below the top row of each window, as (top, height) in the host view.
    static AREAS: RefCell<HashMap<isize, (i32, i32)>> = RefCell::new(HashMap::new());
}

/// Whether the Sidebar of a window is unfolded.
pub fn is_unfolded(window: HWND) -> bool {
    UNFOLDED.with(|set| set.borrow().contains(&(window as isize)))
}

/// Where a window's page area is, in device-independent pixels of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageGeometry {
    pub x: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

/// The page area of a window, below its top row, from the browser view's bounds and the extent
/// the layout recorded.
pub unsafe fn page_geometry(window: HWND) -> Option<PageGeometry> {
    let host = HOST_VIEWS.with(|views| views.borrow().get(&(window as isize)).copied())?;
    let (top, height) = AREAS.with(|areas| areas.borrow().get(&(window as isize)).copied())?;
    // Views keep their bounds at +0x1c8 and their parent at +0x198. The page area lies in the
    // browser view, so its origin is the sum of the bounds of the view and its ancestors.
    let (mut x, mut y, mut width) = (0, 0, 0);
    let mut view = host;
    for depth in 0..16 {
        if !readable(view + 0x1c8, 16) {
            return None;
        }
        let bounds = *((view + 0x1c8) as *const [i32; 4]);
        x += bounds[0];
        y += bounds[1];
        if depth == 0 {
            width = bounds[2];
        }
        view = *((view + 0x198) as *const usize);
        if view == 0 || !readable(view, 8) {
            break;
        }
    }
    Some(PageGeometry { x, right: x + width, top: y + top, bottom: y + top + height })
}

/// The width the rail takes from the right of a window's page area.
fn rail_of(window: HWND) -> i32 {
    if UNFOLDED.with(|set| set.borrow().contains(&(window as isize))) {
        RAIL_WIDTH
    } else {
        0
    }
}

/// `BrowserViewTabbedLayoutImpl::CalculateProposedLayout`, slot 16 of its vtable. It fills a
/// `std::map` of view to bounds that the layout then applies.
const LAYOUT_VTABLE: usize = 0x0FDB_0278;
const PROPOSED_SLOT: usize = 16;
const PROPOSED: usize = 0x0104_8AB0;
/// The layout object keeps the host view at +0x38, the contents container at +0xb8, the side
/// panel at +0xc0 and the content shown while the side panel animates at +0xc8.
const HOST_VIEW: usize = 0x38;
/// The view behind the whole area below the top row, which the rail shares the height of.
const PAGE_AREA: usize = 0x50;
const CONTENTS_CONTAINER: usize = 0xb8;
/// The view that draws the outline around the contents, which has the contents' bounds.
const CONTENTS_OUTLINE: usize = 0x58;
const SIDE_PANEL: usize = 0xc0;
const SIDE_PANEL_ANIMATION: usize = 0xc8;
/// `views::View::DeprecatedLayoutImmediately`.
const LAYOUT_IMMEDIATELY: usize = 0x04BE_C780;
const LAYOUT_IMMEDIATELY_PROLOGUE: [u8; 12] =
    [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];

type ProposedLayout =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void) -> *mut c_void;
static PROPOSED_LAYOUT: OnceLock<ProposedLayout> = OnceLock::new();

/// The bounds Chrome's layout gave one view, from the layout map's red-black tree. The map keeps
/// its root at +0x20, nodes hold left at +0, right at +8, the view at +0x20 and the bounds as
/// four ints at +0x28.
unsafe fn bounds_of(map: *const c_void, view: usize) -> Option<*mut [i32; 4]> {
    let mut node = *(map.cast::<u8>().add(0x20) as *const usize);
    while readable(node, 0x38) {
        let key = *((node + 0x20) as *const usize);
        if key == view {
            return Some((node + 0x28) as *mut [i32; 4]);
        }
        node = *((node + if view < key { 0 } else { 8 }) as *const usize);
    }
    None
}

/// Leaves `rail` DIPs at the right edge of the page area: the contents get narrower and the side
/// panel moves left, so the page, the open panel and the rail sit side by side.
fn make_room(
    contents: &mut [i32; 4],
    outline: Option<&mut [i32; 4]>,
    side_panel: Option<&mut [i32; 4]>,
    animation: Option<&mut [i32; 4]>,
    rail: i32,
) {
    for bounds in [Some(&mut *contents), outline].into_iter().flatten() {
        bounds[2] = (bounds[2] - rail).max(1);
    }
    for bounds in [side_panel, animation].into_iter().flatten() {
        bounds[0] -= rail;
    }
}

static DUMP_LAYOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Traces every view the layout placed, with the layout field that holds it, for a recorded build.
unsafe fn dump_layout(this: *const c_void, node: usize, depth: usize) {
    if depth > 40 || !readable(node, 0x38) {
        return;
    }
    dump_layout(this, *(node as *const usize), depth + 1);
    let view = *((node + 0x20) as *const usize);
    let field = (0..0x140usize)
        .step_by(8)
        .find(|offset| *(this.cast::<u8>().add(*offset) as *const usize) == view);
    let bounds = *((node + 0x28) as *const [i32; 4]);
    let flags = *((node + 0x38) as *const u16);
    trace(&format!("layout view {view:x} field {field:x?} bounds {bounds:?} flags {flags:#x}"));
    dump_layout(this, *((node + 8) as *const usize), depth + 1);
}

unsafe extern "system" fn proposed_layout(
    this: *mut c_void,
    out: *mut c_void,
    params: *const c_void,
) -> *mut c_void {
    let original = *PROPOSED_LAYOUT.get().expect("patched after it is stored");
    let result = original(this, out, params);
    let host = *(this.cast::<u8>().add(HOST_VIEW) as *const usize);
    let Some(window) = chrome_layout::hwnd_of_view(host as *const c_void) else {
        return result;
    };
    HOST_VIEWS.with(|views| views.borrow_mut().insert(window as isize, host));
    if DUMP_LAYOUT.swap(false, std::sync::atomic::Ordering::Relaxed) {
        trace("layout dump");
        dump_layout(this, *(out.cast::<u8>().add(0x20) as *const usize), 0);
    }
    let field = |offset: usize| *(this.cast::<u8>().add(offset) as *const usize);
    if let Some(area) = bounds_of(out, field(PAGE_AREA)) {
        let area = *area;
        AREAS.with(|areas| areas.borrow_mut().insert(window as isize, (area[1], area[3])));
    }
    let rail = rail_of(window);
    if rail > 0 {
        if let Some(contents) = bounds_of(out, field(CONTENTS_CONTAINER)) {
            let side_panel = bounds_of(out, field(SIDE_PANEL));
            let animation = bounds_of(out, field(SIDE_PANEL_ANIMATION));
            let outline = bounds_of(out, field(CONTENTS_OUTLINE));
            make_room(
                &mut *contents,
                outline.map(|bounds| &mut *bounds),
                side_panel.map(|bounds| &mut *bounds),
                animation.map(|bounds| &mut *bounds),
                rail,
            );
        }
    }
    result
}

/// Patches the layout once per process. Without the recorded layout the Sidebar stays folded.
pub unsafe fn install_layout() -> Result<(), String> {
    if PROPOSED_LAYOUT.get().is_some() {
        return Ok(());
    }
    let base = chrome_base();
    let slot = (base + LAYOUT_VTABLE + PROPOSED_SLOT * 8) as *mut usize;
    if slot.read() != base + PROPOSED {
        return Err("this browser build has no recorded layout for the Sidebar".into());
    }
    let _ = PROPOSED_LAYOUT.set(std::mem::transmute::<usize, ProposedLayout>(slot.read()));
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the layout vtable".into());
    }
    slot.write(proposed_layout as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// Lays the browser view out again, which runs the patched layout.
pub unsafe fn relayout_page_area(window: HWND) {
    let base = chrome_base();
    let host = HOST_VIEWS
        .with(|views| views.borrow().get(&(window as isize)).copied())
        .or_else(|| objects(window).map(|found| found.view).filter(|view| *view != 0));
    if let Some(host) =
        host.filter(|_| prologue_is(base + LAYOUT_IMMEDIATELY, &LAYOUT_IMMEDIATELY_PROLOGUE))
    {
        let layout: unsafe extern "system" fn(*mut c_void) =
            std::mem::transmute(base + LAYOUT_IMMEDIATELY);
        layout(host as *mut c_void);
    }
}

/// Folds or unfolds the Sidebar of a window and lays its page area out again.
pub unsafe fn set_unfolded(window: HWND, unfolded: bool) {
    if let Err(error) = install_layout() {
        trace(&error);
        return;
    }
    UNFOLDED.with(|set| {
        let mut set = set.borrow_mut();
        if unfolded {
            set.insert(window as isize);
        } else {
            set.remove(&(window as isize));
        }
    });
    relayout_page_area(window);
}

unsafe fn dump_entries(window: HWND) {
    let ids: Vec<u32> = listed_entries(window).iter().map(entry_id).collect();
    trace(&format!("listed entries {ids:?}"));
}

/// What a test flag file asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Dump,
    Open(u32),
    Rail(bool),
    DumpLayout,
    Click(String),
    Entries,
}

impl Command {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text == "dump" {
            return Some(Self::Dump);
        }
        match text {
            "rail on" => return Some(Self::Rail(true)),
            "rail off" => return Some(Self::Rail(false)),
            "layout" => return Some(Self::DumpLayout),
            "entries" => return Some(Self::Entries),
            _ if text.starts_with("sim ") => return Some(Self::Click(text["sim ".len()..].to_owned())),
            _ => {}
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
        Some(Command::Rail(on)) => set_unfolded(window, on),
        Some(Command::Entries) => dump_entries(window),
        Some(Command::Click(command)) => crate::side_rail::simulate(window, &command),
        Some(Command::DumpLayout) => {
            let _ = install_layout();
            DUMP_LAYOUT.store(true, std::sync::atomic::Ordering::Relaxed);
            relayout_page_area(window);
        }
        Some(Command::Open(id)) => trace(&format!("open {id} -> {}", open_entry(window, id))),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr10_rail_takes_its_width_from_the_contents_and_moves_the_open_panel() {
        let mut contents = [0, 46, 700, 600];
        let mut panel = [700, 46, 378, 600];
        let mut animation = [700, 46, 378, 600];
        make_room(&mut contents, None, Some(&mut panel), Some(&mut animation), RAIL_WIDTH);
        assert_eq!(contents, [0, 46, 660, 600]);
        assert_eq!(panel[0], 660);
        assert_eq!(animation[0], 660);
    }

    #[test]
    fn fr10_rail_never_collapses_the_page() {
        let mut contents = [0, 0, 20, 100];
        make_room(&mut contents, None, None, None, RAIL_WIDTH);
        assert_eq!(contents[2], 1);
    }

    #[test]
    fn fr10_test_flag_opens_an_entry_by_id() {
        assert_eq!(Command::parse("open 1
"), Some(Command::Open(1)));
        assert_eq!(Command::parse("dump"), Some(Command::Dump));
        assert_eq!(Command::parse("open x"), None);
    }
}
