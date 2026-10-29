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

use crate::address_display;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, KillTimer, SetTimer, WindowFromPoint, GA_ROOT, GA_ROOTOWNER,
};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use crate::chrome_layout::image;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::Toolbar {
    &crate::builds::get().toolbar
}

const LEADING_SPACE: i32 = 32;
const TOOLBAR_TOP: i32 = 5;
const TOOLBAR_BOTTOM: i32 = 4;
const LOCATION_ICON_SHIFT: i32 = 2;
/// The padding, in DIPs: a 26 DIP button around an 18 DIP icon, which Chrome pads to 28.
const TOOLBAR_BUTTON_PADDING: i32 = 4;
/// `gfx::Font::Weight` values.
const WEIGHT_NORMAL: u32 = 400;
const WEIGHT_SEMIBOLD: u32 = 600;
/// The title and the rest of the address text, as `0xAARRGGBB`.
const TITLE_COLOUR: u32 = 0xFF4A_4850;
const ADDRESS_COLOUR: u32 = 0xFF8A_8690;
const FIELD_FILL_MARGIN: i32 = 3;
const FIELD_HOVER_FILL: u32 = 0xFFF0_EFEC;
/// The radius of the field's rectangle, in DIPs.
const FIELD_RADIUS: f32 = 8.0;
const TRANSFORM_BYTES: usize = 256;
/// The size of the address text, in DIPs.
const ADDRESS_FONT_SIZE: i32 = 12;


pub(crate) const PAPER: u32 = 0xFFFB_FAF7;
pub(crate) const HAIRLINE: u32 = 0xFFE9_E9E6;
pub(crate) const INK: u32 = 0xFF1F_2024;
pub(crate) const SOFT_INK: u32 = 0xFF4A_4850;
/// The accent #9a5424 at 9 percent and at 25 percent over paper.
pub(crate) const ACCENT_TINT: u32 = 0xFFF2_EBE4;
pub(crate) const SELECTION: u32 = 0xFFE3_D0C2;
/// Keyboard focus keeps a visible ring on the toolbar's buttons, in ink instead of the browser's
/// blue. The address field has none: its fill is its focus.
pub(crate) const FOCUS_RING: u32 = 0x661F_2024;

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

/// A state the address field is kept in for captures, since the pointer may not move: read once
/// from `rivet-toolbar-force.txt` next to the runtime.
static FORCED: AtomicUsize = AtomicUsize::new(0);
const FORCE_HOVER: usize = 1;
const FORCE_PRESSED: usize = 2;
const FORCE_CYCLE: usize = 3;
const FORCE_FOCUS: usize = 4;

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
static ORIGINAL_REFRESH_BACKGROUND: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_ICON: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_LAYOUT_INSETS: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_FORMATTED_URL: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_CHIP_TEXT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_TEXTFIELD_THEME: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_ANCHOR_RECT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_TEXT_STYLE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_TITLE_WAS_SET: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_UPDATE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_REVERT_ALL: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_TAB_CHANGED: AtomicUsize = AtomicUsize::new(0);
/// The provider caches a colour when it is first asked for, so a colour can be swapped only after
/// that. This remembers which ones have been seen, for the trace.
static COLOURS_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());
/// The widgets of Rivet windows.
static WIDGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// The text each Rivet window's address field was last given at rest.
static DISPLAYS: Mutex<Vec<(usize, address_display::Display)>> = Mutex::new(Vec::new());

thread_local! {
    /// The Rivet widget the UI thread works for, or zero.
    static CURRENT: Cell<usize> = const { Cell::new(0) };
    /// The widget whose provider holds the replaced colours: `CURRENT` itself, or a popup of it.
    static SOURCE: Cell<usize> = const { Cell::new(0) };
    /// Whether the page the location icon was last updated for is a local file.
    static LOCAL_PAGE: Cell<bool> = const { Cell::new(false) };
    /// Provider colours replaced for the current Rivet widget, with their own values.
    static SWAPPED: RefCell<Vec<(*mut u32, u32)>> = const { RefCell::new(Vec::new()) };
}

fn hooks() -> Option<&'static Hooks> {
    HOOKS.get()?.as_ref().ok()
}

pub(crate) fn is_rivet(widget: usize) -> bool {
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

/// Runs `work` with every colour of the provider at `provider` replaced by what `map` returns for
/// its id and value, then puts the provider's own values back. Used where a Rivet surface reads
/// colours outside a view's paint.
pub(crate) unsafe fn with_colour_map<R>(
    provider: *const u8,
    map: impl Fn(i32, u32) -> Option<u32>,
    work: impl FnOnce() -> R,
) -> R {
    fn collect(node: *mut u8, out: &mut Vec<*mut u8>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            collect(*(node as *const *mut u8), out, depth + 1);
            out.push(node);
            collect(*(node.add(8) as *const *mut u8), out, depth + 1);
        }
    }
    let mut nodes = Vec::new();
    collect(*(provider.add(0x18) as *const *mut u8), &mut nodes, 0);
    let mut saved = Vec::new();
    for node in nodes {
        let slot = node.add(0x24) as *mut u32;
        let original = slot.read();
        if let Some(colour) = map(*(node.add(0x20) as *const i32), original) {
            slot.write(colour);
            saved.push((slot, original));
        }
    }
    let result = work();
    for (slot, original) in saved {
        slot.write(original);
    }
    result
}

