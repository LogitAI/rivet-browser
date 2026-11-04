//! What the browser's own address bar would do with typed text, the page's security state and its
//! page info bubble, done by Chromium's functions for Rivet's address bar.
//!
//! An app window has no `OmniboxView`, but the services behind it belong to the profile:
//! `AutocompleteClassifier` turns typed text into the address the browser would open, with the
//! profile's default search engine for a search. The addresses come from the build's row, each
//! checked against its first bytes before any call.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;

use crate::builds::Omnibox as Functions;

/// The loaded build's address bar functions and module base, when its row records them and every
/// first byte matches the loaded module.
fn loaded() -> Option<(&'static Functions, usize)> {
    static LOADED: OnceLock<Option<(&'static Functions, usize)>> = OnceLock::new();
    *LOADED.get_or_init(|| unsafe {
        let functions = crate::builds::current()?.omnibox.as_ref()?;
        let image = crate::chromium::image::recorded()?;
        let ok = functions
            .signatures
            .iter()
            .all(|(rva, bytes)| image.bytes(*rva, bytes.len()) == Some(*bytes));
        crate::trace::trace(&format!("omnibox: recorded functions match: {ok}"));
        ok.then_some((functions, image.base))
    })
}

unsafe fn at<T: Copy>(base: usize, rva: usize) -> T {
    std::mem::transmute_copy(&(base + rva))
}

type Pointer = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
/// `Classify(this, text, prefer_keyword, allow_exact_keyword_match, page_classification, match,
/// alternate_nav_url)`. Edge's takes the current page's address and a flag after them. Chromium's
/// leaves the two extra arguments on the stack unread, so both get them.
type Classify = unsafe extern "system" fn(
    *mut c_void,
    *const [usize; 3],
    bool,
    bool,
    i32,
    *mut c_void,
    *mut c_void,
    *const c_void,
    bool,
);
type Construct = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type ReturnsInto = unsafe extern "system" fn(*mut *mut c_void, *mut c_void) -> *mut *mut c_void;
type Level = unsafe extern "system" fn(*const c_void) -> i32;
type Free = unsafe extern "system" fn(*mut c_void);
type ShowPageInfo = unsafe extern "system" fn(*mut c_void, *mut usize, i32, u64) -> bool;

/// `metrics::OmniboxEventProto::OTHER`: a page that is not the new tab page.
const PAGE_OTHER: i32 = 4;

/// Storage for a Chromium object built in place, aligned like any C++ object.
#[repr(C, align(16))]
struct Raw<const N: usize>([u8; N]);

/// A `std::u16string` that borrows `text`. libc++ marks its long form by the top bit of the last
/// byte, and the callee only reads it.
fn borrowed_u16(text: &[u16]) -> [usize; 3] {
    [
        text.as_ptr() as usize,
        text.len(),
        (text.len() + 1) | (1usize << 63),
    ]
}

/// The text of a `GURL`'s spec, a `std::string` at its start.
unsafe fn spec(gurl: *const u8) -> Option<String> {
    let last = *gurl.add(23);
    let (data, size) = if last & 0x80 != 0 {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl, last as usize)
    };
    (size <= 1 << 16)
        .then(|| String::from_utf8_lossy(std::slice::from_raw_parts(data, size)).into_owned())
}

unsafe fn active_contents(window: HWND) -> Option<*mut c_void> {
    crate::features::navigation::active_contents(window)
}

