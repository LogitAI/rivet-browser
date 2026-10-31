//! Samsung Internet's top row is laid out by its own code, so a Rivet window gets the row of the
//! design at the one call every layout ends in: `SetBoundsRect` decides where each view of the row
//! goes. Its colours are swapped while the row and the caption buttons paint, so other windows that
//! share the colour provider keep Samsung's. The sidebar is a widget of its own that lies over the
//! page's right edge; a Rivet window's is hidden as soon as it paints.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

use crate::builds::TopRow;
use crate::chrome_layout::{image, PANEL_GAP};

/// The row of the design, in DIPs: 44 tall with its hairline as the last line, and 28 DIP buttons
/// two apart. The address field keeps 4 DIPs from what follows it.
const ROW: i32 = 44;
const CONTENT: i32 = 28;
const CONTENT_TOP: i32 = 8;
const FIRST_BUTTON: i32 = 6;
const BUTTON_STEP: i32 = 30;
const FIELD_X: i32 = 102;
const FIELD_END: i32 = 4;
const NAVIGATION_BUTTONS: usize = 3;

/// What a view of the row is to the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Caption,
    CaptionButton,
    Toolbar,
    Separator,
    Container,
    Inner,
    Corner,
    Button(usize),
    Field,
    Page,
    Holder,
    Below,
}

/// The rectangle a part gets, from the one the browser chose and the size of its parent. The
/// browser's layout hands its own rectangle in every time, so this always starts from that.
pub fn placed(part: Part, rect: [i32; 4], parent: [i32; 2], browser_row: i32) -> [i32; 4] {
    let [x, y, width, height] = rect;
    match part {
        Part::Caption => [x - PANEL_GAP, PANEL_GAP, width, ROW],
        Part::CaptionButton if width > 0 && height > 0 => [x, 0, width, ROW],
        Part::CaptionButton => rect,
        Part::Toolbar => [x, y, width, ROW],
        Part::Separator => [0, ROW - 1, parent[0], 1],
        Part::Container => [x, CONTENT_TOP, width, CONTENT],
        Part::Inner => [x, 0, width, CONTENT],
        Part::Corner => [x, y, 0, 0],
        Part::Button(index) => [FIRST_BUTTON + index as i32 * BUTTON_STEP, 0, CONTENT, CONTENT],
        Part::Field => [FIELD_X, 0, (parent[0] - FIELD_X - FIELD_END).max(0), CONTENT],
        Part::Page => [0, ROW, parent[0], (parent[1] - ROW).max(0)],
        Part::Holder => [0, 0, parent[0], parent[1]],
        Part::Below => [x, y + ROW - browser_row, width, height],
    }
}

fn row() -> Option<&'static TopRow> {
    crate::builds::get().vertical.top_row.as_ref()
}

unsafe fn parent_of(view: usize, row: &TopRow) -> usize {
    *((view + row.parent_offset) as *const usize)
}

unsafe fn bounds_of(view: usize, row: &TopRow) -> [i32; 4] {
    *((view + row.bounds_offset) as *const [i32; 4])
}

unsafe fn class_of(view: usize, base: usize) -> usize {
    (*(view as *const usize)).wrapping_sub(base)
}

unsafe fn children(view: usize, row: &TopRow) -> Vec<usize> {
    let begin = *((view + row.children_offset) as *const usize);
    let end = *((view + row.children_offset + 8) as *const usize);
    if begin < 0x10000 || end < begin || (end - begin) / 8 > 512 {
        return Vec::new();
    }
    (0..(end - begin) / 8).map(|index| *((begin + index * 8) as *const usize)).collect()
}

fn is_browser_root(view: usize) -> bool {
    crate::hide_strip::registered(view.wrapping_sub(crate::builds::get().layout.browser_window_in_view))
}

/// Whether `view` lies in the browser view of a Rivet window, or next to it in the frame.
unsafe fn in_rivet_window(view: usize, row: &TopRow) -> bool {
    let mut at = view;
    for _ in 0..24 {
        if at < 0x10000 {
            return false;
        }
        if is_browser_root(at) || children(at, row).into_iter().any(is_browser_root) {
            return true;
        }
        at = parent_of(at, row);
    }
    false
}