/// Traces every colour a provider holds as `id=value`, in id order, for a checked build.
pub(crate) unsafe fn dump_colours(provider: *const u8) {
    fn walk(node: *const u8, out: &mut Vec<String>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            walk(*(node as *const *const u8), out, depth + 1);
            out.push(format!("{}={:08x}", *(node.add(0x20) as *const i32), *(node.add(0x24) as *const u32)));
            walk(*(node.add(8) as *const *const u8), out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(*(provider.add(0x18) as *const *const u8), &mut out, 0);
    crate::vertical_tabs::trace(&format!("colours {} {}", out.len(), out.join(" ")));
}

/// The widgets of Rivet windows, for code outside this module that must tell them apart.
pub(crate) fn rivet_widgets() -> Vec<usize> {
    WIDGETS.lock().unwrap_or_else(|error| error.into_inner()).clone()
}

fn first_swap(id: i32) -> bool {
    let mut seen = COLOURS_SEEN.lock().unwrap_or_else(|error| error.into_inner());
    if seen.contains(&id) {
        return false;
    }
    seen.push(id);
    true
}

/// Whether `view` looks like a live browser object: aligned, and its first word points into the
/// browser module, where every views vtable lives.
unsafe fn is_live_view(view: usize) -> bool {
    use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT};
    if view % 8 != 0 || view < 0x10000 || view > 0x7FFF_FFFF_FFFF {
        return false;
    }
    let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
    if VirtualQuery(view as *const c_void, &mut info, std::mem::size_of_val(&info)) == 0
        || info.State != MEM_COMMIT
        || info.Protect & 0xEE == 0
    {
        return false;
    }
    let table = *(view as *const usize);
    image::recorded().is_some_and(|image| table >= image.base && table < image.base + image.size)
}

unsafe fn switch_to(hooks: &Hooks, widget: usize, source: usize) {
    if CURRENT.with(Cell::get) == widget && SOURCE.with(Cell::get) == source {
        return;
    }
    SWAPPED.with(|swapped| {
        for (slot, value) in swapped.borrow_mut().drain(..) {
            slot.write(value);
        }
    });
    CURRENT.with(|current| current.set(widget));
    SOURCE.with(|current| current.set(source));
    if widget == 0 {
        return;
    }
    let root = *((source + bv().root_view_offset) as *const *mut c_void);
    // A widget that is still being built or already torn down can hold a root view that is not a
    // live view, and asking it for its colours would read freed memory.
    if root.is_null() || !is_live_view(root as usize) {
        return;
    }
    let provider = (hooks.provider)(root);
    if provider.is_null() {
        return;
    }
    SWAPPED.with(|swapped| {
        let mut swapped = swapped.borrow_mut();
        for &(id, colour) in bv().colours {
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
    let (outer, outer_source) = (CURRENT.with(Cell::get), SOURCE.with(Cell::get));
    let rivet = if is_rivet(widget) { widget } else { 0 };
    switch_to(hooks, rivet, rivet);
    let result = work();
    switch_to(hooks, outer, outer_source);
    result
}


/// The Rivet window that owns `widget` when it is one of its popups, such as the omnibox's
/// suggestion list, which Chrome shows in a window of its own.
unsafe fn popup_owner(hooks: &Hooks, widget: usize) -> usize {
    if widget == 0 || is_rivet(widget) || WINDOWS.lock().unwrap_or_else(|error| error.into_inner()).is_empty() {
        return 0;
    }
    let to_window: Pointer = std::mem::transmute(hooks.base + bv().hwnd_for_widget);
    let window = to_window(widget as *const c_void) as HWND;
    if window.is_null() {
        return 0;
    }
    let root = GetAncestor(window, GA_ROOTOWNER);
    if root.is_null() || root == window {
        return 0;
    }
    let owner = WINDOWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(_, known)| *known == root as usize)
        .map_or(0, |(owner, _)| *owner);
    if owner != 0 {
        hook_popup_class(hooks, widget);
    }
    owner
}

/// The popups of Rivet windows that have taken the Rivet colours, with their window handles, so
/// that a popup built later at the same address is not mistaken for one of them.
static THEMED_POPUPS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Whether `widget` is a popup of a Rivet window that has not yet taken the Rivet colours.
unsafe fn fresh_popup(hooks: &Hooks, widget: usize) -> bool {
    if popup_owner(hooks, widget) == 0 {
        return false;
    }
    let to_window: Pointer = std::mem::transmute(hooks.base + bv().hwnd_for_widget);
    let window = to_window(widget as *const c_void) as usize;
    let mut themed = THEMED_POPUPS.lock().unwrap_or_else(|error| error.into_inner());
    if themed.contains(&(widget, window)) {
        return false;
    }
    if themed.len() > 64 {
        themed.clear();
    }
    themed.push((widget, window));
    true
}

/// The widget classes whose paint and layout already work in a Rivet scope.
static HOOKED_CLASSES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// A popup of a Rivet window is of a widget class of its own, whose vtable repeats the paint and
/// layout entries of `views::Widget`. Once one of its instances is known to belong to a Rivet
/// window, the class takes the same scope hooks as the browser's own widgets. The hooks do
/// nothing for instances that belong to other windows.
unsafe fn hook_popup_class(hooks: &Hooks, widget: usize) {
    let vtable = *(widget as *const usize);
    let mut done = HOOKED_CLASSES.lock().unwrap_or_else(|error| error.into_inner());
    if done.contains(&vtable) {
        return;
    }
    done.push(vtable);
    let slots = vtable as *mut usize;
    if *slots.add(bv().paint_slot) == hooks.base + bv().paint && *slots.add(bv().layout_slot) == hooks.base + bv().layout {
        let _ = write_raw(slots.add(bv().paint_slot).cast(), &(widget_paint as usize).to_le_bytes());
        let _ = write_raw(slots.add(bv().layout_slot).cast(), &(widget_layout as usize).to_le_bytes());
    }
    // The popup's pages are web pages, which ask the widget for its colour provider through the
    // colour provider source it is, and take their style sheet from what it holds.
    let source_table = *((widget + bv().source_in_widget) as *const usize) as *mut usize;
    let original = *source_table.add(1);
    if original != popup_provider as usize {
        SOURCE_ORIGINALS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((source_table as usize, original));
        let _ = write_raw(source_table.add(1).cast(), &(popup_provider as usize).to_le_bytes());
    }
}

/// The colour provider source tables of popup classes, with their own `GetColorProvider`.
static SOURCE_ORIGINALS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// A popup of a Rivet window hands out its colour provider with the Rivet colours in place until
/// the browser's message loop turns again, which is after the style sheet of the page that asked
/// for it has been written. Nothing else runs in between.
unsafe extern "system" fn popup_provider(source: *mut c_void) -> *const c_void {
    let table = *(source as *const usize);
    let original = SOURCE_ORIGINALS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(known, _)| *known == table)
        .map_or(0, |(_, original)| *original);
    if original == 0 {
        return std::ptr::null();
    }
    let run: unsafe extern "system" fn(*mut c_void) -> *const c_void = std::mem::transmute(original);
    let provider = run(source);
    let widget = (source as usize).wrapping_sub(bv().source_in_widget);
    if let Some(hooks) = hooks() {
        let owner = popup_owner(hooks, widget);
        if owner != 0 && (CURRENT.with(Cell::get), SOURCE.with(Cell::get)) != (owner, widget) {
            switch_to(hooks, owner, widget);
            SetTimer(std::ptr::null_mut(), 0, 1, Some(popup_provider_done));
        }
    }
    provider
}

unsafe extern "system" fn popup_provider_done(_window: HWND, _message: u32, id: usize, _time: u32) {
    KillTimer(std::ptr::null_mut(), id);
    if let Some(hooks) = hooks() {
        switch_to(hooks, 0, 0);
    }
}

/// Like `scoped`, for a widget that is really a widget: a popup of a Rivet window works in the
/// owner's scope, with the colours of the popup's own provider replaced.
unsafe fn scoped_widget<R>(widget: usize, work: impl FnOnce() -> R) -> R {
    let Some(hooks) = hooks() else { return work() };
    let owner = popup_owner(hooks, widget);
    if owner == 0 {
        return scoped(widget, work);
    }
    let (outer, outer_source) = (CURRENT.with(Cell::get), SOURCE.with(Cell::get));
    switch_to(hooks, owner, widget);
    let result = work();
    switch_to(hooks, outer, outer_source);
    result
}

unsafe fn widget_of(window: HWND) -> usize {
    let Some(hooks) = hooks() else { return 0 };
    let content: Pointer = std::mem::transmute(hooks.base + bv().content_window_for_hwnd);
    let widget_for: Pointer = std::mem::transmute(hooks.base + bv().widget_for_window);
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
        if let Some((_, value)) = bv().overrides.iter().find(|(key, _)| *key == constant) {
            return *value;
        }
    }
    original::<LayoutConstant>(&ORIGINAL_LAYOUT_CONSTANT)(constant)
}

unsafe extern "system" fn layout_insets(out: *mut [i32; 4], inset: i32) -> *mut [i32; 4] {
    if active() && inset == bv().toolbar_button_inset {
        out.write([TOOLBAR_BUTTON_PADDING; 4]);
        return out;
    }
    let result = original::<unsafe extern "system" fn(*mut [i32; 4], i32) -> *mut [i32; 4]>(
        &ORIGINAL_LAYOUT_INSETS,
    )(out, inset);
    if active() && inset == bv().toolbar_inset {
        // The insets are top, left, bottom and right. The left one is where the back button
        // starts, and the appbar toggle sits in the space added before it. The row is 43 DIPs
        // tall around the 34 DIP field, with the buttons centred 28 DIPs in it.
        (*out)[0] = TOOLBAR_TOP;
        (*out)[2] = TOOLBAR_BOTTOM;
        (*out)[1] = (*out)[1].saturating_add(LEADING_SPACE);
    }
    if active() && inset == bv().location_icon_inset {
        (*out)[1] = (*out)[1].saturating_add(LOCATION_ICON_SHIFT);
    }
    result
}

unsafe extern "system" fn theme_changed(widget: *mut c_void) {
    let run: WidgetCall = match ORIGINAL_THEME_CHANGED.load(Ordering::Acquire) {
        0 => std::mem::transmute(hooks().map_or(0, |hooks| hooks.base) + bv().theme_changed),
        _ => original(&ORIGINAL_THEME_CHANGED),
    };
    scoped_widget(widget as usize, || run(widget));
}

/// Layer-backed views paint separately from the widget paint callback.
unsafe extern "system" fn paint_layer(view: *mut c_void, context: *const c_void) {
    let table = *(view as *const usize);
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(bv().get_widget_slot));
    let widget = get_widget(view) as usize;
    if is_rivet(widget) && !active() && !LAYER_LOGGED.swap(true, Ordering::Relaxed) {
        crate::vertical_tabs::trace("rivet layer paint outside scope");
    }
    let run: WidgetPaint = original(&ORIGINAL_PAINT_LAYER);
    scoped_widget(widget, || run(view, context));
}

unsafe extern "system" fn widget_paint(widget: *mut c_void, context: *const c_void) {
    let hooks = hooks().expect("patched after it is stored");
    scoped_widget(widget as usize, || (hooks.paint)(widget, context));
}

unsafe extern "system" fn widget_layout(widget: *mut c_void) {
    let hooks = hooks().expect("patched after it is stored");
    if fresh_popup(hooks, widget as usize) {
        // Views take their colours when they are added to a widget, before this layout, so the
        // popup's tree takes them again in the scope of its owner.
        theme_changed(widget);
    }
    scoped_widget(widget as usize, || (hooks.layout)(widget));
}

/// The icon is chosen while its view is being updated, which can happen outside any paint or
/// layout of the window, so the Rivet scope is found from the view's ancestors.
unsafe extern "system" fn update_location_icon(view: *mut c_void) {
    let top = window_top(view);
    let run: WidgetCall = original(&ORIGINAL_UPDATE_LOCATION_ICON);
    scoped(top, || run(view));
}

unsafe extern "system" fn update_icon(view: *mut c_void) {
    let top = widget_holding(view);
    let run: WidgetCall = original(&ORIGINAL_UPDATE_ICON);
    scoped(top, || run(view));
}

/// In Rivet windows the field at rest shows the page title and the address. The text for editing,
/// which has other flags, stays the real address, so what a person edits and submits is always a
/// page address the browser can open.
unsafe extern "system" fn formatted_url(model: *mut c_void, out: *mut [u64; 3], flags: u32) -> *mut [u64; 3] {
    let result = formatted(model, out, flags);
    if address_display::applies(flags, active()) {
        if let Some(display) = rivet_display(model) {
            write_string(out, &display.text);
            let widget = CURRENT.with(Cell::get);
            let mut displays = DISPLAYS.lock().unwrap_or_else(|error| error.into_inner());
            displays.retain(|(known, _)| *known != widget);
            displays.push((widget, display));
        }
    }
    result
}

unsafe fn formatted(model: *mut c_void, out: *mut [u64; 3], flags: u32) -> *mut [u64; 3] {
    let run: unsafe extern "system" fn(*mut c_void, *mut [u64; 3], u32) -> *mut [u64; 3] =
        original(&ORIGINAL_FORMATTED_URL);
    run(model, out, flags)
}

unsafe fn rivet_display(model: *mut c_void) -> Option<address_display::Display> {
    if model_shows_start_page(model) {
        return Some(address_display::compose(&[], &[], true));
    }
    let hooks = hooks()?;
    let delegate = *((model as usize + bv().model_delegate) as *const usize);
    if delegate == 0 || *(delegate as *const usize) != hooks.base + bv().browser_delegate_vtable {
        return None;
    }
    let active_contents: unsafe extern "system" fn(usize) -> usize = std::mem::transmute(
        *((hooks.base + bv().browser_delegate_vtable) as *const usize).add(bv().active_web_contents_slot),
    );
    let contents = active_contents(delegate);
    if contents == 0 || *(contents as *const usize) != hooks.base + bv().web_contents_vtable {
        return None;
    }
    let title_of: unsafe extern "system" fn(usize) -> *const [u64; 3] = std::mem::transmute(
        *((hooks.base + bv().web_contents_vtable) as *const usize).add(bv().web_contents_title_slot),
    );
    let title = read_string(title_of(contents));
    let mut address = [0u64; 3];
    formatted(model, &mut address, address_display::FULL_URL_FLAGS);
    let full = read_string(&address);
    free_string(&mut address);
    Some(address_display::compose(&title, &full, false))
}

/// The text of a `std::u16string`: inline up to ten units, on the heap after that.
unsafe fn read_string(text: *const [u64; 3]) -> Vec<u16> {
    if text.is_null() {
        return Vec::new();
    }
    let words = *text;
    if words[2] >> 63 == 1 {
        std::slice::from_raw_parts(words[0] as *const u16, words[1] as usize).to_vec()
    } else {
        let length = (words[2] >> 56) as usize & 0x7F;
        std::slice::from_raw_parts(text as *const u16, length.min(11)).to_vec()
    }
}

