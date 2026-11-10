//! Where Chromium itself puts a view on the screen.
//!
//! `views::View::ConvertPointToScreen(const View*, gfx::Point*)` is the conversion Chromium's own
//! bubbles use for their anchor, so a place found through it is the browser's own, in screen DIPs,
//! even for a window whose scale differs from that of the monitor it is on. A row may carry the
//! function with its first bytes. The bytes are checked against the loaded module once, and the
//! function is called only after they matched.

use std::sync::atomic::{AtomicUsize, Ordering};

static CONVERT: AtomicUsize = AtomicUsize::new(0);
type ConvertPointToScreen = unsafe extern "system" fn(*const core::ffi::c_void, *mut [i32; 2]);

/// The fewest bytes that tell a function start apart.
const LEAST_PROLOGUE: usize = 8;

/// Whether the bytes found at the recorded place are the ones the row records.
fn accepts(found: Option<&[u8]>, recorded: &[u8]) -> bool {
    recorded.len() >= LEAST_PROLOGUE && found == Some(recorded)
}

/// Checks the row's `ConvertPointToScreen` against the loaded module, and says what it found.
pub(crate) fn install() -> &'static str {
    let Some((at, prologue)) = crate::builds::current().and_then(|build| build.screen_origin)
    else {
        return "the row has no ConvertPointToScreen";
    };
    let Some(image) = crate::chromium::image::recorded() else {
        return "no recorded module";
    };
    if !accepts(unsafe { image.bytes(at, prologue.len()) }, prologue) {
        return "ConvertPointToScreen is not the recorded one";
    }
    CONVERT.store(image.base + at, Ordering::Release);
    "ConvertPointToScreen checked"
}

/// The top left corner of `view` in screen DIPs, as Chromium puts it. `None` when the row has no
/// checked `ConvertPointToScreen`.
///
/// # Safety
/// `view` must be a live `views::View` of the browser, used on the browser's UI thread.
pub(crate) unsafe fn origin(view: usize) -> Option<(i32, i32)> {
    let at = CONVERT.load(Ordering::Acquire);
    if at == 0 || view == 0 {
        return None;
    }
    let convert: ConvertPointToScreen = std::mem::transmute(at);
    let mut point = [0i32; 2];
    convert(view as *const core::ffi::c_void, &mut point);
    Some((point[0], point[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_screen_origin_is_called_only_after_its_bytes_matched() {
        let recorded: &[u8] = &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6];
        assert!(accepts(Some(recorded), recorded));
        assert!(!accepts(Some(&recorded[1..]), recorded));
        assert!(!accepts(None, recorded));
        // Too few bytes say nothing about which function lies there.
        assert!(!accepts(Some(&recorded[..4]), &recorded[..4]));
        // Without a checked function there is no origin, and nothing is called.
        assert_eq!(unsafe { origin(0x1000) }, None);
    }
}