/// Which part of the row `view` is, going by its class and its parent's.
unsafe fn part_of(view: usize, rect: &[i32; 4], row: &TopRow, base: usize) -> Option<Part> {
    let class = class_of(view, base);
    let parent = parent_of(view, row);
    if parent < 0x10000 {
        return None;
    }
    let above = class_of(parent, base);
    if class == row.caption {
        return Some(Part::Caption);
    }
    if class == row.caption_button && above == row.caption {
        return Some(Part::CaptionButton);
    }
    if class == row.toolbar {
        return Some(Part::Toolbar);
    }
    if class == row.separator && above == row.toolbar {
        return Some(Part::Separator);
    }
    if class == row.container && above == row.toolbar {
        return Some(Part::Container);
    }
    if class == row.corner && above == row.container {
        return Some(Part::Corner);
    }
    if class == row.inner && above == row.container {
        return Some(Part::Inner);
    }
    if above == row.inner {
        if class == row.field {
            return Some(Part::Field);
        }
        // Back and forward share a class. Hidden buttons in between are of other classes.
        if row.buttons.contains(&class) && rect[2] > 0 {
            let shown = children(parent, row)
                .into_iter()
                .filter(|child| row.buttons.contains(&class_of(*child, base)));
            return shown.take(NAVIGATION_BUTTONS).position(|child| child == view).map(Part::Button);
        }
    }
    if class == row.holder && above == row.holder_parent {
        return Some(Part::Holder);
    }
    if above == row.content {
        if class == row.page {
            return Some(Part::Page);
        }
        if rect[1] >= row.browser_row {
            return Some(Part::Below);
        }
    }
    None
}

/// Moves a view of a Rivet window's row where the design has it. Runs inside the browser's layout.
pub unsafe fn place(view: *mut c_void, rect: &mut [i32; 4]) {
    let (Some(row), Some(image)) = (row(), image::recorded()) else { return };
    let view = view as usize;
    let Some(part) = part_of(view, rect, row, image.base) else { return };
    if !in_rivet_window(view, row) {
        return;
    }
    let parent = bounds_of(parent_of(view, row), row);
    *rect = placed(part, *rect, [parent[2], parent[3]], row.browser_row);
}

/// Which colour a colour id holds in the row of a Rivet window, or `None` to keep its own.
pub fn colour(colours: &[(i32, u32)], id: i32) -> Option<u32> {
    colours.iter().find(|(known, _)| *known == id).map(|(_, colour)| *colour)
}

/// The original of each replaced slot: the vtable, the slot and the function.
static HELD: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
static SIDEBAR_WIDGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static HIDE_FUNCTION: AtomicUsize = AtomicUsize::new(0);
const HIDE_TIMER: usize = 0x5249_5647;
/// `OnPaint` and `OnPaintBackground`.
const BARE_PAINT: usize = 89;
const BARE_BACKGROUND: usize = 90;

fn held(table: usize, slot: usize) -> Option<usize> {
    let known = HELD.lock().unwrap_or_else(|error| error.into_inner());
    known.iter().find(|(vtable, at, _)| *vtable == table && *at == slot).map(|(_, _, function)| *function)
}

type Paint = unsafe extern "system" fn(*mut c_void, *const c_void);

/// Paints with the row's colours when the view belongs to a Rivet window.
unsafe fn painted(view: *mut c_void, argument: *const c_void, slot: usize) {
    let table = *(view as *const usize);
    let Some(function) = held(table, slot) else { return };
    let run: Paint = std::mem::transmute(function);
    let (Some(row), Some(image)) = (row(), image::recorded()) else { return run(view, argument) };
    if !in_rivet_window(view as usize, row) {
        return run(view, argument);
    }
    let provider_of: unsafe extern "system" fn(*const c_void) -> *const u8 =
        std::mem::transmute(image.base + crate::builds::get().layout.get_color_provider);
    let provider = provider_of(view);
    if provider.is_null() {
        return run(view, argument);
    }
    crate::chrome_toolbar::with_colour_map(provider, |id, _| colour(row.colours, id), || run(view, argument))
}