/// Frees the heap block of a string and leaves it empty.
unsafe fn free_string(text: &mut [u64; 3]) {
    if text[2] >> 63 == 1 {
        if let Some(hooks) = hooks() {
            let delete: unsafe extern "system" fn(*mut c_void, usize) =
                std::mem::transmute(hooks.base + bv().operator_delete);
            delete(text[0] as *mut c_void, (text[2] & !(1 << 63)) as usize * 2);
        }
    }
    *text = [0; 3];
}

/// Replaces the text of a `std::u16string` with Chrome's own allocator, in the layout the
/// browser's strings have: a heap block holds `heap_capacity` units and the third word carries
/// the capacity and a flag in its top bit.
unsafe fn write_string(out: *mut [u64; 3], text: &[u16]) {
    let Some(hooks) = hooks() else { return };
    free_string(&mut *out);
    if text.len() <= 10 {
        let mut words = [0u64; 3];
        let inline = words.as_mut_ptr() as *mut u16;
        std::ptr::copy_nonoverlapping(text.as_ptr(), inline, text.len());
        words[2] = (text.len() as u64) << 56;
        *out = words;
        return;
    }
    let capacity = address_display::heap_capacity(text.len());
    let allocate: unsafe extern "system" fn(usize) -> *mut u16 = std::mem::transmute(hooks.base + bv().operator_new);
    let data = allocate(capacity * 2);
    std::ptr::copy_nonoverlapping(text.as_ptr(), data, text.len());
    data.add(text.len()).write(0);
    *out = [data as u64, text.len() as u64, (1 << 63) | capacity as u64];
}

/// Whether `spec` is a local file named `start.html`, ignoring any query or fragment.
fn is_start_page(spec: &[u8]) -> bool {
    let end = spec.iter().position(|&byte| byte == b'?' || byte == b'#').unwrap_or(spec.len());
    let path = &spec[..end];
    path.len() > 5
        && path[..5].eq_ignore_ascii_case(b"file:")
        && path.len() >= 11
        && path[path.len() - 11..].eq_ignore_ascii_case(b"/start.html")
}

unsafe fn model_shows_start_page(model: *mut c_void) -> bool {
    let Some(hooks) = hooks() else { return false };
    let get_url: unsafe extern "system" fn(*mut c_void, *mut u8) -> *mut u8 =
        std::mem::transmute(hooks.base + bv().model_url);
    let mut gurl = [0u64; 24];
    let gurl = gurl.as_mut_ptr() as *mut u8;
    get_url(model, gurl);
    let long = (*gurl.add(0x17)) & 0x80 != 0;
    let (data, size) = if long {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl as *const u8, *gurl.add(0x17) as usize)
    };
    let shows = is_start_page(std::slice::from_raw_parts(data, size));
    let destroy: unsafe extern "system" fn(*mut u8) = std::mem::transmute(hooks.base + bv().gurl_destructor);
    destroy(gurl);
    shows
}

/// The omnibox refreshes its text outside any paint or layout, so the Rivet scope is found from
/// its text field.
unsafe fn in_omnibox_scope<R>(omnibox: *mut c_void, work: impl FnOnce() -> R) -> R {
    let widget = widget_holding((omnibox as usize + bv().omnibox_textfield) as *mut c_void);
    scoped(widget, work)
}

unsafe extern "system" fn omnibox_update(omnibox: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_OMNIBOX_UPDATE);
    in_omnibox_scope(omnibox, || run(omnibox));
}

unsafe extern "system" fn omnibox_revert_all(omnibox: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_OMNIBOX_REVERT_ALL);
    in_omnibox_scope(omnibox, || run(omnibox));
}

unsafe extern "system" fn omnibox_tab_changed(omnibox: *mut c_void, state: *mut c_void) {
    let run: unsafe extern "system" fn(*mut c_void, *mut c_void) = original(&ORIGINAL_OMNIBOX_TAB_CHANGED);
    in_omnibox_scope(omnibox, || run(omnibox, state));
}

unsafe extern "system" fn title_was_set(observer: *mut c_void, entry: *mut c_void, explicit: usize) {
    let run: unsafe extern "system" fn(*mut c_void, *mut c_void, usize) = original(&ORIGINAL_TITLE_WAS_SET);
    run(observer, entry, explicit);
    let Some(hooks) = hooks() else { return };
    let omnibox = observer as usize - bv().observer_offset;
    if is_rivet(widget_holding((omnibox + bv().omnibox_textfield) as *mut c_void)) {
        let update: WidgetCall = std::mem::transmute(hooks.base + bv().omnibox_update);
        update(omnibox as *mut c_void);
    }
}

/// After Chrome has styled the text of a Rivet window's field, the title gets the title colour
/// and weight and the address the dimmer colour, when the text is the one given at rest. Text a
/// person types keeps Chrome's styling.
unsafe extern "system" fn update_text_style(
    omnibox: *mut c_void,
    text: *const [u64; 3],
    is_url: usize,
    scheme: *mut c_void,
) {
    let run: unsafe extern "system" fn(*mut c_void, *const [u64; 3], usize, *mut c_void) =
        original(&ORIGINAL_UPDATE_TEXT_STYLE);
    run(omnibox, text, is_url, scheme);
    let Some(hooks) = hooks() else { return };
    let textfield = omnibox as usize + bv().omnibox_textfield;
    let widget = widget_holding(textfield as *mut c_void);
    if !is_rivet(widget) {
        return;
    }
    let shown = read_string(text);
    let display = DISPLAYS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(known, display)| *known == widget && display.text == shown)
        .map(|(_, display)| display.clone());
    let holder = *((textfield + bv().textfield_render_text) as *const usize);
    if holder == 0 {
        return;
    }
    let render_text = *((holder + 0x10) as *const usize);
    if render_text == 0 {
        return;
    }
    let apply_colour: unsafe extern "system" fn(usize, u32, *const [u32; 2]) =
        std::mem::transmute(hooks.base + bv().apply_color);
    let apply_weight: unsafe extern "system" fn(usize, u32, *const [u32; 2]) =
        std::mem::transmute(hooks.base + bv().apply_weight);
    // The text of an address being edited is the plain address: one weight and Chrome's colours,
    // not what the title of the resting text left behind.
    let Some(display) = display else {
        apply_weight(render_text, WEIGHT_NORMAL, &[0, shown.len() as u32]);
        return;
    };
    let all = [0, display.text.len() as u32];
    apply_colour(render_text, ADDRESS_COLOUR, &all);
    apply_weight(render_text, WEIGHT_NORMAL, &all);
    if display.title_len > 0 {
        let title = [0, display.title_len];
        apply_colour(render_text, TITLE_COLOUR, &title);
        apply_weight(render_text, WEIGHT_SEMIBOLD, &title);
    }
}

unsafe extern "system" fn refresh_background(view: *mut c_void) {
    // The function takes the `LocationBarView`, whose `views::View` base starts 0x100 bytes in.
    let top = widget_holding((view as usize + bv().location_bar_view_base) as *mut c_void);
    let run: WidgetCall = original(&ORIGINAL_REFRESH_BACKGROUND);
    scoped(top, || {
        if !active() {
            run(view);
            return;
        }
        if FORCED.load(Ordering::Relaxed) >= FORCE_HOVER {
            ((view as usize + bv().hover_value) as *mut f64).write(1.0);
        }
        // Chrome draws a focus ring around the field. The design has none, so the view is taken
        // out; the call does nothing once it is gone.
        if let Some(hooks) = hooks() {
            let remove: unsafe extern "system" fn(*mut c_void) = std::mem::transmute(hooks.base + bv().focus_ring_remove);
            let properties = (view as usize + bv().location_bar_view_base + 0xE8) as *const usize;
            let before = properties.read();
            remove((view as usize + bv().location_bar_view_base) as *mut c_void);
            if first_swap(-1) {
                crate::vertical_tabs::trace(&format!("focus ring: property map root {before:#x} -> {:#x}", properties.read()));
            }
        }
        // For captures: paint the field as it is while focused, without taking focus.
        let state = {
            let holder = *((view as usize + bv().field_state) as *const usize);
            if holder == 0 { 0 } else { *((holder + 0x28) as *const usize) }
        };
        let forced = FORCED.load(Ordering::Relaxed) == FORCE_FOCUS && state != 0;
        let before = if forced { Some(((state + bv().field_state_focused) as *const i32).read()) } else { None };
        if forced {
            ((state + bv().field_state_focused) as *mut i32).write(1);
        }
        let fill = (|| {
            let provider = provider_of(top)?;
            colour_slot(provider, bv().focused_fill_id)
        })();
        let saved = fill.map(|slot| {
            let value = slot.read();
            slot.write(FIELD_HOVER_FILL);
            value
        });
        run(view);
        if let (Some(slot), Some(value)) = (fill, saved) {
            slot.write(value);
        }
        if let Some(value) = before {
            ((state + bv().field_state_focused) as *mut i32).write(value);
        }
        round_field(view);
        watch_field(top, view as usize);
    });
}

/// The colour provider of the Rivet window `widget`, when its root view is live.
unsafe fn provider_of(widget: usize) -> Option<*const u8> {
    let hooks = hooks()?;
    let root = *((widget + bv().root_view_offset) as *const *mut c_void);
    if root.is_null() || !is_live_view(root as usize) {
        return None;
    }
    let provider = (hooks.provider)(root);
    (!provider.is_null()).then_some(provider)
}

/// Pressing the field, as in the design: it sinks quickly to 0.98 of its size and 0.96 of its
/// brightness, stays down for at least a beat, and springs back slowly with a small overshoot.
const PRESS_SCALE: f32 = 0.98;
const PRESS_BRIGHTNESS: f32 = 0.96;
const PRESS_IN_MS: f64 = 120.0;
const PRESS_OUT_MS: f64 = 360.0;
const PRESS_HOLD_MS: f64 = 90.0;
const PRESS_TICK_MS: u32 = 16;

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function: the progress at time `x` in 0..=1.
fn bezier(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let at = |t: f64, a: f64, b: f64| {
        3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t
    };
    let x = x.clamp(0.0, 1.0);
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if at(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    at((low + high) / 2.0, y1, y2)
}

/// How far down the field is: 0 at rest, 1 fully pressed, a little below 0 while it springs back.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    from: f64,
    down: bool,
    changed: f64,
    pressed: f64,
}

