//! Opens pinned destinations through Chromium on the browser UI thread.
//!
//! The addresses below belong to Google Chrome 156.0.8078.48 only. They move to the
//! supported-build table once the experiment holds.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND`.
const CONTENT_WINDOW_FOR_HWND: usize = 0x00D5_64A0;
/// `BrowserView::GetBrowserViewForNativeWindow`.
const BROWSER_VIEW_FOR_WINDOW: usize = 0x0BC8_1E10;
/// `BrowserView::GetActiveWebContents`.
const ACTIVE_WEB_CONTENTS: usize = 0x0BC8_2340;
/// `GURL::GURL(std::string_view)`, `GURL::GURL()` and `GURL::~GURL`.
const GURL_FROM_TEXT: usize = 0x042B_C7A0;
const GURL_EMPTY: usize = 0x0588_DF10;
const GURL_DROP: usize = 0x03E0_01F0;
/// `content::OpenURLParams::OpenURLParams(url, referrer, disposition, transition, renderer)`.
const OPEN_URL_PARAMS: usize = 0x072E_8660;
const OPEN_URL_PARAMS_DROP: usize = 0x0204_54C0;
/// `content::WebContentsImpl::OpenURL`.
const OPEN_URL: usize = 0x0366_BC50;
/// `sizeof(GURL)`, read from its default constructor. A `content::Referrer` is a URL followed by
/// its policy.
const GURL_SIZE: usize = 0x78;
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

const SIGNATURES: &[(usize, &[u8])] = &[
    (
        CONTENT_WINDOW_FOR_HWND,
        &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
    ),
    (BROWSER_VIEW_FOR_WINDOW, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
    (
        GURL_FROM_TEXT,
        &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
    ),
    (GURL_EMPTY, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
    (GURL_DROP, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
    (OPEN_URL_PARAMS, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
    (OPEN_URL_PARAMS_DROP, &[0x41, 0x56, 0x56, 0x57, 0x53]),
    (OPEN_URL, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
    (
        ACTIVE_WEB_CONTENTS,
        &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00],
    ),
];

fn base() -> Option<usize> {
    *BASE.get_or_init(|| unsafe {
        let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
        if base == 0 {
            return None;
        }
        SIGNATURES
            .iter()
            .all(|(rva, expected)| {
                let code = std::slice::from_raw_parts((base + rva) as *const u8, expected.len());
                code == *expected
            })
            .then_some(base)
    })
}

unsafe fn function<T: Copy>(base: usize, rva: usize) -> T {
    std::mem::transmute_copy(&(base + rva))
}

unsafe fn web_contents(base: usize, window: HWND) -> Option<*mut c_void> {
    let content = function::<Pointer>(base, CONTENT_WINDOW_FOR_HWND)(window);
    if content.is_null() {
        return None;
    }
    let view = function::<Pointer>(base, BROWSER_VIEW_FOR_WINDOW)(content);
    if view.is_null() {
        return None;
    }
    let contents = function::<Pointer>(base, ACTIVE_WEB_CONTENTS)(view);
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
    function::<GurlFromText>(base, GURL_FROM_TEXT)(target.at(), &text);
    let mut referrer = Raw::<{ GURL_SIZE + 8 }>::zeroed();
    function::<Construct>(base, GURL_EMPTY)(referrer.at());
    *(referrer.at().cast::<u8>().add(GURL_SIZE) as *mut i32) = REFERRER_POLICY_DEFAULT;
    // Generously sized: the constructor writes the whole object.
    let mut params = Raw::<0x1000>::zeroed();
    function::<OpenUrlParams>(base, OPEN_URL_PARAMS)(
        params.at(),
        target.at(),
        referrer.at(),
        CURRENT_TAB,
        TYPED_IN_ADDRESS_BAR,
        false,
    );
    // An empty `base::OnceCallback`, passed by address and consumed by the callee.
    let mut callback: usize = 0;
    let opened = function::<OpenUrl>(base, OPEN_URL)(contents, params.at(), &mut callback);
    function::<Construct>(base, OPEN_URL_PARAMS_DROP)(params.at());
    function::<Construct>(base, GURL_DROP)(referrer.at());
    function::<Construct>(base, GURL_DROP)(target.at());
    !opened.is_null()
}
