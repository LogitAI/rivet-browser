//! Chrome 156 tab operations on the browser UI thread. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::HWND;
use crate::chrome_layout::image;
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
/// `WebContentsImpl::GetLastCommittedURL` and `GetVisibleURL`, which return a `const GURL&`.
const LAST_COMMITTED_URL: usize = 0x058A_48A0;
const VISIBLE_URL: usize = 0x05CF_8BA0;
type Count = unsafe extern "system" fn(*mut c_void) -> i32;
type TabAt = unsafe extern "system" fn(*mut c_void, i32) -> *mut c_void;
type Activate = unsafe extern "system" fn(*mut c_void, i32, *const [u8; 16]);
type Close = unsafe extern "system" fn(*mut c_void, i32, u32);
type NewTab = unsafe extern "system" fn(*mut c_void, i32);

const GET_GROUP: usize = 0x0311_2320;
const HAS_GROUP: usize = 0x0a1f_10d0;
const LOOKUP_GROUP: usize = 0x0a1f_1130;
const CHANGE_GROUP: usize = 0x0a20_8550;
type GetGroup = unsafe extern "system" fn(*mut c_void, *mut [u8; 24], i32) -> *mut [u8; 24];
type HasGroup = unsafe extern "system" fn(*mut c_void, *const u8) -> bool;
type LookupGroup = unsafe extern "system" fn(*mut c_void, *const u8) -> *mut u8;
type ChangeGroup = unsafe extern "system" fn(*mut c_void, *const u8, *const u8, bool);