impl Press {
    const fn new() -> Self {
        Self { from: 0.0, down: false, changed: 0.0, pressed: 0.0 }
    }

    /// The depth `now` milliseconds in.
    fn depth(&self, now: f64) -> f64 {
        let elapsed = (now - self.changed).max(0.0);
        if self.down {
            let progress = bezier(elapsed / PRESS_IN_MS, 0.25, 0.7, 0.35, 1.0);
            self.from + (1.0 - self.from) * progress
        } else {
            let progress = bezier(elapsed / PRESS_OUT_MS, 0.3, 1.45, 0.55, 1.0);
            self.from * (1.0 - progress)
        }
    }

    /// Whether the field is back at rest and stays there.
    fn at_rest(&self, now: f64) -> bool {
        !self.down && now - self.changed >= PRESS_OUT_MS
    }

    /// Feeds the pointer state at `now`. A short tap still keeps the field down for a beat.
    fn feed(&mut self, held: bool, now: f64) {
        let held = held || (self.down && now - self.pressed < PRESS_HOLD_MS);
        if held == self.down {
            return;
        }
        self.from = self.depth(now).clamp(0.0, 1.0);
        self.down = held;
        self.changed = now;
        if held {
            self.pressed = now;
        }
    }
}

/// A Rivet window's address field and what is known of its press.
struct Field {
    widget: usize,
    view: usize,
    window: usize,
    press: Press,
    /// The pointer went down inside the field and has not been released.
    latched: bool,
    was_held: bool,
    /// The size and depth last applied, to apply only changes.
    applied: Option<(i32, i32, f32)>,
}

static FIELDS: Mutex<Vec<Field>> = Mutex::new(Vec::new());
/// The native window of each Rivet widget.
static WINDOWS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
static TIMER_STARTED: AtomicBool = AtomicBool::new(false);
static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();

fn now_ms() -> f64 {
    EPOCH.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// Remembers the field of a Rivet window so its press can be shown.
fn watch_field(widget: usize, view: usize) {
    let mut fields = FIELDS.lock().unwrap_or_else(|error| error.into_inner());
    match fields.iter_mut().find(|field| field.widget == widget) {
        Some(field) => field.view = view,
        None => fields.push(Field {
            widget,
            view,
            window: 0,
            press: Press::new(),
            latched: false,
            was_held: false,
            applied: None,
        }),
    }
}

unsafe fn screen_bounds(hooks: &Hooks, view: usize) -> [i32; 4] {
    let bounds: unsafe extern "system" fn(*mut u8, *mut [i32; 4]) -> *mut [i32; 4] =
        std::mem::transmute(hooks.base + bv().bounds_in_screen);
    let mut rect = [0i32; 4];
    bounds(view as *mut u8, &mut rect);
    rect
}

/// Whether the left button is held, and whether the pointer is on the field of `window`.
unsafe fn pointer_on_field(hooks: &Hooks, window: usize, widget: usize, view: usize) -> (bool, bool) {
    let held = GetAsyncKeyState(0x01) < 0;
    let mut cursor = POINT { x: 0, y: 0 };
    if window == 0 || GetCursorPos(&mut cursor) == 0 {
        return (held, false);
    }
    let window = window as HWND;
    if GetAncestor(WindowFromPoint(cursor), GA_ROOT) != window {
        return (held, false);
    }
    ScreenToClient(window, &mut cursor);
    let scale = GetDpiForWindow(window).max(96) as f64 / 96.0;
    let root = *((widget + bv().root_view_offset) as *const usize);
    if root == 0 {
        return (held, false);
    }
    // Both rectangles are in screen DIPs, so their difference is the field's place in the widget.
    let origin = screen_bounds(hooks, root);
    let field = screen_bounds(hooks, view + bv().location_bar_view_base);
    let (x, y) = (cursor.x as f64 / scale, cursor.y as f64 / scale);
    let (left, top) = ((field[0] - origin[0]) as f64, (field[1] - origin[1]) as f64);
    let inside = x >= left && x < left + field[2] as f64 && y >= top && y < top + field[3] as f64;
    (held, inside)
}

/// The size of the field, in DIPs.
unsafe fn field_size(view: usize) -> (i32, i32) {
    let base = view + bv().location_bar_view_base;
    (*((base + bv().view_size) as *const i32), *((base + bv().view_size + 4) as *const i32))
}

/// Sets the scale and brightness the press gives the field, or clears them at rest.
unsafe fn show_press(hooks: &Hooks, view: usize, depth: Option<f64>) {
    let base = view + bv().location_bar_view_base;
    let set_transform: unsafe extern "system" fn(*mut c_void, *const u8) =
        std::mem::transmute(hooks.base + bv().set_transform);
    let mut transform = [0u8; TRANSFORM_BYTES];
    let mut write = |at: usize, value: f32| transform[at..at + 4].copy_from_slice(&value.to_le_bytes());
    let (width, height) = field_size(view);
    let depth = depth.unwrap_or(0.0) as f32;
    let scale = 1.0 - (1.0 - PRESS_SCALE) * depth;
    write(8, scale);
    write(12, scale);
    // The layer scales about its top left corner, so the field is moved to scale about its centre.
    write(0x10, (1.0 - scale) * width as f32 / 2.0);
    write(0x14, (1.0 - scale) * height as f32 / 2.0);
    set_transform(base as *mut c_void, transform.as_ptr());
    let layer = *((base + bv().view_layer) as *const usize);
    if layer != 0 && depth != 0.0 {
        let brightness: unsafe extern "system" fn(*mut c_void, f32) =
            std::mem::transmute(hooks.base + bv().set_brightness);
        // The compositor adds this to every colour channel, so 0 is unchanged and -0.04 is darker.
        brightness(layer as *mut c_void, -(1.0 - PRESS_BRIGHTNESS) * depth.clamp(0.0, 1.0));
    }
}

/// Runs on the UI thread every few milliseconds: reads the pointer and moves each Rivet field
/// toward its pressed or resting look.
unsafe extern "system" fn press_tick(_window: HWND, _message: u32, _id: usize, _time: u32) {
    let Some(hooks) = hooks() else { return };
    let now = now_ms();
    let forced = FORCED.load(Ordering::Relaxed);
    let windows = WINDOWS.lock().unwrap_or_else(|error| error.into_inner()).clone();
    let snapshot: Vec<(usize, usize)> = FIELDS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|field| (field.widget, field.view))
        .collect();
    for (widget, view) in snapshot {
        if !is_rivet(widget) {
            continue;
        }
        let window = windows.iter().find(|(known, _)| *known == widget).map_or(0, |(_, hwnd)| *hwnd);
        let (held, inside) = pointer_on_field(hooks, window, widget, view);
        let (width, height) = field_size(view);
        let mut fields = FIELDS.lock().unwrap_or_else(|error| error.into_inner());
        let Some(field) = fields.iter_mut().find(|field| field.widget == widget) else { continue };
        // A press counts when it starts on the field and lasts while the button stays down on it.
        field.latched = match (held, field.was_held) {
            (false, _) => false,
            (true, false) => inside,
            (true, true) => field.latched && inside,
        };
        field.was_held = held;
        let down = match forced {
            FORCE_PRESSED => true,
            FORCE_CYCLE => (2000.0..2150.0).contains(&now),
            _ => field.latched,
        };
        field.press.feed(down, now);
        let depth = field.press.depth(now);
        let wanted = if field.press.at_rest(now) {
            None
        } else {
            Some((width, height, (depth * 1000.0).round() as f32 / 1000.0))
        };
        if field.applied == wanted {
            continue;
        }
        field.applied = wanted;
        drop(fields);
        if forced == FORCE_CYCLE {
            crate::vertical_tabs::trace(&format!("press t={now:.0} depth={depth:.3}"));
        }
        show_press(hooks, view, wanted.map(|_| depth));
    }
}

/// Starts the press timer on the calling UI thread, once.
unsafe fn start_press_timer() {
    if !TIMER_STARTED.swap(true, Ordering::AcqRel) {
        now_ms();
        SetTimer(std::ptr::null_mut(), 0, PRESS_TICK_MS, Some(press_tick));
    }
}

/// Chrome rounds the field's fill into a pill. Rivet's field is a rectangle with 8 DIP corners,
/// like every other control of the top row.
unsafe fn round_field(view: *mut c_void) {
    let Some(hooks) = hooks() else { return };
    let background = *((view as usize + bv().field_background) as *const usize);
    if background == 0 || *(background as *const usize) != hooks.base + bv().background_vtable {
        return;
    }
    let painter = *((background + 0x10) as *const usize);
    if painter == 0 || *(painter as *const usize) != hooks.base + bv().painter_vtable {
        return;
    }
    for corner in 0..4 {
        ((painter + bv().painter_radii + corner * 4) as *mut f32).write(FIELD_RADIUS);
    }
    // Chrome's field is 34 DIPs tall. The fill is 28, centred, like a button of the row.
    ((painter + bv().painter_insets) as *mut [i32; 4]).write([FIELD_FILL_MARGIN, 0, FIELD_FILL_MARGIN, 0]);
}

/// The label is built here, which can also happen outside any paint or layout of the window.
unsafe extern "system" fn update_location(view: *mut c_void, first: usize, second: usize, third: usize) {
    let top = window_top(view);
    let run: unsafe extern "system" fn(*mut c_void, usize, usize, usize) = original(&ORIGINAL_UPDATE_LOCATION);
    scoped(top, || run(view, first, second, third));
}

unsafe extern "system" fn textfield_theme(view: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_TEXTFIELD_THEME);
    run(view);
    if !active() {
        return;
    }
    let Some(hooks) = hooks() else { return };
    let holder = *((view as usize + bv().textfield_render_text) as *const usize);
    if holder == 0 {
        return;
    }
    let render_text = *((holder + 0x10) as *const usize);
    if render_text == 0 {
        return;
    }
    let font_list = (render_text + bv().render_text_font_list) as *mut c_void;
    if (*(font_list as *const usize)) == 0 {
        return;
    }
    let size: unsafe extern "system" fn(*mut c_void) -> i32 = std::mem::transmute(hooks.base + bv().font_size);
    let current = size(font_list);
    if current == ADDRESS_FONT_SIZE {
        return;
    }
    let derive: unsafe extern "system" fn(*mut c_void, *mut usize, i32) -> *mut usize =
        std::mem::transmute(hooks.base + bv().derive_size);
    let mut smaller: usize = 0;
    derive(font_list, &mut smaller, ADDRESS_FONT_SIZE - current);
    let set: unsafe extern "system" fn(*mut c_void, *const usize) = std::mem::transmute(hooks.base + bv().textfield_set_font);
    set(view, &smaller);
}

