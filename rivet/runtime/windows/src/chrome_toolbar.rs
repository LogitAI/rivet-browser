//! Experiment: restyling Chromium's own toolbar so the top row of a Rivet window follows the
//! Rivet design, while other windows of the same browser process keep Chromium's look.
//!
//! Rivet runs in the person's own profile, so Rivet windows and ordinary windows share one
//! process, one UI thread and one colour provider. Every change here therefore applies only while
//! that thread works for a Rivet window: while one of its widgets lays out, paints or refreshes
//! its theme, and while its window procedure runs. That span is the Rivet scope.
//!
//! - `GetLayoutConstant` answers Rivet sizes inside the scope.
//! - The colour provider holds Rivet colours inside the scope and its own outside.
//! - Back, forward and reload icons are swapped when a Rivet window builds its button images.
//! - The app menu, profile and page action buttons stay hidden in Rivet windows.
//!
//! The addresses below belong to Google Chrome 156.0.8078.48 only.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// `GetLayoutConstant(LayoutConstant)`.
const LAYOUT_CONSTANT: usize = 0x0471_3020;
const LAYOUT_CONSTANT_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0xF0, 0x45, 0x0D,
];
/// `views::Widget::ThemeChanged`.
const THEME_CHANGED: usize = 0x01DC_5870;
const THEME_CHANGED_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xC0, 0xC7, 0xDA, 0x0F,
];
/// `views::View::OnPaintLayer` implementation.
const PAINT_LAYER: usize = 0x007E_1A30;
const PAINT_LAYER_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x00, 0x06, 0x39, 0x11,
];
/// `views::View::SetVisible`. Its first sixteen bytes save registers and reserve the frame.
const SET_VISIBLE: usize = 0x0573_5770;
const SET_VISIBLE_PROLOGUE: [u8; 16] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x48,
];
/// `ui::ImageModel::FromVectorIcon(out, icon, colour, size)`.
const FROM_VECTOR_ICON: usize = 0x009F_C510;
const FROM_VECTOR_ICON_PROLOGUE: [u8; 14] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
];
/// `LocationIconView::Update(this, bool, bool)` sets the label and then the icon.
const UPDATE_LOCATION: usize = 0x00EE_3320;
const UPDATE_LOCATION_PROLOGUE: [u8; 15] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0, 0, 0,
];
/// `LocationIconView::UpdateIcon` runs after navigation, outside widget painting.
const UPDATE_LOCATION_ICON: usize = 0x00EE_4220;
const UPDATE_LOCATION_ICON_PROLOGUE: [u8; 16] = [
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0, 0, 0, 0x48, 0x89, 0xCE,
];
/// `location_bar::GetSecurityChipText(out, delegate, url, flag)` returns the label Chrome puts
/// before the address of local pages (file, chrome and similar schemes) and of insecure ones.
const CHIP_TEXT: usize = 0x00E4_D780;
const CHIP_TEXT_PROLOGUE: [u8; 15] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0, 0, 0,
];
/// `BubbleDialogDelegate::GetAnchorRect`, `View::IsDrawn` and `View::GetBoundsInScreen`.
const ANCHOR_RECT: usize = 0x0ACA_C450;
const ANCHOR_RECT_PROLOGUE: [u8; 18] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC,
    0xD0, 0, 0, 0,
];
const ANCHOR_VIEW_ACCESS: [u8; 21] = [
    0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
    0x48, 0x85, 0xDB, 0x74, 0x4D,
];
const IS_DRAWN: usize = 0x00CA_89C0;
const IS_DRAWN_BYTES: [u8; 32] = [
    0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
    0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
];
const BOUNDS_IN_SCREEN: usize = 0x0086_1140;
const BOUNDS_BYTES: [u8; 13] = [0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF];
const PARENT_OFFSET: usize = 0x198;
const IS_DRAWN_SLOT: usize = 19;
/// Both prologues with a stack cookie copy their first nine bytes and reload the cookie, which
/// ends at sixteen bytes.
const COOKIE_COPIED: usize = 9;

/// `BrowserWidget`'s vtable, with `OnNativeWidgetPaint` and `LayoutRootViewIfNecessary`.
const BROWSER_WIDGET_VTABLE: usize = 0x0FBC_AD10;
const PAINT_SLOT: usize = 30;
const PAINT: usize = 0x0315_D050;
const LAYOUT_SLOT: usize = 44;
const LAYOUT: usize = 0x00F9_7180;
/// `views::View::GetColorProvider`, asked of the widget's root view, which the widget keeps at
/// `+0x1A0`.
const COLOR_PROVIDER: usize = 0x04AA_27D0;
const ROOT_VIEW_OFFSET: usize = 0x1A0;

