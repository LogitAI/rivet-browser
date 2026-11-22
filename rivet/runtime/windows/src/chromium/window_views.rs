//! Finds the views and the widget of a browser window by Chromium's own class names and the shape
//! of its view tree, so the window needs no member offset or vtable slot of its build for them.
//!
//! Each path below follows public Chromium source:
//!
//! - `BrowserViewLayout` keeps the views it lays out in `BrowserViewLayoutViews views_`
//!   (`chrome/browser/ui/views/frame/layout/browser_view_layout.h`). Its `browser_view` is the
//!   `BrowserView` as a `views::View`, `main_background_region` is a `MainBackgroundRegionView` and
//!   `multi_contents_view` is a `MultiContentsView`. `BrowserView::BrowserView` adds both of the
//!   latter with `AddChildView` (`browser_view.cc`), so each names the browser view as its parent.
//!   The layout's fields are found by the class names of the views they point at.
//! - `BrowserFrameView` keeps the browser view in `views::ViewTracker browser_view_`
//!   (`browser_frame_view.h`), which holds the `BrowserView`'s `views::View`: the view the layout
//!   lays out. The frame view's field is the one that holds the same view.
//! - A view's widget is what `View::GetWidget` returns: the root view's `widget_`, a
//!   `raw_ptr<Widget>` of `views::internal::RootView` (`ui/views/widget/root_view.h`). The root
//!   view is reached through `View::parent_` until it is null. `Widget` owns the root view in
//!   `std::unique_ptr<internal::RootView> root_view_` (`ui/views/widget/widget.h`), so the root's
//!   widget field is the one whose object points back at the root, and `Widget` carries its own
//!   class metadata (`METADATA_HEADER_BASE(Widget)`).
//!
//! The logic reads memory through `Memory`, so it is tested on plain data. Where a build's row also
//! has the value, the two must agree or the value is not used.

/// Reads of the browser's memory that the paths need.
pub(crate) trait Memory {
    /// The word at `address`, or `None` when it cannot be read.
    fn word(&self, address: usize) -> Option<usize>;
    /// The `ui::metadata` class name of the object at `object`, when it has one.
    fn class_name(&self, object: usize) -> Option<String>;
}

pub(crate) const BROWSER_VIEW: &str = "BrowserView";
pub(crate) const MAIN_BACKGROUND_REGION: &str = "MainBackgroundRegionView";
pub(crate) const MULTI_CONTENTS: &str = "MultiContentsView";

/// How far into a layout object its view fields are looked for.
const LAYOUT_REACH: usize = 0x400;
/// How far into a frame view, a root view or a widget a field is looked for.
const OBJECT_REACH: usize = 0x1000;
/// How many parents a view may have before the walk to its root gives up.
const MAX_DEPTH: usize = 64;

/// The fields of `object` within `reach` that hold the one object named `class`, with that object.
/// Several fields may hold the same object. Two different objects of the class leave it unknown.
pub(crate) fn fields_naming<M: Memory>(
    memory: &M,
    object: usize,
    reach: usize,
    class: &str,
    accept: &dyn Fn(usize) -> bool,
) -> Option<(Vec<usize>, usize)> {
    let mut found: Option<(Vec<usize>, usize)> = None;
    for field in (0..reach).step_by(8) {
        let Some(value) = memory.word(object + field) else {
            break;
        };
        if value == 0 || value == object || value % 8 != 0 {
            continue;
        }
        if let Some((fields, known)) = found.as_mut() {
            if *known == value {
                fields.push(field);
                continue;
            }
        }
        if memory.class_name(value).as_deref() != Some(class) || !accept(value) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some((vec![field], value));
    }
    found
}

/// The fields of a browser layout object, as offsets into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LayoutFields {
    /// The fields that hold the `BrowserView`, the view the layout lays out.
    pub host: Vec<usize>,
    /// The fields that hold the `MainBackgroundRegionView`, the area below the top row.
    pub page_area: Vec<usize>,
    /// The fields that hold the `MultiContentsView`, which holds the pages.
    pub contents: Vec<usize>,
}