/// The topmost ancestor of `view` that is a Rivet widget, or the topmost ancestor.
unsafe fn window_top(view: *mut c_void) -> usize {
    let mut top = view as usize;
    for _ in 0..32 {
        let parent = *((top + bv().parent_offset) as *const usize);
        // A view being built or destroyed can hold a parent that is no object at all.
        let object = parent != 0 && parent % 8 == 0 && parent < 0x0000_8000_0000_0000;
        if !object || is_rivet(top) {
            break;
        }
        top = parent;
    }
    top
}

/// The widget that holds `view`, which must point at a `views::View`.
pub(crate) unsafe fn widget_holding(view: *mut c_void) -> usize {
    let table = *(view as *const usize);
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(bv().get_widget_slot));
    get_widget(view) as usize
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
        // The icon is chosen right after the label, from the same page.
        let local = page_is_local_file(delegate);
        LOCAL_PAGE.with(|page| page.set(local));
        if local {
            out.write([0; 3]);
            return out;
        }
    }
    original::<unsafe extern "system" fn(*mut [u64; 3], *mut c_void, *mut c_void, u8) -> *mut [u64; 3]>(
        &ORIGINAL_CHIP_TEXT,
    )(out, delegate, url, flag)
}

/// Whether `spec[begin..begin + length]` is the `file` scheme.
fn is_file_scheme(spec: &[u8], begin: i32, length: i32) -> bool {
    let (Ok(begin), Ok(length)) = (usize::try_from(begin), usize::try_from(length)) else {
        return false;
    };
    spec.get(begin..begin.saturating_add(length))
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"file"))
}

/// Asks the delegate for the page address, the way `GetSecurityChipText` does, and frees it.
unsafe fn page_is_local_file(delegate: *mut c_void) -> bool {
    let Some(hooks) = hooks() else { return false };
    if delegate.is_null() {
        return false;
    }
    let table = *(delegate as *const *const usize);
    let get_url: unsafe extern "system" fn(*mut c_void, *mut u8) = std::mem::transmute(*table.add(3));
    // A `GURL` is smaller than this, and its destructor runs below.
    let mut gurl = [0u64; 24];
    let gurl = gurl.as_mut_ptr() as *mut u8;
    get_url(delegate, gurl);
    let long = (*gurl.add(0x17)) & 0x80 != 0;
    let (data, size) = if long {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl as *const u8, *gurl.add(0x17) as usize)
    };
    let local = is_file_scheme(
        std::slice::from_raw_parts(data, size),
        *(gurl.add(bv().gurl_scheme_begin) as *const i32),
        *(gurl.add(bv().gurl_scheme_length) as *const i32),
    );
    let destroy: unsafe extern "system" fn(*mut u8) = std::mem::transmute(hooks.base + bv().gurl_destructor);
    destroy(gurl);
    local
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
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(bv().get_widget_slot));
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
    let observer = *(delegate.add(bv().anchor_tracker_offset) as *const *mut u8);
    if observer.is_null() {
        return ret;
    }
    let view = *(observer.add(0x18) as *const *mut c_void);
    if view.is_null() {
        return ret;
    }
    let table = *(view as *const *const usize);
    let get_widget: Pointer = std::mem::transmute(*table.add(bv().get_widget_slot));
    if !is_rivet(get_widget(view) as usize) {
        return ret;
    }
    let drawn = |v: usize| {
        let table = *(v as *const *const usize);
        let call: Drawn = std::mem::transmute(*table.add(bv().is_drawn_slot));
        call(v as *mut c_void)
    };
    if drawn(view as usize) {
        return ret;
    }
    let parent = |v: usize| *((v + bv().parent_offset) as *const usize);
    if let Some(ancestor) = drawn_ancestor(parent(view as usize), parent, drawn) {
        if let Some(hooks) = hooks() {
            let bounds: RectCall = std::mem::transmute(hooks.base + bv().bounds_in_screen);
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
    let Some(icons) = ICONS.get() else { return icon };
    // Chrome's warning for plain HTTP is the icon of local files too. Local files get the
    // information mark, and a page that is not secure gets Rivet's warning.
    let key = icons
        .iter()
        .map(|(rva, _)| *rva)
        .find(|rva| *rva != LOCAL_PAGE_ICON && hooks.base + rva == icon)
        .map(|rva| if rva == bv().http_icon && LOCAL_PAGE.with(Cell::get) { LOCAL_PAGE_ICON } else { rva });
    key.and_then(|key| icons.iter().find(|(rva, _)| *rva == key))
        .map_or(icon, |(_, ours)| ours.as_ref() as *const VectorIcon as usize)
}

/// What an installation overwrote, oldest first: the address and the bytes that were there. A
/// failed installation puts all of it back, so the browser is never left half patched.
static JOURNAL: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(Vec::new());
/// Set once every change of the installation has been made.
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn journal() -> std::sync::MutexGuard<'static, Vec<(usize, Vec<u8>)>> {
    JOURNAL.lock().unwrap_or_else(|error| error.into_inner())
}

/// Overwrites `bytes.len()` bytes at `target`, whatever their protection, without recording.
unsafe fn write_raw(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(
        target.cast(),
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect browser memory".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    FlushInstructionCache(GetCurrentProcess(), target.cast(), bytes.len());
    Ok(())
}

/// Overwrites browser memory and remembers what was there.
unsafe fn write_recorded(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let original = std::slice::from_raw_parts(target, bytes.len()).to_vec();
    write_raw(target, bytes)?;
    journal().push((target as usize, original));
    Ok(())
}

unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    write_recorded(target, bytes)
}

/// Puts back everything the journal holds, newest first. The trampolines already allocated stay
/// where they are: a thread may still be inside one, and they are only a few bytes.
unsafe fn rollback() {
    let entries = std::mem::take(&mut *journal());
    for (address, original) in entries.into_iter().rev() {
        let _ = write_raw(address as *mut u8, &original);
    }
}