/// `views::View` vtables of the buttons a Rivet window hides, with their `OnThemeChanged`.
const THEME_SLOT: usize = 99;
const HIDDEN: [(usize, usize); 4] = [
    (0x0FEA_63C8, 0x01D1_3080), // BrowserAppMenuButton
    (0x0FEE_C268, 0x01AE_4DC0), // AvatarToolbarButton
    (0x0FED_B708, 0x003B_A0E0), // page_actions::PageActionView
    (0x0FF1_7AA8, 0x00A3_E970), // VerticalTabStripRegionView
];
/// `views::View::GetWidget` is the twelfth entry of every view's vtable.
const GET_WIDGET_SLOT: usize = 11;

/// `LayoutConstant` values of this build and the sizes of the Rivet top row, in DIPs.
const OVERRIDES: [(i32, i32); 7] = [
    (12, 8),  // kLocationBarChildCornerRadius
    (16, 28), // kLocationBarHeight
    (29, 12), // kMainBackgroundRegionCornerRadius
    (46, 28), // kToolbarButtonHeight
    (47, 16), // kToolbarButtonIconSize
    (55, 8),  // kToolbarCornerRadius
    (60, 0),  // kVerticalTabMinWidth
];

const PAPER: u32 = 0xFFFB_FAF7;
const HAIRLINE: u32 = 0xFFE9_E9E6;
/// Colour ids of this build and the Rivet colours, as `0xAARRGGBB`.
const COLOURS: [(i32, u32); 16] = [
    (0x13C, PAPER),      // kColorFrameActive
    (796, PAPER),        // kColorLocationBarBackground
    (797, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
    (1013, PAPER),       // Location bar background in the alternate state
    (1055, 0xFF4A_4850), // kColorOmniboxText
    (1056, 0xFF8A_8690), // kColorOmniboxTextDimmed
    (1590, PAPER),       // kColorToolbar
    (1645, 0xFF1A_1A1A), // Caption button glyph, active window
    (1646, 0xFF1A_1A1A), // Caption button glyph, inactive window
    (1647, 0xFFC4_2B1C), // Close button hover fill
    (1595, 0xFF4A_4850), // kColorToolbarButtonIcon
    (1598, 0xFF1F_2024), // kColorToolbarButtonIconHovered
    (1599, 0xFFC4_C1C8), // kColorToolbarButtonIconInactive
    (1603, HAIRLINE),    // kColorToolbarContentAreaSeparator
    (1611, 0x0F1F_2024), // kColorToolbarInkDropHover
    (1612, 0x1F1F_2024), // kColorToolbarInkDropRipple
];

type LayoutConstant = unsafe extern "system" fn(i32) -> i32;
type WidgetCall = unsafe extern "system" fn(*mut c_void);
type WidgetPaint = unsafe extern "system" fn(*mut c_void, *const c_void);
type SetVisible = unsafe extern "system" fn(*mut c_void, bool);
type Provider = unsafe extern "system" fn(*mut c_void) -> *const u8;
type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;

struct Hooks {
    base: usize,
    paint: WidgetPaint,
    layout: WidgetCall,
    provider: Provider,
    hidden: Vec<(usize, WidgetCall)>,
}

static HOOKS: OnceLock<Result<Hooks, String>> = OnceLock::new();
/// Trampolines to the original functions. Each is stored before its function is patched, so a
/// hook never runs without one.
static ORIGINAL_LAYOUT_CONSTANT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_THEME_CHANGED: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_PAINT_LAYER: AtomicUsize = AtomicUsize::new(0);
static LAYER_LOGGED: AtomicBool = AtomicBool::new(false);
static ORIGINAL_SET_VISIBLE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_LOCATION: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_LOCATION_ICON: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_CHIP_TEXT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_ANCHOR_RECT: AtomicUsize = AtomicUsize::new(0);
/// The provider caches a colour when it is first asked for, so a colour can be swapped only after
/// that. This remembers which ones have been seen, for the trace.
static COLOURS_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());
/// The widgets of Rivet windows.
static WIDGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

thread_local! {
    /// The Rivet widget the UI thread works for, or zero.
    static CURRENT: Cell<usize> = const { Cell::new(0) };
    /// Provider colours replaced for the current Rivet widget, with their own values.
    static SWAPPED: RefCell<Vec<(*mut u32, u32)>> = const { RefCell::new(Vec::new()) };
}

fn hooks() -> Option<&'static Hooks> {
    HOOKS.get()?.as_ref().ok()
}

fn is_rivet(widget: usize) -> bool {
    widget != 0
        && WIDGETS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&widget)
}

fn active() -> bool {
    CURRENT.with(Cell::get) != 0
}

unsafe fn colour_slot(provider: *const u8, id: i32) -> Option<*mut u32> {
    let mut node = *(provider.add(0x18) as *const *mut u8);
    while !node.is_null() {
        let key = *(node.add(0x20) as *const i32);
        if key == id {
            return Some(node.add(0x24) as *mut u32);
        }
        node = if id < key {
            *(node as *const *mut u8)
        } else {
            *(node.add(8) as *const *mut u8)
        };
    }
    None
}

fn first_swap(id: i32) -> bool {
    let mut seen = COLOURS_SEEN.lock().unwrap_or_else(|error| error.into_inner());
    if seen.contains(&id) {
        return false;
    }
    seen.push(id);
    true
}

