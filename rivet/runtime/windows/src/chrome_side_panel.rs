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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_RESERVE,
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
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
/// The id of the entries that extensions register.
const EXTENSION_ENTRY: u32 = 0x17;
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

/// The extension id of an extension's entry key, or `None` for Chrome's own entries. The key
/// holds an optional `std::string` at +8: libc++'s alternate layout keeps a long string as a
/// pointer, a size and a capacity whose top bit marks it long, and the optional's flag follows.
pub fn extension_id(key: &EntryKey) -> Option<String> {
    if key[0x20] == 0 || entry_id(key) != EXTENSION_ENTRY {
        return None;
    }
    let word = |at: usize| u64::from_le_bytes(key[at..at + 8].try_into().expect("eight bytes")) as usize;
    let long = key[8 + 23] & 0x80 != 0;
    let text = if long {
        let (data, size) = (word(8), word(16));
        if size == 0 || size > 64 || !readable(data, size) {
            return None;
        }
        unsafe { std::slice::from_raw_parts(data as *const u8, size) }.to_vec()
    } else {
        key[8..8 + (key[8 + 23] as usize).min(22)].to_vec()
    };
    let text = String::from_utf8(text).ok()?;
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_lowercase())).then_some(text)
}

/// What the rail shows of an installed extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionLook {
    pub name: String,
    pub icon: Option<std::path::PathBuf>,
}

/// The value of the first `"key": "value"` string pair in JSON text, with its escapes resolved.
fn json_string(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("\"{key}\""))?;
    let rest = text[at + key.len() + 2..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// The icon file a manifest names, preferring the smallest of at least 32 pixels: the rail draws it at 16
/// DIPs, so the picture stays sharp at 200 percent.
fn manifest_icon(manifest: &str) -> Option<String> {
    let at = manifest.find("\"icons\"")?;
    let body = &manifest[at..];
    let body = &body[body.find('{')? + 1..];
    let body = &body[..body.find('}')?];
    let mut best: Option<(u32, String)> = None;
    for part in body.split(',') {
        let (size, path) = part.split_once(':')?;
        let size: u32 = size.trim().trim_matches('"').parse().ok()?;
        let path = path.trim().trim_matches('"').replace("\\/", "/");
        if !path.to_ascii_lowercase().ends_with(".png") {
            continue;
        }
        // A picture at least 32 pixels wide stays sharp, so a smaller one only wins when none is.
        let distance = |size: u32| (size < 32, size.abs_diff(32));
        if best.as_ref().map_or(true, |(other, _)| distance(size) < distance(*other)) {
            best = Some((size, path));
        }
    }
    best.map(|(_, path)| path)
}

/// The name a manifest gives, resolving a `__MSG_name__` reference in its default locale.
fn manifest_name(directory: &std::path::Path, manifest: &str) -> Option<String> {
    let name = json_string(manifest, "name")?;
    let Some(message) = name.strip_prefix("__MSG_").and_then(|rest| rest.strip_suffix("__")) else {
        return Some(name);
    };
    let locale = json_string(manifest, "default_locale").unwrap_or_else(|| "en".into());
    let messages = std::fs::read_to_string(directory.join("_locales").join(locale).join("messages.json")).ok()?;
    let at = messages.to_ascii_lowercase().find(&format!("\"{}\"", message.to_ascii_lowercase()))?;
    json_string(&messages[at..], "message")
}

/// The folders that hold installed extensions, one for each profile of the browser's user data.
fn extension_roots() -> Vec<std::path::PathBuf> {
    let given = std::env::args_os()
        .filter_map(|argument| argument.into_string().ok())
        .find_map(|argument| argument.strip_prefix("--user-data-dir=").map(str::to_owned));
    let data = given.map(std::path::PathBuf::from).or_else(|| {
        std::env::var_os("LOCALAPPDATA").map(|local| std::path::Path::new(&local).join("Google/Chrome/User Data"))
    });
    let Some(data) = data else { return Vec::new() };
    let Ok(profiles) = std::fs::read_dir(&data) else { return Vec::new() };
    let mut roots: Vec<_> = profiles
        .flatten()
        .map(|profile| profile.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "Default" || name.starts_with("Profile "))
        })
        .map(|path| path.join("Extensions"))
        .collect();
    roots.sort();
    roots
}

