//! Rivet's app window: tabs kept in an app window through Chromium's own tab strip functions.
//!
//! An app window has no maker tab strip, but its `Browser` still owns a `TabStripModel`. The
//! functions below are Chromium's. Their addresses come from the build row's `app_window` part,
//! which is used only once every prologue it records matches the loaded module. A checked test
//! profile may name other addresses in `rivet-spike.txt` among its debug switches
//! (`name 0xRVA` per line), which goes first, so an address can be measured before a row changes.
use std::ffi::c_void;
use windows_sys::Win32::Foundation::HWND;

use crate::builds::AppWindow;

/// An address or value of the app window, as an RVA of the browser module where it is a function.
pub(crate) fn recorded(name: &str) -> Option<usize> {
    switched(name).or_else(|| checked_row().and_then(|row| row_value(row, name)))
}

/// A value named in the test profile's `rivet-spike.txt`.
fn switched(name: &str) -> Option<usize> {
    let text = std::fs::read_to_string(crate::frame::debug_switch("rivet-spike.txt")?).ok()?;
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next()? == name).then(|| usize::from_str_radix(words.next()?.trim_start_matches("0x"), 16).ok())?
    })
}

/// The functions a row records, with their prologues. A function without a prologue cannot be
/// checked, so it fails the check.
pub(crate) fn prologues(row: &AppWindow) -> Vec<(&'static str, usize, &'static [u8])> {
    let mut all = vec![
        ("operator_new", Some(row.operator_new), row.operator_new_prologue),
        ("web_view_ctor", Some(row.web_view_ctor), row.web_view_ctor_prologue),
        ("add_child", Some(row.add_child), row.add_child_prologue),
        ("set_contents", Some(row.set_contents), row.set_contents_prologue),
        ("feature_impl", row.feature_impl, row.feature_impl_prologue),
        ("detach", row.detach, row.detach_prologue),
        ("insert", row.insert, row.insert_prologue),
        ("browser_context", row.browser_context, row.browser_context_prologue),
        ("set_bounds", row.set_bounds, row.set_bounds_prologue),
        ("focus_behavior", row.focus_behavior, row.focus_behavior_prologue),
        ("request_focus", row.request_focus, row.request_focus_prologue),
    ];
    all.retain(|(_, rva, _)| rva.is_some());
    all.into_iter().map(|(name, rva, bytes)| (name, rva.unwrap_or_default(), bytes)).collect()
}

/// The row's app window part, when every recorded prologue matches the loaded module.
fn checked_row() -> Option<&'static AppWindow> {
    static CHECKED: std::sync::OnceLock<Option<&'static AppWindow>> = std::sync::OnceLock::new();
    if let Some(known) = CHECKED.get() {
        return *known;
    }
    let row = crate::builds::current()?.app_window.as_ref();
    let image = crate::chromium::image::recorded()?;
    let verdict = row.filter(|row| {
        let wrong: Vec<&str> = prologues(row)
            .into_iter()
            .filter(|(_, rva, bytes)| bytes.is_empty() || unsafe { image.bytes(*rva, bytes.len()) } != Some(*bytes))
            .map(|(name, _, _)| name)
            .collect();
        if !wrong.is_empty() {
            crate::trace::trace(&format!("app window: the row is off, these functions differ: {wrong:?}"));
        }
        wrong.is_empty()
    });
    *CHECKED.get_or_init(|| verdict)
}

/// One value of the row by the name the runtime asks for.
pub(crate) fn row_value(row: &AppWindow, name: &str) -> Option<usize> {
    match name {
        "feature_impl" => row.feature_impl,
        "feature_len" => row.feature_impl.map(|_| row.feature_impl_prologue.len()),
        "kind" => Some(row.browser_type),
        "detach" => row.detach,
        "insert" => row.insert,
        "vparent" => Some(row.view_parent),
        "vchildren" => Some(row.view_children),
        "new" => Some(row.operator_new),
        "ctor" => Some(row.web_view_ctor),
        "size" => Some(row.web_view_size),
        "ctx" => row.browser_context,
        "ctxoff" => row.browser_context_offset,
        "add_child" => Some(row.add_child),
        "set_contents" => Some(row.set_contents),
        "set_bounds" => row.set_bounds,
        "focus_behavior" => row.focus_behavior,
        "request_focus" => row.request_focus,
        "focus_slot" => row.request_focus_slot,
        "dtor_slot" => Some(row.destructor_slot),
        "bounds_slot" => row.bounds_changed_slot,
        "feature_titlebar" => Some(row.window_features.map_or(FEATURE_TITLEBAR, |numbers| numbers.0) as usize),
        "feature_tabstrip" => Some(row.window_features.map_or(FEATURE_TABSTRIP, |numbers| numbers.1) as usize),
        // Everything else is a measuring aid that only a test profile names.
        _ => None,
    }
}