unsafe fn switch_to(hooks: &Hooks, widget: usize) {
    if CURRENT.with(Cell::get) == widget {
        return;
    }
    SWAPPED.with(|swapped| {
        for (slot, value) in swapped.borrow_mut().drain(..) {
            slot.write(value);
        }
    });
    CURRENT.with(|current| current.set(widget));
    if widget == 0 {
        return;
    }
    let root = *((widget + ROOT_VIEW_OFFSET) as *const *mut c_void);
    if root.is_null() {
        return;
    }
    let provider = (hooks.provider)(root);
    if provider.is_null() {
        return;
    }
    SWAPPED.with(|swapped| {
        let mut swapped = swapped.borrow_mut();
        for (id, colour) in COLOURS {
            if let Some(slot) = colour_slot(provider, id) {
                let original = slot.read();
                if first_swap(id) {
                    crate::vertical_tabs::trace(&format!("colour {id:#x} replaced {original:#010x} with {colour:#010x}"));
                }
                swapped.push((slot, original));
                slot.write(colour);
            }
        }
    });
}

/// Runs `work` in the scope of `widget`: Rivet's look when it belongs to a Rivet window, the
/// browser's otherwise. Scopes nest, and leaving one restores the outer one.
unsafe fn scoped<R>(widget: usize, work: impl FnOnce() -> R) -> R {
    let Some(hooks) = hooks() else { return work() };
    let outer = CURRENT.with(Cell::get);
    switch_to(hooks, if is_rivet(widget) { widget } else { 0 });
    let result = work();
    switch_to(hooks, outer);
    result
}

unsafe fn widget_of(window: HWND) -> usize {
    let Some(hooks) = hooks() else { return 0 };
    let content: Pointer = std::mem::transmute(hooks.base + CONTENT_WINDOW_FOR_HWND);
    let widget_for: Pointer = std::mem::transmute(hooks.base + WIDGET_FOR_WINDOW);
    let aura = content(window as *const c_void);
    if aura.is_null() {
        return 0;
    }
    widget_for(aura) as usize
}

/// Runs `work` in the scope of `window`, for its window procedure.
pub unsafe fn for_window<R>(window: HWND, work: impl FnOnce() -> R) -> R {
    if hooks().is_none() {
        return work();
    }
    scoped(widget_of(window), work)
}

unsafe fn original<T: Copy>(slot: &AtomicUsize) -> T {
    std::mem::transmute_copy(&slot.load(Ordering::Acquire))
}

unsafe extern "system" fn layout_constant(constant: i32) -> i32 {
    if active() {
        if let Some((_, value)) = OVERRIDES.iter().find(|(key, _)| *key == constant) {
            return *value;
        }
    }
    original::<LayoutConstant>(&ORIGINAL_LAYOUT_CONSTANT)(constant)
}

unsafe extern "system" fn theme_changed(widget: *mut c_void) {
    let run: WidgetCall = match ORIGINAL_THEME_CHANGED.load(Ordering::Acquire) {
        0 => std::mem::transmute(hooks().map_or(0, |hooks| hooks.base) + THEME_CHANGED),
        _ => original(&ORIGINAL_THEME_CHANGED),
    };
    scoped(widget as usize, || run(widget));
}

/// Layer-backed views paint separately from the widget paint callback.
unsafe extern "system" fn paint_layer(view: *mut c_void, context: *const c_void) {
    let table = *(view as *const usize);
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(GET_WIDGET_SLOT));
    let widget = get_widget(view) as usize;
    if is_rivet(widget) && !active() && !LAYER_LOGGED.swap(true, Ordering::Relaxed) {
        crate::vertical_tabs::trace("rivet layer paint outside scope");
    }
    let run: WidgetPaint = original(&ORIGINAL_PAINT_LAYER);
    scoped(widget, || run(view, context));
}

unsafe extern "system" fn widget_paint(widget: *mut c_void, context: *const c_void) {
    let hooks = hooks().expect("patched after it is stored");
    scoped(widget as usize, || (hooks.paint)(widget, context));
}

unsafe extern "system" fn widget_layout(widget: *mut c_void) {
    let hooks = hooks().expect("patched after it is stored");
    scoped(widget as usize, || (hooks.layout)(widget));
}

/// The icon is chosen while its view is being updated, which can happen outside any paint or
/// layout of the window, so the Rivet scope is found from the view's ancestors.
unsafe extern "system" fn update_location_icon(view: *mut c_void) {
    let top = window_top(view);
    let run: WidgetCall = original(&ORIGINAL_UPDATE_LOCATION_ICON);
    scoped(top, || run(view));
}

/// The label is built here, which can also happen outside any paint or layout of the window.
unsafe extern "system" fn update_location(view: *mut c_void, first: usize, second: usize, third: usize) {
    let top = window_top(view);
    let run: unsafe extern "system" fn(*mut c_void, usize, usize, usize) = original(&ORIGINAL_UPDATE_LOCATION);
    scoped(top, || run(view, first, second, third));
}

