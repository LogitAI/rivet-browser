//! Opens pinned destinations through Chromium on the browser UI thread.
//!
//! The addresses below belong to Google Chrome 156.0.8078.48 only. They move to the
//! supported-build table once the experiment holds.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;
use crate::chrome_layout::image;

/// Size of a `GURL`, which Chromium 156 lays out the same way in every build it ships.
const GURL_SIZE: usize = 0x78;

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::Navigation {
    &crate::builds::get().navigation
}

const REFERRER_POLICY_DEFAULT: i32 = 1;
const CURRENT_TAB: i32 = 1;
/// `ui::PAGE_TRANSITION_TYPED | ui::PAGE_TRANSITION_FROM_ADDRESS_BAR`.
const TYPED_IN_ADDRESS_BAR: u32 = 0x0200_0001;

type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;
type GurlFromText = unsafe extern "system" fn(*mut c_void, *const [usize; 2]) -> *mut c_void;
type Construct = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type OpenUrlParams = unsafe extern "system" fn(
    *mut c_void,
    *const c_void,
    *const c_void,
    i32,
    u32,
    bool,
) -> *mut c_void;
type OpenUrl = unsafe extern "system" fn(*mut c_void, *const c_void, *mut usize) -> *mut c_void;
/// Storage for a Chromium object built in place, aligned like any C++ object.
#[repr(C, align(16))]
struct Raw<const N: usize>([u8; N]);

impl<const N: usize> Raw<N> {
    fn zeroed() -> Box<Self> {
        Box::new(Raw([0; N]))
    }
    fn at(&mut self) -> *mut c_void {
        self.0.as_mut_ptr().cast()
    }
}

static BASE: OnceLock<Option<usize>> = OnceLock::new();


fn base() -> Option<usize> {
    *BASE.get_or_init(|| unsafe {
        if !bv().supported {
            return None;
        }
        let image = image::recorded()?;
        bv().signatures
            .iter()
            .all(|(rva, expected)| image.has(*rva, expected))
            .then_some(image.base)
    })
}

unsafe fn function<T: Copy>(base: usize, rva: usize) -> T {
    std::mem::transmute_copy(&(base + rva))
}

unsafe fn web_contents(base: usize, window: HWND) -> Option<*mut c_void> {
    let content = function::<Pointer>(base, bv().content_window_for_hwnd)(window);
    if content.is_null() {
        return None;
    }
    let view = function::<Pointer>(base, bv().browser_view_for_window)(content);
    if view.is_null() {
        return None;
    }
    let contents = function::<Pointer>(base, bv().active_web_contents)(view);
    (!contents.is_null()).then_some(contents)
}

/// Opens `url` in the active page of `window`, as if typed in the address bar.
pub unsafe fn open(window: HWND, url: &str) -> bool {
    let Some(base) = base() else { return false };
    let Some(contents) = web_contents(base, window) else {
        return false;
    };
    let mut target = Raw::<GURL_SIZE>::zeroed();
    let text = [url.as_ptr() as usize, url.len()];
    function::<GurlFromText>(base, bv().gurl_from_text)(target.at(), &text);
    let mut referrer = Raw::<{ GURL_SIZE + 8 }>::zeroed();
    function::<Construct>(base, bv().gurl_empty)(referrer.at());
    *(referrer.at().cast::<u8>().add(GURL_SIZE) as *mut i32) = REFERRER_POLICY_DEFAULT;
    // Generously sized: the constructor writes the whole object.
    let mut params = Raw::<0x1000>::zeroed();
    function::<OpenUrlParams>(base, bv().open_url_params)(
        params.at(),
        target.at(),
        referrer.at(),
        CURRENT_TAB,
        TYPED_IN_ADDRESS_BAR,
        false,
    );
    // An empty `base::OnceCallback`, passed by address and consumed by the callee.
    let mut callback: usize = 0;
    let opened = function::<OpenUrl>(base, bv().open_url)(contents, params.at(), &mut callback);
    function::<Construct>(base, bv().open_url_params_drop)(params.at());
    function::<Construct>(base, bv().gurl_drop)(referrer.at());
    function::<Construct>(base, bv().gurl_drop)(target.at());
    !opened.is_null()
}
