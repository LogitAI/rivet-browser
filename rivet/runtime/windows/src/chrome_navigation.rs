//! Experiment: navigation of a Rivet window's active page through Chromium's own functions.
//!
//! The top row Rivet paints needs to know whether the page can go back or forward, and to open
//! an address the person typed. Chromium keeps both on the active `WebContents`, reached from the
//! window through its `BrowserView`. Every call here runs on the browser UI thread.
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
/// `chrome::CanGoBack(content::WebContents*)`.
const CAN_GO_BACK: usize = 0x0A0B_7C00;
/// `content::NavigationControllerImpl::CanGoForward`.
const CAN_GO_FORWARD: usize = 0x082F_3C50;
/// `GURL::GURL(std::string_view)`, `GURL::GURL()` and `GURL::~GURL`.
const GURL_FROM_TEXT: usize = 0x042B_C7A0;
const GURL_EMPTY: usize = 0x0588_DF10;
const GURL_DROP: usize = 0x03E0_01F0;
/// `content::OpenURLParams::OpenURLParams(url, referrer, disposition, transition, renderer)`.
const OPEN_URL_PARAMS: usize = 0x072E_8660;
const OPEN_URL_PARAMS_DROP: usize = 0x0204_54C0;
/// `content::WebContentsImpl::GetVisibleURL`.
const VISIBLE_URL: usize = 0x05CF_8BA0;
/// `content::WebContentsImpl::OpenURL`.
const OPEN_URL: usize = 0x0366_BC50;
/// `WebContents::GetController` is the seventh entry of the `WebContents` vtable.
const GET_CONTROLLER_SLOT: usize = 6;
/// `sizeof(GURL)`, read from its default constructor. A `content::Referrer` is a URL followed by
/// its policy.
const GURL_SIZE: usize = 0x78;
const REFERRER_POLICY_DEFAULT: i32 = 1;
const CURRENT_TAB: i32 = 1;
/// `ui::PAGE_TRANSITION_TYPED | ui::PAGE_TRANSITION_FROM_ADDRESS_BAR`.
const TYPED_IN_ADDRESS_BAR: u32 = 0x0200_0001;

type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;
type Query = unsafe extern "system" fn(*mut c_void) -> bool;
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
    (CAN_GO_BACK, &[0x56, 0x48, 0x83, 0xEC, 0x20]),
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

/// Whether the active page of `window` can go back and forward, or `None` when the browser is
/// not the recorded build.
pub unsafe fn history(window: HWND) -> Option<(bool, bool)> {
    let base = base()?;
    let contents = web_contents(base, window)?;
    let back = function::<Query>(base, CAN_GO_BACK)(contents);
    let table = *(contents as *const *const usize);
    let get_controller: Pointer = std::mem::transmute(*table.add(GET_CONTROLLER_SLOT));
    let controller = get_controller(contents);
    let forward = function::<Query>(base, CAN_GO_FORWARD)(controller);
    Some((back, forward))
}

/// The address the active page of `window` shows, as the address bar would.
pub unsafe fn visible_url(window: HWND) -> Option<String> {
    let base = base()?;
    let contents = web_contents(base, window)?;
    let url = function::<Pointer>(base, VISIBLE_URL)(contents);
    // A GURL starts with its spec, a libc++ string in Chromium's layout: the top bit of the
    // last byte marks the long form, which keeps its data pointer and size in the first two
    // words. The short form keeps the text in place and its size in the last byte.
    let last = *(url as *const u8).add(23);
    let (data, size) = if last & 0x80 != 0 {
        let words = url as *const usize;
        (*words as *const u8, *words.add(1))
    } else {
        (url as *const u8, last as usize)
    };
    if size > 1 << 20 {
        return None;
    }
    String::from_utf8(std::slice::from_raw_parts(data, size).to_vec()).ok()
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

/// Turns what a person typed into an address: a URL as typed, a bare host with `https://`, or
/// otherwise a web search.
pub fn address_for(input: &str) -> String {
    let input = input.trim();
    if input.contains("://") || input.starts_with("about:") {
        return input.to_owned();
    }
    if !input.contains(char::is_whitespace)
        && (input.contains('.') || input.starts_with("localhost"))
    {
        return format!("https://{input}");
    }
    let mut query = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                query.push(byte as char)
            }
            b' ' => query.push('+'),
            _ => query.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("https://www.google.com/search?q={query}")
}

#[cfg(test)]
mod tests {
    use super::address_for;

    #[test]
    fn typed_addresses_become_urls_or_searches() {
        assert_eq!(address_for("https://a.test/x"), "https://a.test/x");
        assert_eq!(address_for(" example.com "), "https://example.com");
        assert_eq!(address_for("localhost:8080"), "https://localhost:8080");
        assert_eq!(
            address_for("rivet browser"),
            "https://www.google.com/search?q=rivet+browser"
        );
    }
}
