//! Some builds lay their browser view out with Chromium's older layout manager. That one sets each
//! view's bounds as it goes and keeps no map of them, so Rivet lets it finish and then moves the views it
//! changed with `View::SetBoundsRect`, as the browser itself would.
use std::ffi::c_void;

use crate::chromium::image;
use crate::chromium::memory::readable;
use crate::features::frame_layout;
use crate::hooks::page_area::{self, LayoutBounds};

type Layout = unsafe extern "system" fn(*mut c_void, *mut c_void);
type SetBoundsRect = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);

/// A view's bounds as the browser left them, and as the steps of the page area want them.
struct Edit {
    view: usize,
    before: [i32; 4],
    wanted: [i32; 4],
}

/// The views one run of the layout touched. The boxes keep the bounds where a step's pointer to
/// them stays valid while more views are added.
#[derive(Default)]
#[allow(
    clippy::vec_box,
    reason = "each edit must keep its address while more are added"
)]
struct Views {
    edits: Vec<Box<Edit>>,
}

impl Views {
    /// The views a step moved. A view left as the browser had it is not set again.
    fn changed(&self) -> impl Iterator<Item = &Edit> {
        self.edits
            .iter()
            .map(|edit| &**edit)
            .filter(|edit| edit.wanted != edit.before)
    }
}

impl LayoutBounds for Views {
    unsafe fn of(&mut self, view: usize) -> Option<*mut [i32; 4]> {
        let at = view.wrapping_add(crate::builds::get().layout.view_bounds_offset);
        if view == 0 || !readable(at, 16) {
            return None;
        }
        if let Some(edit) = self.edits.iter_mut().find(|edit| edit.view == view) {
            return Some(&mut edit.wanted);
        }
        let bounds = *(at as *const [i32; 4]);
        self.edits.push(Box::new(Edit {
            view,
            before: bounds,
            wanted: bounds,
        }));
        self.edits
            .last_mut()
            .map(|edit| &mut edit.wanted as *mut [i32; 4])
    }

    unsafe fn commit(&mut self) {
        let Some(image) = image::recorded() else {
            return;
        };
        let set: SetBoundsRect =
            std::mem::transmute(image.base + crate::builds::get().layout.set_bounds_rect);
        for edit in self.changed() {
            set(edit.view as *mut c_void, &edit.wanted);
        }
    }

    unsafe fn dump(&self, this: *const c_void, trace: &dyn Fn(&str)) {
        let base = image::recorded().map_or(0, |image| image.base);
        let size = image::recorded().map_or(0, |image| image.size);
        let offset = crate::builds::get().layout.view_bounds_offset;
        for field in (0x30..0x300usize).step_by(8) {
            let view = *(this.cast::<u8>().add(field) as *const usize);
            if !readable(view, offset + 16) {
                continue;
            }
            let vtable = *(view as *const usize);
            if vtable < base || vtable >= base + size {
                continue;
            }
            let bounds = *((view + offset) as *const [i32; 4]);
            let parent = *((view + crate::chromium::view::View::PARENT) as *const usize);
            trace(&format!(
                "layout view {view:x} vt {:x} field {field:#x} bounds {bounds:?} parent {parent:x}",
                vtable - base
            ));
        }
    }
}

/// Takes the place of `BrowserViewLayoutImplOld::Layout`.
pub(crate) unsafe extern "system" fn old_layout(this: *mut c_void, host: *mut c_void) {
    let original: Layout = std::mem::transmute(page_area::original());
    original(this, host);
    if let Some(window) = frame_layout::hwnd_of_view(host) {
        page_area::run(this, host as usize, window, &mut Views::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view in plain memory with its bounds where the row says views keep them.
    fn view(bounds: [i32; 4]) -> Vec<u8> {
        let mut memory = vec![0u8; 0x400];
        let at = crate::builds::get().layout.view_bounds_offset;
        for (index, value) in bounds.into_iter().enumerate() {
            memory[at + index * 4..at + index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        memory
    }

    #[test]
    fn fr10_old_layout_hands_a_step_the_views_own_bounds_and_remembers_what_it_changed() {
        let (page, panel) = (view([0, 44, 1164, 793]), view([0, 0, 0, 0]));
        let (page_at, panel_at) = (page.as_ptr() as usize, panel.as_ptr() as usize);
        let mut views = Views::default();
        unsafe {
            let bounds = views.of(page_at).expect("a readable view has bounds");
            assert_eq!(*bounds, [0, 44, 1164, 793]);
            (*bounds)[2] -= 40;
            // The same view gives the same bounds again, with the step's change in them.
            assert_eq!(*views.of(page_at).unwrap(), [0, 44, 1124, 793]);
            // A view that is only read is not set again.
            assert_eq!(*views.of(panel_at).unwrap(), [0, 0, 0, 0]);
            assert_eq!(views.edits.len(), 2);
        }
        let moved: Vec<_> = views
            .changed()
            .map(|edit| (edit.view, edit.wanted))
            .collect();
        assert_eq!(moved, [(page_at, [0, 44, 1124, 793])]);
    }

    #[test]
    fn fr10_old_layout_has_no_bounds_for_a_view_it_does_not_have() {
        let mut views = Views::default();
        unsafe {
            assert!(views.of(0).is_none(), "an empty field of the layout object");
            assert!(views.of(0x10).is_none(), "an address that is not mapped");
        }
        assert!(views.edits.is_empty());
    }
}