unsafe extern "system" fn paint_children(view: *mut c_void, info: *const c_void) {
    let Some(row) = row() else { return };
    painted(view, info, row.paint_slots[0])
}

unsafe extern "system" fn paint(view: *mut c_void, canvas: *const c_void) {
    let Some(row) = row() else { return };
    painted(view, canvas, row.paint_slots[1])
}

/// Leaves a baked paint out in a Rivet window. Children still paint.
unsafe fn bare(view: *mut c_void, canvas: *const c_void, slot: usize) {
    let Some(row) = row() else { return };
    let table = *(view as *const usize);
    let Some(function) = held(table, slot) else { return };
    if !in_rivet_window(view as usize, row) {
        let run: Paint = std::mem::transmute(function);
        run(view, canvas);
        return;
    }
    // The row's line is the design's hairline. Any other view of that class has no room to draw in.
    let Some(image) = image::recorded() else { return };
    if class_of(view as usize, image.base) == row.separator {
        let draw: unsafe extern "system" fn(*const c_void, u32) =
            std::mem::transmute(image.base + crate::builds::get().layout.draw_color);
        draw(canvas, crate::chrome_toolbar::HAIRLINE);
    }
}

unsafe extern "system" fn bare_paint(view: *mut c_void, canvas: *const c_void) {
    bare(view, canvas, BARE_PAINT)
}

unsafe extern "system" fn bare_background(view: *mut c_void, canvas: *const c_void) {
    bare(view, canvas, BARE_BACKGROUND)
}

/// Hides the sidebars their paint asked for. A paint is no place to change a widget's visibility.
unsafe extern "system" fn hide_sidebars(window: HWND, _message: u32, id: usize, _time: u32) {
    windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(window, id);
    let Some(row) = row() else { return };
    let hide: unsafe extern "system" fn(usize) = std::mem::transmute(HIDE_FUNCTION.load(Ordering::Acquire));
    let widgets = std::mem::take(&mut *SIDEBAR_WIDGETS.lock().unwrap_or_else(|error| error.into_inner()));
    for widget in widgets {
        let native = *((widget + row.widget_native) as *const usize);
        if native >= 0x10000 {
            hide(native);
        }
    }
}

/// A Rivet window's sidebar draws nothing and goes away. Any other window's paints as usual.
unsafe extern "system" fn sidebar_paint_children(root: *mut c_void, info: *const c_void) {
    let Some(row) = row() else { return };
    let table = *(root as *const usize);
    let Some(function) = held(table, row.paint_slots[0]) else { return };
    let window = crate::chrome_layout::hwnd_of_view(root).filter(|window| crate::frame::attached_window(*window));
    let Some(window) = window else {
        let run: Paint = std::mem::transmute(function);
        return run(root, info);
    };
    let widget = *((root as usize + row.root_widget) as *const usize);
    if widget >= 0x10000 {
        let mut waiting = SIDEBAR_WIDGETS.lock().unwrap_or_else(|error| error.into_inner());
        if !waiting.contains(&widget) {
            waiting.push(widget);
        }
        windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(window, HIDE_TIMER, 0, Some(hide_sidebars));
    }
}