/// The view fields of the layout object at `layout`. With the parent field known, a page area or
/// contents view counts only when it names the browser view as its parent, as `AddChildView` left
/// it. `None` when no field holds a browser view.
pub(crate) fn layout_fields<M: Memory>(
    memory: &M,
    layout: usize,
    parent: Option<usize>,
) -> Option<LayoutFields> {
    let anything = |_: usize| true;
    let (host, view) = fields_naming(memory, layout, LAYOUT_REACH, BROWSER_VIEW, &anything)?;
    let child_of_host = |child: usize| match parent {
        Some(parent) => memory.word(child + parent) == Some(view),
        None => true,
    };
    let child = |class: &str| {
        fields_naming(memory, layout, LAYOUT_REACH, class, &child_of_host)
            .map_or_else(Vec::new, |(fields, _)| fields)
    };
    Some(LayoutFields {
        host,
        page_area: child(MAIN_BACKGROUND_REGION),
        contents: child(MULTI_CONTENTS),
    })
}

/// The fields of the frame view at `frame` that hold its browser view. When the layout already
/// gave the browser view, the field must hold that one; otherwise every view in the browser view's
/// own list of children must name it as their parent, which tells a view's own address from a
/// pointer into another part of the object.
pub(crate) fn frame_fields<M: Memory>(
    memory: &M,
    frame: usize,
    host: Option<usize>,
    parent: Option<usize>,
) -> Option<Vec<usize>> {
    let is_host = |view: usize| match (host, parent) {
        (Some(host), _) => view == host,
        (None, Some(parent)) => is_listing_parent(memory, view, parent),
        (None, None) => false,
    };
    fields_naming(memory, frame, OBJECT_REACH, BROWSER_VIEW, &is_host).map(|(fields, _)| fields)
}

/// Whether the view at `view` lists children that all name it at `parent`. `View::children_` is
/// the `std::vector` right after `parent_` in `ui/views/view.h`.
fn is_listing_parent<M: Memory>(memory: &M, view: usize, parent: usize) -> bool {
    let children = children(memory, view, parent + 8);
    !children.is_empty()
        && children
            .iter()
            .all(|child| memory.word(child + parent) == Some(view))
}

fn children<M: Memory>(memory: &M, view: usize, field: usize) -> Vec<usize> {
    let (Some(begin), Some(end)) = (memory.word(view + field), memory.word(view + field + 8))
    else {
        return Vec::new();
    };
    if begin == 0 || end < begin || (end - begin) % 8 != 0 || (end - begin) / 8 > 256 {
        return Vec::new();
    }
    (begin..end)
        .step_by(8)
        .filter_map(|at| memory.word(at))
        .collect()
}

/// The root of the view tree `view` is in: the view whose parent is null.
pub(crate) fn root_of<M: Memory>(memory: &M, view: usize, parent: usize) -> Option<usize> {
    let mut at = view;
    for _ in 0..MAX_DEPTH {
        match memory.word(at + parent)? {
            0 => return Some(at),
            next => at = next,
        }
    }
    None
}

/// Where a root view keeps its widget and where the widget keeps the root view, as offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WidgetFields {
    pub widget_in_root: usize,
    pub root_in_widget: usize,
}

/// Finds the widget fields from a root view at `root`: a field of the root whose object has class
/// metadata and holds the root in turn, which is not a child view naming the root as its parent.
/// The root must be named as a `RootView` is. `None` when no field or two different ones qualify.
pub(crate) fn widget_fields<M: Memory>(
    memory: &M,
    root: usize,
    parent: usize,
) -> Option<WidgetFields> {
    if !memory
        .class_name(root)
        .is_some_and(|name| name.ends_with("RootView"))
    {
        return None;
    }
    let listed = children(memory, root, parent + 8);
    let mut found: Option<(usize, WidgetFields)> = None;
    for field in (0..OBJECT_REACH).step_by(8) {
        let Some(widget) = memory.word(root + field) else {
            break;
        };
        if widget == 0 || widget == root || widget % 8 != 0 || listed.contains(&widget) {
            continue;
        }
        if found.is_some_and(|(known, _)| known == widget) {
            continue;
        }
        // Reading stops where the object's memory ends.
        let Some(back) = (0..OBJECT_REACH)
            .step_by(8)
            .map_while(|offset| memory.word(widget + offset).map(|word| (offset, word)))
            .find(|(offset, word)| *offset != parent && *word == root)
            .map(|(offset, _)| offset)
        else {
            continue;
        };
        if memory.class_name(widget).is_none() {
            continue;
        }
        let fields = WidgetFields {
            widget_in_root: field,
            root_in_widget: back,
        };
        if found.is_some() {
            return None;
        }
        found = Some((widget, fields));
    }
    found.map(|(_, fields)| fields)
}