/// Runs `install` and undoes every change it made when it fails.
unsafe fn transaction(install: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    journal().clear();
    let result = install();
    match &result {
        Ok(()) => journal().clear(),
        Err(_) => rollback(),
    }
    result
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
/// stack cookie relative to itself: then `prologue` ends with that seven byte load, and the
/// trampoline copies what comes before it and loads the same cookie from its absolute address.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    let code = (base + rva) as *mut u8;
    if std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    // A prologue that ends with the seven byte stack cookie load is relocated, see above.
    let cookie = prologue.len() >= 7 && prologue[prologue.len() - 7..prologue.len() - 4] == [0x48, 0x8B, 0x05];
    let mut trampoline = Vec::new();
    if cookie {
        // The load is the last seven bytes: its opcode and its four byte displacement.
        trampoline.extend_from_slice(&prologue[..prologue.len() - 7]);
        let displacement = i32::from_le_bytes(prologue[prologue.len() - 4..].try_into().unwrap());
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
    write_recorded(slot.cast(), &value.to_le_bytes())
}

unsafe fn patch(skip: &str) -> Result<Hooks, String> {
    let image = image::recorded().ok_or("this browser build has no recorded toolbar")?;
    let base = image.base;
    let slot = |vtable: usize, index: usize| vtable.checked_add(index.checked_mul(8)?);
    let holds = |vtable: usize, index: usize, function: usize| {
        slot(vtable, index).is_some_and(|rva| image.points_to(rva, function))
    };
    if !holds(bv().browser_widget_vtable, bv().paint_slot, bv().paint)
        || !holds(bv().browser_widget_vtable, bv().layout_slot, bv().layout)
        || !holds(bv().plain_widget_vtable, bv().paint_slot, bv().paint)
        || !holds(bv().plain_widget_vtable, bv().layout_slot, bv().layout)
    {
        return Err("this browser build has no recorded browser widget".into());
    }
    let address = !skip.contains("address");
    let icons = !skip.contains("icon");
    if address
        && (!holds(bv().browser_delegate_vtable, bv().active_web_contents_slot, bv().active_web_contents)
            || !holds(bv().web_contents_vtable, bv().web_contents_title_slot, bv().web_contents_title))
    {
        return Err("this browser build has no recorded web contents".into());
    }
    for &(vtable, theme) in bv().hidden {
        if !holds(vtable, bv().theme_slot, theme) {
            return Err(format!(
                "this browser build has no recorded view at {vtable:#x}"
            ));
        }
    }
    let prologue = |rva: usize, bytes: &[u8]| image.has(rva, bytes);
    if !prologue(bv().layout_constant, &bv().layout_constant_prologue)
        || !prologue(bv().theme_changed, &bv().theme_changed_prologue)
        || !prologue(bv().paint_layer, &bv().paint_layer_prologue)
        || !prologue(bv().set_visible, &bv().set_visible_prologue)
        || !prologue(bv().layout_insets, &bv().layout_insets_prologue)
        || !prologue(bv().anchor_rect, &bv().anchor_rect_prologue)
        || !prologue(bv().anchor_rect + bv().anchor_view_access_at, &bv().anchor_view_access)
        || !prologue(bv().is_drawn, &bv().is_drawn_bytes)
        || !prologue(bv().bounds_in_screen, &bv().bounds_bytes)
        || !prologue(bv().hwnd_for_widget, &bv().hwnd_for_widget_head)
        || !prologue(bv().hwnd_for_widget + 9, &bv().hwnd_for_widget_tail)
    {
        return Err("this browser build has other toolbar functions".into());
    }
    if icons
        && (!prologue(bv().from_vector_icon, &bv().from_vector_icon_prologue)
            || !prologue(bv().set_transform, &bv().set_transform_prologue)
            || !prologue(bv().set_brightness, &bv().set_brightness_prologue)
            || (!skip.contains("updateicon") && !prologue(bv().update_icon, &bv().update_icon_prologue)))
    {
        return Err("this browser build has other icon functions".into());
    }
    if address
        && (!prologue(bv().update_location, &bv().update_location_prologue)
            || !prologue(bv().textfield_theme, &bv().textfield_theme_prologue)
            || !prologue(bv().textfield_set_font, &bv().textfield_set_font_bytes)
            || !prologue(bv().font_size, &bv().font_size_bytes)
            || !prologue(bv().derive_size, &bv().derive_size_bytes)
            || !prologue(bv().update_location_icon, &bv().update_location_icon_prologue)
            || !prologue(bv().chip_text, &bv().chip_text_prologue)
            || !prologue(bv().formatted_url, &bv().formatted_url_prologue)
            || !prologue(bv().model_url, &bv().model_url_prologue)
            || !prologue(bv().update_text_style, &bv().update_text_style_prologue)
            || !prologue(bv().apply_color, &bv().apply_color_bytes)
            || !prologue(bv().apply_weight, &bv().apply_weight_bytes)
            || !prologue(bv().title_was_set, &bv().title_was_set_prologue)
            || !prologue(bv().operator_new, &bv().operator_new_bytes)
            || !prologue(bv().operator_delete, &bv().operator_delete_bytes)
            || !prologue(bv().omnibox_update, &bv().omnibox_update_prologue)
            || !prologue(bv().omnibox_revert_all, &bv().omnibox_revert_all_prologue)
            || !prologue(bv().omnibox_tab_changed, &bv().omnibox_tab_changed_prologue)
            || !prologue(bv().refresh_background, &bv().refresh_background_prologue)
            || !prologue(bv().gurl_destructor, &bv().gurl_destructor_bytes))
    {
        return Err("this browser build has other address field functions".into());
    }
    // The icons are read through their addresses, so each must lie inside the image.
    if icons {
    if [bv().back_icon, bv().forward_icon, bv().reload_icon, bv().page_info_icon, bv().google_g_icon, bv().google_g_monochrome_icon]
        .iter()
        .chain(bv().edit_icons.iter())
        .any(|rva| image.at(*rva, std::mem::size_of::<VectorIcon>()).is_none())
        || image.at(bv().page_info_reps, std::mem::size_of::<IconRep>()).is_none()
    {
        return Err("this browser build has another page info icon".into());
    }
    let icon = &*((base + bv().page_info_icon) as *const VectorIcon);
    if icon.count != 1 || icon.reps as usize != base + bv().page_info_reps
        || icon.name as usize != base + bv().page_info_id
        || (*icon.reps).path as usize != base + bv().page_info_path
        || (*icon.reps).size != 282
        || !prologue(bv().page_info_path, &bv().page_info_bytes)
    {
        return Err("this browser build has another page info icon".into());
    }
    let http = &*((base + bv().http_icon) as *const VectorIcon);
    if http.count != 1
        || (*http.reps).size != 263
        || std::slice::from_raw_parts(http.name, bv().http_icon_name.len()) != bv().http_icon_name
    {
        return Err("this browser build has another icon for plain HTTP".into());
    }
    }
    Ok(Hooks {
        base,
        paint: std::mem::transmute::<usize, WidgetPaint>(base + bv().paint),
        layout: std::mem::transmute::<usize, WidgetCall>(base + bv().layout),
        provider: std::mem::transmute::<usize, Provider>(base + bv().color_provider),
        hidden: bv().hidden
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
    let address = !skip.contains("address");
    let icons = !skip.contains("icon");
    if icons {
        build_icons(base);
    }
    crate::vertical_tabs::trace(&format!("toolbar skip: {skip:?}"));
    if !skip.contains("anchor") {
        detour(
            base,
            bv().anchor_rect,
            &bv().anchor_rect_prologue,
            anchor_rect as *const () as usize,
            &ORIGINAL_ANCHOR_RECT,
        )?;
    }
    if !skip.contains("constant") {
        detour(
            base,
            bv().layout_constant,
            &bv().layout_constant_prologue,
            layout_constant as usize,
            &ORIGINAL_LAYOUT_CONSTANT,
        )?;
    }
    if !skip.contains("insets") {
        detour(
            base,
            bv().layout_insets,
            &bv().layout_insets_prologue,
            layout_insets as *const () as usize,
            &ORIGINAL_LAYOUT_INSETS,
        )?;
    }
    if !skip.contains("theme") {
        detour(
            base,
            bv().theme_changed,
            &bv().theme_changed_prologue,
            theme_changed as usize,
            &ORIGINAL_THEME_CHANGED,
        )?;
    }
    if !skip.contains("layer") {
        detour(base, bv().paint_layer, &bv().paint_layer_prologue, paint_layer as usize, &ORIGINAL_PAINT_LAYER)?;
    }
    if !skip.contains("visible") {
        detour(
            base,
            bv().set_visible,
            &bv().set_visible_prologue,
            set_visible as usize,
            &ORIGINAL_SET_VISIBLE,
        )?;
    }
    if !address {
        return icons_and_slots(base, skip);
    }
    detour(
        base,
        bv().update_location_icon,
        &bv().update_location_icon_prologue,
        update_location_icon as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION_ICON,
    )?;
    detour(
        base,
        bv().textfield_theme,
        &bv().textfield_theme_prologue,
        textfield_theme as *const () as usize,
        &ORIGINAL_TEXTFIELD_THEME,
    )?;
    detour(
        base,
        bv().update_location,
        &bv().update_location_prologue,
        update_location as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION,
    )?;
    detour(
        base,
        bv().refresh_background,
        &bv().refresh_background_prologue,
        refresh_background as *const () as usize,
        &ORIGINAL_REFRESH_BACKGROUND,
    )?;
    detour(
        base,
        bv().formatted_url,
        &bv().formatted_url_prologue,
        formatted_url as *const () as usize,
        &ORIGINAL_FORMATTED_URL,
    )?;
    for (rva, prologue, hook, store) in [
        (
            bv().update_text_style,
            &bv().update_text_style_prologue[..],
            update_text_style as *const () as usize,
            &ORIGINAL_UPDATE_TEXT_STYLE,
        ),
        (
            bv().title_was_set,
            &bv().title_was_set_prologue[..],
            title_was_set as *const () as usize,
            &ORIGINAL_TITLE_WAS_SET,
        ),
        (
            bv().omnibox_update,
            &bv().omnibox_update_prologue[..],
            omnibox_update as *const () as usize,
            &ORIGINAL_OMNIBOX_UPDATE,
        ),
        (
            bv().omnibox_revert_all,
            &bv().omnibox_revert_all_prologue[..],
            omnibox_revert_all as *const () as usize,
            &ORIGINAL_OMNIBOX_REVERT_ALL,
        ),
        (
            bv().omnibox_tab_changed,
            &bv().omnibox_tab_changed_prologue[..],
            omnibox_tab_changed as *const () as usize,
            &ORIGINAL_OMNIBOX_TAB_CHANGED,
        ),
    ] {
        detour(base, rva, prologue, hook, store)?;
    }
    detour(
        base,
        bv().chip_text,
        &bv().chip_text_prologue,
        chip_text as *const () as usize,
        &ORIGINAL_CHIP_TEXT,
    )?;
    icons_and_slots(base, skip)
}

unsafe fn icons_and_slots(base: usize, skip: &str) -> Result<(), String> {
    if skip.contains("icon") {
        return slots(base, skip);
    }
    if !skip.contains("updateicon") {
        detour(
        base,
        bv().update_icon,
        &bv().update_icon_prologue,
        update_icon as *const () as usize,
        &ORIGINAL_UPDATE_ICON,
    )?;
    }
    let mut icon_trampoline = bv().from_vector_icon_prologue.to_vec();
    jump(
        &mut icon_trampoline,
        base + bv().from_vector_icon + bv().from_vector_icon_prologue.len(),
    );
    let stub = second_argument_stub(choose_icon as usize, executable(&icon_trampoline)?)?;
    let mut icon_patch = Vec::new();
    load_rax(&mut icon_patch, stub);
    icon_patch.extend_from_slice(&[0xFF, 0xE0]);
    icon_patch.resize(bv().from_vector_icon_prologue.len(), 0xCC);
    write_code((base + bv().from_vector_icon) as *mut u8, &icon_patch)?;
    slots(base, skip)
}

unsafe fn slots(base: usize, skip: &str) -> Result<(), String> {
    if skip.contains("widget") {
        return Ok(());
    }
    for vtable in [bv().browser_widget_vtable, bv().plain_widget_vtable] {
        let widget = (base + vtable) as *mut usize;
        replace_slot(widget.add(bv().paint_slot), widget_paint as usize)?;
        replace_slot(widget.add(bv().layout_slot), widget_layout as usize)?;
    }
    for (vtable, _) in bv().hidden {
        replace_slot(
            ((base + vtable) as *mut usize).add(bv().theme_slot),
            hidden_theme_changed as usize,
        )?;
    }
    Ok(())
}

/// Installs the changes once per process. They stay inert until a window is adopted.
pub unsafe fn install(directory: Option<std::path::PathBuf>) -> Result<(), String> {
    let read = |name: &str| {
        directory
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join(name)).ok())
            .unwrap_or_default()
    };
    let skip = read("rivet-toolbar-skip.txt") + " " + bv().skip;
    if skip.contains("toolbar") {
        // This build keeps the browser's own toolbar, so nothing is patched.
        return Ok(());
    }
    FORCED.store(
        match read("rivet-toolbar-force.txt").trim() {
            "hover" => FORCE_HOVER,
            "pressed" => FORCE_PRESSED,
            "cycle" => FORCE_CYCLE,
            "focus" => FORCE_FOCUS,
            _ => 0,
        },
        Ordering::Relaxed,
    );
    static WIRED: OnceLock<Result<(), String>> = OnceLock::new();
    WIRED
        .get_or_init(|| {
            let hooks = HOOKS
                .get_or_init(|| patch(&skip))
                .as_ref()
                .map_err(Clone::clone)?;
            let result = transaction(|| wire(hooks.base, &skip));
            INSTALLED.store(result.is_ok(), Ordering::Release);
            result
        })
        .clone()
}


/// Makes `window` a Rivet window and rebuilds its views the way a theme change does, so it
/// picks up the Rivet look.
pub unsafe fn adopt(window: HWND) {
    if hooks().is_none() || !INSTALLED.load(Ordering::Acquire) {
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
    {
        let mut windows = WINDOWS.lock().unwrap_or_else(|error| error.into_inner());
        windows.retain(|(known, _)| *known != widget);
        windows.push((widget, window as usize));
    }
    start_press_timer();
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
    FIELDS.lock().unwrap_or_else(|error| error.into_inner()).retain(|field| field.widget != widget);
    WINDOWS.lock().unwrap_or_else(|error| error.into_inner()).retain(|(known, _)| *known != widget);
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

/// The stroke of the Rivet icons, in DIPs. The design's icon font draws one DIP lines.
const ICON_STROKE: f32 = 1.0;

const ROUND_RECT: u32 = 26;
const CLOSE: u32 = 27;
const QUADRATIC_TO: u32 = 20;

/// Chrome builds the page info icon as an 18 DIP image of a 16 DIP canvas. The marks are drawn on
/// an 18 DIP canvas instead, so that one unit is one DIP, centred 9.1 DIPs from the left edge and
/// 7.8 from the top, which is where the Rivet design puts its 13 DIP glyphs.
const MARK_X: f32 = 9.1;
const MARK_Y: f32 = 7.8;
const MARK_CANVAS: f32 = 18.0;

fn mark(path: &mut Vec<PathElement>, x1: f32, y1: f32, x2: f32, y2: f32) {
    path.extend([op(MOVE_TO), at(MARK_X + x1), at(MARK_Y + y1), op(LINE_TO), at(MARK_X + x2), at(MARK_Y + y2)]);
}

fn mark_canvas() -> Vec<PathElement> {
    vec![op(CANVAS_DIMENSIONS), at(MARK_CANVAS), op(STROKE), at(ICON_STROKE)]
}

/// The information mark of Rivet pages and local pages: a 12 DIP circle with an i.
fn info_path() -> Vec<PathElement> {
    let mut path = mark_canvas();
    path.extend([op(CIRCLE), at(MARK_X), at(MARK_Y), at(5.75)]);
    mark(&mut path, 0.0, -2.7, 0.0, -2.65);
    mark(&mut path, 0.0, -0.6, 0.0, 2.7);
    path
}

/// The magnifier of the field while editing: a 10 DIP lens and a short handle.
fn search_path() -> Vec<PathElement> {
    let mut path = mark_canvas();
    path.extend([op(CIRCLE), at(MARK_X - 1.0), at(MARK_Y - 1.0), at(4.9)]);
    mark(&mut path, 2.5, 2.5, 5.7, 5.7);
    path
}

/// The padlock of secure pages: a 9.8 by 6 DIP body with a 6 DIP wide shackle and a keyhole dot.
fn lock_path() -> Vec<PathElement> {
    let mut path = mark_canvas();
    path.extend([op(ROUND_RECT), at(MARK_X - 4.4), at(MARK_Y + 0.95), at(8.8), at(5.0), at(1.3)]);
    let (radius, centre) = (2.5f32, MARK_Y - 3.45);
    let steps = 16;
    for step in 0..=steps {
        let angle = std::f32::consts::PI * (1.0 + step as f32 / steps as f32);
        path.extend([
            op(if step == 0 { MOVE_TO } else { LINE_TO }),
            at(MARK_X + radius * angle.cos()),
            at(centre + radius * angle.sin()),
        ]);
    }
    path.extend([op(LINE_TO), at(MARK_X + radius), at(MARK_Y + 0.95)]);
    path.extend([op(MOVE_TO), at(MARK_X - radius), at(centre), op(LINE_TO), at(MARK_X - radius), at(MARK_Y + 0.95)]);
    mark(&mut path, 0.0, 3.45, 0.0, 3.5);
    path
}

/// The warning of pages that are not secure: a 12 DIP triangle with softened corners and a !.
fn warning_path() -> Vec<PathElement> {
    let mut path = mark_canvas();
    let corners = [(0.0f32, -5.55f32), (5.55, 5.55), (-5.55, 5.55)];
    let soften = 1.1f32;
    let toward = |from: (f32, f32), to: (f32, f32)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        (from.0 + dx / length * soften, from.1 + dy / length * soften)
    };
    for (index, corner) in corners.iter().enumerate() {
        let before = toward(*corner, corners[(index + 2) % 3]);
        let after = toward(*corner, corners[(index + 1) % 3]);
        path.extend([
            op(if index == 0 { MOVE_TO } else { LINE_TO }),
            at(MARK_X + before.0),
            at(MARK_Y + before.1),
            op(QUADRATIC_TO),
            at(MARK_X + corner.0),
            at(MARK_Y + corner.1),
            at(MARK_X + after.0),
            at(MARK_Y + after.1),
        ]);
    }
    path.push(op(CLOSE));
    mark(&mut path, 0.0, -2.0, 0.0, 0.6);
    mark(&mut path, 0.0, 2.7, 0.0, 2.75);
    path
}

/// The navigation icons are drawn on a canvas the size of the image Chrome builds for them (their
/// 28 DIP button less 4 DIPs of padding on each side), so that one canvas unit is one DIP, around
/// the centre of the button. Offsets from that
/// centre are in DIPs.
const NAV_CANVAS: f32 = 20.0;
const NAV_CENTRE: f32 = NAV_CANVAS / 2.0;

fn nav(x: f32, y: f32) -> [PathElement; 2] {
    [at(NAV_CENTRE + x), at(NAV_CENTRE + y)]
}

/// A line along `dy` between `from` and `tip` and the two strokes of a head 7 DIPs either side.
fn arrow(from: f32, tip: f32, head: f32, dy: f32) -> Vec<PathElement> {
    let mut path = vec![op(CANVAS_DIMENSIONS), at(NAV_CANVAS), op(STROKE), at(ICON_STROKE)];
    for (command, x, y) in [
        (MOVE_TO, from, dy),
        (LINE_TO, tip, dy),
        (MOVE_TO, head, dy - 7.0),
        (LINE_TO, tip, dy),
        (LINE_TO, head, dy + 7.0),
    ] {
        path.push(op(command));
        path.extend(nav(x, y));
    }
    path
}

/// The Rivet back arrow: a 15.5 DIP line and a head 7.3 DIPs deep, one DIP above the centre.
fn back_path() -> Vec<PathElement> {
    arrow(7.75, -7.75, -0.45, -1.0)
}

/// The Rivet forward arrow, the back arrow mirrored.
fn forward_path() -> Vec<PathElement> {
    arrow(-7.75, 7.75, 0.45, -1.0)
}

/// The Rivet reload mark: an open circle whose short gap sits just above its right side, and a
/// corner at the gap that points along the circle. It uses the same move and line commands as
/// the arrows, because a generated arc produced a malformed vector path in Chrome 156.
fn reload_path() -> Vec<PathElement> {
    let radius = 7.45f32;
    let point = |degrees: f32| {
        let angle = degrees.to_radians();
        (radius * angle.cos(), -0.5 + radius * angle.sin())
    };
    let mut path = vec![op(CANVAS_DIMENSIONS), at(NAV_CANVAS), op(STROKE), at(ICON_STROKE)];
    // From just above the right side clockwise on screen to 25 degrees above it again.
    let (start, sweep, steps) = (-5.0f32, 340.0f32, 68);
    for step in 0..=steps {
        let (x, y) = point(start + sweep * step as f32 / steps as f32);
        path.push(op(if step == 0 { MOVE_TO } else { LINE_TO }));
        path.extend(nav(x, y));
    }
    let (tip_x, tip_y) = point(start + sweep);
    for (command, x, y) in [(MOVE_TO, tip_x - 4.6, tip_y), (LINE_TO, tip_x, tip_y), (LINE_TO, tip_x, tip_y - 4.3)] {
        path.push(op(command));
        path.extend(nav(x, y));
    }
    path
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

/// The key of the information mark in `ICONS`, which replaces the HTTP icon on local pages.
const LOCAL_PAGE_ICON: usize = 1;

static ICONS: OnceLock<Vec<(usize, Box<VectorIcon>)>> = OnceLock::new();

unsafe fn build_icons(base: usize) {
    ICONS.get_or_init(|| {
        let leak = |path: Vec<PathElement>| -> &'static [PathElement] { Box::leak(path.into_boxed_slice()) };
        [
            (bv().back_icon, leak(back_path())),
            (bv().forward_icon, leak(forward_path())),
            (bv().reload_icon, leak(reload_path())),
            (bv().page_info_icon, leak(lock_path())),
            (bv().http_icon, leak(warning_path())),
            (LOCAL_PAGE_ICON, leak(info_path())),
            (bv().google_g_icon, leak(search_path())),
            (bv().google_g_monochrome_icon, leak(search_path())),
        ]
        .into_iter()
        .chain(bv().edit_icons.iter().map(|&rva| (rva, leak(search_path()))))
        .map(|(rva, path)| {
            let rep: &'static IconRep = Box::leak(Box::new(IconRep {
                path: path.as_ptr(),
                size: path.len(),
            }));
            let named = if rva == LOCAL_PAGE_ICON { bv().http_icon } else { rva };
            let original = (base + named) as *const VectorIcon;
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

    /// A page of plain read-write memory to stand in for browser code and vtables.
    unsafe fn scratch_page() -> *mut u8 {
        use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, MEM_RESERVE};
        let page = VirtualAlloc(
            std::ptr::null(),
            0x1000,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        ) as *mut u8;
        assert!(!page.is_null());
        for index in 0..0x1000 {
            *page.add(index) = (index % 251) as u8;
        }
        page
    }

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn nfr3_failed_installation_restores_every_patched_byte() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let before = std::slice::from_raw_parts(page, 0x1000).to_vec();
            static STORE: AtomicUsize = AtomicUsize::new(0);
            let prologue = before[0x100..0x10c].to_vec();
            let result = transaction(|| {
                detour(page as usize, 0x100, &prologue, 0x1122_3344, &STORE)?;
                replace_slot(page.add(0x200).cast(), 0xdead_beef)?;
                write_code(page.add(0x300), &[0xcc; 7])?;
                assert_ne!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
                Err("the next change cannot be made".to_owned())
            });
            assert!(result.is_err());
            assert_eq!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
            assert!(journal().is_empty());
        }
    }

    #[test]
    fn nfr3_complete_installation_keeps_its_changes() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let result = transaction(|| {
                replace_slot(page.add(0x40).cast(), 0x0102_0304_0506_0708)?;
                write_code(page.add(0x80), &[0x90, 0x90])
            });
            assert!(result.is_ok());
            assert_eq!(*(page.add(0x40) as *const usize), 0x0102_0304_0506_0708);
            assert_eq!(*page.add(0x80), 0x90);
            assert!(journal().is_empty());
        }
    }

    #[test]
    fn fr12_page_info_uses_a_thin_circle_without_replacing_warnings() {
        assert_eq!(bv().page_info_icon, 0x10F5_D470);
        assert_ne!(bv().page_info_icon, 0x10F5_CE60);
        let path = info_path();
        let commands: Vec<u32> = [0, 2, 4, 8, 11, 14].iter().map(|&index| unsafe { path[index].command }).collect();
        assert_eq!(commands, [CANVAS_DIMENSIONS, STROKE, CIRCLE, MOVE_TO, LINE_TO, MOVE_TO]);
        assert_eq!(unsafe { path[3].value }, ICON_STROKE);
    }

    #[test]
    fn fr12_secure_pages_get_a_padlock_and_insecure_pages_a_warning() {
        let lock = lock_path();
        assert_eq!(unsafe { lock[4].command }, ROUND_RECT);
        let warning = warning_path();
        assert!(warning.iter().any(|element| unsafe { element.command } == CLOSE));
        assert!(warning.iter().any(|element| unsafe { element.command } == QUADRATIC_TO));
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
        let colour = |id| bv().colours.iter().find(|(key, _)| *key == id).map(|(_, value)| *value);
        assert_eq!(colour(1055), Some(INK));
        // The focused field keeps the hover fill instead of the list colour, and has no ring colour of its own.
        assert_eq!(FIELD_HOVER_FILL, 0xFFF0_EFEC);
        assert_eq!(colour(797), Some(FIELD_HOVER_FILL));
        assert_eq!(colour(1056), Some(0xFF8A_8690));
        assert_eq!(colour(1590), Some(PAPER));
        assert_eq!(colour(1595), Some(0xFF4A_4850));
        assert_eq!(colour(796), Some(PAPER));
        assert_eq!(colour(1013), Some(PAPER));
        // "Not secure" is the design's red on an 8 percent tint of it.
        assert_eq!(colour(1046), Some(0xFFB3_261E));
        assert_eq!(colour(1047), Some(0x14B3_261E));
        assert_eq!(colour(1603), Some(HAIRLINE));
        assert_eq!(colour(1647), Some(0xFFC4_2B1C));
        assert_eq!(colour(1645), Some(0xFF1A_1A1A));
        // Active, inactive and unthemed glyphs, and the alpha an inactive window applies.
        assert_eq!(colour(1644), Some(0xFF1A_1A1A));
        assert_eq!(colour(1646), Some(0xFF1A_1A1A));
        assert_eq!(colour(1650), Some(0xFF1A_1A1A));
        for (index, (id, _)) in bv().colours.iter().enumerate() {
            assert!(!bv().colours[..index].iter().any(|(other, _)| other == id));
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
        assert!(values.iter().all(|value| (0.0..=NAV_CANVAS).contains(value)));
    }

    #[test]
    fn fr12_label_hooks_copy_whole_instructions() {
        // `sub rsp, imm32` is seven bytes after the eight bytes of pushes.
        assert_eq!(bv().chip_text_prologue.len(), 15);
        assert_eq!(bv().update_location_prologue.len(), 15);
        assert_eq!(&bv().chip_text_prologue[8..11], &[0x48, 0x81, 0xEC]);
        assert_eq!(&bv().update_location_prologue[8..11], &[0x48, 0x81, 0xEC]);
    }

    #[test]
    fn fr12_only_file_pages_lose_the_label() {
        assert!(is_file_scheme(b"file:///C:/start.html", 0, 4));
        assert!(!is_file_scheme(b"http://example.com/", 0, 4));
        assert!(!is_file_scheme(b"https://example.com/", 0, 5));
        assert!(!is_file_scheme(b"filesystem:http://a/", 0, 10));
        assert!(!is_file_scheme(b"file", -1, -1));
    }

    #[test]
    fn fr12_background_refresh_hook_copies_whole_instructions() {
        // Seven pushes, then `sub rsp, 0x60`, and no relative operand within the copy.
        assert_eq!(bv().refresh_background_prologue.len(), 14);
        assert_eq!(&bv().refresh_background_prologue[10..], &[0x48, 0x83, 0xEC, 0x60]);
    }

    #[test]
    fn fr12_navigation_buttons_use_a_fixed_pitch() {
        assert_eq!(bv().layout_insets_prologue.len(), 13);
        assert_eq!(NAV_CANVAS as i32, 20);
        assert!(bv().overrides.contains(&(47, 18)));
        assert!(bv().update_icon_prologue.len() >= 12);
    }

    #[test]
    fn fr12_reload_icon_has_a_gap_and_a_head() {
        // One open arc and the corner that sits in its gap.
        let path = reload_path();
        let moves = path.iter().filter(|element| unsafe { element.command } == MOVE_TO).count();
        assert_eq!(moves, 2);
    }

    #[test]
    fn fr12_only_the_start_page_is_shown_as_rivet_start() {
        assert!(is_start_page(b"file:///D:/rivet/start.html?space=Work"));
        assert!(is_start_page(b"FILE:///C:/Program%20Files/Rivet/start.html"));
        assert!(!is_start_page(b"file:///D:/notes/other.html?x=/start.html"));
        assert!(!is_start_page(b"https://example.com/start.html"));
        assert!(!is_start_page(b"file:///start.htm"));
        assert_eq!(address_display::START_PAGE_ADDRESS.encode_utf16().count(), 13);
    }

    #[test]
    fn fr12_cookie_hooks_end_with_the_cookie_load() {
        for prologue in [&bv().omnibox_update_prologue[..], &bv().omnibox_revert_all_prologue[..]] {
            assert_eq!(&prologue[prologue.len() - 7..prologue.len() - 4], [0x48, 0x8B, 0x05]);
            assert!(prologue.len() >= 12);
        }
        assert!(bv().omnibox_tab_changed_prologue.len() >= 12);
    }

    #[test]
    fn fr12_rivet_toolbar_leaves_thirty_two_dips_before_the_back_button() {
        assert_eq!(LEADING_SPACE, 32);
        assert_ne!(bv().toolbar_inset, bv().toolbar_button_inset);
        assert_eq!(i32::MAX.saturating_add(LEADING_SPACE), i32::MAX);
    }

    #[test]
    fn fr12_address_text_is_twelve_dips() {
        assert_eq!(ADDRESS_FONT_SIZE, 12);
        assert_eq!(bv().textfield_theme_prologue.len(), 12);
    }

    #[test]
    fn fr12_address_field_is_one_eight_dip_rectangle_in_a_43_dip_row() {
        assert_eq!(FIELD_RADIUS, 8.0);
        // Chrome's 34 DIP field, the 28 DIP fill inside it and the row around it.
        assert_eq!(28 + 2 * FIELD_FILL_MARGIN, 34);
        assert_eq!(TOOLBAR_TOP + 34 + TOOLBAR_BOTTOM, 43);
        // The page info chip paints nothing of its own, so no circle shows inside the field.
        assert!(bv().colours.contains(&(1003, 0)));
        assert_eq!(LOCATION_ICON_SHIFT, 2);
    }

    #[test]
    fn fr12_press_sinks_fast_and_springs_back_slowly_with_an_overshoot() {
        let mut press = Press::new();
        assert_eq!(press.depth(0.0), 0.0);
        press.feed(true, 100.0);
        assert!(press.depth(100.0) < 0.01);
        assert!(press.depth(160.0) > 0.7, "quick in");
        assert!((press.depth(100.0 + PRESS_IN_MS) - 1.0).abs() < 1e-3);
        press.feed(false, 400.0);
        let mid = press.depth(400.0 + PRESS_OUT_MS / 2.0);
        assert!(mid < 0.5 && mid > -0.5);
        let lowest = (0..=36)
            .map(|step| press.depth(400.0 + step as f64 * 10.0))
            .fold(f64::MAX, f64::min);
        assert!(lowest < 0.0 && lowest > -0.15, "small overshoot, got {lowest}");
        assert!(press.at_rest(400.0 + PRESS_OUT_MS));
        assert!((1.0 - (1.0 - PRESS_SCALE) * 1.0 - 0.98).abs() < 1e-6);
        assert!((PRESS_BRIGHTNESS - 0.96).abs() < 1e-6);
    }

    #[test]
    fn fr12_press_stays_down_for_a_beat_after_a_short_tap() {
        let mut press = Press::new();
        press.feed(true, 0.0);
        press.feed(false, 30.0);
        assert!(press.down, "still down after 30 ms");
        press.feed(false, PRESS_HOLD_MS + 1.0);
        assert!(!press.down);
    }

    #[test]
    fn fr12_press_timing_functions_match_css() {
        assert!(bezier(0.0, 0.25, 0.7, 0.35, 1.0).abs() < 1e-9);
        assert!((bezier(1.0, 0.25, 0.7, 0.35, 1.0) - 1.0).abs() < 1e-9);
        assert!(bezier(0.5, 0.3, 1.45, 0.55, 1.0) > 1.0);
        // `ease` is `cubic-bezier(.25, .1, .25, 1)`: 0.5 of the time is about 0.8 of the way.
        assert!((bezier(0.5, 0.25, 0.1, 0.25, 1.0) - 0.8024).abs() < 0.01);
    }

    #[test]
    fn fr12_only_local_pages_replace_the_plain_http_icon() {
        assert_ne!(bv().http_icon, bv().page_info_icon);
        assert_eq!(bv().http_icon_name.len(), 25);
        assert!(ICON_STROKE < 1.5);
    }

    #[test]
    fn fr12_overrides_only_known_constants() {
        assert!(bv().overrides
            .iter()
            .all(|(constant, _)| (0..75).contains(constant)));
    }
}

