//! The browser calls a feature makes through the small functions below. Rivet calls only Chromium's
//! functions, so every supported browser takes the same path.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;

pub(crate) mod chromium;

use crate::builds::Build;
use crate::feature::Needs;
use crate::features::split::Pane;

/// Whether the build's row says Rivet knows its windows by the frame it attached rather than by
/// their widgets.
fn own_toolbar() -> bool {
    static OWN: OnceLock<bool> = OnceLock::new();
    *OWN.get_or_init(|| crate::builds::get().toolbar.skip.contains("toolbar"))
}

/// The function that takes the place of the browser's layout of its window, so the page area can
/// change where the page goes. Chromium lays out in two ways, and the row says which one the
/// build's slot holds.
pub(crate) fn layout_hook() -> usize {
    if crate::builds::get().side_panel.old_layout == Some(true) {
        chromium::old_layout::old_layout as *const () as usize
    } else {
        chromium::layout::proposed_layout as *const () as usize
    }
}

/// The key of the Rivet window a view of the browser belongs to, or none when the window is not one
/// of Rivet's. Views are asked from inside the browser's layout, which runs on the UI thread.
pub(crate) unsafe fn rivet_owner(view: *mut std::ffi::c_void) -> Option<usize> {
    if own_toolbar() {
        let window = crate::features::frame_layout::hwnd_of_view(view)?;
        crate::frame::attached_window(window).then_some(window as usize)
    } else {
        let widget = crate::features::widgets::widget_holding(view);
        crate::features::widgets::is_rivet(widget).then_some(widget)
    }
}

/// How split view is made and shown, which is the browser's own work. Chromium keeps a split as two
/// tabs of its tab strip model and lays the pages out itself. Every call below is made on the
/// browser's UI thread.
pub(crate) trait SplitBackend: Sync {
    /// What the backend needs from the build, which is unavailable when the row does not record it.
    fn needs(&self, build: &Build) -> Needs;
    /// Makes the changes the backend needs once per process.
    unsafe fn install(&self) -> Result<(), String>;
    /// Gives the pages of a split Rivet's panel look.
    unsafe fn install_panels(&self) -> Result<(), String>;
    /// Whether the window's active page is shown in a split.
    unsafe fn active_is_split(&self, window: HWND) -> bool;
    /// Splits the tabs `first` and `second` of the strip, `first` shown first. `stacked` puts them on
    /// top of each other. Returns false when the backend does not split them.
    unsafe fn split_pair(&self, window: HWND, first: usize, second: usize, stacked: bool) -> bool;
    /// Ends the split of the active page. Both pages stay open.
    unsafe fn unsplit_active(&self, window: HWND) -> bool;
    /// The pages of the window's split in window coordinates, or none when it is not split.
    unsafe fn panes(&self, window: HWND) -> Vec<Pane>;
    /// The area the pages fill, as left, top, right and bottom.
    unsafe fn page_area(&self, window: HWND) -> Option<[i32; 4]>;
    /// What the page area layout does for a split.
    unsafe fn on_page_area(&self, _area: &mut crate::hooks::page_area::PageArea) {}
    /// Whether the browser's own address follows the focused page.
    fn address_follows_page(&self) -> bool;
    /// The title and address of a page of the split that is no tab of the strip.
    unsafe fn pane_text(&self, _window: HWND, _pane: &Pane) -> Option<(String, String)> {
        None
    }
    /// What a button of the row of a page that is no tab does. Returns whether it did anything.
    unsafe fn pane_action(
        &self,
        _window: HWND,
        _pane: &Pane,
        _button: crate::features::split::RowButton,
    ) -> bool {
        false
    }
    /// Whether a button of the row of a page that is no tab can act now. A button that cannot is drawn
    /// faded and does not react.
    unsafe fn pane_can(
        &self,
        _window: HWND,
        _pane: &Pane,
        _button: crate::features::split::RowButton,
    ) -> bool {
        true
    }
}

/// The backend for a build: Chromium's tab model.
pub(crate) fn split_backend_for(_build: &Build) -> &'static dyn SplitBackend {
    static TAB_MODEL: chromium::split::TabModelSplit = chromium::split::TabModelSplit;
    &TAB_MODEL
}

/// The backend of the loaded build, chosen on first use.
pub(crate) fn split_backend() -> &'static dyn SplitBackend {
    static CHOSEN: OnceLock<&'static dyn SplitBackend> = OnceLock::new();
    *CHOSEN.get_or_init(|| split_backend_for(crate::builds::get()))
}