/// The name and icon of an installed extension, from the files the browser unpacked for it. Only
/// the extension's own manifest, locale messages and icon are read.
pub fn extension_look(id: &str) -> Option<ExtensionLook> {
    for root in extension_roots() {
        let Ok(versions) = std::fs::read_dir(root.join(id)) else { continue };
        let mut versions: Vec<_> = versions.flatten().map(|version| version.path()).collect();
        versions.sort();
        let Some(directory) = versions.pop() else { continue };
        let Ok(manifest) = std::fs::read_to_string(directory.join("manifest.json")) else { continue };
        let manifest = manifest.trim_start_matches('\u{feff}');
        return Some(ExtensionLook {
            name: manifest_name(&directory, manifest).unwrap_or_else(|| id.to_owned()),
            icon: manifest_icon(manifest).map(|path| directory.join(path)).filter(|path| path.is_file()),
        });
    }
    None
}

/// What Chrome calls an entry in the Sidebar's rail, for entries that are not extensions'.
pub fn builtin_name(id: u32) -> &'static str {
    match id {
        0 => "Reading list",
        1 => "Bookmarks",
        2 | 3 => "History",
        4 => "Reading mode",
        _ => "Side panel",
    }
}

/// The entries the rail lists: the ones a person uses from a side panel, which are the reading
/// list, bookmarks, history, the reading mode and the extensions' own panels. Chrome's other
/// entries belong to features that open them themselves.
fn listed(id: u32) -> bool {
    matches!(id, 0..=4 | EXTENSION_ENTRY)
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

// The side panel's pages (bookmarks, reading list, history) are web pages. Their colours reach
// them as a style sheet that Chrome writes from the window's colour provider, outside any view's
// paint. The sheet is written for a Rivet window with the Rivet side panel colours in place, so
// those pages take the paper, hairline and accent of the window. Two functions are involved.
/// `WebContentsImpl::GetColorProvider`, which hands back the provider of the window's widget.
const GET_COLOR_PROVIDER: usize = 0x0366_DB00;
const GET_COLOR_PROVIDER_PROLOGUE: [u8; 14] =
    [0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08];
/// Where the colour provider source lies in the widget.
const SOURCE_IN_WIDGET: usize = 0x48;
/// Where `WebContentsImpl` keeps the colour provider source it asks.
const COLOR_SOURCE: usize = 0x98;
/// `ThemeSource::GenerateColorsCss(out, provider, url, flag, flag)`, which writes the style sheet.
const GENERATE_COLORS_CSS: usize = 0x058C_6090;
const GENERATE_COLORS_CSS_PROLOGUE: [u8; 12] =
    [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];

type GenerateCss = unsafe extern "system" fn(*mut c_void, *const c_void, *const c_void, u32, u32) -> usize;
static ORIGINAL_CSS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// The provider the last web contents asked for and whether its window is a Rivet window.
    static ASKED: std::cell::Cell<(usize, bool)> = const { std::cell::Cell::new((0, false)) };
}

/// Colours a checked build adds to the side panel's own, to learn which ids paint which part.
static EXPERIMENT: Mutex<Vec<(i32, u32)>> = Mutex::new(Vec::new());

/// The hue of Rivet's tint, in degrees. Chrome's blues become this hue.
const TINT_HUE: f32 = 27.0;
/// The lightness from which a blue tinted surface is the Rivet paper.
const PAPER_FROM: f32 = 0.94;
const PAPER: u32 = 0xfbfaf7;
/// How much of its saturation a blue keeps when it takes the tint's hue.
const TINT_SATURATION: f32 = 0.55;

/// Rivet's version of a colour of Chrome's side panel pages, as `0xAARRGGBB`. Chrome's blues, which
/// are its accent and the surfaces tinted with it, take the tint's hue at the same lightness, and the
/// palest surfaces are the paper.
/// Neutral and semantic colours stay as they are.
fn warm(colour: u32) -> Option<u32> {
    let channel = |shift: u32| ((colour >> shift) & 0xff) as f32 / 255.0;
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (high + low) / 2.0;
    let spread = high - low;
    if spread <= 0.0 {
        return None;
    }
    let saturation = spread / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if high == r {
        60.0 * (((g - b) / spread) % 6.0)
    } else if high == g {
        60.0 * ((b - r) / spread + 2.0)
    } else {
        60.0 * ((r - g) / spread + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    if !(190.0..=260.0).contains(&hue) || saturation < 0.2 {
        return None;
    }
    if lightness >= PAPER_FROM {
        return Some((colour & 0xff00_0000) | PAPER);
    }
    let saturation = saturation * TINT_SATURATION;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = TINT_HUE / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, second, 0.0),
        _ => (second, chroma, 0.0),
    };
    let offset = lightness - chroma / 2.0;
    let byte = |value: f32| ((value + offset) * 255.0).round().clamp(0.0, 255.0) as u32;
    Some((colour & 0xff00_0000) | byte(r) << 16 | byte(g) << 8 | byte(b))
}

unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous = 0;
    if VirtualProtect(target.cast(), bytes.len(), PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect browser code".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
        windows_sys::Win32::System::Threading::GetCurrentProcess(),
        target.cast(),
        bytes.len(),
    );
    Ok(())
}

/// `mov rax, to; jmp rax`, padded with `int3` to `length` bytes.
fn jump_to(to: usize, length: usize) -> Vec<u8> {
    let mut patch = vec![0x48, 0xB8];
    patch.extend_from_slice(&(to as u64).to_le_bytes());
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(length, 0xCC);
    patch
}

/// The widget of a Rivet window whose colour provider source this is.
fn rivet_source(source: usize) -> Option<usize> {
    crate::chrome_toolbar::rivet_widgets()
        .into_iter()
        .find(|widget| source == *widget + SOURCE_IN_WIDGET)
}

/// Same as the browser's own: the provider of the web contents' window. It also notes whose it was.
unsafe extern "system" fn get_color_provider(contents: *mut c_void) -> *const c_void {
    let source = *(contents.cast::<u8>().add(COLOR_SOURCE) as *const *mut c_void);
    let table = *(source as *const *const usize);
    let provider: unsafe extern "system" fn(*mut c_void) -> *const c_void = std::mem::transmute(*table.add(1));
    let provider = provider(source);
    let rivet = rivet_source(source as usize);
    ASKED.with(|asked| asked.set((provider as usize, rivet.is_some())));
    provider
}

unsafe extern "system" fn generate_colors_css(
    out: *mut c_void,
    provider: *const c_void,
    url: *const c_void,
    flag: u32,
    last: u32,
) -> usize {
    let original: GenerateCss = std::mem::transmute(ORIGINAL_CSS.load(Ordering::Acquire));
    let (asked, rivet) = ASKED.with(|asked| asked.get());
    if !rivet || asked != provider as usize {
        return original(out, provider, url, flag, last);
    }
    let experiment = EXPERIMENT.lock().unwrap_or_else(|error| error.into_inner()).clone();
    let map = |id: i32, value: u32| {
        experiment.iter().find(|(wanted, _)| *wanted == id).map(|(_, colour)| *colour).or_else(|| warm(value))
    };
    crate::chrome_toolbar::with_colour_map(provider.cast(), map, || original(out, provider, url, flag, last))
}

