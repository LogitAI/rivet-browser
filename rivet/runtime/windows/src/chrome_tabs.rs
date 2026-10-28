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
fn base() -> Option<usize> {
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
    // ChangeTabGroupVisuals copies the borrowed string before replacing the old visual data.
    let mut copy = [0u8; 32];
    std::ptr::copy_nonoverlapping(visual, copy.as_mut_ptr(), 32);
    copy[28] ^= 1;
    function::<ChangeGroup>(base, CHANGE_GROUP)(model, id.as_ptr(), copy.as_ptr(), false);
    true
}