/// `chrome::AddAndReturnTabAt(window interface, url, index, foreground, group, group, window action)`.
type AddAndReturnTabAt = unsafe extern "system" fn(
    *mut c_void,
    *const c_void,
    i32,
    bool,
    *const [u64; 8],
    *const [u64; 8],
    u64,
) -> *mut c_void;

/// Opens `url` as a new foreground tab of this window's own tab strip, whatever the window's type.
pub(crate) unsafe fn add_tab(window: HWND, url: &str) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    let Some(rva) = recorded("add_tab") else { return false };
    let Some(browser) = crate::features::tabs::browser(base, window) else { return false };
    let none = [0u64; 8];
    let opened = crate::features::navigation::with_address(url, |gurl| {
        crate::features::tabs::function::<AddAndReturnTabAt>(base, rva)(browser, gurl, -1, true, &none, &none, 0)
    });
    crate::trace::trace(&format!("app window add_tab {url}: {opened:?}"));
    let Some(contents) = opened.filter(|contents| !contents.is_null()) else { return false };
    let _ = contents;
    true
}

/// Chromium puts a tab of an app window into a tabbed window. This moves the last tab of the first
/// other window back with Chromium's own move-to-window function, so the app window's own tab strip
/// model owns it.
pub(crate) unsafe fn move_tab_in(window: HWND) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    let Some(browser) = crate::features::tabs::browser(base, window) else { return false };
    let move_rva = recorded("move_tabs").unwrap_or(0);
    let mut moved = false;
    for other in crate::frame::browser_windows_of_process() {
        crate::trace::trace(&format!("app window move: candidate {other:?}"));
        if other == window {
            continue;
        }
        let Some(from) = crate::features::tabs::browser(base, other) else { continue };
        let Some(model) = crate::features::tabs::tab_model(base, other) else { continue };
        crate::trace::trace(&format!("app window move: from {from:?} model {model:?}"));
        let count = crate::features::tabs::function::<unsafe extern "system" fn(*mut c_void) -> i32>(base, crate::builds::get().tabs.tab_count)(model);
        crate::trace::trace(&format!("app window move: count {count}"));
        if count < 1 {
            continue;
        }
        if let (Some(detach), Some(insert), Some(target)) =
            (recorded("detach"), recorded("insert"), crate::features::tabs::tab_model(base, window))
        {
            // The tab model of the other window gives the tab up, and the app window's model takes it.
            type Detach = unsafe extern "system" fn(*mut c_void, *mut usize, i32) -> *mut usize;
            type Insert = unsafe extern "system" fn(*mut c_void, i32, *mut usize, i32, *const [u64; 8]) -> i32;
            let mut tab: usize = 0;
            crate::features::tabs::function::<Detach>(base, detach)(model, &mut tab, count - 1);
            crate::trace::trace(&format!("app window move: detached {tab:#x}"));
            let none = [0u64; 8];
            // At the end of the strip, active: `ADD_ACTIVE | ADD_FORCE_INDEX`.
            let own = crate::features::tabs::function::<unsafe extern "system" fn(*mut c_void) -> i32>(base, crate::builds::get().tabs.tab_count)(target);
            let at = crate::features::tabs::function::<Insert>(base, insert)(target, own, &mut tab, 1 | 4, &none);
            crate::trace::trace(&format!("app window move: inserted at {at}"));
        } else {
            let indices: [i32; 2] = [count - 1, 0];
            let vector: [usize; 3] = [indices.as_ptr() as usize, indices.as_ptr() as usize + 4, indices.as_ptr() as usize + 4];
            type Move = unsafe extern "system" fn(*mut c_void, *mut c_void, *const [usize; 3]);
            crate::features::tabs::function::<Move>(base, move_rva)(from, browser, &vector);
        }
        crate::trace::trace(&format!("app window moved tab {} of window {other:?}", count - 1));
        moved = true;
        break;
    }
    moved
}

