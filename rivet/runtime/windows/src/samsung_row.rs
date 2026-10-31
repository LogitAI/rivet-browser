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
use crate::chrome_toolbar::{NavMark, PageMark};

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
/// The security chip inside the field: 22 tall, 6 from the field's start, and the text 8 after it.
/// A chip with only its glyph is 25 wide, 6 either side of a 13 DIP glyph.
const CHIP_X: i32 = 6;
const CHIP_Y: i32 = 3;
const CHIP_HEIGHT: i32 = 22;
const CHIP_GLYPH_ONLY: i32 = 25;
const CHIP_GLYPH_CENTRE: [f32; 2] = [12.5, 11.0];
const TEXT_GAP: i32 = 8;
const TEXT_Y: i32 = 2;
const TEXT_HEIGHT: i32 = 24;
/// The width the browser gives the location icon when it shows no text.
const BROWSER_ICON_WIDTH: i32 = 24;
const LOCK_INK: u32 = 0xFF4A_4850;
const WARNING_INK: u32 = 0xFFB3_261E;
/// The field's text: the design's muted address at rest, ink while a person edits it.
const ADDRESS_INK: u32 = 0xFF8A_8690;
const EDITING_INK: u32 = 0xFF1F_2024;
const ADDRESS_FONT: &[u8] = b"Segoe UI";
const ADDRESS_FONT_SIZE: i32 = 12;
const FONT_TIMER: usize = 0x5249_5648;
/// A range that reaches past any text. `RenderText` keeps it inside its own length.
const WHOLE_TEXT: [u32; 2] = [0, 0x7FFF_FFFF];

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
    ButtonImage,
    Chip,
    Text { chip_end: i32 },
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
        Part::ButtonImage => [x, y, 0, 0],
        Part::Chip if width <= BROWSER_ICON_WIDTH => [CHIP_X, CHIP_Y, CHIP_GLYPH_ONLY, CHIP_HEIGHT],
        Part::Chip => [CHIP_X, CHIP_Y, width, CHIP_HEIGHT],
        Part::Text { chip_end } => {
            let start = chip_end + TEXT_GAP;
            [start, TEXT_Y, (x + width - start).max(0), TEXT_HEIGHT]
        }
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
    if class == row.button_image && (row.buttons.contains(&above) || above == row.location_icon) {
        return Some(Part::ButtonImage);
    }
    if above == row.location_bar {
        if class == row.location_icon && rect[2] > 0 {
            return Some(Part::Chip);
        }
        if class == row.omnibox {
            let chip = children(parent, row).into_iter().find(|child| class_of(*child, base) == row.location_icon)?;
            let bounds = bounds_of(chip, row);
            return Some(Part::Text { chip_end: bounds[0] + bounds[2] });
        }
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
    // The chip lays its image out once and not again, so a later layout does not take it away.
    if part == Part::Chip {
        for child in children(view, row) {
            if class_of(child, image.base) == row.button_image {
                let bounds = (child + row.bounds_offset) as *mut [i32; 4];
                (*bounds)[2] = 0;
                (*bounds)[3] = 0;
            }
        }
    }
}