/// The address the browser's address bar would open for `text`: a typed address as the browser
/// completes it, a search on the profile's default search engine for anything else. `None` when the
/// build has no recorded classifier.
pub(crate) unsafe fn classify(window: HWND, text: &str) -> Option<String> {
    let (f, base) = loaded()?;
    let contents = active_contents(window)?;
    let context = at::<Pointer>(base, f.browser_context)(contents);
    let profile = at::<Pointer>(base, f.profile_from_context)(context);
    let classifier = at::<Pointer>(base, f.classifier_for_profile)(profile);
    if classifier.is_null() {
        return None;
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let string = borrowed_u16(&wide);
    let mut found = Box::new(Raw::<0x1000>([0; 0x1000]));
    debug_assert!(f.match_size <= 0x1000);
    let found = found.0.as_mut_ptr().cast::<c_void>();
    at::<Construct>(base, f.match_new)(found);
    // An empty address stands for the current page: the text alone decides what opens.
    crate::features::navigation::with_address("", |current| {
        at::<Classify>(base, f.classify)(
            classifier,
            &string,
            false,
            false,
            PAGE_OTHER,
            found,
            std::ptr::null_mut(),
            current,
            false,
        )
    })?;
    let url = spec(found.cast::<u8>().add(f.match_destination));
    at::<Construct>(base, f.match_drop)(found);
    url.filter(|url| !url.is_empty())
}

/// Opens what was typed as the browser's address bar would.
pub(crate) unsafe fn open_typed(window: HWND, text: &str) -> Option<bool> {
    let url = classify(window, text)?;
    crate::trace::trace(&format!("omnibox: {text:?} classified as {url:?}"));
    Some(crate::features::navigation::open(window, &url))
}

/// The page's `security_state::SecurityLevel` as Chromium computes it for its own address bar.
pub(crate) unsafe fn security_level(window: HWND) -> Option<i32> {
    let (f, base) = loaded()?;
    let contents = active_contents(window)?;
    let mut state: *mut c_void = std::ptr::null_mut();
    at::<ReturnsInto>(base, f.visible_security_state)(&mut state, contents);
    if state.is_null() {
        return None;
    }
    let level = at::<Level>(base, f.security_level)(state);
    at::<Construct>(base, f.security_state_drop)(state);
    at::<Free>(base, f.delete)(state);
    Some(level)
}

/// Opens the browser's page info bubble for the active page. The anchor is the location bar's, which
/// an app window resolves to its own frame.
pub(crate) unsafe fn page_info(window: HWND) -> bool {
    let Some((f, base)) = loaded() else {
        return false;
    };
    let Some(contents) = active_contents(window) else {
        return false;
    };
    // An empty `base::OnceCallback`, passed by address and consumed by the callee, and no setting to
    // highlight: an empty `std::optional`.
    let mut callback: usize = 0;
    const ANCHOR_LOCATION_BAR: i32 = 0;
    crate::features::bubble_anchor::with_page_info(|| {
        at::<ShowPageInfo>(base, f.show_page_info)(contents, &mut callback, ANCHOR_LOCATION_BAR, 0)
    })
}

/// One suggestion of the browser's autocomplete, as its own dropdown shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Suggestion {
    pub contents: String,
    pub description: String,
    pub destination: String,
}

type Allocate = unsafe extern "system" fn(usize) -> *mut c_void;
type WithProfile = unsafe extern "system" fn(*mut c_void, *mut c_void) -> *mut c_void;
/// Chromium returns an `int`, some builds an `int64_t`. Only the
/// low half is kept, which both read the same.
type DefaultProviders = unsafe extern "system" fn(bool) -> i32;
type ControllerNew =
    unsafe extern "system" fn(*mut c_void, *mut *mut c_void, *const Config) -> *mut c_void;
type InputNew = unsafe extern "system" fn(
    *mut c_void,
    *const [usize; 3],
    i32,
    *const c_void,
    bool,
    i32,
    bool,
) -> *mut c_void;
type Start = unsafe extern "system" fn(*mut c_void, *const c_void);

/// `AutocompleteControllerConfig`: the providers, how long a query may run, and three switches.
#[repr(C)]
struct Config {
    /// Chromium reads an `int` here and some builds an `int64_t`, so the upper half is written as zero.
    provider_types: i64,
    stop_timer_duration_us: i64,
    unscoped_open_tab_suggestions: bool,
    disable_ml: bool,
    show_iph_matches: bool,
}

/// The browser's autocomplete of one profile, made once and kept, as the browser keeps the one of
/// its address bar. Building its providers is far slower than a query.
struct Autocomplete {
    profile: usize,
    controller: usize,
    scheme_classifier: usize,
}

thread_local! {
    static AUTOCOMPLETE: std::cell::RefCell<Vec<Autocomplete>> = const { std::cell::RefCell::new(Vec::new()) };
}

unsafe fn profile_of(f: &Functions, base: usize, window: HWND) -> Option<*mut c_void> {
    let contents = active_contents(window)?;
    let context = at::<Pointer>(base, f.browser_context)(contents);
    let profile = at::<Pointer>(base, f.profile_from_context)(context);
    (!profile.is_null()).then_some(profile)
}

unsafe fn autocomplete_for(f: &Functions, base: usize, profile: *mut c_void) -> (usize, usize) {
    if let Some(found) = AUTOCOMPLETE.with(|all| {
        all.borrow()
            .iter()
            .find(|known| known.profile == profile as usize)
            .map(|known| (known.controller, known.scheme_classifier))
    }) {
        return found;
    }
    let allocate = at::<Allocate>(base, f.allocate);
    let client = allocate(f.provider_client_size);
    at::<WithProfile>(base, f.provider_client_new)(client, profile);
    let classifier = allocate(f.scheme_classifier_size);
    at::<WithProfile>(base, f.scheme_classifier_new)(classifier, profile);
    let config = Config {
        provider_types: at::<DefaultProviders>(base, f.default_providers)(false) as u32 as i64,
        stop_timer_duration_us: 1_500_000,
        unscoped_open_tab_suggestions: false,
        disable_ml: false,
        show_iph_matches: false,
    };
    let controller = allocate(f.controller_size);
    // The client goes in as a `std::unique_ptr` passed by address, which the controller takes.
    let mut owned = client;
    at::<ControllerNew>(base, f.controller_new)(controller, &mut owned, &config);
    crate::trace::trace(&format!(
        "omnibox: autocomplete made with providers {:#x}",
        config.provider_types
    ));
    AUTOCOMPLETE.with(|all| {
        all.borrow_mut().push(Autocomplete {
            profile: profile as usize,
            controller: controller as usize,
            scheme_classifier: classifier as usize,
        })
    });
    (controller as usize, classifier as usize)
}

