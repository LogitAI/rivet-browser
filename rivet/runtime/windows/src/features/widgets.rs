//! The `views::Widget` of each Rivet window. Code that runs inside the browser's paint and layout
//! is handed views and widgets, not windows, and asks here whether one belongs to Rivet.

use std::ffi::c_void;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;

type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;

/// Every Rivet window's widget with its window handle.
static WIDGETS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

fn row() -> &'static crate::builds::Toolbar {
    &crate::builds::get().toolbar
}

fn all() -> std::sync::MutexGuard<'static, Vec<(usize, usize)>> {
    WIDGETS.lock().unwrap_or_else(|error| error.into_inner())
}

/// The widget of `window`, through `DesktopWindowTreeHostWin::GetContentWindowForHWND` and
/// `Widget::GetWidgetForNativeView`. Zero when the build records neither.
pub(crate) unsafe fn widget_of(window: HWND) -> usize {
    let row = row();
    if row.content_window_for_hwnd == crate::builds::ABSENT
        || row.widget_for_window == crate::builds::ABSENT
    {
        return 0;
    }
    let Some(image) = crate::chromium::image::recorded() else {
        return 0;
    };
    let content: Pointer = std::mem::transmute(image.base + row.content_window_for_hwnd);
    let widget_for: Pointer = std::mem::transmute(image.base + row.widget_for_window);
    let aura = content(window as *const c_void);
    if aura.is_null() {
        return 0;
    }
    widget_for(aura) as usize
}

/// Records `window` as a Rivet window. Returns its widget, or zero when it has none.
pub(crate) unsafe fn register(window: HWND) -> usize {
    let widget = widget_of(window);
    if widget == 0 {
        return 0;
    }
    let mut widgets = all();
    widgets.retain(|(known, _)| *known != widget);
    widgets.push((widget, window as usize));
    widget
}

/// Forgets a Rivet window that is going away.
pub(crate) fn forget(window: HWND) {
    all().retain(|(_, known)| *known != window as usize);
}

/// Whether `widget` is the widget of a Rivet window.
pub(crate) fn is_rivet(widget: usize) -> bool {
    widget != 0 && all().iter().any(|(known, _)| *known == widget)
}

/// The widgets of Rivet windows.
pub(crate) fn rivet_widgets() -> Vec<usize> {
    all().iter().map(|(widget, _)| *widget).collect()
}

/// The widget that holds `view`, which must point at a `views::View`: `View::GetWidget` through
/// the view's own vtable.
pub(crate) unsafe fn widget_holding(view: *mut c_void) -> usize {
    let table = *(view as *const usize);
    let get_widget: Pointer =
        std::mem::transmute(*(table as *const usize).add(row().get_widget_slot));
    get_widget(view) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_a_widget_is_rivet_only_while_its_window_is_registered() {
        all().push((0x5000, 0x77));
        assert!(is_rivet(0x5000));
        assert!(!is_rivet(0));
        forget(0x77 as HWND);
        assert!(!is_rivet(0x5000));
    }
}