/// The topmost ancestor of `view` that is a Rivet widget, or the topmost ancestor.
unsafe fn window_top(view: *mut c_void) -> usize {
    let mut top = view as usize;
    for _ in 0..32 {
        let parent = *((top + PARENT_OFFSET) as *const usize);
        if parent == 0 || is_rivet(top) {
            break;
        }
        top = parent;
    }
    top
}

/// Rivet windows show the address without a label before it. The label is built while the icon
/// updates inside the scope of `update_location_icon`, and an empty `std::string` is three zeroed
/// words, so nothing is allocated.
unsafe extern "system" fn chip_text(
    out: *mut [u64; 3],
    delegate: *mut c_void,
    url: *mut c_void,
    flag: u8,
) -> *mut [u64; 3] {
    if active() {
        out.write([0; 3]);
        return out;
    }
    original::<unsafe extern "system" fn(*mut [u64; 3], *mut c_void, *mut c_void, u8) -> *mut [u64; 3]>(
        &ORIGINAL_CHIP_TEXT,
    )(out, delegate, url, flag)
}

/// Whether `view` is one of the hidden buttons of a Rivet window.
unsafe fn hidden_here(hooks: &Hooks, view: *mut c_void) -> bool {
    let table = *(view as *const usize);
    if !hooks.hidden.iter().any(|(vtable, _)| *vtable == table) {
        return false;
    }
    if active() {
        return true;
    }
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(GET_WIDGET_SLOT));
    is_rivet(get_widget(view) as usize)
}

unsafe extern "system" fn set_visible(view: *mut c_void, visible: bool) {
    let run = original::<SetVisible>(&ORIGINAL_SET_VISIBLE);
    let hide = visible && hooks().is_some_and(|hooks| hidden_here(hooks, view));
    run(view, visible && !hide);
}

unsafe extern "system" fn hidden_theme_changed(view: *mut c_void) {
    let hooks = hooks().expect("patched after it is stored");
    let table = *(view as *const usize);
    if let Some((_, theme)) = hooks.hidden.iter().find(|(vtable, _)| *vtable == table) {
        theme(view);
    }
    if hidden_here(hooks, view) {
        original::<SetVisible>(&ORIGINAL_SET_VISIBLE)(view, false);
    }
}

fn drawn_ancestor(
    mut view: usize,
    parent: impl Fn(usize) -> usize,
    drawn: impl Fn(usize) -> bool,
) -> Option<usize> {
    while view != 0 {
        if drawn(view) {
            return Some(view);
        }
        view = parent(view);
    }
    None
}

/// Preserve Chrome's bubble state and change only the returned screen rectangle.
unsafe extern "system" fn anchor_rect(
    delegate: *mut u8,
    result: *mut [i32; 4],
) -> *mut [i32; 4] {
    type RectCall = unsafe extern "system" fn(*mut u8, *mut [i32; 4]) -> *mut [i32; 4];
    type Drawn = unsafe extern "system" fn(*mut c_void) -> bool;
    let run: RectCall = original(&ORIGINAL_ANCHOR_RECT);
    let ret = run(delegate, result);
    let observer = *(delegate.add(0x300) as *const *mut u8);
    if observer.is_null() {
        return ret;
    }
    let view = *(observer.add(0x18) as *const *mut c_void);
    if view.is_null() {
        return ret;
    }
    let table = *(view as *const *const usize);
    let get_widget: Pointer = std::mem::transmute(*table.add(GET_WIDGET_SLOT));
    if !is_rivet(get_widget(view) as usize) {
        return ret;
    }
    let drawn = |v: usize| {
        let table = *(v as *const *const usize);
        let call: Drawn = std::mem::transmute(*table.add(IS_DRAWN_SLOT));
        call(v as *mut c_void)
    };
    if drawn(view as usize) {
        return ret;
    }
    let parent = |v: usize| *((v + PARENT_OFFSET) as *const usize);
    if let Some(ancestor) = drawn_ancestor(parent(view as usize), parent, drawn) {
        if let Some(hooks) = hooks() {
            let bounds: RectCall = std::mem::transmute(hooks.base + BOUNDS_IN_SCREEN);
            bounds(ancestor as *mut u8, result);
        }
    }
    ret
}

/// Chooses the icon a button image is built from.
unsafe extern "system" fn choose_icon(icon: usize) -> usize {
    if !active() {
        return icon;
    }
    let Some(hooks) = hooks() else { return icon };
    match ICONS.get() {
        Some(icons) => icons
            .iter()
            .find(|(rva, _)| hooks.base + rva == icon)
            .map_or(icon, |(_, ours)| {
                ours.as_ref() as *const VectorIcon as usize
            }),
        None => icon,
    }
}

unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(
        target.cast(),
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect browser code".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    FlushInstructionCache(GetCurrentProcess(), target.cast(), bytes.len());
    Ok(())
}