/// Makes the style sheets of the side panel's pages in Rivet windows use the Rivet colours. It
/// does nothing unless both functions still start with the recorded bytes.
pub unsafe fn install_page_colours() -> Result<(), String> {
    if ORIGINAL_CSS.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    let base = chrome_base();
    let provider = (base + GET_COLOR_PROVIDER) as *mut u8;
    let css = (base + GENERATE_COLORS_CSS) as *mut u8;
    if std::slice::from_raw_parts(provider, 14) != GET_COLOR_PROVIDER_PROLOGUE
        || std::slice::from_raw_parts(css, 12) != GENERATE_COLORS_CSS_PROLOGUE
    {
        return Err("this browser build has no recorded colour provider functions".into());
    }
    // The style sheet function continues after its first twelve bytes in a copy of them.
    let mut trampoline = GENERATE_COLORS_CSS_PROLOGUE.to_vec();
    trampoline.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    trampoline.extend_from_slice(&((base + GENERATE_COLORS_CSS + 12) as u64).to_le_bytes());
    let memory = VirtualAlloc(std::ptr::null(), trampoline.len(), MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(trampoline.as_ptr(), memory, trampoline.len());
    ORIGINAL_CSS.store(memory as usize, Ordering::Release);
    write_code(css, &jump_to(generate_colors_css as *const () as usize, 12))?;
    write_code(provider, &jump_to(get_color_provider as *const () as usize, 14))
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

/// The width of an open side panel, in DIPs. The mock-up's extension panel is 240 DIPs wide.
pub const PANEL_WIDTH: i32 = 240;
/// How far Chrome insets the contents from the left, top and bottom of the page area while a side
/// panel is open, so they float. Rivet's page, panel and rail are one flat card, so the inset goes.
const CONTENTS_INSET: i32 = 8;
/// The height Rivet keeps free above an open side panel for its own header row.
pub const HEADER_ROW: i32 = 40;

/// Leaves `rail` DIPs at the right edge of the page area, and sizes an open side panel to
/// `PANEL_WIDTH` beside the rail: the page, the open panel and the rail sit side by side with
/// nothing between them, and the contents take what is left.
fn make_room(
    contents: &mut [i32; 4],
    outline: Option<&mut [i32; 4]>,
    side_panel: Option<&mut [i32; 4]>,
    animation: Option<&mut [i32; 4]>,
    rail: i32,
) {
    // The panel's right edge ends at the rail. While Chrome animates the panel in, it is narrower.
    let panel = side_panel
        .as_ref()
        .filter(|bounds| bounds[2] > 0)
        .map(|bounds| (bounds[0] + bounds[2] - rail, bounds[2].min(PANEL_WIDTH), bounds[1], bounds[3]));
    for bounds in [side_panel, animation].into_iter().flatten() {
        bounds[0] = match panel {
            Some((edge, width, _, _)) => {
                // The header row above the panel is Rivet's, so the browser's panel starts below it.
                bounds[1] += HEADER_ROW;
                bounds[3] = (bounds[3] - HEADER_ROW).max(0);
                // Rivet's hairline takes the panel's first column.
                bounds[2] = width - 1;
                edge - width + 1
            }
            None => bounds[0] - rail,
        };
    }
    for bounds in [Some(&mut *contents), outline].into_iter().flatten() {
        let right = match panel {
            Some((edge, width, top, height)) => {
                bounds[0] -= CONTENTS_INSET;
                bounds[1] = top;
                bounds[3] = height;
                edge - width
            }
            None => bounds[0] + bounds[2] - rail,
        };
        bounds[2] = (right - bounds[0]).max(1);
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

unsafe fn dump_entry_memory(window: HWND) {
    let Some(found) = objects(window).filter(|found| found.coordinator != 0) else { return };
    let base = chrome_base();
    let browser = *((found.coordinator + 8) as *const usize);
    let from: unsafe extern "system" fn(usize) -> usize = std::mem::transmute(base + REGISTRY_FROM);
    let registry = from(browser);
    let data = *((registry + 0x38) as *const usize);
    let count = *((registry + 0x40) as *const usize);
    for index in 0..count.min(32) {
        let entry = *((data + index * 8) as *const usize);
        let words: Vec<String> = (0..0x24).map(|word| format!("{:x}", *((entry + word * 8) as *const usize))).collect();
        trace(&format!("entry {index} at {entry:x}: {}", words.join(" ")));
    }
}

unsafe fn dump_entries(window: HWND) {
    dump_entry_memory(window);
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
    Colours,
    Paint(Vec<(i32, u32)>),
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
            "colours" => return Some(Self::Colours),
            _ if text.starts_with("paint") => {
                let pairs = text["paint".len()..].split_whitespace().filter_map(|pair| {
                    let (id, value) = pair.split_once('=')?;
                    Some((id.parse().ok()?, u32::from_str_radix(value, 16).ok()?))
                });
                return Some(Self::Paint(pairs.collect()));
            }
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
        Some(Command::Paint(list)) => *EXPERIMENT.lock().unwrap_or_else(|error| error.into_inner()) = list,
        Some(Command::Colours) => {
            let (provider, _) = ASKED.with(|asked| asked.get());
            if provider != 0 {
                crate::chrome_toolbar::dump_colours(provider as *const u8);
            }
        }
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
        let mut contents = [8, 47, 692, 591];
        let mut panel = [700, 46, 378, 600];
        let mut animation = [700, 46, 378, 600];
        make_room(&mut contents, None, Some(&mut panel), Some(&mut animation), RAIL_WIDTH);
        assert_eq!(panel, [799, 46 + HEADER_ROW, PANEL_WIDTH - 1, 600 - HEADER_ROW], "ends at the rail, below Rivet's header row, after its hairline");
        assert_eq!(contents, [0, 46, 798, 600], "the contents lose Chrome's inset and end where the panel begins");
        assert_eq!(animation, panel);
    }

    #[test]
    fn fr10_rail_leaves_a_closed_side_panel_without_width_and_the_page_whole() {
        let mut contents = [0, 46, 1078, 600];
        let mut panel = [1078, 46, 0, 600];
        make_room(&mut contents, None, Some(&mut panel), None, RAIL_WIDTH);
        assert_eq!(contents, [0, 46, 1038, 600]);
        assert_eq!(panel[2], 0);
    }

    #[test]
    fn fr10_rail_never_collapses_the_page() {
        let mut contents = [0, 0, 20, 100];
        make_room(&mut contents, None, None, None, RAIL_WIDTH);
        assert_eq!(contents[2], 1);
    }

    #[test]
    fn fr10_side_panel_pages_turn_chromes_blues_to_the_tint_and_keep_neutrals() {
        let accent = warm(0xff0b57d0).expect("Chrome's accent blue is a blue");
        assert_eq!(accent >> 24, 0xff);
        let (r, g, b) = ((accent >> 16) & 0xff, (accent >> 8) & 0xff, accent & 0xff);
        assert!(r > g && g > b, "the accent turns warm: {accent:#x}");
        let surface = warm(0xffedf2fa).expect("the blue tinted surface is a blue");
        assert_eq!(surface, 0xfffbfaf7, "a pale tinted surface is the paper");
        assert_eq!(warm(0xff1f1f1f), None);
        assert_eq!(warm(0xffffffff), None);
        assert_eq!(warm(0xffd93025), None, "red stays red");
        assert_eq!(warm(0x1f0b57d0).map(|c| c >> 24), Some(0x1f), "alpha is kept");
    }

    #[test]
    fn fr10_rail_reads_an_extension_name_and_icon_from_its_manifest() {
        let manifest = r#"{"name": "Notes \u00e9 Panel", "icons": {"16": "a16.png", "48": "icons\/a48.png", "128": "a128.png"}}"#;
        assert_eq!(json_string(manifest, "name").as_deref(), Some("Notes é Panel"));
        assert_eq!(manifest_icon(manifest).as_deref(), Some("icons/a48.png"), "the smallest icon of at least 32 pixels");
        assert_eq!(manifest_icon(r#"{"icons": {"16": "a.svg"}}"#), None, "only PNG icons are drawn");
        assert_eq!(manifest_icon(r#"{"name": "x"}"#), None);
    }

    #[test]
    fn fr10_rail_resolves_a_localised_extension_name() {
        let directory = std::env::temp_dir().join(format!("rivet-ext-{}", std::process::id()));
        let locale = directory.join("_locales").join("ko");
        std::fs::create_dir_all(&locale).unwrap();
        std::fs::write(locale.join("messages.json"), r#"{"appDesc": {"message": "x"}, "appName": {"message": "메모 패널"}}"#).unwrap();
        let manifest = r#"{"name": "__MSG_appName__", "default_locale": "ko"}"#;
        assert_eq!(manifest_name(&directory, manifest).as_deref(), Some("메모 패널"));
        assert_eq!(manifest_name(&directory, r#"{"name": "Plain"}"#).as_deref(), Some("Plain"));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn fr10_rail_takes_an_extension_id_from_a_long_string_in_the_key() {
        let id = b"abcdefghijklmnopabcdefghijklmnop".to_vec();
        let backing: Vec<u64> = id.chunks(8).map(|part| u64::from_le_bytes(part.try_into().unwrap())).collect();
        let mut key: EntryKey = [0; 0x28];
        key[..4].copy_from_slice(&EXTENSION_ENTRY.to_le_bytes());
        key[8..16].copy_from_slice(&(backing.as_ptr() as u64).to_le_bytes());
        key[16..24].copy_from_slice(&(id.len() as u64).to_le_bytes());
        key[24..32].copy_from_slice(&(0x8000_0000_0000_0000u64 | 33).to_le_bytes());
        key[0x20] = 1;
        assert_eq!(extension_id(&key).as_deref(), Some("abcdefghijklmnopabcdefghijklmnop"));
        key[0x20] = 0;
        assert_eq!(extension_id(&key), None, "the optional holds no value");
        let mut own: EntryKey = [0; 0x28];
        own[..4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(extension_id(&own), None, "Chrome's own entries are not extensions");
    }

    #[test]
    fn fr10_test_flag_opens_an_entry_by_id() {
        assert_eq!(Command::parse("open 1
"), Some(Command::Open(1)));
        assert_eq!(Command::parse("dump"), Some(Command::Dump));
        assert_eq!(Command::parse("open x"), None);
    }
}