/// The widget that holds `view` through fields already found: the root's widget, which must hold
/// the root back. Zero for a view in no widget, as `View::GetWidget` gives.
pub(crate) fn widget_of<M: Memory>(
    memory: &M,
    view: usize,
    parent: usize,
    fields: WidgetFields,
) -> usize {
    let Some(root) = root_of(memory, view, parent) else {
        return 0;
    };
    let Some(widget) = memory.word(root + fields.widget_in_root) else {
        return 0;
    };
    if widget == 0 || memory.word(widget + fields.root_in_widget) != Some(root) {
        return 0;
    }
    widget
}

/// One value from the fields reflection found and the value of the row. Both must agree when both
/// are known: the row's field must be one of the fields that hold the object. A row with no value
/// takes the reflected one, and a build reflection finds nothing in keeps its row's.
pub(crate) fn settle(reflected: Option<&[usize]>, row: Option<usize>) -> Option<usize> {
    match (reflected.filter(|fields| !fields.is_empty()), row) {
        (Some(fields), Some(row)) => fields.contains(&row).then_some(row),
        (Some(fields), None) => fields.first().copied(),
        (None, row) => row,
    }
}

/// The browser's own memory, read only where it can be read, with class names read through each
/// object's `ui::metadata` as `reflect` reads them.
pub(crate) struct Live {
    ranges: crate::chromium::reflect::Ranges,
}

impl Memory for Live {
    fn word(&self, address: usize) -> Option<usize> {
        crate::chromium::memory::readable(address, 8)
            .then(|| unsafe { (address as *const usize).read() })
    }
    fn class_name(&self, object: usize) -> Option<String> {
        crate::chromium::reflect::class_name_far(&self.ranges, object)
    }
}

/// The browser module's memory, once its sections are known.
fn live() -> Option<&'static Live> {
    static LIVE: std::sync::OnceLock<Option<Live>> = std::sync::OnceLock::new();
    LIVE.get_or_init(|| {
        let image = crate::chromium::image::recorded()?;
        let headers =
            unsafe { std::slice::from_raw_parts(image.base as *const u8, 0x1000.min(image.size)) };
        let list = crate::chromium::reflect::sections(headers);
        Some(Live {
            ranges: crate::chromium::reflect::ranges_of(image.base, &list),
        })
    })
    .as_ref()
}

/// Where a view of this build keeps its parent.
fn parent_field() -> Option<usize> {
    crate::builds::current()
        .and_then(crate::features::app_window::view_fields)
        .map(|(parent, _)| parent)
}

/// What one value became: not looked for yet, or settled, with `None` for a value that neither
/// reflection nor the row gave, or where the two disagree.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Settled {
    Unknown,
    Known(Option<usize>),
}

/// How many times a value is looked for before the row alone decides it, so a build whose views
/// carry other names does not scan on every layout.
const TRIES: u32 = 8;

struct Slot {
    value: std::sync::Mutex<(Settled, u32)>,
}

impl Slot {
    const fn new() -> Self {
        Self {
            value: std::sync::Mutex::new((Settled::Unknown, 0)),
        }
    }