/// Offsets of the browser object that hold the browser type of an app window (`TYPE_APP`, 2).
pub(crate) unsafe fn scan_type(window: HWND) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    let Some(browser) = crate::features::tabs::browser(base, window) else { return false };
    let mut hits = Vec::new();
    for offset in (0..0x600usize).step_by(4) {
        if *(browser.cast::<u8>().add(offset) as *const i32) == 2 {
            hits.push(offset);
        }
    }
    crate::trace::trace(&format!("app window type scan: browser {browser:?} offsets holding 2: {hits:x?}"));
    true
}

/// For every browser window of the process, the small values (0 to 6) its browser object holds in
/// its first 0x600 bytes. The app window's type is where it differs from a normal window's.
pub(crate) unsafe fn dump_small(_window: HWND) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    for other in crate::frame::browser_windows_of_process() {
        let Some(browser) = crate::features::tabs::browser(base, other) else { continue };
        let mut values = String::new();
        for offset in (0..0x600usize).step_by(4) {
            let value = *(browser.cast::<u8>().add(offset) as *const i32);
            if (0..=6).contains(&value) {
                values.push_str(&format!("{offset:x}={value} "));
            }
        }
        let own = crate::frame::rivet_windows().contains(&other);
        crate::trace::trace(&format!("app window dump: window {other:?} rivet {own}: {values}"));
    }
    true
}

/// Opens a new tab with the browser type of this window set to a normal window's for the call, so the
/// browser's own new-tab code keeps the tab in this window. The type is put back afterwards.
pub(crate) unsafe fn flip_new_tab(window: HWND, offsets: &[usize]) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    let Some(browser) = crate::features::tabs::browser(base, window) else { return false };
    for offset in offsets {
        let value = *(browser.cast::<u8>().add(*offset) as *const i32);
        if value != 2 {
            crate::trace::trace(&format!("app window flip: offset {offset:#x} holds {value}, not 2"));
            return false;
        }
    }
    for offset in offsets {
        *(browser.cast::<u8>().add(*offset) as *mut i32) = 0;
    }
    let opened = crate::features::tabs::new_tab(window);
    for offset in offsets {
        *(browser.cast::<u8>().add(*offset) as *mut i32) = 2;
    }
    crate::trace::trace(&format!("app window flip: offsets {offsets:x?} opened {opened}"));
    opened
}

/// A word of memory that may not be mapped.
unsafe fn safe_word(address: usize) -> Option<usize> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut value = 0usize;
    let mut done = 0usize;
    let ok = ReadProcessMemory(GetCurrentProcess(), address as *const c_void, (&mut value as *mut usize).cast(), 8, &mut done);
    (ok != 0 && done == 8).then_some(value)
}


/// Where a View keeps its parent and its child list in this build, as recorded for the spike.
fn view_offsets() -> (usize, usize) {
    (recorded("vparent").unwrap_or(0x198), recorded("vchildren").unwrap_or(0x1A0))
}

/// The children of a view. The list is (begin, end, capacity) or (begin, count, capacity), by build.
unsafe fn children_of(view: usize, offset: usize) -> Vec<usize> {
    let begin = safe_word(view + offset).unwrap_or(0);
    let second = safe_word(view + offset + 8).unwrap_or(0);
    let capacity = safe_word(view + offset + 16).unwrap_or(0);
    if begin < 0x10000 {
        return Vec::new();
    }
    let count = if second >= begin && (second - begin) % 8 == 0 && (second - begin) / 8 <= 512 {
        (second - begin) / 8
    } else if second <= 512 && second <= capacity {
        second
    } else {
        return Vec::new();
    };
    (0..count).filter_map(|index| safe_word(begin + index * 8)).collect()
}

/// A word that points into the browser module.
fn in_image(word: usize, base: usize, end: usize) -> bool {
    word > base && word < end
}