unsafe fn patch(row: &'static TopRow) -> Result<(), String> {
    let image = image::recorded().ok_or("no recorded browser image")?;
    let base = image.base;
    let layout = &crate::builds::get().layout;
    if !image.has(row.native_hide, row.native_hide_prologue)
        || !image.has(layout.get_color_provider, layout.get_color_provider_prologue)
        || !image.points_to(row.native_vtable + row.native_hide_slot * 8, row.native_hide)
    {
        return Err("this browser build has other top row functions".into());
    }
    if !row.bare.iter().all(|(vtable, slot, function)| {
        matches!(*slot, BARE_PAINT | BARE_BACKGROUND) && image.points_to(vtable + slot * 8, *function)
    }) {
        return Err("this browser build has another address field".into());
    }
    // Every slot is read before any is replaced, so a build with other classes is left untouched.
    for (vtable, functions) in row.painted.iter().chain(std::iter::once(&row.sidebar_root)) {
        for (index, slot) in row.paint_slots.iter().enumerate() {
            if !image.points_to(*vtable + slot * 8, functions[index]) {
                return Err(format!("{vtable:#x} does not hold the recorded paint functions"));
            }
        }
    }
    HIDE_FUNCTION.store(base + row.native_hide, Ordering::Release);
    {
        let mut known = HELD.lock().unwrap_or_else(|error| error.into_inner());
        for (vtable, functions) in row.painted {
            known.push((base + vtable, row.paint_slots[0], base + functions[0]));
            known.push((base + vtable, row.paint_slots[1], base + functions[1]));
        }
        known.push((base + row.sidebar_root.0, row.paint_slots[0], base + row.sidebar_root.1[0]));
        for (vtable, slot, function) in row.bare {
            known.push((base + vtable, *slot, base + function));
        }
    }
    crate::chrome_toolbar::transaction(|| {
        for (vtable, _) in row.painted {
            let table = (base + vtable) as *mut usize;
            crate::chrome_toolbar::replace_slot(table.add(row.paint_slots[0]), paint_children as *const () as usize)?;
            crate::chrome_toolbar::replace_slot(table.add(row.paint_slots[1]), paint as *const () as usize)?;
        }
        for (vtable, slot, _) in row.bare {
            let hook = if *slot == BARE_PAINT { bare_paint as *const () as usize } else { bare_background as *const () as usize };
            crate::chrome_toolbar::replace_slot(((base + vtable) as *mut usize).add(*slot), hook)?;
        }
        let table = (base + row.sidebar_root.0) as *mut usize;
        crate::chrome_toolbar::replace_slot(table.add(row.paint_slots[0]), sidebar_paint_children as *const () as usize)
    })
}

/// Puts the paint hooks in place once for the process. `place` works without them.
pub unsafe fn install() -> Result<(), String> {
    let Some(row) = row() else { return Ok(()) };
    INSTALLED.get_or_init(|| patch(row)).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_samsung_row_is_44_dips_with_a_hairline_as_its_last_line() {
        assert_eq!(placed(Part::Toolbar, [0, 0, 700, 38], [700, 983], 38), [0, 0, 700, 44]);
        assert_eq!(placed(Part::Separator, [0, 34, 700, 4], [700, 44], 38), [0, 43, 700, 1]);
    }

    #[test]
    fn fr12_samsung_caption_buttons_fill_the_row_inside_the_panel_gap() {
        assert_eq!(placed(Part::Caption, [662, 1, 138, 37], [800, 500], 38), [654, 8, 138, 44]);
        assert_eq!(placed(Part::CaptionButton, [46, 0, 46, 37], [138, 44], 38), [46, 0, 46, 44]);
        // A button with no room, like restore on a window that is not maximized, keeps having none.
        assert_eq!(placed(Part::CaptionButton, [0, 0, 0, 0], [138, 44], 38), [0, 0, 0, 0]);
    }

    #[test]
    fn fr12_samsung_buttons_are_28_dips_two_apart_before_the_field() {
        assert_eq!(placed(Part::Button(0), [8, 3, 24, 24], [300, 30], 38), [6, 0, 28, 28]);
        assert_eq!(placed(Part::Button(2), [72, 3, 24, 24], [300, 30], 38), [66, 0, 28, 28]);
        assert_eq!(placed(Part::Field, [104, 0, 100, 30], [300, 28], 38), [102, 0, 194, 28]);
    }

    #[test]
    fn fr12_samsung_page_takes_the_whole_panel_below_the_row() {
        // The browser leaves 4 DIPs left and below, and the sidebar's column on the right.
        assert_eq!(placed(Part::Page, [4, 38, 476, 441], [524, 483], 38), [0, 44, 524, 439]);
        assert_eq!(placed(Part::Below, [0, 38, 524, 30], [524, 483], 38), [0, 44, 524, 30]);
        // The holder of the page leaves no room for Samsung's rounded border.
        assert_eq!(placed(Part::Holder, [2, 2, 520, 435], [524, 439], 38), [0, 0, 524, 439]);
    }

    #[test]
    fn fr12_samsung_colours_keep_other_ids() {
        assert_eq!(colour(&[(1, 2)], 1), Some(2));
        assert_eq!(colour(&[(1, 2)], 3), None);
    }
}