    /// The settled value, or what `find` and the row settle it to. A miss counts as a try, and
    /// after the last try the row's value stands alone.
    fn get(&self, row: Option<usize>, find: impl FnOnce() -> Option<Vec<usize>>) -> Option<usize> {
        let mut value = self.value.lock().unwrap_or_else(|error| error.into_inner());
        if let Settled::Known(known) = value.0 {
            return known;
        }
        match find() {
            Some(fields) => {
                let settled = settle(Some(fields.as_slice()), row);
                if settled.is_none() {
                    crate::trace::trace(&format!(
                        "window views: reflection found {fields:x?} where the row has {row:x?},                          so the value is not used"
                    ));
                }
                value.0 = Settled::Known(settled);
                settled
            }
            None => {
                value.1 += 1;
                if value.1 >= TRIES {
                    value.0 = Settled::Known(row);
                }
                row
            }
        }
    }

    fn known(&self, row: Option<usize>) -> Option<usize> {
        match self
            .value
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .0
        {
            Settled::Known(known) => known,
            Settled::Unknown => row,
        }
    }
}

static HOST: Slot = Slot::new();
static PAGE_AREA: Slot = Slot::new();
static CONTENTS: Slot = Slot::new();
static FRAME: Slot = Slot::new();

/// A row value that is recorded: an offset the build has, not the absent marker.
fn recorded(value: Option<usize>) -> Option<usize> {
    value.filter(|value| *value != crate::builds::ABSENT)
}

fn side_panel_row() -> &'static crate::builds::SidePanel {
    &crate::builds::get().side_panel
}

/// Settles the view fields of the browser layout object at `layout`, which the layout hook hands
/// over, and returns the field that holds the view it lays out. Reads memory only.
pub(crate) unsafe fn host_field(layout: usize) -> Option<usize> {
    let row = side_panel_row();
    let mut found: Option<LayoutFields> = None;
    let mut fields = || {
        if found.is_none() {
            found = live().and_then(|live| layout_fields(live, layout, parent_field()));
        }
        found.clone()
    };
    let host = HOST.get(recorded(row.host_view), || fields().map(|found| found.host));
    PAGE_AREA.get(recorded(row.page_area), || {
        fields()
            .map(|found| found.page_area)
            .filter(|fields| !fields.is_empty())
    });
    CONTENTS.get(recorded(row.contents_container), || {
        fields()
            .map(|found| found.contents)
            .filter(|fields| !fields.is_empty())
    });
    host
}

/// The layout object's field that holds the area below the top row, or `ABSENT`.
pub(crate) fn page_area_field() -> usize {
    PAGE_AREA
        .known(recorded(side_panel_row().page_area))
        .unwrap_or(crate::builds::ABSENT)
}

/// The layout object's field that holds the view of the pages, or `ABSENT`.
pub(crate) fn contents_field() -> usize {
    CONTENTS
        .known(recorded(side_panel_row().contents_container))
        .unwrap_or(crate::builds::ABSENT)
}

/// The browser view the frame view at `frame` holds, the same view the browser's layout lays out.
/// `host` is that view when the layout already ran for the window.
pub(crate) unsafe fn frame_browser_view(frame: usize, host: Option<usize>) -> Option<usize> {
    let row = recorded(crate::builds::get().layout.frame_browser_window_offset);
    let field = FRAME.get(row, || {
        live().and_then(|live| frame_fields(live, frame, host, parent_field()))
    })?;
    live()?.word(frame + field).filter(|view| *view != 0)
}

/// Starts the view walk of `reflect` from `window`'s own `HWNDMessageHandler`, once per process, on
/// a worker thread: it only reads memory, and its result is kept by build fingerprint for
/// `walked_view_fields`.
pub(crate) unsafe fn walk_views_once(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let (Some(live), Some(build)) = (live(), fingerprint()) else {
        return;
    };
    if STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let ranges = live.ranges;
    let handler = GetWindowLongPtrW(window, GWLP_USERDATA) as usize;
    std::thread::spawn(move || {
        let found = crate::chromium::reflect::find_cached(
            &ranges,
            handler,
            std::time::Duration::from_millis(500),
            build,
        );
        match found {
            Ok((found, _)) => crate::trace::trace(&format!(
                "window views: the view walk found the parent at {:#x} and the view list at {:#x}",
                found.parent, found.children
            )),
            Err(error) => crate::trace::trace(&format!("window views: {error}")),
        }
    });
}