/// The browser view's `views::View`, found in the object the window part points into: a View with
/// a child list of views, whose own vtable and its children's are in the module.
unsafe fn find_browser_view(window_part: usize, base: usize, end: usize, children_offset: usize) -> Option<usize> {
    for step in (0..0x2000usize).step_by(8) {
        let q = window_part.wrapping_sub(0x1000).wrapping_add(step);
        if !safe_word(q).is_some_and(|vptr| in_image(vptr, base, end)) {
            continue;
        }
        let children = children_of(q, children_offset);
        if (3..=64).contains(&children.len()) && children.iter().all(|child| safe_word(*child).is_some_and(|vptr| in_image(vptr, base, end))) {
            crate::trace::trace(&format!("view tree: the browser view's View is {:+#x} from the window part, {} children", q as isize - window_part as isize, children.len()));
            return Some(q);
        }
    }
    None
}

/// Traces the views below the browser view with their vtables and the page pointer they hold.
pub(crate) unsafe fn view_tree(window: HWND) -> bool {
    let Some(base) = crate::features::tabs::base() else { return false };
    let end = base + crate::chromium::image::recorded().map_or(0, |image| image.size);
    let (parent_offset, children_offset) = view_offsets();
    let Some(part) = crate::features::tabs::browser_view(window) else { return false };
    let Some(root) = find_browser_view(part as usize, base, end, children_offset) else {
        crate::trace::trace("view tree: the browser view's View was not found");
        return false;
    };
    let Some(model) = crate::features::tabs::tab_model(base, window) else { return false };
    let contents = crate::features::tabs::contents_at(base, model, 0) as usize;
    let mut queue = vec![(root, 0usize)];
    let mut seen = 0;
    while let Some((view, depth)) = queue.pop() {
        seen += 1;
        if seen > 300 || depth > 6 {
            continue;
        }
        let vptr = safe_word(view).unwrap_or(0);
        let mut holds = Vec::new();
        for offset in (8..0x700usize).step_by(8) {
            let word = safe_word(view + offset).unwrap_or(0);
            if contents != 0 && word == contents {
                holds.push(format!("{offset:#x}"));
            }
        }
        let parent = safe_word(view + parent_offset).unwrap_or(0);
        let kids = children_of(view, children_offset);
        crate::trace::trace(&format!("view tree: {}{view:#x} vtable {:#x} parent_ {parent:#x} children {} holds the first page at [{}] bounds_ {:?}", "  ".repeat(depth), vptr.wrapping_sub(base), kids.len(), holds.join(","), [0usize, 4, 8, 12].map(|o| safe_word(view + parent_offset + 0x30 + (o & !7)).map_or(0, |w| if o % 8 == 0 { w as u32 as i32 } else { (w >> 32) as u32 as i32 }))));
        if !holds.is_empty() && kids.len() == 1 {
            // A pointer the page and this view both hold, whose own first word points into the module,
            // is the browser context (a Profile), which the view keeps from its constructor.
            let mut shared = Vec::new();
            for view_offset in (8..0x540usize).step_by(8) {
                let word = safe_word(view + view_offset).unwrap_or(0);
                if word < 0x10000 || word == contents {
                    continue;
                }
                for page_offset in (0..0x700usize).step_by(8) {
                    if safe_word(contents + page_offset) == Some(word) && safe_word(word).is_some_and(|vptr| in_image(vptr, base, end)) {
                        shared.push(format!("view+{view_offset:#x}=page+{page_offset:#x}={word:#x}"));
                    }
                }
            }
            crate::trace::trace(&format!("view tree: shared pointers of the page and the page view: {}", shared.join(" ")));
        }
        for kid in kids {
            queue.push((kid, depth + 1));
        }
    }
    true
}

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The trampoline that runs `WindowFeatureController::SupportsWindowFeatureImpl` as the browser wrote it.
static ORIGINAL_FEATURE: AtomicUsize = AtomicUsize::new(0);
/// The window features an app window answers yes to, one bit per `WindowFeature` value.
static FEATURE_MASK: AtomicU32 = AtomicU32::new(0);
static FEATURE_CALLS: AtomicU32 = AtomicU32::new(0);

