//! The hook on the browser's layout of the page area, which is shared by every feature that changes
//! where the page goes. The browser's own layout runs first. Then each step below gets the bounds it
//! produced and may change them, in this order and no other.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;

use crate::chromium::image;
use crate::chromium::memory::{prologue_is, readable};
use crate::chromium::view::View;
use crate::features::frame_layout;

thread_local! {
    /// The host view each window's browser layout works on, so Rivet can ask for a layout.
    static HOST_VIEWS: RefCell<HashMap<isize, usize>> = RefCell::new(HashMap::new());
    /// The vertical extent below the top row of each window, as (top, height) in the host view.
    static AREAS: RefCell<HashMap<isize, (i32, i32)>> = RefCell::new(HashMap::new());
}

fn row() -> &'static crate::builds::SidePanel {
    &crate::builds::get().side_panel
}

/// Where a window's page area is, in device-independent pixels of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageGeometry {
    pub x: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

/// The host view of the browser's layout of this window, the `views::View` of its browser view.
pub fn host_view(window: HWND) -> Option<usize> {
    HOST_VIEWS.with(|views| views.borrow().get(&(window as isize)).copied())
}

/// The top of a window's page area in the browser view, as the layout recorded it.
pub fn area_top(window: HWND) -> Option<i32> {
    AREAS.with(|areas| areas.borrow().get(&(window as isize)).map(|area| area.0))
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
        if !readable(view + View::BOUNDS, 16) {
            return None;
        }
        let bounds = View::at(view).bounds();
        x += bounds[0];
        y += bounds[1];
        if depth == 0 {
            width = bounds[2];
        }
        view = View::at(view).parent_address();
        if view == 0 || !readable(view, 8) {
            break;
        }
    }
    Some(PageGeometry {
        x,
        right: x + width,
        top: y + top,
        bottom: y + top + height,
    })
}

/// Where the bounds of the browser's views are when its layout has run. Chrome keeps them in a map
/// it applies afterwards, and Edge sets them on the views at once, so each has its own source.
pub(crate) trait LayoutBounds {
    /// The bounds the layout gave the view, which a step may change.
    unsafe fn of(&mut self, view: usize) -> Option<*mut [i32; 4]>;
    /// Makes the changes of the steps take effect, once they all ran.
    unsafe fn commit(&mut self) {}
    /// Traces every view of the layout with the layout field that holds it.
    unsafe fn dump(&self, _this: *const c_void, _trace: &dyn Fn(&str)) {}
    /// Every view the layout placed, where the source can list them.
    unsafe fn views(&self) -> Vec<usize> {
        Vec::new()
    }
}

/// One run of the browser's layout, which the steps of the pipeline work on.
pub(crate) struct PageArea<'a> {
    this: *mut c_void,
    bounds: &'a mut dyn LayoutBounds,
    pub window: HWND,
    /// The width a step reserved at the right of the page area for the Sidebar's rail.
    pub rail: i32,
}

impl PageArea<'_> {
    /// The view the layout object keeps at `offset`.
    pub(crate) unsafe fn field(&self, offset: usize) -> usize {
        if offset == crate::builds::ABSENT {
            return 0;
        }
        *(self.this.cast::<u8>().add(offset) as *const usize)
    }

    /// The bounds the layout gave the view that sits at `offset` of the layout object.
    pub(crate) unsafe fn bounds(&mut self, offset: usize) -> Option<*mut [i32; 4]> {
        let view = self.field(offset);
        self.bounds.of(view)
    }

    /// The bounds the layout gave every view it placed, where the source can list them.
    pub(crate) unsafe fn all_bounds(&mut self) -> Vec<(usize, *mut [i32; 4])> {
        let views = self.bounds.views();
        views.into_iter().filter_map(|view| self.bounds.of(view).map(|bounds| (view, bounds))).collect()
    }
}

/// The steps that change the page area, in the order they run. The top bar takes its row, the Sidebar reserves its rail
/// next, and the pages of a split fill what is left. An extension page in the second view leaves
/// the browser's own panel no width, and the bars above the page keep to their pane.
const STEPS: [unsafe fn(&mut PageArea); 5] = [
    crate::features::address_bar::on_page_area,
    crate::features::side_panel::on_page_area,
    crate::features::sidebar_page::on_page_area,
    crate::features::pane_infobar::on_page_area,
    crate::features::split::on_page_area,
];