unsafe fn executable(code: &[u8]) -> Result<usize, String> {
    let memory = VirtualAlloc(
        std::ptr::null(),
        code.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(code.as_ptr(), memory, code.len());
    Ok(memory as usize)
}

/// `mov rax, imm64`.
fn load_rax(code: &mut Vec<u8>, value: usize) {
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(value as u64).to_le_bytes());
}

/// `jmp [rip + 0]` to `to`, which leaves every register as it is.
fn jump(code: &mut Vec<u8>, to: usize) {
    code.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    code.extend_from_slice(&(to as u64).to_le_bytes());
}

/// Replaces the start of a function with a jump to `hook` and returns a trampoline that runs the
/// original. `prologue` is checked first and is copied whole, unless the function reloads its
/// stack cookie relative to itself right after `COOKIE_COPIED` bytes: then the trampoline loads
/// the same cookie from its absolute address.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    cookie: bool,
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    let code = (base + rva) as *mut u8;
    if std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    let mut trampoline = Vec::new();
    if cookie {
        trampoline.extend_from_slice(&prologue[..COOKIE_COPIED]);
        let displacement = i32::from_le_bytes(prologue[12..16].try_into().unwrap());
        let address = (base + rva + prologue.len()).wrapping_add_signed(displacement as isize);
        // mov rax, [address]
        trampoline.extend_from_slice(&[0x48, 0xA1]);
        trampoline.extend_from_slice(&(address as u64).to_le_bytes());
    } else {
        trampoline.extend_from_slice(prologue);
    }
    jump(&mut trampoline, base + rva + prologue.len());
    store.store(executable(&trampoline)?, Ordering::Release);
    let mut patch = Vec::new();
    load_rax(&mut patch, hook);
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(prologue.len(), 0xCC);
    write_code(code, &patch)
}

/// A stub that replaces the second argument with `choose(second)` and continues in `original`,
/// so every other argument, including those on the stack, reaches it untouched.
unsafe fn second_argument_stub(choose: usize, original: usize) -> Result<usize, String> {
    let mut code = vec![
        0x51, // push rcx
        0x52, // push rdx
        0x41, 0x50, // push r8
        0x41, 0x51, // push r9
        0x48, 0x83, 0xEC, 0x28, // sub rsp, 0x28
        0x48, 0x89, 0xD1, // mov rcx, rdx
    ];
    load_rax(&mut code, choose);
    code.extend_from_slice(&[
        0xFF, 0xD0, // call rax
        0x48, 0x83, 0xC4, 0x28, // add rsp, 0x28
        0x41, 0x59, // pop r9
        0x41, 0x58, // pop r8
        0x5A, // pop rdx
        0x59, // pop rcx
        0x48, 0x89, 0xC2, // mov rdx, rax
    ]);
    jump(&mut code, original);
    executable(&code)
}

unsafe fn replace_slot(slot: *mut usize, value: usize) -> Result<(), String> {
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect a vtable".into());
    }
    slot.write(value);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

unsafe fn patch() -> Result<Hooks, String> {
    let base = GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize;
    if base == 0 {
        return Err("this browser process has no chrome.dll".into());
    }
    let widget = (base + BROWSER_WIDGET_VTABLE) as *mut usize;
    if widget.add(PAINT_SLOT).read() != base + PAINT
        || widget.add(LAYOUT_SLOT).read() != base + LAYOUT
    {
        return Err("this browser build has no recorded browser widget".into());
    }
    for (vtable, theme) in HIDDEN {
        if ((base + vtable) as *const usize).add(THEME_SLOT).read() != base + theme {
            return Err(format!(
                "this browser build has no recorded view at {vtable:#x}"
            ));
        }
    }
    let prologue = |rva: usize, bytes: &[u8]| {
        std::slice::from_raw_parts((base + rva) as *const u8, bytes.len()) == bytes
    };
    if !prologue(LAYOUT_CONSTANT, &LAYOUT_CONSTANT_PROLOGUE)
        || !prologue(THEME_CHANGED, &THEME_CHANGED_PROLOGUE)
        || !prologue(PAINT_LAYER, &PAINT_LAYER_PROLOGUE)
        || !prologue(SET_VISIBLE, &SET_VISIBLE_PROLOGUE)
        || !prologue(FROM_VECTOR_ICON, &FROM_VECTOR_ICON_PROLOGUE)
        || !prologue(UPDATE_LOCATION, &UPDATE_LOCATION_PROLOGUE)
        || !prologue(UPDATE_LOCATION_ICON, &UPDATE_LOCATION_ICON_PROLOGUE)
        || !prologue(CHIP_TEXT, &CHIP_TEXT_PROLOGUE)
        || !prologue(ANCHOR_RECT, &ANCHOR_RECT_PROLOGUE)
        || !prologue(ANCHOR_RECT + 0x2a, &ANCHOR_VIEW_ACCESS)
        || !prologue(IS_DRAWN, &IS_DRAWN_BYTES)
        || !prologue(BOUNDS_IN_SCREEN, &BOUNDS_BYTES)
    {
        return Err("this browser build has other toolbar functions".into());
    }
    let icon = &*((base + PAGE_INFO_ICON) as *const VectorIcon);
    if icon.count != 1 || icon.reps as usize != base + 0x10F5_D438
        || icon.name as usize != base + 0x10F5_D448
        || (*icon.reps).path as usize != base + 0x10F6_6E40
        || (*icon.reps).size != 282
        || !prologue(0x10F6_6E40, &PAGE_INFO_BYTES)
    {
        return Err("this browser build has another page info icon".into());
    }
    Ok(Hooks {
        base,
        paint: std::mem::transmute::<usize, WidgetPaint>(base + PAINT),
        layout: std::mem::transmute::<usize, WidgetCall>(base + LAYOUT),
        provider: std::mem::transmute::<usize, Provider>(base + COLOR_PROVIDER),
        hidden: HIDDEN
            .iter()
            .map(|(vtable, theme)| {
                (
                    base + vtable,
                    std::mem::transmute::<usize, WidgetCall>(base + theme),
                )
            })
            .collect(),
    })
}