type FeatureImpl = unsafe extern "system" fn(*mut u8, u32, u8) -> u8;

/// Chromium's `Browser::WindowFeature` values that Rivet answers for an app window. A build that
/// numbers them otherwise records its own values in its row.
const FEATURE_TITLEBAR: u32 = 1;
const FEATURE_TABSTRIP: u32 = 2;
/// The loaded build's title bar and tab strip values.
static TITLEBAR: AtomicU32 = AtomicU32::new(FEATURE_TITLEBAR);
static TABSTRIP: AtomicU32 = AtomicU32::new(FEATURE_TABSTRIP);

/// The title bar and tab strip values of a build's `WindowFeature`.
#[derive(Clone, Copy)]
pub(crate) struct Features {
    pub title_bar: u32,
    pub tab_strip: u32,
}
/// `BrowserWindowInterface::TYPE_APP`.
const TYPE_APP: i32 = 2;

/// What an app window answers. `check` is set for `CanSupportWindowFeature`, which Chromium's
/// navigation asks before it puts a new tab into a window: yes keeps every tab Rivet or a page
/// opens inside the app window's own tab strip model, so no normal window is made. Layout asks
/// `SupportsWindowFeature` (no `check`), which stays no, so no tab strip is drawn. The title bar
/// is never asked for, because Rivet's frame draws the top of the window.
pub(crate) fn app_feature_answer(
    numbers: Features,
    kind: i32,
    feature: u32,
    check: bool,
    opening_tab: bool,
    original: bool,
) -> bool {
    if kind != TYPE_APP {
        return original;
    }
    // The browser's new-tab command asks `SupportsWindowFeature` before it adds the tab.
    if feature == numbers.tab_strip && (check || opening_tab) {
        return true;
    }
    if feature == numbers.title_bar {
        return false;
    }
    original
}

/// Answers the browser's window feature question. The controller keeps the browser type at `+0x10`
/// (`kind` in `rivet-spike.txt` names another offset).
unsafe extern "system" fn feature_hook(this: *mut u8, feature: u32, check: u8) -> u8 {
    let original: FeatureImpl = std::mem::transmute_copy(&ORIGINAL_FEATURE.load(Ordering::Acquire));
    let answer = original(this, feature, check);
    let kind = *(this.add(KIND_OFFSET.load(Ordering::Relaxed)) as *const i32);
    let numbers = Features {
        title_bar: TITLEBAR.load(Ordering::Relaxed),
        tab_strip: TABSTRIP.load(Ordering::Relaxed),
    };
    let mut result = app_feature_answer(numbers, kind, feature, check != 0, crate::features::tabs::opening_tab(), answer != 0);
    if kind == TYPE_APP && feature < 32 && FEATURE_MASK.load(Ordering::Relaxed) & (1 << feature) != 0 {
        result = !result;
    }
    if FEATURE_CALLS.fetch_add(1, Ordering::Relaxed) < 400 {
        crate::trace::trace(&format!("window feature: type {kind} feature {feature} check {check} answer {answer} gives {result}"));
    }
    u8::from(result)
}
static KIND_OFFSET: AtomicUsize = AtomicUsize::new(0x10);