/// Reads the class tables of `window`'s views from the live views once per process: the browser
/// view the view walk found, its frame view and the frame view's caption buttons and corners. The
/// walk's result is reused when the worker thread already has it. Reads memory only.
pub(crate) unsafe fn read_tables(
    window: windows_sys::Win32::Foundation::HWND,
) -> crate::chromium::live_tables::Tables {
    use crate::chromium::live_tables;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
    let known = live_tables::found();
    if known != live_tables::Tables::default() {
        return known;
    }
    let (Some(live), Some(build), Some(image)) =
        (live(), fingerprint(), crate::chromium::image::recorded())
    else {
        return known;
    };
    let handler = GetWindowLongPtrW(window, GWLP_USERDATA) as usize;
    let found = crate::chromium::reflect::find_cached(
        &live.ranges,
        handler,
        std::time::Duration::from_millis(300),
        build,
    );
    let Ok((found, _)) = found else {
        return known;
    };
    let Some(browser_view) = found.browser_view else {
        return known;
    };
    let ranges = live.ranges;
    // A class table lies in read-only data and its first slot holds code.
    let is_table = |table: usize| {
        table >= ranges.rdata.0
            && table < ranges.rdata.1
            && live
                .word(table)
                .is_some_and(|slot| slot >= ranges.text.0 && slot < ranges.text.1)
    };
    let tables = live_tables::note(live, browser_view, found.parent, image.base, &is_table);
    crate::trace::trace(&format!(
        "window views: class tables from live views {tables:x?}"
    ));
    tables
}

/// The browser module's fingerprint, which keys what the view walk found.
fn fingerprint() -> Option<(u32, u32)> {
    let image = crate::chromium::image::recorded()?;
    let headers =
        unsafe { std::slice::from_raw_parts(image.base as *const u8, 0x1000.min(image.size)) };
    crate::chromium::reflect::fingerprint(headers)
}

/// The parent and view list fields the view walk found in this process's build, once it ran.
pub(crate) fn walked_view_fields() -> Option<(usize, usize)> {
    crate::chromium::reflect::walked_fields(fingerprint()?)
}

/// What the widget path became: the fields found, or that it gave nothing.
static WIDGET: std::sync::Mutex<(Option<Option<WidgetFields>>, u32)> =
    std::sync::Mutex::new((None, 0));