unsafe fn u16_text(value: *const u8) -> String {
    let last = *value.add(23);
    let (data, size) = if last & 0x80 != 0 {
        (
            *(value as *const *const u16),
            *(value.add(8) as *const usize),
        )
    } else {
        (value as *const u16, last as usize)
    };
    if size > 1 << 14 {
        return String::new();
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(data, size))
}

/// The browser's suggestions for `text`, history and typed addresses included, from the providers
/// that answer at once. Suggestions that need the network are left out, so typing never waits.
pub(crate) unsafe fn suggest(window: HWND, text: &str) -> Option<Vec<Suggestion>> {
    let (f, base) = loaded()?;
    let profile = profile_of(f, base, window)?;
    let (controller, classifier) = autocomplete_for(f, base, profile);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let string = borrowed_u16(&wide);
    let mut input = Box::new(Raw::<0x800>([0; 0x800]));
    let input = input.0.as_mut_ptr().cast::<c_void>();
    at::<InputNew>(base, f.input_new)(
        input,
        &string,
        PAGE_OTHER,
        classifier as *const c_void,
        false,
        0,
        false,
    );
    *input.cast::<u8>().add(f.input_prevent_inline) = 1;
    *input.cast::<u8>().add(f.input_omit_asynchronous) = 1;
    at::<Start>(base, f.start)(controller as *mut c_void, input);
    at::<Construct>(base, f.input_drop)(input);
    // `AutocompleteResult` starts with its `std::vector<AutocompleteMatch>`.
    let result = (controller + f.controller_published_result) as *const usize;
    // Chromium builds libc++ with the vector that keeps its size, not its end: data, size, capacity.
    let (begin, count) = (*result, *result.add(1));
    if begin == 0 || count > 64 {
        return None;
    }
    let mut out = Vec::new();
    for index in 0..count {
        let one = (begin + index * f.match_size) as *const u8;
        out.push(Suggestion {
            contents: u16_text(one.add(f.match_contents)),
            description: u16_text(one.add(f.match_description)),
            destination: spec(one.add(f.match_destination)).unwrap_or_default(),
        });
    }
    Some(out)
}

/// Patches the app window's layout so the page area steps run for it too. An app window is laid out
/// by `BrowserViewAppLayoutImpl`, not the tabbed layout the Sidebar patches.
pub(crate) unsafe fn install_app_layout() -> Result<(), String> {
    let (f, _) = loaded().ok_or("this build has no recorded app window layout")?;
    crate::browsers::chromium::layout::install_app_layout(
        f.app_layout_vtable,
        f.app_layout_slot,
        f.app_layout_proposed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_controller_config_keeps_its_switches_after_a_64_bit_provider_field() {
        // Chromium's `int` and the `int64_t` of some builds both start the config, and the timer sits at 8 in both.
        assert_eq!(std::mem::offset_of!(Config, provider_types), 0);
        assert_eq!(std::mem::offset_of!(Config, stop_timer_duration_us), 8);
        assert_eq!(
            std::mem::offset_of!(Config, unscoped_open_tab_suggestions),
            0x10
        );
        assert_eq!(std::mem::offset_of!(Config, show_iph_matches), 0x12);
    }

    #[test]
    fn fr12_typed_text_is_passed_to_chromium_as_a_borrowed_long_u16string() {
        let text: Vec<u16> = "검색 words".encode_utf16().collect();
        let string = borrowed_u16(&text);
        assert_eq!(string[0], text.as_ptr() as usize);
        assert_eq!(string[1], text.len());
        assert_eq!(string[2].to_le_bytes()[7] & 0x80, 0x80);
    }

    #[test]
    fn fr12_every_recorded_function_has_its_first_bytes() {
        for build in crate::builds::compiled() {
            let Some(known) = build.omnibox.as_ref() else {
                continue;
            };
            assert!(known.match_size <= 0x1000);
            for rva in [
                known.classify,
                known.classifier_for_profile,
                known.show_page_info,
                known.security_level,
            ] {
                assert!(
                    known.signatures.iter().any(|(at, _)| *at == rva),
                    "{} {:?}",
                    build.browser,
                    build.version
                );
            }
        }
    }
}