/// Installs the feature answer as soon as the browser module is loaded, before any window exists.
pub(crate) fn start_early() {
    std::thread::spawn(|| unsafe {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        for _ in 0..4000 {
            let loaded = crate::builds::all().iter().any(|build| {
                let name: Vec<u16> = build.module.encode_utf16().chain(Some(0)).collect();
                !GetModuleHandleW(name.as_ptr()).is_null()
            });
            if loaded {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        install_features();
    });
}

/// Installs the window feature answer once, at attach or before the first window.
pub(crate) fn install_features() {
    crate::features::bubble_anchor::install();
    let Some(image) = crate::chromium::image::recorded() else {
        crate::trace::trace("app window features: no recorded image");
        return;
    };
    let (Some(rva), Some(length)) = (recorded("feature_impl"), recorded("feature_len")) else { return };
    if ORIGINAL_FEATURE.load(Ordering::Acquire) != 0 {
        return;
    }
    FEATURE_MASK.store(recorded("feature_mask").unwrap_or(0) as u32, Ordering::Relaxed);
    KIND_OFFSET.store(recorded("kind").unwrap_or(0x10), Ordering::Relaxed);
    TITLEBAR.store(recorded("feature_titlebar").map_or(FEATURE_TITLEBAR, |value| value as u32), Ordering::Relaxed);
    TABSTRIP.store(recorded("feature_tabstrip").map_or(FEATURE_TABSTRIP, |value| value as u32), Ordering::Relaxed);
    let Some(prologue) = (unsafe { image.bytes(rva, length) }) else { return };
    // A row's prologue was already compared with the module. An address from a test profile is
    // patched with the bytes it finds, which only a test run accepts.
    let result = unsafe {
        crate::targets::transaction(|| {
            crate::targets::detour(image.base, rva, prologue, feature_hook as usize, &ORIGINAL_FEATURE)
        })
    };
    crate::trace::trace(&format!(
        "app window features: hook {result:?} pid {} mask {:#x}",
        std::process::id(),
        FEATURE_MASK.load(Ordering::Relaxed)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROMIUM: Features = Features { title_bar: FEATURE_TITLEBAR, tab_strip: FEATURE_TABSTRIP };

    #[test]
    fn app_window_new_tabs_stay_in_the_app_window_without_a_drawn_tab_strip() {
        // Navigation asks with `check`: yes, so a new tab is not sent to a normal window.
        assert!(app_feature_answer(CHROMIUM, TYPE_APP, FEATURE_TABSTRIP, true, false, false));
        // The new-tab command asks without `check` while it runs.
        assert!(app_feature_answer(CHROMIUM, TYPE_APP, FEATURE_TABSTRIP, false, true, false));
        // Layout asks without `check`: the app window still draws no tab strip.
        assert!(!app_feature_answer(CHROMIUM, TYPE_APP, FEATURE_TABSTRIP, false, false, false));
        // No title bar strip above the page.
        assert!(!app_feature_answer(CHROMIUM, TYPE_APP, FEATURE_TITLEBAR, false, false, true));
        // A normal window is answered as the browser wrote it.
        assert!(app_feature_answer(CHROMIUM, 0, FEATURE_TITLEBAR, false, false, true));
        assert!(!app_feature_answer(CHROMIUM, 0, FEATURE_TABSTRIP, true, true, false));
    }

    #[test]
    fn fr13_whale_new_tab_is_added_to_the_app_window_by_chromium_add_tab_at() {
        // Only the Whale row records `AddTabAt`, the Chromium function its new tabs need.
        let (rva, prologue) = crate::builds::add_tab_at_for(&crate::builds::WHALE).expect("Whale records AddTabAt");
        assert_eq!(rva, 0x0E93_5CA0);
        assert_eq!(prologue.len(), 15);
        assert!(crate::builds::add_tab_at_for(&crate::builds::BRAVE_63).is_none());
        assert!(crate::builds::add_tab_at_for(&crate::builds::EDGE).is_none());
    }

    #[test]
    fn fr13_app_window_answers_with_the_build_s_own_window_feature_numbers() {
        let whale = crate::builds::WHALE.app_window.as_ref().unwrap();
        let (title_bar, tab_strip, toolbar) = whale.window_features.expect("Whale records its own numbers");
        assert_eq!((title_bar, tab_strip, toolbar), (1, 5, 6));
        let numbers = Features { title_bar, tab_strip };
        // The row's tab strip number is 5. Number 2 is another feature there and keeps its answer.
        assert!(app_feature_answer(numbers, TYPE_APP, 5, true, false, false));
        assert!(!app_feature_answer(numbers, TYPE_APP, 2, true, false, false));
        assert!(!app_feature_answer(numbers, TYPE_APP, 1, false, false, true));
        assert_eq!(row_value(whale, "feature_tabstrip"), Some(5));
        // A row without its own numbers uses Chromium's.
        let brave = crate::builds::BRAVE_63.app_window.as_ref().unwrap();
        assert_eq!(row_value(brave, "feature_tabstrip"), Some(2));
        assert_eq!(row_value(brave, "feature_titlebar"), Some(1));
    }
}