/// Patches the browser once every check has passed and `HOOKS` is stored.
unsafe fn wire(base: usize, skip: &str) -> Result<(), String> {
    build_icons(base);
    crate::vertical_tabs::trace(&format!("toolbar skip: {skip:?}"));
    if !skip.contains("anchor") {
        detour(
            base,
            ANCHOR_RECT,
            &ANCHOR_RECT_PROLOGUE,
            false,
            anchor_rect as *const () as usize,
            &ORIGINAL_ANCHOR_RECT,
        )?;
    }
    if !skip.contains("constant") {
        detour(
            base,
            LAYOUT_CONSTANT,
            &LAYOUT_CONSTANT_PROLOGUE,
            true,
            layout_constant as usize,
            &ORIGINAL_LAYOUT_CONSTANT,
        )?;
    }
    if !skip.contains("theme") {
        detour(
            base,
            THEME_CHANGED,
            &THEME_CHANGED_PROLOGUE,
            true,
            theme_changed as usize,
            &ORIGINAL_THEME_CHANGED,
        )?;
    }
    if !skip.contains("layer") {
        detour(base, PAINT_LAYER, &PAINT_LAYER_PROLOGUE, true, paint_layer as usize, &ORIGINAL_PAINT_LAYER)?;
    }
    if !skip.contains("visible") {
        detour(
            base,
            SET_VISIBLE,
            &SET_VISIBLE_PROLOGUE,
            false,
            set_visible as usize,
            &ORIGINAL_SET_VISIBLE,
        )?;
    }
    if skip.contains("icon") {
        return slots(base, skip);
    }
    detour(
        base,
        UPDATE_LOCATION_ICON,
        &UPDATE_LOCATION_ICON_PROLOGUE,
        false,
        update_location_icon as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION_ICON,
    )?;
    detour(
        base,
        UPDATE_LOCATION,
        &UPDATE_LOCATION_PROLOGUE,
        false,
        update_location as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION,
    )?;
    detour(
        base,
        CHIP_TEXT,
        &CHIP_TEXT_PROLOGUE,
        false,
        chip_text as *const () as usize,
        &ORIGINAL_CHIP_TEXT,
    )?;
    let mut icon_trampoline = FROM_VECTOR_ICON_PROLOGUE.to_vec();
    jump(
        &mut icon_trampoline,
        base + FROM_VECTOR_ICON + FROM_VECTOR_ICON_PROLOGUE.len(),
    );
    let stub = second_argument_stub(choose_icon as usize, executable(&icon_trampoline)?)?;
    let mut icon_patch = Vec::new();
    load_rax(&mut icon_patch, stub);
    icon_patch.extend_from_slice(&[0xFF, 0xE0]);
    icon_patch.resize(FROM_VECTOR_ICON_PROLOGUE.len(), 0xCC);
    write_code((base + FROM_VECTOR_ICON) as *mut u8, &icon_patch)?;
    slots(base, skip)
}

unsafe fn slots(base: usize, skip: &str) -> Result<(), String> {
    if skip.contains("widget") {
        return Ok(());
    }
    let widget = (base + BROWSER_WIDGET_VTABLE) as *mut usize;
    replace_slot(widget.add(PAINT_SLOT), widget_paint as usize)?;
    replace_slot(widget.add(LAYOUT_SLOT), widget_layout as usize)?;
    for (vtable, _) in HIDDEN {
        replace_slot(
            ((base + vtable) as *mut usize).add(THEME_SLOT),
            hidden_theme_changed as usize,
        )?;
    }
    Ok(())
}

/// Installs the changes once per process. They stay inert until a window is adopted.
pub unsafe fn install(directory: Option<std::path::PathBuf>) -> Result<(), String> {
    let skip = directory
        .and_then(|dir| std::fs::read_to_string(dir.join("rivet-toolbar-skip.txt")).ok())
        .unwrap_or_default();
    static WIRED: OnceLock<Result<(), String>> = OnceLock::new();
    WIRED
        .get_or_init(|| {
            let hooks = HOOKS
                .get_or_init(|| patch())
                .as_ref()
                .map_err(Clone::clone)?;
            wire(hooks.base, &skip)
        })
        .clone()
}

/// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND` and
/// `views::Widget::GetWidgetForNativeWindow`.
const CONTENT_WINDOW_FOR_HWND: usize = 0x00D5_64A0;
const WIDGET_FOR_WINDOW: usize = 0x0315_C7C0;

/// Makes `window` a Rivet window and rebuilds its views the way a theme change does, so it
/// picks up the Rivet look.
pub unsafe fn adopt(window: HWND) {
    if hooks().is_none() {
        return;
    }
    let widget = widget_of(window);
    if widget == 0 {
        return;
    }
    {
        let mut widgets = WIDGETS.lock().unwrap_or_else(|error| error.into_inner());
        if !widgets.contains(&widget) {
            widgets.push(widget);
        }
    }
    crate::vertical_tabs::trace("adopt");
    theme_changed(widget as *mut c_void);
    crate::vertical_tabs::trace("adopted");
}

/// Forgets a Rivet window that is going away.
pub unsafe fn forget(window: HWND) {
    let widget = widget_of(window);
    WIDGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|known| *known != widget);
}

/// One `gfx::PathElement`: a command or a number, both four bytes.
#[derive(Clone, Copy)]
#[repr(C)]
union PathElement {
    command: u32,
    value: f32,
}

const fn op(command: u32) -> PathElement {
    PathElement { command }
}
const fn at(value: f32) -> PathElement {
    PathElement { value }
}

// `gfx::CommandType` values of this build.
const STROKE: u32 = 5;
const MOVE_TO: u32 = 7;
const LINE_TO: u32 = 11;
const CIRCLE: u32 = 24;
const CANVAS_DIMENSIONS: u32 = 28;

/// A 13 DIP information mark on Chrome's 16 DIP icon canvas.
static INFO_PATH: [PathElement; 20] = [
    op(CANVAS_DIMENSIONS), at(16.0),
    op(STROKE), at(1.5),
    op(CIRCLE), at(8.0), at(8.0), at(6.5),
    op(MOVE_TO), at(8.0), at(7.4),
    op(LINE_TO), at(8.0), at(11.0),
    op(MOVE_TO), at(8.0), at(5.0),
    op(LINE_TO), at(8.0), at(5.2),
];

/// The Rivet back arrow: a thin stroked arrow on a 16 DIP canvas.
static BACK_PATH: [PathElement; 19] = [
    op(CANVAS_DIMENSIONS),
    at(16.0),
    op(STROKE),
    at(1.5),
    op(MOVE_TO),
    at(12.8),
    at(8.0),
    op(LINE_TO),
    at(3.6),
    at(8.0),
    op(MOVE_TO),
    at(7.6),
    at(4.0),
    op(LINE_TO),
    at(3.6),
    at(8.0),
    op(LINE_TO),
    at(7.6),
    at(12.0),
];

/// The Rivet forward arrow, the back arrow mirrored.
static FORWARD_PATH: [PathElement; 19] = [
    op(CANVAS_DIMENSIONS),
    at(16.0),
    op(STROKE),
    at(1.5),
    op(MOVE_TO),
    at(3.2),
    at(8.0),
    op(LINE_TO),
    at(12.4),
    at(8.0),
    op(MOVE_TO),
    at(8.4),
    at(4.0),
    op(LINE_TO),
    at(12.4),
    at(8.0),
    op(LINE_TO),
    at(8.4),
    at(12.0),
];

/// The Rivet reload arrow: three quarters of a circle drawn as short strokes, with a head.
fn reload_path() -> Vec<PathElement> {
    // Keep this as the same move/line grammar used by the verified back and forward paths.
    // The previous generated arc produced a malformed vector path in Chrome 156.
    [
        op(CANVAS_DIMENSIONS),
        at(16.0),
        op(STROKE),
        at(1.5),
        op(MOVE_TO),
        at(12.8),
        at(3.0),
        op(LINE_TO),
        at(12.8),
        at(6.4),
        op(LINE_TO),
        at(9.4),
        at(6.4),
        op(MOVE_TO),
        at(12.8),
        at(6.4),
        op(LINE_TO),
        at(11.5),
        at(4.2),
        op(LINE_TO),
        at(9.5),
        at(3.0),
        op(LINE_TO),
        at(7.2),
        at(2.8),
        op(LINE_TO),
        at(4.9),
        at(3.6),
        op(LINE_TO),
        at(3.3),
        at(5.5),
        op(LINE_TO), at(2.8), at(7.8),
        op(LINE_TO), at(3.3), at(10.1),
        op(LINE_TO), at(4.9), at(12.0),
        op(LINE_TO), at(7.2), at(12.9),
        op(LINE_TO), at(9.5), at(12.7),
        op(LINE_TO), at(11.5), at(11.5),
        op(LINE_TO), at(12.6), at(9.5),
    ]
    .to_vec()
}

#[repr(C)]
struct IconRep {
    path: *const PathElement,
    size: usize,
}

