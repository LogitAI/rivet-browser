//! Chrome, Brave and the browsers that follow them ask `BrowserViewTabbedLayoutImpl` for a map of
//! view to bounds and then apply it. The map is the place to change where the page goes: a view's
//! entry is edited before the browser applies it, so nothing is laid out twice.
use std::ffi::c_void;

use crate::chromium::image;
use crate::chromium::memory::readable;
use crate::features::frame_layout;
use crate::hooks::page_area::{self, LayoutBounds};

type ProposedLayout =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void) -> *mut c_void;

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

/// The map one run of the browser's layout produced.
struct Map {
    out: *mut c_void,
}

impl LayoutBounds for Map {
    unsafe fn of(&mut self, view: usize) -> Option<*mut [i32; 4]> {
        bounds_of(self.out, view)
    }

    unsafe fn dump(&self, this: *const c_void, trace: &dyn Fn(&str)) {
        dump_node(
            this,
            *(self.out.cast::<u8>().add(0x20) as *const usize),
            0,
            trace,
        );
    }

    unsafe fn views(&self) -> Vec<usize> {
        let mut found = Vec::new();
        let mut stack = vec![(*(self.out.cast::<u8>().add(0x20) as *const usize), 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > 40 || found.len() > 256 || !readable(node, 0x38) {
                continue;
            }
            found.push(*((node + 0x20) as *const usize));
            stack.push((*(node as *const usize), depth + 1));
            stack.push((*((node + 8) as *const usize), depth + 1));
        }
        found
    }
}

/// Traces every view the layout placed, with the layout field that holds it.
unsafe fn dump_node(this: *const c_void, node: usize, depth: usize, trace: &dyn Fn(&str)) {
    if depth > 40 || !readable(node, 0x38) {
        return;
    }
    dump_node(this, *(node as *const usize), depth + 1, trace);
    let view = *((node + 0x20) as *const usize);
    let field = (0..0x600usize)
        .step_by(8)
        .find(|offset| *(this.cast::<u8>().add(*offset) as *const usize) == view);
    let bounds = *((node + 0x28) as *const [i32; 4]);
    let flags = *((node + 0x38) as *const u16);
    trace(&format!(
        "layout view {view:x} vt {:x} field {field:x?} bounds {bounds:?} flags {flags:#x}",
        (*(view as *const usize)).wrapping_sub(image::recorded().map_or(0, |image| image.base))
    ));
    dump_node(this, *((node + 8) as *const usize), depth + 1, trace);
}

/// Takes the place of `CalculateProposedLayout`.
pub(crate) unsafe extern "system" fn proposed_layout(
    this: *mut c_void,
    out: *mut c_void,
    params: *const c_void,
) -> *mut c_void {
    let original: ProposedLayout = std::mem::transmute(page_area::original());
    let result = original(this, out, params);
    let host = *(this
        .cast::<u8>()
        .add(crate::builds::get().side_panel.host_view) as *const usize);
    if let Some(window) = frame_layout::hwnd_of_view(host as *const c_void) {
        page_area::run(this, host, window, &mut Map { out });
    }
    result
}

static APP_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Takes the place of `BrowserViewAppLayoutImpl::CalculateProposedLayout`, the layout of an app
/// window, which keeps its views where the tabbed layout keeps them.
unsafe extern "system" fn app_proposed_layout(
    this: *mut c_void,
    out: *mut c_void,
    params: *const c_void,
) -> *mut c_void {
    let original: ProposedLayout =
        std::mem::transmute(APP_ORIGINAL.load(std::sync::atomic::Ordering::Acquire));
    let result = original(this, out, params);
    let host = *(this
        .cast::<u8>()
        .add(crate::builds::get().side_panel.host_view) as *const usize);
    if let Some(window) = frame_layout::hwnd_of_view(host as *const c_void) {
        page_area::run(this, host, window, &mut Map { out });
    }
    result
}

/// Patches the app window's layout once: slot `slot` of the vtable at `vtable` must point at
/// `proposed`, or nothing is changed.
pub(crate) unsafe fn install_app_layout(
    vtable: usize,
    slot: usize,
    proposed: usize,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    if APP_ORIGINAL.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    let image = image::recorded().ok_or("this browser build is not recorded")?;
    let slot_rva = vtable + slot * 8;
    if !image.points_to(slot_rva, proposed) {
        return Err("the app window's layout is not where the record says".into());
    }
    let at = (image.base + slot_rva) as *mut usize;
    let mut previous: windows_sys::Win32::System::Memory::PAGE_PROTECTION_FLAGS = 0;
    if windows_sys::Win32::System::Memory::VirtualProtect(
        at.cast(),
        8,
        windows_sys::Win32::System::Memory::PAGE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect the app window's layout".into());
    }
    APP_ORIGINAL.store(at.read(), Ordering::Release);
    at.write(app_proposed_layout as *const () as usize);
    windows_sys::Win32::System::Memory::VirtualProtect(at.cast(), 8, previous, &mut previous);
    Ok(())
}