static DUMP_LAYOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Traces every view of the next layout with the layout field that holds it.
pub(crate) fn dump_next() {
    DUMP_LAYOUT.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn trace(message: &str) {
    crate::trace::trace(&format!("side panel: {message}"));
}

/// The browser's own layout function, which the hook of the browser's family calls first.
pub(crate) fn original() -> usize {
    ORIGINAL.load(std::sync::atomic::Ordering::Acquire)
}

static ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Runs the steps of the page area after the browser laid `host` out. `this` is the layout object.
pub(crate) unsafe fn run(this: *mut c_void, host: usize, window: HWND, bounds: &mut dyn LayoutBounds) {
    HOST_VIEWS.with(|views| views.borrow_mut().insert(window as isize, host));
    if DUMP_LAYOUT.swap(false, std::sync::atomic::Ordering::Relaxed) {
        trace("layout dump");
        bounds.dump(this, &trace);
    }
    let mut area = PageArea {
        this,
        bounds,
        window,
        rail: 0,
    };
    if let Some(page) = area.bounds(row().page_area) {
        let page = *page;
        AREAS.with(|areas| {
            areas
                .borrow_mut()
                .insert(window as isize, (page[1], page[3]))
        });
    }
    for step in STEPS {
        step(&mut area);
    }
    area.bounds.commit();
}

/// Patches the layout once per process. Without the recorded layout nothing here runs.
pub(crate) unsafe fn install() -> Result<(), String> {
    const MISSING: &str = "this browser build has no recorded layout for the Sidebar";
    if row().layout_vtable == crate::builds::ABSENT {
        return Err(MISSING.into());
    }
    if original() != 0 {
        return Ok(());
    }
    if !crate::browsers::split_reads_page_area() && !row().supported {
        return Err("the Sidebar is off in this build, and nothing else reads its page area".into());
    }
    let Some(image) = image::recorded() else {
        return Err(MISSING.into());
    };
    let base = image.base;
    let slot_rva = row().layout_vtable + row().proposed_slot * 8;
    if !image.points_to(slot_rva, row().proposed) {
        return Err(MISSING.into());
    }
    let slot = (base + slot_rva) as *mut usize;
    let mut previous: windows_sys::Win32::System::Memory::PAGE_PROTECTION_FLAGS = 0;
    if windows_sys::Win32::System::Memory::VirtualProtect(
        slot.cast(),
        8,
        windows_sys::Win32::System::Memory::PAGE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect the layout vtable".into());
    }
    ORIGINAL.store(slot.read(), std::sync::atomic::Ordering::Release);
    slot.write(crate::browsers::layout_hook());
    windows_sys::Win32::System::Memory::VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// The browser view of a window, from the frame view the browser laid out last. The frame view holds
/// the `BrowserWindow` subobject of `BrowserView` and that subobject's address is the `views::View`
/// the browser's layout works on.
unsafe fn host_of(window: HWND) -> Option<usize> {
    let frame = frame_layout::frame_view(window)?;
    let at = frame + crate::builds::get().layout.frame_browser_window_offset;
    readable(at, 8).then(|| *(at as *const usize)).filter(|view| *view != 0)
}

/// Lays the browser view out again, which runs the patched layout. A window the layout has not run
/// for yet is found through its frame view. Does nothing while the layout is not patched.
pub(crate) unsafe fn relayout(window: HWND) {
    let Some(image) = image::recorded() else {
        return;
    };
    if original() == 0 {
        return;
    }
    let base = image.base;
    let host = HOST_VIEWS
        .with(|views| views.borrow().get(&(window as isize)).copied())
        .or_else(|| host_of(window));
    if let Some(host) = host.filter(|_| {
        prologue_is(
            base + row().layout_immediately,
            &row().layout_immediately_prologue,
        )
    }) {
        let layout: unsafe extern "system" fn(*mut c_void) =
            std::mem::transmute(base + row().layout_immediately);
        layout(host as *mut c_void);
    }
}

/// A browser that lays its view out without the proposed layout gives no page area, so the area is
/// the split view's own bounds in the window.
static OWN_AREAS: std::sync::Mutex<Vec<(isize, [i32; 4])>> = std::sync::Mutex::new(Vec::new());

unsafe fn window_position(view: usize) -> [i32; 2] {
    let offset = crate::builds::get().layout.view_bounds_offset;
    let (mut x, mut y, mut at) = (0, 0, view);
    for _ in 0..16 {
        if at == 0 {
            break;
        }
        let bounds = *((at + offset) as *const [i32; 4]);
        x += bounds[0];
        y += bounds[1];
        at = *((at + VIEW_PARENT) as *const usize);
    }
    [x, y]
}

/// Where a view keeps its parent.
const VIEW_PARENT: usize = 0x198;

pub(crate) unsafe fn remember_own_area(window: HWND, view: usize) {
    if crate::builds::get().side_panel.proposed != crate::builds::ABSENT
        && area_top(window).is_some()
    {
        return;
    }
    let [x, y] = window_position(view);
    let bounds = *((view + crate::builds::get().layout.view_bounds_offset) as *const [i32; 4]);
    let area = [x, y, x + bounds[2], y + bounds[3]];
    let mut all = OWN_AREAS.lock().unwrap_or_else(|error| error.into_inner());
    match all.iter_mut().find(|(key, _)| *key == window as isize) {
        Some(entry) => entry.1 = area,
        None => all.push((window as isize, area)),
    }
}

pub(crate) fn own_area(window: HWND) -> Option<[i32; 4]> {
    OWN_AREAS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window as isize)
        .map(|(_, area)| *area)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds that sit in a list, as a stand-in for the browser's own.
    struct Listed(Vec<(usize, [i32; 4])>);

    impl LayoutBounds for Listed {
        unsafe fn of(&mut self, view: usize) -> Option<*mut [i32; 4]> {
            self.0.iter_mut().find(|(key, _)| *key == view).map(|(_, bounds)| bounds as *mut _)
        }
    }

    #[test]
    fn fr10_a_view_the_build_does_not_have_is_never_looked_up() {
        // A layout object whose first word is a pointer a lookup would otherwise take for a view.
        let object = [0x1234_5678usize; 4];
        let mut bounds = Listed(vec![(0x1234_5678, [1, 2, 3, 4])]);
        let mut area = PageArea { this: object.as_ptr() as *mut c_void, bounds: &mut bounds, window: 0 as HWND, rail: 0 };
        unsafe {
            assert_eq!(area.field(0), 0x1234_5678);
            assert_eq!(area.field(crate::builds::ABSENT), 0);
            assert!(area.bounds(crate::builds::ABSENT).is_none());
            assert_eq!(*area.bounds(0).unwrap(), [1, 2, 3, 4]);
        }
    }
}