static BASE: OnceLock<Option<usize>> = OnceLock::new();
pub(crate) fn base() -> Option<usize> {
    *BASE.get_or_init(|| unsafe {
        let image = image::recorded()?;
        let signatures: &[(usize, &[u8])] = &[
            (
                GET_GROUP,
                &[
                    0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x45, 0x85, 0xc0, 0x78,
                ],
            ),
            (
                HAS_GROUP,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xec, 0x38, 0x49, 0x89, 0xd0, 0x48, 0x8b, 0x05,
                ],
            ),
            (
                LOOKUP_GROUP,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x89, 0xd6,
                ],
            ),
            (
                CHANGE_GROUP,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
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
            (LAST_COMMITTED_URL, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
            (VISIBLE_URL, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
        ];
        for (rva, bytes) in signatures {
            if !image.has(*rva, bytes) {
                crate::vertical_tabs::trace(&format!("tabs: signature mismatch {rva:#x}"));
                return None;
            }
        }
        Some(image.base)
    })
}
pub(crate) unsafe fn function<T: Copy>(base: usize, rva: usize) -> T {
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

pub(crate) unsafe fn tab_model(base: usize, window: HWND) -> Option<*mut c_void> {
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
pub unsafe fn tabs(window: HWND) -> Option<Vec<crate::tab_rows::Tab>> {
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
    let group_model = *(model.cast::<u8>().add(8) as *const *mut c_void);
    let mut tabs = Vec::with_capacity(titles.len());
    for (index, title) in titles.into_iter().enumerate() {
        let mut optional = [0u8; 24];
        function::<GetGroup>(base, GET_GROUP)(model, &mut optional, index as i32);
        let group = if optional[16] != 0 && !group_model.is_null() {
            let mut id = [0; 16];
            id.copy_from_slice(&optional[..16]);
            let present = function::<HasGroup>(base, HAS_GROUP)(group_model, id.as_ptr());
            if present {
                let ptr = function::<LookupGroup>(base, LOOKUP_GROUP)(group_model, id.as_ptr());
                let visual = if ptr.is_null() {
                    std::ptr::null()
                } else {
                    *(ptr.add(0x20) as *const *const u8)
                };
                if visual.is_null() {
                    None
                } else {
                    let color = *(visual.add(24) as *const i32);
                    let collapsed = *visual.add(28) != 0;
                    Some(crate::tab_rows::Group {
                        id,
                        title: chromium_string(visual).unwrap_or_default(),
                        color,
                        collapsed,
                    })
                }
            } else {
                None
            }
        } else {
            None
        };
        tabs.push(crate::tab_rows::Tab {
            index,
            title,
            active: index == active_index,
            group,
        });
    }
    Some(tabs)
}

/// A tab of the window as a space keeps it, with the identity of its page.
pub struct OpenTab {
    /// The page object, which stays the same while the tab moves in the strip.
    pub id: usize,
    pub saved: crate::spaces::SavedTab,
}

/// The text of a `std::string` of bytes inside a `GURL`.
unsafe fn narrow_string(value: *const u8) -> Option<String> {
    let last = *value.add(23);
    let (data, size) = if last & 0x80 != 0 {
        (*(value as *const *const u8), *(value.add(8) as *const usize))
    } else {
        (value, last as usize)
    };
    if size > 1 << 16 {
        return None;
    }
    Some(String::from_utf8_lossy(std::slice::from_raw_parts(data, size)).into_owned())
}

/// The window's tabs with their addresses and titles, in strip order.
pub unsafe fn open_tabs(window: HWND) -> Option<Vec<OpenTab>> {
    let base = base()?;
    let model = tab_model(base, window)?;
    let titles = tabs(window)?;
    let mut result = Vec::with_capacity(titles.len());
    for tab in titles {
        let contents = function::<TabAt>(base, TAB_AT)(model, tab.index as i32);
        if contents.is_null() {
            return None;
        }
        let mut url = String::new();
        for rva in [LAST_COMMITTED_URL, VISIBLE_URL] {
            let gurl = function::<Pointer>(base, rva)(contents).cast::<u8>();
            if !gurl.is_null() {
                url = narrow_string(gurl).unwrap_or_default();
            }
            if !url.is_empty() {
                break;
            }
        }
        result.push(OpenTab {
            id: contents as usize,
            saved: crate::spaces::SavedTab {
                url,
                title: tab.title,
                active: tab.active,
                group: tab.group.map(|group| crate::spaces::SavedGroup {
                    key: group.id.iter().map(|byte| format!("{byte:02x}")).collect(),
                    title: group.title,
                    color: group.color,
                    collapsed: group.collapsed,
                }),
            },
        });
    }
    Some(result)
}

/// Closes the tab whose page is `id`, wherever it is in the strip now.
pub unsafe fn close_tab_by_id(window: HWND, id: usize) -> bool {
    let Some(tabs) = open_tabs(window) else {
        return false;
    };
    match tabs.iter().position(|tab| tab.id == id) {
        Some(index) => close_tab(window, index),
        None => false,
    }
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

/// One-shot diagnostic for the isolated test profile.
pub unsafe fn probe_group(window: HWND) {
    let Some(base) = base() else { return };
    let checks: &[(usize, &[u8])] = &[
        (
            0x05d3_d6a0,
            &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        ),
        (
            0x0a20_60b0,
            &[
                0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            ],
        ),
        (
            0x0311_2320,
            &[
                0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x45, 0x85, 0xc0, 0x78,
            ],
        ),
        (
            0x0a1f_10d0,
            &[
                0x56, 0x57, 0x48, 0x83, 0xec, 0x38, 0x49, 0x89, 0xd0, 0x48, 0x8b, 0x05,
            ],
        ),
        (
            0x0a1f_1130,
            &[
                0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x89, 0xd6,
            ],
        ),
        (
            0x0a20_8550,
            &[
                0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
            ],
        ),
    ];
    let Some(image) = image::recorded() else { return };
    if checks.iter().any(|(r, b)| !image.has(*r, b)) {
        crate::vertical_tabs::trace("group probe: signature mismatch");
        return;
    }
    let Some(model) = tab_model(base, window) else {
        return;
    };
    if function::<Count>(base, TAB_COUNT)(model) != 1 {
        crate::vertical_tabs::trace("group probe: requires exactly one tab");
        return;
    }
    crate::vertical_tabs::trace("group probe: before new tab 1");
    if !new_tab(window) {
        return;
    }
    crate::vertical_tabs::trace("group probe: before new tab 2");
    if !new_tab(window) {
        return;
    }
    crate::vertical_tabs::trace("group probe: before add");
    let count = function::<Count>(base, TAB_COUNT)(model);
    crate::vertical_tabs::trace(&format!("group probe: tab count {count}"));
    if count != 3 {
        return;
    }
    type Allocate = unsafe extern "system" fn(usize) -> *mut i32;
    let indices = function::<Allocate>(base, 0x05d3_d6a0)(8);
    if indices.is_null() {
        return;
    }
    indices.write(1);
    indices.add(1).write(2);
    // This Chrome build's vector is a pointer, element count and capacity.
    let span = [indices as usize, 2, 2];
    let mut id = [0u8; 16];
    type Add = unsafe extern "system" fn(
        *mut c_void,
        *mut [u8; 16],
        *const [usize; 3],
        bool,
    ) -> *mut [u8; 16];
    function::<Add>(base, 0x0a20_60b0)(model, &mut id, &span, false);
    crate::vertical_tabs::trace(&format!("group probe: created {id:02x?}"));
    type Get = unsafe extern "system" fn(*mut c_void, *mut [u8; 24], i32) -> *mut [u8; 24];
    type Lookup = unsafe extern "system" fn(*mut c_void, *const u8) -> *mut u8;
    let group_model = *(model.cast::<u8>().add(8) as *const *mut c_void);
    for i in 0..3 {
        let mut optional = [0u8; 24];
        function::<Get>(base, 0x0311_2320)(model, &mut optional, i);
        crate::vertical_tabs::trace(&format!("group probe: tab {i} optional {optional:02x?}"));
    }
    let group = function::<Lookup>(base, 0x0a1f_1130)(group_model, id.as_ptr());
    if group.is_null() {
        return;
    }
    let data = *(group.add(0x20) as *const *const u8);
    if data.is_null() {
        return;
    }
    crate::vertical_tabs::trace(&format!(
        "group probe: group {:02x?}",
        std::slice::from_raw_parts(group, 0x30)
    ));
    crate::vertical_tabs::trace(&format!(
        "group probe: visual before {:02x?}",
        std::slice::from_raw_parts(data, 32)
    ));
    let mut visual = [0u8; 32];
    for (i, c) in "Rivet test".encode_utf16().enumerate() {
        visual[i * 2..i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    visual[23] = 10;
    visual[24..28].copy_from_slice(&3i32.to_le_bytes());
    type Change = unsafe extern "system" fn(*mut c_void, *const u8, *const u8, bool);
    function::<Change>(base, 0x0a20_8550)(model, id.as_ptr(), visual.as_ptr(), false);
    let data = *(group.add(0x20) as *const *const u8);
    crate::vertical_tabs::trace(&format!(
        "group probe: visual named {:02x?}",
        std::slice::from_raw_parts(data, 32)
    ));
    visual[24..28].copy_from_slice(&5i32.to_le_bytes());
    visual[28] = 1;
    function::<Change>(base, 0x0a20_8550)(model, id.as_ptr(), visual.as_ptr(), false);
    let data = *(group.add(0x20) as *const *const u8);
    crate::vertical_tabs::trace(&format!(
        "group probe: visual collapsed {:02x?}",
        std::slice::from_raw_parts(data, 32)
    ));
    visual[28] = 0;
    function::<Change>(base, 0x0a20_8550)(model, id.as_ptr(), visual.as_ptr(), false);
    let collapsed_ok = toggle_group(window, &id);
    let collapsed = tabs(window).and_then(|tabs| {
        tabs.get(1)
            .and_then(|tab| tab.group.as_ref().map(|group| group.collapsed))
    });
    crate::vertical_tabs::trace(&format!(
        "group probe: toggle collapsed {collapsed_ok} {collapsed:?}"
    ));
    let expanded_ok = toggle_group(window, &id);
    let expanded = tabs(window).and_then(|tabs| {
        tabs.get(1)
            .and_then(|tab| tab.group.as_ref().map(|group| group.collapsed))
    });
    crate::vertical_tabs::trace(&format!(
        "group probe: toggle expanded {expanded_ok} {expanded:?}"
    ));
    let selected = activate_tab(window, 1);
    let active =
        tabs(window).and_then(|tabs| tabs.iter().find(|tab| tab.active).map(|tab| tab.index));
    crate::vertical_tabs::trace(&format!(
        "group probe: activate child {selected} {active:?}"
    ));
    activate_tab(window, 0);
}

/// `operator new` of the browser and `TabStripModel::AddToNewGroup`, which the group probe uses.
const ALLOCATE: usize = 0x05d3_d6a0;
const ADD_TO_NEW_GROUP: usize = 0x0a20_60b0;
type Allocate = unsafe extern "system" fn(usize) -> *mut i32;
type AddToNewGroup = unsafe extern "system" fn(
    *mut c_void,
    *mut [u8; 16],
    *const [usize; 3],
    bool,
) -> *mut [u8; 16];

/// Whether the two functions that make a group are the ones this build expects.
fn group_calls_known() -> bool {
    static KNOWN: OnceLock<bool> = OnceLock::new();
    *KNOWN.get_or_init(|| unsafe {
        let Some(image) = image::recorded() else { return false };
        image.has(ALLOCATE, &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9])
            && image.has(
                ADD_TO_NEW_GROUP,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81],
            )
    })
}

/// Puts the tabs whose pages are `ids` into one new group with this title and colour, and returns
/// the group's id. `collapsed` is left to the caller, because a collapsed group must not hold the
/// active tab.
pub unsafe fn group_tabs(window: HWND, ids: &[usize], title: &str, color: i32) -> Option<[u8; 16]> {
    let base = base()?;
    if ids.is_empty() || !group_calls_known() {
        return None;
    }
    let model = tab_model(base, window)?;
    let open = open_tabs(window)?;
    let mut positions: Vec<i32> = ids
        .iter()
        .filter_map(|id| open.iter().position(|tab| tab.id == *id))
        .map(|position| position as i32)
        .collect();
    positions.sort_unstable();
    if positions.is_empty() {
        return None;
    }
    // Chrome takes over this vector of a pointer, an element count and a capacity.
    let indices = function::<Allocate>(base, ALLOCATE)(positions.len() * 4);
    if indices.is_null() {
        return None;
    }
    for (slot, position) in positions.iter().enumerate() {
        indices.add(slot).write(*position);
    }
    let span = [indices as usize, positions.len(), positions.len()];
    let mut id = [0u8; 16];
    function::<AddToNewGroup>(base, ADD_TO_NEW_GROUP)(model, &mut id, &span, false);
    set_group_visuals(base, model, &id, title, color, false)?;
    Some(id)
}

/// Names, colours and folds a group. The visual data goes to the browser by value, so the browser
/// destroys it after the call. A title of up to ten UTF-16 units fits the string's own storage,
/// and a longer one needs a buffer from the browser's own allocator for that to be sound.
unsafe fn set_group_visuals(
    base: usize,
    model: *mut c_void,
    id: &[u8; 16],
    title: &str,
    color: i32,
    collapsed: bool,
) -> Option<()> {
    let units: Vec<u16> = title.encode_utf16().collect();
    let mut visual = [0u8; 32];
    if units.len() <= 10 {
        for (slot, unit) in units.iter().enumerate() {
            visual[slot * 2..slot * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
        visual[23] = units.len() as u8;
    } else {
        if !group_calls_known() {
            return None;
        }
        let buffer = function::<Allocate>(base, ALLOCATE)((units.len() + 1) * 2).cast::<u16>();
        if buffer.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), buffer, units.len());
        buffer.add(units.len()).write(0);
        visual[..8].copy_from_slice(&(buffer as usize).to_le_bytes());
        visual[8..16].copy_from_slice(&units.len().to_le_bytes());
        visual[16..24].copy_from_slice(&((units.len() as u64) | 1 << 63).to_le_bytes());
    }
    visual[24..28].copy_from_slice(&color.to_le_bytes());
    visual[28] = collapsed as u8;
    function::<ChangeGroup>(base, CHANGE_GROUP)(model, id.as_ptr(), visual.as_ptr(), false);
    Some(())
}

pub unsafe fn toggle_group(window: HWND, id: &[u8; 16]) -> bool {
    let Some(base) = base() else { return false };
    let Some(model) = tab_model(base, window) else {
        return false;
    };
    let group_model = *(model.cast::<u8>().add(8) as *const *mut c_void);
    if group_model.is_null() || !function::<HasGroup>(base, HAS_GROUP)(group_model, id.as_ptr()) {
        return false;
    }
    let group = function::<LookupGroup>(base, LOOKUP_GROUP)(group_model, id.as_ptr());
    if group.is_null() {
        return false;
    }
    let visual = *(group.add(0x20) as *const *const u8);
    if visual.is_null() {
        return false;
    }
    let title = chromium_string(visual).unwrap_or_default();
    let color = *(visual.add(24) as *const i32);
    let collapsed = *visual.add(28) != 0;
    set_group_visuals(base, model, id, &title, color, !collapsed).is_some()
}