/// The widget that holds `view`, found through the root of its view tree. When the build's row
/// has `View::GetWidget`'s vtable slot, `by_slot` calls it, once, to confirm the path: if the two
/// disagree neither is used, and a view is then in no widget. Without a path the slot decides.
pub(crate) unsafe fn widget_holding(
    view: usize,
    by_slot: Option<&dyn Fn(usize) -> usize>,
) -> usize {
    let Some((live, parent)) = live().zip(parent_field()) else {
        return by_slot.map_or(0, |slot| slot(view));
    };
    let mut state = WIDGET.lock().unwrap_or_else(|error| error.into_inner());
    if state.0.is_none() {
        let fields = root_of(live, view, parent).and_then(|root| widget_fields(live, root, parent));
        match fields {
            Some(fields) => {
                let found = widget_of(live, view, parent, fields);
                let agrees = by_slot.is_none_or(|slot| slot(view) == found);
                if !agrees {
                    crate::trace::trace(
                        "window views: the widget path and the widget slot disagree,                          so neither is used",
                    );
                }
                state.0 = Some(agrees.then_some(fields));
            }
            None => {
                state.1 += 1;
                if state.1 >= TRIES {
                    state.0 = Some(None);
                }
                drop(state);
                return by_slot.map_or(0, |slot| slot(view));
            }
        }
    }
    let (settled, tries) = (state.0, state.1);
    drop(state);
    match settled {
        Some(Some(fields)) => widget_of(live, view, parent, fields),
        // The slot stands alone only where no path was found; a disagreement uses neither.
        Some(None) if tries >= TRIES => by_slot.map_or(0, |slot| slot(view)),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Browser memory as plain words and the class names some objects carry.
    #[derive(Default)]
    struct Words {
        words: HashMap<usize, usize>,
        names: HashMap<usize, &'static str>,
    }

    impl Words {
        fn set(&mut self, address: usize, value: usize) {
            self.words.insert(address, value);
        }
        fn name(&mut self, object: usize, name: &'static str) {
            self.names.insert(object, name);
        }
        /// A list of views at `list` held by the `std::vector` at `view + field`.
        fn list(&mut self, view: usize, field: usize, list: usize, items: &[usize]) {
            self.set(view + field, list);
            self.set(view + field + 8, list + items.len() * 8);
            for (index, item) in items.iter().enumerate() {
                self.set(list + index * 8, *item);
            }
        }
    }

    impl Memory for Words {
        fn word(&self, address: usize) -> Option<usize> {
            // Objects are small blocks: every address inside one reads, zero where nothing is set.
            let block = address & !0xFFFF;
            self.words
                .keys()
                .any(|known| (known & !0xFFFF) == block)
                .then(|| self.words.get(&address).copied().unwrap_or(0))
        }
        fn class_name(&self, object: usize) -> Option<String> {
            self.names.get(&object).map(|name| (*name).to_owned())
        }
    }

    const PARENT: usize = 0x198;
    const LAYOUT: usize = 0x10_0000;
    const BROWSER: usize = 0x20_0000;
    const PAGE_AREA: usize = 0x30_0000;
    const CONTENTS: usize = 0x40_0000;
    const PANEL: usize = 0x50_0000;
    const FRAME: usize = 0x60_0000;
    const ROOT: usize = 0x70_0000;
    const WIDGET: usize = 0x80_0000;
    const LISTS: usize = 0x90_0000;
    const OTHER: usize = 0xA0_0000;

    /// A browser window laid out as Chromium 157's layout keeps it, with its views at the fields
    /// `BrowserViewLayoutViews` gives them and the tree `BrowserView` builds.
    fn window(host: usize, page_area: usize, contents: usize) -> Words {
        let mut memory = Words::default();
        memory.set(LAYOUT, 0x1234); // the layout's vtable
        memory.set(LAYOUT + host, BROWSER);
        memory.set(LAYOUT + page_area, PAGE_AREA);
        memory.set(LAYOUT + contents, CONTENTS);
        memory.set(LAYOUT + 0xC0, PANEL);
        memory.name(BROWSER, "BrowserView");
        memory.name(PAGE_AREA, "MainBackgroundRegionView");
        memory.name(CONTENTS, "MultiContentsView");
        memory.name(PANEL, "SidePanel");
        for child in [PAGE_AREA, CONTENTS, PANEL] {
            memory.set(child + PARENT, BROWSER);
        }
        memory.list(BROWSER, PARENT + 8, LISTS, &[PAGE_AREA, CONTENTS, PANEL]);
        // The browser view sits in the root view, which belongs to the widget.
        memory.set(BROWSER + PARENT, ROOT);
        memory.set(ROOT + PARENT, 0);
        memory.name(ROOT, "BrowserRootView");
        memory.list(ROOT, PARENT + 8, LISTS + 0x100, &[BROWSER]);
        memory.set(ROOT + 0x3A0, WIDGET);
        memory.set(WIDGET + 0x1D8, ROOT);
        memory.name(WIDGET, "BrowserWidget");
        // The frame view's tracker of the browser view, and the tracker's observation of it.
        memory.set(FRAME + 0x338, 0x5678);
        memory.set(FRAME + 0x340, BROWSER);
        memory.set(FRAME + 0x348, BROWSER);
        memory
    }

    #[test]
    fn nfr6_the_side_panel_views_are_found_by_class_name() {
        // Chrome 157 keeps them at 0x38, 0x50 and 0xb8; Edge has its own fields between them.
        for (host, page_area, contents) in [(0x38, 0x50, 0xB8), (0x38, 0x58, 0xD8)] {
            let memory = window(host, page_area, contents);
            let fields = layout_fields(&memory, LAYOUT, Some(PARENT)).expect("a browser view");
            assert_eq!(fields.host, [host]);
            assert_eq!(fields.page_area, [page_area]);
            assert_eq!(fields.contents, [contents]);
        }
    }

    #[test]
    fn nfr6_a_layout_view_that_is_not_the_browser_views_child_is_not_taken() {
        let mut memory = window(0x38, 0x50, 0xB8);
        memory.set(CONTENTS + PARENT, OTHER);
        let fields = layout_fields(&memory, LAYOUT, Some(PARENT)).unwrap();
        assert!(fields.contents.is_empty());
        assert_eq!(fields.page_area, [0x50]);
        // A layout with no browser view has no fields at all.
        let mut bare = Words::default();
        bare.set(LAYOUT + 0x38, PAGE_AREA);
        bare.name(PAGE_AREA, "MainBackgroundRegionView");
        assert_eq!(layout_fields(&bare, LAYOUT, Some(PARENT)), None);
    }

    #[test]
    fn nfr6_two_different_views_of_one_class_leave_the_field_unknown() {
        let mut memory = window(0x38, 0x50, 0xB8);
        memory.set(LAYOUT + 0x100, OTHER);
        memory.name(OTHER, "MultiContentsView");
        memory.set(OTHER + PARENT, BROWSER);
        let fields = layout_fields(&memory, LAYOUT, Some(PARENT)).unwrap();
        assert!(fields.contents.is_empty());
    }

    #[test]
    fn nfr6_the_frame_view_field_is_the_one_holding_the_browser_view() {
        let memory = window(0x38, 0x50, 0xB8);
        // With the laid out view known, and without it through the browser view's own children.
        assert_eq!(
            frame_fields(&memory, FRAME, Some(BROWSER), Some(PARENT)),
            Some(vec![0x340, 0x348])
        );
        assert_eq!(
            frame_fields(&memory, FRAME, None, Some(PARENT)),
            Some(vec![0x340, 0x348])
        );
        assert_eq!(
            frame_fields(&memory, FRAME, Some(OTHER), Some(PARENT)),
            None
        );
        assert_eq!(frame_fields(&memory, FRAME, None, None), None);
    }

    #[test]
    fn nfr6_a_views_widget_is_found_without_a_vtable_slot() {
        let memory = window(0x38, 0x50, 0xB8);
        assert_eq!(root_of(&memory, CONTENTS, PARENT), Some(ROOT));
        let fields = widget_fields(&memory, ROOT, PARENT).expect("the widget fields");
        assert_eq!(
            fields,
            WidgetFields {
                widget_in_root: 0x3A0,
                root_in_widget: 0x1D8
            }
        );
        assert_eq!(widget_of(&memory, CONTENTS, PARENT, fields), WIDGET);
        assert_eq!(widget_of(&memory, BROWSER, PARENT, fields), WIDGET);
        // A view whose tree has no widget, or whose root is not the widget's, has none.
        let mut loose = window(0x38, 0x50, 0xB8);
        loose.set(WIDGET + 0x1D8, OTHER);
        assert_eq!(widget_of(&loose, CONTENTS, PARENT, fields), 0);
        // Only a root view gives the fields.
        assert_eq!(widget_fields(&memory, BROWSER, PARENT), None);
    }

    #[test]
    fn nfr6_a_reflected_value_must_agree_with_the_row() {
        let both: &[usize] = &[0x340, 0x348];
        assert_eq!(settle(Some(both), Some(0x340)), Some(0x340));
        assert_eq!(settle(Some(both), Some(0x348)), Some(0x348));
        assert_eq!(settle(Some(&both[..1]), Some(0x3B0)), None);
        assert_eq!(settle(Some(&[0x3B0][..]), None), Some(0x3B0));
        assert_eq!(settle(None, Some(0x38)), Some(0x38));
        assert_eq!(settle(Some(&[][..]), Some(0x38)), Some(0x38));
        assert_eq!(settle(None, None), None);
    }
}
