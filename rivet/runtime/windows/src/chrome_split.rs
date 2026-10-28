//! Chrome 156 split view on the browser UI thread. Two tabs shown side by side in one window are
//! Chrome's own split tabs, so Rivet starts and ends them through the tab strip model and lets the
//! browser lay out, focus and restore them. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::HWND;

use crate::chrome_tabs::{base, function, tab_model};

const ADD_TO_NEW_SPLIT: usize = 0x0A20_5410;
const REMOVE_SPLIT: usize = 0x0A20_7580;
const GET_SPLIT_FOR_TAB: usize = 0x0311_2370;
/// PartitionAlloc's operator new, which frees the index vector the callee takes over.
const ALLOCATE: usize = 0x05d3_d6a0;

/// `AddToNewSplit(this, out id, indices, visual data, created source)`.
type AddToNewSplit = unsafe extern "system" fn(
    *mut c_void,
    *mut [u8; 16],
    *const [usize; 3],
    *const [u8; 16],
    u32,
) -> *mut [u8; 16];
/// `RemoveSplit(this, id)`.
type RemoveSplit = unsafe extern "system" fn(*mut c_void, *const [u8; 16]);
/// `GetSplitForTab(this, out optional id, index)`.
type GetSplitForTab = unsafe extern "system" fn(*mut c_void, *mut [u8; 24], i32) -> *mut [u8; 24];
type Allocate = unsafe extern "system" fn(usize) -> *mut i32;

/// The side-by-side layout and an even split. Chrome's visual data is a layout value, then a
/// double ratio at offset 8.
const LAYOUT_SIDE_BY_SIDE: u32 = 0;
const EVEN_RATIO: f64 = 0.5;
/// Chrome only uses the creation source for metrics.
const CREATED_SOURCE: u32 = 0;

fn checked_base() -> Option<usize> {
    static CHECK: OnceLock<bool> = OnceLock::new();
    let base = base()?;
    let signatures: &[(usize, &[u8])] = &[
        (
            ADD_TO_NEW_SPLIT,
            &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        ),
        (REMOVE_SPLIT, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58]),
        (
            GET_SPLIT_FOR_TAB,
            &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
        ),
        (ALLOCATE, &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9]),
    ];
    let ok = *CHECK.get_or_init(|| {
        signatures.iter().all(|(rva, bytes)| unsafe {
            std::slice::from_raw_parts((base + rva) as *const u8, bytes.len()) == *bytes
        })
    });
    if !ok {
        crate::vertical_tabs::trace("split: signature mismatch");
    }
    ok.then_some(base)
}

/// The id of the split the tab at `index` belongs to.
unsafe fn split_of(base: usize, model: *mut c_void, index: usize) -> Option<[u8; 16]> {
    let mut optional = [0u8; 24];
    function::<GetSplitForTab>(base, GET_SPLIT_FOR_TAB)(model, &mut optional, index as i32);
    (optional[16] != 0).then(|| {
        let mut id = [0; 16];
        id.copy_from_slice(&optional[..16]);
        id
    })
}

/// Whether the active tab of the window is shown in a split.
pub unsafe fn active_is_split(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    tabs.iter()
        .find(|tab| tab.active)
        .is_some_and(|tab| split_of(base, model, tab.index).is_some())
}

/// Splits the active tab with the tab after it, or before it when it is the last one. Returns false
/// when the window has a single tab or the active tab is already split.
pub unsafe fn split_active(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let other = if active + 1 < tabs.len() {
        active + 1
    } else if active > 0 {
        active - 1
    } else {
        return false;
    };
    if split_of(base, model, active).is_some() || split_of(base, model, other).is_some() {
        return false;
    }
    let indices = function::<Allocate>(base, ALLOCATE)(8);
    if indices.is_null() {
        return false;
    }
    indices.write(active.min(other) as i32);
    indices.add(1).write(active.max(other) as i32);
    // Chrome takes over this vector of a pointer, an element count and a capacity.
    let span = [indices as usize, 2, 2];
    let mut visual = [0u8; 16];
    visual[..4].copy_from_slice(&LAYOUT_SIDE_BY_SIDE.to_le_bytes());
    visual[8..16].copy_from_slice(&EVEN_RATIO.to_le_bytes());
    let mut id = [0u8; 16];
    function::<AddToNewSplit>(base, ADD_TO_NEW_SPLIT)(
        model,
        &mut id,
        &span,
        &visual,
        CREATED_SOURCE,
    );
    crate::vertical_tabs::trace(&format!("split: created {id:02x?}"));
    true
}

/// Ends the split of the active tab. Both tabs stay open.
pub unsafe fn unsplit_active(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let Some(id) = split_of(base, model, active) else { return false };
    function::<RemoveSplit>(base, REMOVE_SPLIT)(model, &id);
    true
}

/// One-shot diagnostic for the isolated test profile: opens a second tab and splits.
pub unsafe fn probe(window: HWND) {
    if crate::chrome_tabs::tabs(window).is_some_and(|tabs| tabs.len() == 1) {
        crate::chrome_tabs::new_tab(window);
    }
    let split = split_active(window);
    crate::vertical_tabs::trace(&format!(
        "split probe: split {split} active_is_split {}",
        active_is_split(window)
    ));
}
