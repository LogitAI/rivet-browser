//! Chrome 156 split view on the browser UI thread. Two tabs shown side by side in one window are
//! Chrome's own split tabs, so Rivet starts and ends them through the tab strip model and lets the
//! browser lay out, focus and restore them. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

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
    if unsafe { crate::frame::runtime_directory() }.is_some_and(|dir| dir.join("rivet-unsplit.flag").exists()) {
        let ended = unsplit_active(window);
        let count = crate::chrome_tabs::tabs(window).map(|tabs| tabs.len());
        crate::vertical_tabs::trace(&format!(
            "split probe: unsplit {ended} active_is_split {} tabs {count:?}",
            active_is_split(window)
        ));
    }
}

/// The corner radius of a page panel in a split, in device-independent pixels.
const PANEL_RADIUS: f32 = 12.0;

/// `ContentsContainerView::SetRoundedCorners(this, const RoundedCornersF*)`. It starts with
/// `movss xmm0, [rdx]; ucomiss xmm0, [rcx + 0x484]; jne changed`, 13 bytes.
const SET_ROUNDED_CORNERS: usize = 0x0369_EB40;
const SET_ROUNDED_CORNERS_PROLOGUE: [u8; 13] = [
    0xf3, 0x0f, 0x10, 0x02, 0x0f, 0x2e, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
];
/// Where the `jne` goes when the corners differ, and the `jp` after it.
const SET_ROUNDED_CORNERS_CHANGED: usize = 0x0369_EB80;
const SET_ROUNDED_CORNERS_PARITY: usize = 0x0369_EB4D;

type SetRoundedCorners = unsafe extern "system" fn(*mut c_void, *const [f32; 4]);
static ORIGINAL_CORNERS: AtomicUsize = AtomicUsize::new(0);

/// Pages of Rivet windows are panels with the shell study's radius. Other windows keep Chrome's.
unsafe extern "system" fn set_rounded_corners(view: *mut c_void, corners: *const [f32; 4]) {
    let original: SetRoundedCorners = std::mem::transmute(ORIGINAL_CORNERS.load(Ordering::Acquire));
    let widget = crate::chrome_toolbar::widget_holding(view);
    if crate::chrome_toolbar::is_rivet(widget) {
        let wanted = (*corners).map(|radius| if radius > 0.0 { PANEL_RADIUS } else { 0.0 });
        original(view, &wanted);
    } else {
        original(view, corners);
    }
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

/// `jmp [rip + 0]` to an absolute address.
fn absolute_jump(code: &mut Vec<u8>, to: usize) {
    code.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    code.extend_from_slice(&(to as u64).to_le_bytes());
}

/// Makes the pages of split tabs in Rivet windows rounded panels. It does nothing unless the
/// function still starts with the recorded bytes.
pub unsafe fn install() -> Result<(), String> {
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    let code = (base + SET_ROUNDED_CORNERS) as *mut u8;
    if std::slice::from_raw_parts(code, 13) != SET_ROUNDED_CORNERS_PROLOGUE {
        return Err("the rounded corners function is not the recorded one".into());
    }
    if ORIGINAL_CORNERS.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    // The comparison runs in the trampoline. Its `jne` has a short offset, so it is rebuilt: taken
    // goes to the original's changed path, and not taken goes on to the original's `jp`.
    let mut trampoline = SET_ROUNDED_CORNERS_PROLOGUE[..11].to_vec();
    trampoline.extend_from_slice(&[0x75, 0x0E]);
    absolute_jump(&mut trampoline, base + SET_ROUNDED_CORNERS_PARITY);
    absolute_jump(&mut trampoline, base + SET_ROUNDED_CORNERS_CHANGED);
    let memory = VirtualAlloc(
        std::ptr::null(),
        trampoline.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(trampoline.as_ptr(), memory, trampoline.len());
    ORIGINAL_CORNERS.store(memory as usize, Ordering::Release);
    // mov rax, hook; jmp rax. The rest of the old jne is never run.
    let mut patch = vec![0x48, 0xB8];
    patch.extend_from_slice(&(set_rounded_corners as *const () as u64).to_le_bytes());
    patch.extend_from_slice(&[0xFF, 0xE0]);
    write_code(code, &patch)
}

/// `ContentsContainerView::UpdateBorderAndOverlay(this, show, a, b)`. Chrome rings the page it
/// works in with a 1 DIP outline and keeps 4 DIPs of space inside the ring. A Rivet panel has no
/// ring, so the page takes that space and the panel's depth marks it instead.
const UPDATE_BORDER: usize = 0x01FD_C720;
const UPDATE_BORDER_PROLOGUE: [u8; 12] = [
    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x48, 0x48, 0x89, 0xce,
];
type UpdateBorder = unsafe extern "system" fn(*mut c_void, u32, u32, u32);
static ORIGINAL_BORDER: AtomicUsize = AtomicUsize::new(0);

/// `views::View::SetBorder` and `SetVisible` in the view's vtable.
const SET_BORDER_SLOT: usize = 0xf0 / 8;
const SET_VISIBLE_SLOT: usize = 0x90 / 8;
/// The member of the container that holds Chrome's outline view.
const OUTLINE_MEMBER: usize = 0x448;

unsafe fn virtual_slot<T: Copy>(view: *mut c_void, slot: usize) -> T {
    let table = *(view as *const *const usize);
    std::mem::transmute_copy(&*table.add(slot))
}

unsafe extern "system" fn update_border(view: *mut c_void, show: u32, a: u32, b: u32) {
    let original: UpdateBorder = std::mem::transmute(ORIGINAL_BORDER.load(Ordering::Acquire));
    original(view, show, a, b);
    let widget = crate::chrome_toolbar::widget_holding(view);
    if !crate::chrome_toolbar::is_rivet(widget) {
        return;
    }
    // Chrome has just laid its outline and the space inside it out. The mini toolbar stays.
    let no_border: usize = 0;
    virtual_slot::<unsafe extern "system" fn(*mut c_void, *const usize)>(view, SET_BORDER_SLOT)(
        view,
        &no_border,
    );
    let child = *((view as usize + OUTLINE_MEMBER) as *const *mut c_void);
    if !child.is_null() {
        virtual_slot::<unsafe extern "system" fn(*mut c_void, u32)>(child, SET_VISIBLE_SLOT)(
            child, 0,
        );
    }
}

/// Replaces the start of a function, which must be whole instructions that do not depend on their
/// own address, with a jump to `hook`. The trampoline that runs the original is stored.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    let code = (base + rva) as *mut u8;
    if std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    if store.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    let mut trampoline = prologue.to_vec();
    absolute_jump(&mut trampoline, base + rva + prologue.len());
    let memory = VirtualAlloc(
        std::ptr::null(),
        trampoline.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(trampoline.as_ptr(), memory, trampoline.len());
    store.store(memory as usize, Ordering::Release);
    let mut patch = vec![0x48, 0xB8];
    patch.extend_from_slice(&(hook as u64).to_le_bytes());
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(prologue.len(), 0xCC);
    write_code(code, &patch)
}

/// Rivet's panel look for the pages of a split: Chrome's outline goes.
pub unsafe fn install_panels() -> Result<(), String> {
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    detour(
        base,
        UPDATE_BORDER,
        &UPDATE_BORDER_PROLOGUE,
        update_border as *const () as usize,
        &ORIGINAL_BORDER,
    )
}