/// `gfx::VectorIcon`: its representations, their count and the icon's name.
#[repr(C)]
struct VectorIcon {
    reps: *const IconRep,
    count: usize,
    name: *const u8,
}
unsafe impl Sync for VectorIcon {}
unsafe impl Send for VectorIcon {}

/// The navigation icons of this build: `kBackArrowChromeRefreshOldIcon`,
/// `kForwardArrowChromeRefreshOldIcon` and `kReloadChromeRefreshOldIcon`.
const BACK_ICON: usize = 0x10E0_2EA8;
const FORWARD_ICON: usize = 0x10E0_55C0;
const RELOAD_ICON: usize = 0x10E0_7DD0;
const PAGE_INFO_ICON: usize = 0x10F5_D470;
const PAGE_INFO_BYTES: [u8; 32] = [
    0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
    0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
];

static ICONS: OnceLock<Vec<(usize, Box<VectorIcon>)>> = OnceLock::new();

unsafe fn build_icons(base: usize) {
    ICONS.get_or_init(|| {
        let reload: &'static [PathElement] = Box::leak(reload_path().into_boxed_slice());
        [
            (BACK_ICON, &BACK_PATH[..]),
            (FORWARD_ICON, &FORWARD_PATH[..]),
            (RELOAD_ICON, reload),
            (PAGE_INFO_ICON, &INFO_PATH[..]),
        ]
        .into_iter()
        .map(|(rva, path)| {
            let rep: &'static IconRep = Box::leak(Box::new(IconRep {
                path: path.as_ptr(),
                size: path.len(),
            }));
            let original = (base + rva) as *const VectorIcon;
            let icon = Box::new(VectorIcon {
                reps: rep,
                count: 1,
                name: (*original).name,
            });
            (rva, icon)
        })
        .collect()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_page_info_uses_a_thin_circle_without_replacing_warnings() {
        assert_eq!(PAGE_INFO_ICON, 0x10F5_D470);
        assert_ne!(PAGE_INFO_ICON, 0x10F5_CE60);
        let commands: Vec<u32> = [0, 2, 4, 8, 11, 14]
            .iter().map(|&index| unsafe { INFO_PATH[index].command }).collect();
        assert_eq!(commands, [CANVAS_DIMENSIONS, STROKE, CIRCLE, MOVE_TO, LINE_TO, MOVE_TO]);
        assert_eq!(unsafe { INFO_PATH[3].value }, 1.5);
    }

    #[test]
    fn fr12_hidden_bubble_anchor_uses_nearest_drawn_ancestor() {
        let parent = |view| match view { 1 => 2, 2 => 3, _ => 0 };
        let drawn = |view| view == 3;
        assert_eq!(drawn_ancestor(1, parent, drawn), Some(3));
        assert_eq!(drawn_ancestor(3, parent, drawn), Some(3));
        assert_eq!(drawn_ancestor(1, parent, |_| false), None);
    }

    #[test]
    fn fr12_toolbar_colours_use_distinct_stock_keys() {
        let colour = |id| COLOURS.iter().find(|(key, _)| *key == id).map(|(_, value)| *value);
        assert_eq!(colour(1055), Some(0xFF4A_4850));
        assert_eq!(colour(1056), Some(0xFF8A_8690));
        assert_eq!(colour(1590), Some(PAPER));
        assert_eq!(colour(1595), Some(0xFF4A_4850));
        assert_eq!(colour(796), Some(PAPER));
        assert_eq!(colour(1013), Some(PAPER));
        assert_eq!(colour(1603), Some(HAIRLINE));
        assert_eq!(colour(1647), Some(0xFFC4_2B1C));
        assert_eq!(colour(1645), Some(0xFF1A_1A1A));
        for (index, (id, _)) in COLOURS.iter().enumerate() {
            assert!(!COLOURS[..index].iter().any(|(other, _)| other == id));
        }
    }

    #[test]
    fn fr12_reload_icon_stays_on_its_canvas() {
        let path = reload_path();
        let values: Vec<f32> = path[4..]
            .iter()
            .map(|element| unsafe { element.value })
            .filter(|value| value.is_finite() && *value < 100.0)
            .collect();
        assert!(values.iter().all(|value| (0.0..=16.0).contains(value)));
    }

    #[test]
    fn fr12_label_hooks_copy_whole_instructions() {
        // `sub rsp, imm32` is seven bytes after the eight bytes of pushes.
        assert_eq!(CHIP_TEXT_PROLOGUE.len(), 15);
        assert_eq!(UPDATE_LOCATION_PROLOGUE.len(), 15);
        assert_eq!(&CHIP_TEXT_PROLOGUE[8..11], &[0x48, 0x81, 0xEC]);
        assert_eq!(&UPDATE_LOCATION_PROLOGUE[8..11], &[0x48, 0x81, 0xEC]);
    }

    #[test]
    fn fr12_overrides_only_known_constants() {
        assert!(OVERRIDES
            .iter()
            .all(|(constant, _)| (0..75).contains(constant)));
    }
}