/// Which mark a page gets, as in the design: the address decides, and a browser that shows a
/// warning text next to the mark overrules a padlock.
pub fn page_mark(address: &str, warns: bool) -> PageMark {
    if warns {
        PageMark::Warning
    } else if address.starts_with("https://") {
        PageMark::Lock
    } else if crate::dev_mode::starts_in_dev_mode(address) || !address.starts_with("http://") {
        PageMark::Info
    } else {
        PageMark::Warning
    }
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

type PaintIcon = unsafe extern "system" fn(*const c_void, *const c_void, i32, u32);

/// Back, forward, reload and stop, and the three marks of the field.
struct Icons {
    navigation: [usize; 4],
    marks: [usize; 3],
}
static ICONS: OnceLock<Icons> = OnceLock::new();

fn mark_index(mark: PageMark) -> usize {
    match mark {
        PageMark::Lock => 0,
        PageMark::Warning => 1,
        PageMark::Info => 2,
    }
}

/// The address of the window's active tab.
unsafe fn active_address(view: *mut c_void) -> Option<String> {
    let window = crate::chrome_layout::hwnd_of_view(view)?;
    let tabs = crate::chrome_tabs::open_tabs(window)?;
    tabs.into_iter().find(|tab| tab.saved.active).map(|tab| tab.saved.url)
}

/// A button of a Rivet window's row draws Rivet's icon instead of what the browser baked.
unsafe extern "system" fn button_paint(view: *mut c_void, canvas: *const c_void) {
    let Some(row) = row() else { return };
    let table = *(view as *const usize);
    let Some(function) = held(table, row.button_paint_slot) else { return };
    if !in_rivet_window(view as usize, row) {
        let run: Paint = std::mem::transmute(function);
        return run(view, canvas);
    }
    let (Some(image), Some(icons)) = (image::recorded(), ICONS.get()) else { return };
    let draw: PaintIcon = std::mem::transmute(image.base + row.paint_vector_icon);
    if class_of(view as usize, image.base) == row.location_icon {
        let warns = bounds_of(view as usize, row)[2] > CHIP_GLYPH_ONLY;
        let mark = page_mark(&active_address(view).unwrap_or_default(), warns);
        let ink = if mark == PageMark::Warning { WARNING_INK } else { LOCK_INK };
        return draw(canvas, icons.marks[mark_index(mark)] as *const c_void, CHIP_GLYPH_ONLY, ink);
    }
    let parent = parent_of(view as usize, row);
    let shown = children(parent, row).into_iter().filter(|child| row.buttons.contains(&class_of(*child, image.base)));
    let Some(position) = shown.take(NAVIGATION_BUTTONS).position(|child| child == view as usize) else { return };
    let stopping = position == 2 && *((view as usize + row.reload_mode_offset) as *const i32) != 0;
    let state = *((view as usize + row.button_state_offset) as *const i32);
    let icon = icons.navigation[if stopping { 3 } else { position }];
    draw(canvas, icon as *const c_void, CONTENT, crate::whale_toolbar::nav_colour(state))
}

/// The fields a person is editing, and those that already have the design's font or wait for it.
static EDITING: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static FONT_SET: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static FONT_WAITING: Mutex<Vec<usize>> = Mutex::new(Vec::new());

unsafe fn apply_text_colour(view: usize, row: &TopRow, base: usize, colour: u32) {
    let holder = *((view + row.textfield_render_text) as *const usize);
    if holder < 0x10000 {
        return;
    }
    let render_text = *((holder + 0x10) as *const usize);
    if render_text < 0x10000 {
        return;
    }
    let apply: unsafe extern "system" fn(usize, u32, *const [u32; 2]) = std::mem::transmute(base + row.apply_color);
    apply(render_text, colour, &WHOLE_TEXT);
}

/// Gives the waiting fields the design's font. A paint is no place to change what lays a view out.
unsafe extern "system" fn set_field_fonts(window: HWND, _message: u32, id: usize, _time: u32) {
    windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(window, id);
    let (Some(row), Some(image)) = (row(), image::recorded()) else { return };
    // A `std::string` of up to 22 bytes lives inside its three words, with the length in the last byte.
    let mut name = [0u64; 3];
    std::ptr::copy_nonoverlapping(ADDRESS_FONT.as_ptr(), name.as_mut_ptr().cast::<u8>(), ADDRESS_FONT.len());
    name[2] = (ADDRESS_FONT.len() as u64) << 56;
    let new_font: unsafe extern "system" fn(*mut usize, *const [u64; 3], i32) -> *mut usize =
        std::mem::transmute(image.base + row.font_new);
    let new_list: unsafe extern "system" fn(*mut usize, *const usize) -> *mut usize =
        std::mem::transmute(image.base + row.font_list_new);
    let set: unsafe extern "system" fn(usize, *const usize) = std::mem::transmute(image.base + row.textfield_set_font);
    let waiting = std::mem::take(&mut *FONT_WAITING.lock().unwrap_or_else(|error| error.into_inner()));
    for view in waiting {
        let mut font = [0usize; 2];
        new_font(font.as_mut_ptr(), &name, ADDRESS_FONT_SIZE);
        let mut list: usize = 0;
        new_list(&mut list, font.as_ptr());
        if list != 0 {
            set(view, &list);
        }
    }
}

/// The browser colours the field's text when the text changes, so a paint at rest colours it again.
unsafe extern "system" fn omnibox_paint(view: *mut c_void, canvas: *const c_void) {
    let Some(row) = row() else { return };
    let table = *(view as *const usize);
    let Some(function) = held(table, row.omnibox_paint.0) else { return };
    let run: Paint = std::mem::transmute(function);
    let at = view as usize;
    if in_rivet_window(at, row) {
        if let Some(image) = image::recorded() {
            if !EDITING.lock().unwrap_or_else(|error| error.into_inner()).contains(&at) {
                apply_text_colour(at, row, image.base, ADDRESS_INK);
            }
            let fresh = {
                let mut set = FONT_SET.lock().unwrap_or_else(|error| error.into_inner());
                let fresh = !set.contains(&at);
                if fresh {
                    set.push(at);
                }
                fresh
            };
            if fresh {
                if let Some(window) = crate::chrome_layout::hwnd_of_view(view) {
                    FONT_WAITING.lock().unwrap_or_else(|error| error.into_inner()).push(at);
                    windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(window, FONT_TIMER, 0, Some(set_field_fonts));
                }
            }
        }
    }
    run(view, canvas)
}

type Called = unsafe extern "system" fn(*mut c_void);

unsafe extern "system" fn omnibox_focus(view: *mut c_void) {
    let Some(row) = row() else { return };
    let Some(function) = held(*(view as *const usize), row.omnibox_focus.0) else { return };
    std::mem::transmute::<usize, Called>(function)(view);
    if !in_rivet_window(view as usize, row) {
        return;
    }
    EDITING.lock().unwrap_or_else(|error| error.into_inner()).push(view as usize);
    if let Some(image) = image::recorded() {
        apply_text_colour(view as usize, row, image.base, EDITING_INK);
    }
}

unsafe extern "system" fn omnibox_blur(view: *mut c_void) {
    let Some(row) = row() else { return };
    let Some(function) = held(*(view as *const usize), row.omnibox_blur.0) else { return };
    EDITING.lock().unwrap_or_else(|error| error.into_inner()).retain(|known| *known != view as usize);
    std::mem::transmute::<usize, Called>(function)(view);
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
    if !image.has(row.apply_color, row.apply_color_prologue)
        || !image.has(row.textfield_set_font, row.textfield_set_font_prologue)
        || !image.has(row.font_new, row.font_new_prologue)
        || !image.has(row.font_list_new, row.font_list_new_prologue)
        || ![row.omnibox_paint, row.omnibox_focus, row.omnibox_blur].iter().all(|(slot, function)| image.points_to(row.omnibox + slot * 8, *function))
    {
        return Err("this browser build has another address text field".into());
    }
    if !image.has(row.paint_vector_icon, row.paint_vector_icon_prologue)
        || !row.button_paints.iter().all(|(vtable, function)| image.points_to(vtable + row.button_paint_slot * 8, *function))
    {
        return Err("this browser build has other row buttons".into());
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
    let _ = ICONS.set(Icons {
        navigation: [NavMark::Back, NavMark::Forward, NavMark::Reload, NavMark::Stop]
            .map(|mark| crate::chrome_toolbar::nav_icon(mark, CONTENT as f32, row.canvas_command) as usize),
        marks: [PageMark::Lock, PageMark::Warning, PageMark::Info].map(|mark| {
            crate::chrome_toolbar::page_mark_icon(mark, CHIP_GLYPH_ONLY as f32, CHIP_GLYPH_CENTRE, row.canvas_command) as usize
        }),
    });
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
        for (vtable, function) in row.button_paints {
            known.push((base + vtable, row.button_paint_slot, base + function));
        }
        for (slot, function) in [row.omnibox_paint, row.omnibox_focus, row.omnibox_blur] {
            known.push((base + row.omnibox, slot, base + function));
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
        for (vtable, _) in row.button_paints {
            crate::chrome_toolbar::replace_slot(((base + vtable) as *mut usize).add(row.button_paint_slot), button_paint as *const () as usize)?;
        }
        let field = (base + row.omnibox) as *mut usize;
        crate::chrome_toolbar::replace_slot(field.add(row.omnibox_paint.0), omnibox_paint as *const () as usize)?;
        crate::chrome_toolbar::replace_slot(field.add(row.omnibox_focus.0), omnibox_focus as *const () as usize)?;
        crate::chrome_toolbar::replace_slot(field.add(row.omnibox_blur.0), omnibox_blur as *const () as usize)?;
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
    fn fr12_samsung_chip_and_text_sit_where_the_design_has_them() {
        // The image views go: Rivet draws the icons in the buttons.
        assert_eq!(placed(Part::ButtonImage, [6, 4, 20, 20], [28, 28], 38), [6, 4, 0, 0]);
        assert_eq!(placed(Part::Chip, [8, 2, 24, 24], [1048, 28], 38), [6, 3, 25, 22]);
        assert_eq!(placed(Part::Chip, [8, 2, 92, 24], [1048, 28], 38), [6, 3, 92, 22]);
        assert_eq!(placed(Part::Text { chip_end: 31 }, [37, 2, 993, 24], [1048, 28], 38), [39, 2, 991, 24]);
    }

    #[test]
    fn fr12_samsung_page_marks_follow_the_address_like_the_design() {
        assert_eq!(page_mark("https://example.com/", false), PageMark::Lock);
        assert_eq!(page_mark("http://example.com/", false), PageMark::Warning);
        assert_eq!(page_mark("http://localhost:3000/", false), PageMark::Info);
        assert_eq!(page_mark("rivet://start", false), PageMark::Info);
        assert_eq!(page_mark("file:///C:/a.html", false), PageMark::Info);
        assert_eq!(page_mark("https://expired.example/", true), PageMark::Warning);
    }

    #[test]
    fn fr12_samsung_address_text_is_the_designs() {
        // 12 DIP text in the design's muted colour, and a range that holds any text.
        assert_eq!((ADDRESS_FONT_SIZE, ADDRESS_INK), (12, 0xFF8A_8690));
        assert!(ADDRESS_FONT.len() <= 22);
        assert_ne!(WHOLE_TEXT, [u32::MAX, u32::MAX]);
    }

    #[test]
    fn fr12_samsung_colours_keep_other_ids() {
        assert_eq!(colour(&[(1, 2)], 1), Some(2));
        assert_eq!(colour(&[(1, 2)], 3), None);
    }
}
