//! Opens DevTools on its Network or Performance panel for the page of a Rivet window.
//!
//! Chromium has no browser command for these panels. DevTools opens on the panel its preferences
//! last selected, and Chromium's own `ToggleDevToolsWindow` applies a settings text to those
//! preferences before a new DevTools window loads its frontend. Rivet asks it to open DevTools with
//! the panel selected in that text.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;

use crate::builds::page_tool_rows::PageTools;
use crate::dev_mode::Action;

/// The DevTools frontend's name for the panel of `action`.
pub fn panel_name(action: Action) -> Option<&'static str> {
    match action {
        Action::Network => Some("network"),
        Action::Performance => Some("timeline"),
        _ => None,
    }
}

/// The settings text that selects `panel`. The frontend keeps each preference as JSON text, so the
/// panel's name is quoted twice.
pub fn panel_settings(panel: &str) -> String {
    format!(r#"{{"panel-selected-tab":"\"{panel}\""}}"#)
}

/// The recorded functions of this build and the module base, when every one of them matches.
fn loaded() -> Option<(&'static PageTools, usize)> {
    static LOADED: OnceLock<Option<(&'static PageTools, usize)>> = OnceLock::new();
    *LOADED.get_or_init(|| unsafe {
        let tools = crate::builds::page_tool_rows::page_tools_for(crate::builds::current()?)?;
        let image = crate::chromium::image::recorded()?;
        let ok = [
            tools.toggle_devtools,
            tools.show_action,
            tools.browser_context,
            tools.profile_from_context,
        ]
        .iter()
        .all(|(rva, bytes)| image.bytes(*rva, bytes.len()) == Some(*bytes));
        crate::trace::trace(&format!("devtools panels: recorded functions match: {ok}"));
        ok.then_some((tools, image.base))
    })
}

/// Whether this build can open DevTools on a panel.
pub(crate) fn available() -> bool {
    loaded().is_some()
}

type Pointer = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type Toggle = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    bool,
    *const c_void,
    *const c_void,
    i32,
    *const c_void,
);

/// `DevToolsOpenedByAction`, which Chromium records only for its metrics.
const OPENED_BY: i32 = 0;

#[repr(C, align(16))]
struct Raw<const N: usize>([u8; N]);

/// A `std::string` that borrows `text`, in libc++'s long form: data, size, and a capacity whose top
/// bit marks the long form. The callee only reads it.
fn borrowed(text: &str) -> [usize; 3] {
    [
        text.as_ptr() as usize,
        text.len(),
        (text.len() + 1) | (1usize << 63),
    ]
}

/// Opens DevTools for the active page of `window` on the panel of `action`.
pub(crate) unsafe fn open_panel(window: HWND, action: Action) -> bool {
    let (Some((tools, base)), Some(panel)) = (loaded(), panel_name(action)) else {
        return false;
    };
    let Some(contents) = crate::features::navigation::active_contents(window) else {
        return false;
    };
    let at = |rva: usize| base + rva;
    let context = std::mem::transmute::<usize, Pointer>(at(tools.browser_context.0))(contents);
    let profile = std::mem::transmute::<usize, Pointer>(at(tools.profile_from_context.0))(context);
    if profile.is_null() {
        return false;
    }
    // The action `DevToolsToggleAction::Show()` writes: type 0 and no reveal parameters.
    let show: [usize; 2] = [0, 0];
    let text = panel_settings(panel);
    let settings = borrowed(&text);
    // `DevToolsOptions` is a small struct of defaults; zero is each member's default.
    let options = Raw::<0x100>([0; 0x100]);
    std::mem::transmute::<usize, Toggle>(at(tools.toggle_devtools.0))(
        contents,
        profile,
        true,
        show.as_ptr().cast(),
        settings.as_ptr().cast(),
        OPENED_BY,
        options.0.as_ptr().cast(),
    );
    crate::trace::trace(&format!("developer bar: devtools on {panel}"));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_network_and_performance_buttons_name_their_devtools_panels() {
        assert_eq!(panel_name(Action::Network), Some("network"));
        assert_eq!(panel_name(Action::Performance), Some("timeline"));
        assert_eq!(panel_name(Action::Console), None);
    }

    #[test]
    fn fr11_panel_settings_select_the_panel_as_json_text() {
        assert_eq!(
            panel_settings("network"),
            r#"{"panel-selected-tab":"\"network\""}"#
        );
    }

    #[test]
    fn fr11_settings_text_is_lent_in_libcxx_long_form() {
        let text = panel_settings("timeline");
        let [data, size, capacity] = borrowed(&text);
        assert_eq!((data, size), (text.as_ptr() as usize, text.len()));
        assert_ne!(capacity & (1 << 63), 0);
        // Long enough that the browser never reads it as a short string.
        assert!(text.len() > 22);
    }
}
