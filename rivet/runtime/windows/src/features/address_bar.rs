//! Rivet's top bar of an app window: back, forward, reload and the address.
//!
//! An app window has no toolbar of its own, so the bar is drawn on the window's composition surface
//! (see `appbar`) and takes its input from the window's own messages. The address is typed into the
//! Windows text engine (`rivet_platform_windows::textservices`), which does the editing, selection,
//! clipboard, undo and input method. The page area is moved down by one row so the bar covers nothing
//! of the page.

use std::cell::RefCell;
use std::collections::HashMap;

use rivet_platform_windows::textservices::{Field, TIMER_BASE, TIMER_LAST};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::Ime::{ImmAssociateContextEx, IACE_DEFAULT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, PostMessageW};

use crate::features::split::{CAPTION_ROOM, ROW_HEIGHT, TOGGLE_ROOM};

/// Sizes of the study's `.toolbar` and `.where`, in DIPs.
const BUTTON: f32 = 28.0;
const BUTTON_GAP: f32 = 2.0;
const LEAD: f32 = 8.0;
const ADDRESS_MARGIN_LEFT: f32 = 8.0;
const ADDRESS_MARGIN_RIGHT: f32 = 4.0;
const ADDRESS_HEIGHT: f32 = 28.0;
const ADDRESS_PAD: f32 = 10.0;
/// Room the security chip takes before the address text.
pub(crate) const CHIP: f32 = 22.0;
pub(crate) const TEXT_SIZE: f32 = 12.0;
/// The appbar toggle is the bar's first button, before back, so the bar leaves room for it.
const TOGGLE_OVER_BAR: bool = true;
/// The study's `.caption { margin-left: 4px }` between the address and the caption buttons.
const CAPTION_MARGIN: f32 = 4.0;
/// The address while it is edited: the study's hover tint over the row's paper, as one colour so
/// the text engine can draw on it.
pub(crate) const FIELD_GROUND: u32 = 0xf0efec;
/// An address is limited by what Chromium shows, not by a name's length.
const MAX_ADDRESS: u32 = 8192;

/// A control of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Back,
    Forward,
    Reload,
    Chip,
    Address,
}

/// Where each control sits, from the bar's left edge, as (part, x, width), for a bar `width` DIPs
/// wide whose right end carries the caption buttons. `toggle` leaves room for the appbar toggle.
pub(crate) fn parts(width: f32, toggle: bool) -> Vec<(Part, f32, f32)> {
    // The toggle is a 28 DIP button of the row, with the row's 2 DIP gap after it.
    let mut x = LEAD
        + if toggle {
            TOGGLE_ROOM + BUTTON_GAP
        } else {
            0.0
        };
    let mut out = Vec::new();
    for part in [Part::Back, Part::Forward, Part::Reload] {
        out.push((part, x, BUTTON));
        x += BUTTON + BUTTON_GAP;
    }
    let left = x + ADDRESS_MARGIN_LEFT;
    let right = width - CAPTION_ROOM - CAPTION_MARGIN - ADDRESS_MARGIN_RIGHT;
    if right - left >= 56.0 {
        out.push((Part::Chip, left + ADDRESS_PAD - 4.0, CHIP));
        out.push((Part::Address, left, right - left));
    }
    out
}

/// Rivet's own minimize, maximize and close buttons at the right end of the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// The study's `.caption button { width: 46px }`, full row height.
pub(crate) const CAPTION_BUTTON: f32 = 46.0;

/// The caption buttons of a row `width` DIPs wide as (button, x, width), ending at its right edge.
pub(crate) fn caption_buttons(width: f32) -> [(Caption, f32, f32); 3] {
    let x = width - 3.0 * CAPTION_BUTTON;
    [
        (Caption::Minimize, x, CAPTION_BUTTON),
        (Caption::Maximize, x + CAPTION_BUTTON, CAPTION_BUTTON),
        (Caption::Close, x + 2.0 * CAPTION_BUTTON, CAPTION_BUTTON),
    ]
}

/// The Segoe Fluent glyph of a caption button. A maximized window shows restore.
pub(crate) fn caption_glyph(button: Caption, zoomed: bool) -> char {
    match button {
        Caption::Minimize => '\u{E921}',
        Caption::Maximize if zoomed => '\u{E923}',
        Caption::Maximize => '\u{E922}',
        Caption::Close => '\u{E8BB}',
    }
}

/// Whether an address is Rivet's start page: the local `start.html` shipped beside Rivet.
pub(crate) fn is_start_page(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or("");
    path.starts_with("file:") && path.ends_with("/start.html")
}

/// What the security chip says about the page, from its address. Chromium's own security state
/// replaces this where the build records how to read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Security {
    Secure,
    NotSecure,
    Local,
    Internal,
}

impl Security {
    pub(crate) fn of(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("https://") || lower.starts_with("wss://") {
            Security::Secure
        } else if let Some(rest) = lower.strip_prefix("http://") {
            let host = rest.split(['/', ':']).next().unwrap_or("");
            if matches!(host, "localhost" | "127.0.0.1" | "[") || lower.starts_with("http://[::1]")
            {
                Security::Local
            } else {
                Security::NotSecure
            }
        } else if lower.starts_with("file:") {
            Security::Local
        } else {
            Security::Internal
        }
    }

    /// The chip for Chromium's `security_state::SecurityLevel`: 3 and 4 are secure, 1 and 5 warn.
    /// Level 0 says nothing about the connection yet: Chromium reports it for an https page while the
    /// page is still loading, before the certificate is known. The chip stays neutral then rather
    /// than warning, and the address tells a local, internal or plain http page.
    pub(crate) fn from_level(level: Option<i32>, url: &str) -> Self {
        match level {
            Some(3 | 4) => Security::Secure,
            Some(1 | 5) => Security::NotSecure,
            Some(_) => match Self::of(url) {
                Security::Secure => Security::Internal,
                other => other,
            },
            None => Self::of(url),
        }
    }

    /// The glyph of Segoe Fluent Icons, and the words shown beside it.
    pub(crate) fn look(self) -> (char, &'static str) {
        match self {
            Security::Secure => ('\u{E72E}', ""),
            Security::NotSecure => ('\u{E7BA}', "Not secure"),
            Security::Local | Security::Internal => ('\u{E946}', ""),
        }
    }
}

/// What the address shows while it is not being edited, as the study's `<b>title</b> · url`: the
/// title in bold, then the address without its scheme.
pub(crate) fn shown(title: &str, url: &str) -> (String, String) {
    // The start page is a local file, but people see it as Rivet's new tab. Only the text changes.
    if is_start_page(url) {
        return (
            crate::address_display::START_PAGE_TITLE.to_owned(),
            format!(" \u{b7} {}", crate::address_display::START_PAGE_ADDRESS),
        );
    }
    let bare = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .trim_end_matches('/');
    let title = title.trim();
    if title.is_empty() || title == bare || title == url {
        (String::new(), bare.to_owned())
    } else {
        (title.to_owned(), format!(" \u{b7} {bare}"))
    }
}

/// Typed text that is plainly an address. Everything else is a search, which the browser's own
/// search engine answers.
pub(crate) fn looks_like_address(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("about:") || lower.starts_with("localhost") {
        return true;
    }
    let host = lower.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
}

/// The address to open for typed text that is an address: a bare host gets `https://`.
pub(crate) fn address_for(text: &str) -> String {
    let text = text.trim();
    if text.contains("://") || text.starts_with("about:") {
        text.to_owned()
    } else if text.starts_with("localhost") || text.starts_with("127.0.0.1") {
        format!("http://{text}")
    } else {
        format!("https://{text}")
    }
}

/// What the appbar draws of the bar, in window pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub toggle: bool,
    /// Set when the bar is only the address of a split pane's row, which draws the rest.
    pub pane: bool,
    pub title: String,
    pub rest: String,
    /// Set while the page is in developer mode: the row takes the developer ground and the address
    /// is shown whole in a monospace font.
    pub dev: bool,
    pub url: String,
    /// The room in DIPs the developer buttons take before the caption buttons.
    pub reserve: f32,
    pub security: Security,
    pub hover: Option<Part>,
    pub can_back: bool,
    pub can_forward: bool,
    /// The caption button under the pointer, and whether the window is maximized.
    pub caption_hover: Option<Caption>,
    pub zoomed: bool,
    /// Set while the address is edited: the field's pixels are drawn in place of the text.
    pub editing: bool,
    /// The browser's suggestions for the text being edited, as contents, description and whether the
    /// row is a search, and the
    /// one the arrow keys or the pointer picked.
    pub suggestions: Vec<(String, String, bool)>,
    pub selected: Option<usize>,
    /// Bumped whenever the field draws something new, so the appbar repaints.
    pub generation: u64,
}

struct Bar {
    field: Option<Field>,
    editing: bool,
    /// The address the field started with, for Escape.
    original: String,
    hover: Option<Part>,
    press: Option<Part>,
    generation: u64,
    suggestions: Vec<crate::features::omnibox::Suggestion>,
    selected: Option<usize>,
    /// The text the suggestions are for, as typed: without the inline completion.
    queried: String,
    /// The browser's inline completion shown selected after the typed text, or empty.
    inline: String,
    /// Whether the last key deleted text. The browser completes nothing inline then.
    deleting: bool,
}

thread_local! {
    static BARS: RefCell<HashMap<isize, Bar>> = RefCell::new(HashMap::new());
}

/// Whether the bar is inside one of its own calls now.
pub(crate) fn busy() -> bool {
    BARS.with(|bars| bars.try_borrow_mut().is_err())
}

fn with_bar<R>(window: HWND, work: impl FnOnce(&mut Bar) -> R) -> R {
    BARS.with(|bars| {
        let mut bars = bars.borrow_mut();
        let bar = bars.entry(window as isize).or_insert_with(|| Bar {
            field: None,
            editing: false,
            original: String::new(),
            hover: None,
            press: None,
            generation: 0,
            suggestions: Vec::new(),
            selected: None,
            queried: String::new(),
            inline: String::new(),
            deleting: false,
        });
        work(bar)
    })
}

/// Pixels per DIP of the window, at the scale the browser laid the window out. The bar's place
/// comes from that layout, so the window's own DPI would put it elsewhere while the two disagree,
/// as they do while the window moves between monitors.
fn factor(window: HWND) -> f32 {
    unsafe { crate::frame::layout_dpi(window) as f32 / 96.0 }
}

/// The bar of a window as left, top and width in DIPs, while the window shows a single page. A
/// split's pages carry rows of their own.
unsafe fn place(window: HWND) -> Option<[f32; 3]> {
    let panes = crate::features::split::panes(window);
    if !panes.is_empty() {
        // In a split of Rivet's second view, the bar is only the address of the focused pane's
        // row. The rest of every row is drawn with the row.
        if !crate::features::second_view::is_split(window) {
            return None;
        }
        let tabs = crate::features::tabs::tabs(window)?;
        let active = tabs.iter().find(|tab| tab.active).map(|tab| tab.index);
        let pane = panes
            .iter()
            .find(|pane| pane.active && pane.tab.is_some() && pane.tab == active)?;
        let (_, x, width) =
            crate::features::split::row_buttons(pane.width as f32, pane.first, pane.caption)
                .into_iter()
                .find(|button| button.0 == crate::features::split::RowButton::Address)?;
        return Some([pane.x as f32 + x, pane.row_y as f32, width]);
    }
    let panel = crate::features::frame_layout::panel_bounds(window)?;
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let right = client.right as f32 / factor(window);
    // The bar is the panel's top row, so it ends where the panel does.
    Some([
        panel.x as f32,
        panel.y as f32,
        (panel.width as f32).min(right - panel.x as f32),
    ])
}

/// Whether a window shows the bar, so the page area leaves its row free.
pub(crate) unsafe fn shown_in(window: HWND) -> bool {
    crate::frame::attached_window(window)
        && (crate::features::second_view::is_split(window)
            || crate::features::split::panes(window).is_empty())
}

/// Whether the bar is only the address of a split pane's row.
unsafe fn in_pane(window: HWND) -> bool {
    crate::features::second_view::is_split(window)
}

/// The controls of the bar: the full row of a single page, or the address alone in a pane's row.
pub(crate) fn parts_of(width: f32, pane: bool) -> Vec<(Part, f32, f32)> {
    parts_reserving(width, pane, 0.0)
}

/// The controls of the bar with `reserve` DIPs left free before the caption buttons, where the
/// developer buttons of a page in developer mode sit.
pub(crate) fn parts_reserving(width: f32, pane: bool, reserve: f32) -> Vec<(Part, f32, f32)> {
    let mut out = if pane {
        vec![
            (Part::Chip, ADDRESS_PAD - 4.0, CHIP),
            (Part::Address, 0.0, width),
        ]
    } else {
        parts(width, TOGGLE_OVER_BAR)
    };
    if !pane && reserve > 0.0 {
        out.retain_mut(|(part, _, w)| {
            if *part == Part::Address {
                *w -= reserve;
                *w >= 56.0
            } else {
                true
            }
        });
        if !out.iter().any(|p| p.0 == Part::Address) {
            out.retain(|p| p.0 != Part::Chip);
        }
    }
    out
}

/// The room the developer buttons take at the end of the bar of `window`, in DIPs.
unsafe fn dev_reserve(window: HWND) -> f32 {
    if crate::features::dev_page::window_in_dev_mode(window) {
        crate::dev_mode::bar_width(crate::features::dev_page::dev_actions().len())
    } else {
        0.0
    }
}

/// Moves the page below the bar. The browser lays the page out at the top of its panel, since an app
/// window has no toolbar there.
pub(crate) unsafe fn on_page_area(area: &mut crate::hooks::page_area::PageArea) {
    // The second view moves the page instead where the browser's layout could not be patched.
    if !BY_LAYOUT.load(std::sync::atomic::Ordering::Acquire) || !shown_in(area.window) {
        return;
    }
    let row = &crate::builds::get().side_panel;
    let Some(contents) = area.bounds(row.contents_container) else {
        crate::trace::trace_changed(
            "bar layout",
            "address bar: the layout has no page container",
        );
        return;
    };
    // An app window's top container holds the browser's title row, which Rivet's bar replaces. When
    // it sits right on top of the page, the page takes its place and it keeps no height.
    if let Some(toolbar) = area
        .bounds(row.toolbar)
        .filter(|toolbar| *toolbar != contents)
    {
        let between: Vec<_> = area
            .all_bounds()
            .into_iter()
            .map(|(_, b)| b)
            .filter(|b| *b != toolbar && *b != contents)
            .collect();
        let views: Vec<[i32; 4]> = between.iter().map(|b| **b).collect();
        if let Some((top, page, moved)) = page_over_top_row(*toolbar, *contents, &views) {
            *toolbar = top;
            *contents = page;
            for (bounds, view) in between.into_iter().zip(moved) {
                *bounds = view;
            }
        }
    }
    let before = *contents;
    // A bar the browser keeps right above the page, like the infobar of a tab, moves down with it.
    // Left where it was, it sits under the address bar and leaves an empty strip above the page.
    for (_, bounds) in area.all_bounds() {
        if bounds != contents && rides_above_page(*bounds, before) {
            (*bounds)[1] += ROW_HEIGHT;
        }
    }
    let contents = &mut *contents;
    contents[1] += ROW_HEIGHT;
    contents[3] = (contents[3] - ROW_HEIGHT).max(1);
    crate::trace::trace_changed(
        "bar layout",
        &format!("address bar: layout page {before:?} to {contents:?}"),
    );
}

/// The top row, the page and the views that moved with them.
pub(crate) type TopRowMove = ([i32; 4], [i32; 4], Vec<[i32; 4]>);

/// The top row, the page and the other views after the page takes the place of a top row above
/// it. The row keeps its place with no height. Views between the row and the page, such as an
/// infobar, move up with the page, and the page grows up by the row's height.
pub(crate) fn page_over_top_row(
    top: [i32; 4],
    contents: [i32; 4],
    views: &[[i32; 4]],
) -> Option<TopRowMove> {
    let bottom = top[1] + top[3];
    let across = |v: [i32; 4]| v[0] < contents[0] + contents[2] && v[0] + v[2] > contents[0];
    if top[3] <= 0 || bottom > contents[1] || !across(top) {
        return None;
    }
    let between =
        |v: [i32; 4]| v[3] > 0 && v[1] >= bottom && v[1] + v[3] <= contents[1] && across(v);
    // The row must reach the page, directly or through the views between them.
    let covered = views
        .iter()
        .filter(|v| between(**v))
        .map(|v| v[3])
        .sum::<i32>();
    if bottom + covered != contents[1] {
        return None;
    }
    let shift = top[3];
    let moved = views
        .iter()
        .map(|v| {
            if between(*v) {
                [v[0], v[1] - shift, v[2], v[3]]
            } else {
                *v
            }
        })
        .collect();
    let [x, y, width, height] = contents;
    Some((
        [top[0], top[1], top[2], 0],
        [x, y - shift, width, height + shift],
        moved,
    ))
}

/// Whether a view the browser laid out sits right on top of the page and across it, so it moves
/// down with the page below the bar.
pub(crate) fn rides_above_page(view: [i32; 4], contents: [i32; 4]) -> bool {
    let [x, y, width, height] = view;
    height > 0
        && height < contents[3]
        && y + height == contents[1]
        && x < contents[0] + contents[2]
        && x + width > contents[0]
}

/// The active page's title and address. The visible address is what the page shows now, including
/// a navigation that has not committed yet.
unsafe fn page_text(window: HWND) -> (String, String) {
    crate::features::tabs::open_tabs(window)
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.saved.active))
        .map(|tab| (tab.saved.title, tab.saved.url))
        .unwrap_or_default()
}

/// How the page moves below the bar in this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Offset {
    /// The browser's own layout of the page area is patched and leaves the bar's row free.
    Layout,
    /// Rivet's second view keeps the row free by moving the window's page view, which needs no
    /// record of the browser's layout object.
    PageView,
    /// Neither is recorded, so the bar covers the top of the page.
    None(String),
}

/// Picks how the page moves below the bar: the patched layout when it installed, else the page
/// view, which every build that records the second view can move.
pub(crate) fn offset_for(layout: Result<(), String>, second_view: bool) -> Offset {
    match layout {
        Ok(()) => Offset::Layout,
        Err(_) if second_view => Offset::PageView,
        Err(error) => Offset::None(error),
    }
}

/// Set once the patched layout moves the page, so only one mechanism ever leaves the row free.
static BY_LAYOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Patches the browser's layout of the page area once, then asks the window to lay out again, so the
/// page moves below the bar. A build without the layout's record moves the page view instead.
unsafe fn ensure_layout(window: HWND) {
    thread_local! {
        static DONE: RefCell<std::collections::HashSet<isize>> = RefCell::new(std::collections::HashSet::new());
    }
    if !DONE.with(|done| done.borrow_mut().insert(window as isize)) {
        return;
    }
    let installed = crate::hooks::page_area::install()
        .and_then(|()| crate::features::omnibox::install_app_layout());
    let reason = installed.as_ref().err().cloned().unwrap_or_default();
    match offset_for(installed, crate::features::second_view::available()) {
        Offset::Layout => {
            crate::trace::trace("address bar: the browser's layout moves the page below the bar");
            BY_LAYOUT.store(true, std::sync::atomic::Ordering::Release);
            let name: Vec<u16> = crate::features::split::RELAYOUT_MESSAGE
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let message =
                windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(name.as_ptr());
            PostMessageW(window, message, 0, 0);
        }
        Offset::PageView => {
            crate::trace::trace(&format!(
                "address bar: the page view moves below the bar ({reason})"
            ));
            crate::features::second_view::set_top(window, ROW_HEIGHT);
        }
        Offset::None(error) => crate::trace::trace(&format!(
            "address bar: the page cannot move below the bar: {error}"
        )),
    }
}

pub(crate) unsafe fn view(window: HWND) -> Option<View> {
    let [x, y, width] = place(window)?;
    ensure_layout(window);
    let f = factor(window);
    let (title, url) = page_text(window);
    let (title, rest) = shown(&title, &url);
    let toggle = TOGGLE_OVER_BAR;
    let pane = in_pane(window);
    let dev = crate::features::dev_page::window_in_dev_mode(window);
    let reserve = dev_reserve(window);
    let security = Security::from_level(crate::features::omnibox::security_level(window), &url);
    Some(with_bar(window, |bar| {
        // The field tells when its text or selection changed. A change made without a key, such as
        // a select all, counts too. The top bar is painted again only when the field draws anew.
        if bar.field.as_ref().is_some_and(|field| field.take_changed()) {
            bar.generation += 1;
        }
        View {
            dev,
            url: url.clone(),
            reserve: if pane { 0.0 } else { reserve },
            pane,
            x: x * f,
            y: y * f,
            width: width * f,
            height: ROW_HEIGHT as f32 * f,
            toggle,
            title,
            rest,
            security,
            hover: bar.hover,
            can_back: true,
            can_forward: true,
            caption_hover: crate::frame::caption_hover(window),
            zoomed: windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed(window) != 0,
            editing: bar.editing,
            generation: bar.generation,
            suggestions: bar
                .suggestions
                .iter()
                .map(|s| {
                    (
                        s.contents.clone(),
                        s.description.clone(),
                        is_search(&s.contents, &s.description, &s.destination),
                    )
                })
                .collect(),
            selected: bar.selected,
        }
    }))
}

/// The address and its security chip as left, top, width and height in client DIPs, which the
/// browser's bubbles anchor to.
pub(crate) unsafe fn anchors(window: HWND) -> Option<([f32; 4], [f32; 4])> {
    let [x, y, width] = place(window)?;
    let parts = parts_reserving(width, in_pane(window), dev_reserve(window));
    let top = y + (ROW_HEIGHT as f32 - ADDRESS_HEIGHT) / 2.0;
    let find = |part: Part| {
        parts
            .iter()
            .find(|p| p.0 == part)
            .map(|(_, left, w)| [x + left, top, *w, ADDRESS_HEIGHT])
    };
    let address = find(Part::Address)?;
    Some((address, find(Part::Chip).unwrap_or(address)))
}

unsafe fn field_rect(window: HWND) -> Option<RECT> {
    let [x, y, width] = place(window)?;
    let (_, left, w) = parts_reserving(width, in_pane(window), dev_reserve(window))
        .into_iter()
        .find(|p| p.0 == Part::Address)?;
    let f = factor(window);
    let top = y + (ROW_HEIGHT as f32 - ADDRESS_HEIGHT) / 2.0;
    let text_left = x + left + ADDRESS_PAD + CHIP + 4.0;
    Some(RECT {
        left: (text_left * f).round() as i32,
        top: ((top + 6.0) * f).round() as i32,
        right: ((x + left + w - ADDRESS_PAD) * f).round() as i32,
        bottom: ((top + ADDRESS_HEIGHT - 6.0) * f).round() as i32,
    })
}

/// Lets the appbar draw the field's pixels at the field's place, in client pixels.
pub(crate) unsafe fn with_field_pixels<R>(
    window: HWND,
    work: impl FnOnce(RECT, &[u8], i32, i32, Option<RECT>) -> R,
) -> Option<R> {
    let rect = field_rect(window)?;
    with_bar(window, |bar| {
        let field = bar.field.as_mut()?;
        if !bar.editing {
            return None;
        }
        let caret = field.caret();
        let pixels = field.render(FIELD_GROUND)?;
        Some(work(rect, pixels.bytes, pixels.width, pixels.height, caret))
    })
}

unsafe fn part_at(window: HWND, point: POINT) -> Option<Part> {
    let [x, y, width] = place(window)?;
    let f = factor(window);
    let (px, py) = (point.x as f32 / f - x, point.y as f32 / f - y);
    if py < 0.0 || py >= ROW_HEIGHT as f32 {
        return None;
    }
    let parts = parts_reserving(width, in_pane(window), dev_reserve(window));
    // The chip lies inside the address, so it is looked for first.
    parts
        .iter()
        .filter(|p| p.0 != Part::Address)
        .chain(parts.iter().filter(|p| p.0 == Part::Address))
        .find(|(_, left, w)| px >= *left && px < left + w)
        .map(|(part, _, _)| *part)
}

/// Starts editing: the field shows the whole address, selected, as in any browser.
unsafe fn begin(window: HWND, bar: &mut Bar) {
    if bar.field.is_none() {
        crate::appbar::make_fonts_known_to_gdi();
        match Field::new(window, "Manrope", 0x1f2024) {
            Ok(field) => {
                field.set_max_length(MAX_ADDRESS);
                bar.field = Some(field);
            }
            Err(error) => {
                crate::trace::trace(&format!("address bar: {error}"));
                return;
            }
        }
    }
    let Some(rect) = field_rect(window) else {
        return;
    };
    let url = page_text(window).1;
    let field = bar.field.as_mut().unwrap();
    field.place(rect, TEXT_SIZE, crate::frame::layout_dpi(window));
    // The browser turns the input method off while its page has no text field focused. The bar is a
    // text field, so it gets the window's default input context back.
    ImmAssociateContextEx(window, std::ptr::null_mut(), IACE_DEFAULT);
    field.activate();
    field.set_text(&url);
    field.select_all();
    // The page's own address asks for no suggestions until it is changed.
    bar.queried = url.clone();
    bar.original = url;
    bar.editing = true;
    bar.generation += 1;
}

unsafe fn end(bar: &mut Bar) {
    if let Some(field) = bar.field.as_ref() {
        field.send(0x0008, 0, 0); // WM_KILLFOCUS
    }
    bar.editing = false;
    bar.suggestions.clear();
    bar.selected = None;
    bar.queried.clear();
    bar.inline.clear();
    bar.deleting = false;
    bar.generation += 1;
}

/// Opens what was typed: an address directly, anything else through the browser's search engine.
unsafe fn go(window: HWND, text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    // The browser's classifier decides, with its default search engine. A build without it still
    // opens plain addresses, and a search there does nothing rather than guess an engine.
    let opened = match crate::features::omnibox::open_typed(window, text) {
        Some(opened) => opened,
        None if looks_like_address(text) => {
            crate::features::navigation::open(window, &address_for(text))
        }
        None => false,
    };
    crate::trace::trace(&format!("address bar: open {text:?}: {opened}"));
    opened
}

/// Whether a suggestion searches for the text rather than opening a page. A page's row, whether
/// typed or from history, opens an address its contents name, and the browser describes a search
/// row, so a described row whose destination is not the address it shows is a search.
pub(crate) fn is_search(contents: &str, description: &str, destination: &str) -> bool {
    fn bare(address: &str) -> String {
        let lower = address.trim().to_lowercase();
        let rest = lower
            .split_once("://")
            .map_or(lower.as_str(), |(_, rest)| rest);
        rest.strip_prefix("www.").unwrap_or(rest).to_string()
    }
    !description.is_empty() && !bare(destination).starts_with(&bare(contents))
}

/// The height of one suggestion row and the room around the list, in DIPs.
pub(crate) const SUGGESTION_ROW: f32 = 32.0;
pub(crate) const SUGGESTION_PAD: f32 = 6.0;
/// The most suggestions shown, as the browser's own dropdown.
const SUGGESTIONS_SHOWN: usize = 8;

/// The list of suggestions as left and width from the bar's left edge: under the address, of a
/// single page's row or of a pane's address alone.
pub(crate) fn dropdown_of(width: f32, pane: bool) -> Option<(f32, f32)> {
    parts_of(width, pane)
        .into_iter()
        .find(|p| p.0 == Part::Address)
        .map(|(_, x, w)| (x, w))
}

/// The suggestion row under a point in client pixels.
unsafe fn row_at(window: HWND, point: POINT) -> Option<usize> {
    let [x, y, width] = place(window)?;
    let (left, w) = dropdown_of(width, in_pane(window))?;
    let f = factor(window);
    let (px, py) = (
        point.x as f32 / f - x,
        point.y as f32 / f - y - ROW_HEIGHT as f32 - SUGGESTION_PAD,
    );
    let count = with_bar(window, |bar| {
        if bar.editing {
            bar.suggestions.len()
        } else {
            0
        }
    });
    if px < left || px >= left + w || py < 0.0 {
        return None;
    }
    let index = (py / SUGGESTION_ROW) as usize;
    (index < count).then_some(index)
}

const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;

/// The field's selection as start and end in UTF-16 units.
fn selection(field: &Field) -> (usize, usize) {
    let (mut start, mut end) = (0u32, 0u32);
    field.send(
        EM_GETSEL,
        &mut start as *mut u32 as usize,
        &mut end as *mut u32 as isize,
    );
    (start as usize, end as usize)
}

/// Whether the input method holds an open composition in the window.
unsafe fn composing(window: HWND) -> bool {
    use windows_sys::Win32::UI::Input::Ime::{
        ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext, GCS_COMPSTR,
    };
    let context = ImmGetContext(window);
    if context as isize == 0 {
        return false;
    }
    let length = ImmGetCompositionStringW(context, GCS_COMPSTR, std::ptr::null_mut(), 0);
    ImmReleaseContext(window, context);
    length > 0
}

/// Whether the browser may complete `text` inline: the caret sits at its end with nothing
/// selected and the last key added text, as in the browser's own address bar.
pub(crate) fn may_complete_inline(text: &str, selection: (usize, usize), deleting: bool) -> bool {
    let length = text.encode_utf16().count();
    !deleting && selection == (length, length)
}

/// The field's text with the browser's completion after the typed text, and the completion's
/// start and end in UTF-16 units, selected so the next key replaces it.
pub(crate) fn completed(typed: &str, inline: &str) -> (String, usize, usize) {
    let start = typed.encode_utf16().count();
    (
        format!("{typed}{inline}"),
        start,
        start + inline.encode_utf16().count(),
    )
}

/// Asks the browser for suggestions when the text changed since it last asked. The browser answers
/// at once, so nothing waits or polls: the list is drawn again only when the text changed.
unsafe fn refresh_suggestions(window: HWND) {
    let Some((text, inline)) = with_bar(window, |bar| {
        let field = bar.field.as_ref()?;
        let text = field.text();
        // Deleting the selected completion leaves the typed text, which needs no new query.
        if text == bar.queried && !bar.inline.is_empty() {
            bar.inline.clear();
            bar.generation += 1;
        }
        if !bar.editing
            || text == bar.queried
            || (!bar.inline.is_empty() && text == format!("{}{}", bar.queried, bar.inline))
        {
            return None;
        }
        Some((
            text.clone(),
            may_complete_inline(&text, selection(field), bar.deleting),
        ))
    }) else {
        return;
    };
    let inline = inline && !composing(window);
    let found = if text.trim().is_empty() {
        Some(Vec::new())
    } else {
        crate::features::omnibox::suggest_with(window, &text, inline)
    };
    let shown = with_bar(window, |bar| {
        bar.suggestions = found.unwrap_or_default();
        bar.suggestions.truncate(SUGGESTIONS_SHOWN);
        bar.selected = None;
        bar.inline = bar
            .suggestions
            .first()
            .map(|first| first.inline.clone())
            .unwrap_or_default();
        if !bar.inline.is_empty() {
            if let Some(field) = bar.field.as_ref() {
                let (full, start, end) = completed(&text, &bar.inline);
                field.set_text(&full);
                field.send(EM_SETSEL, start, end as isize);
                field.take_changed();
            }
        }
        bar.queried = text.clone();
        bar.generation += 1;
        let rows: Vec<&str> = bar
            .suggestions
            .iter()
            .map(|s| s.destination.as_str())
            .collect();
        format!("{rows:?} inline {:?}", bar.inline)
    });
    crate::trace::trace(&format!("address bar: suggestions for {text:?}: {shown}"));
}

/// Opens a suggestion as the browser's dropdown does.
unsafe fn open_suggestion(window: HWND, index: usize) -> bool {
    let Some(destination) = with_bar(window, |bar| {
        let destination = bar.suggestions.get(index).map(|s| s.destination.clone());
        end(bar);
        destination
    }) else {
        return false;
    };
    let opened = crate::features::navigation::open(window, &destination);
    crate::trace::trace(&format!(
        "address bar: open suggestion {index} {destination:?}: {opened}"
    ));
    crate::frame::update_appbar(window);
    opened
}

fn point_of(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    }
}

const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
const WM_IME_COMPOSITION: u32 = 0x010F;
const WM_IME_SETCONTEXT: u32 = 0x0281;
const WM_IME_NOTIFY: u32 = 0x0282;
const WM_IME_CHAR: u32 = 0x0286;
const WM_IME_REQUEST: u32 = 0x0288;
const WM_INPUTLANGCHANGE: u32 = 0x0051;
const WM_APPCOMMAND: u32 = 0x0319;

/// Handles a message of the browser window for the bar. `Some` means the bar used it and the browser
/// does not see it.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    // The text engine and the input method send the window messages while the bar is busy with one,
    // such as a change of capture. Those are the browser's to handle.
    if !shown_in(window) || busy() {
        return None;
    }
    let editing = with_bar(window, |bar| bar.editing);
    let result = match message {
        WM_MOUSEMOVE if editing && row_at(window, point_of(lparam)).is_some() => {
            let row = row_at(window, point_of(lparam));
            let changed = with_bar(window, |bar| {
                std::mem::replace(&mut bar.selected, row) != row
            });
            if changed {
                with_bar(window, |bar| bar.generation += 1);
                crate::frame::update_appbar(window);
            }
            Some(0)
        }
        WM_LBUTTONDOWN if editing && row_at(window, point_of(lparam)).is_some() => {
            if let Some(row) = row_at(window, point_of(lparam)) {
                open_suggestion(window, row);
            }
            Some(0)
        }
        WM_MOUSEMOVE => {
            let point = point_of(lparam);
            let now = part_at(window, point);
            let (hover_changed, content_changed) = with_bar(window, |bar| {
                let changed = bar.hover != now;
                bar.hover = now;
                let mut drawn = false;
                if bar.editing && ((wparam & 1) != 0 || now == Some(Part::Address)) {
                    if let Some(field) = bar.field.as_ref() {
                        field.send(message, wparam, lparam);
                        drawn = field.take_changed();
                    }
                }
                if drawn {
                    bar.generation += 1;
                }
                (changed, drawn)
            });
            redraw(
                window,
                redraw_after(editing, hover_changed, content_changed),
            );
            if now.is_some() || (editing && (wparam & 1) != 0) {
                Some(0)
            } else {
                None
            }
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let part = part_at(window, point_of(lparam));
            match part {
                Some(Part::Address) => {
                    with_bar(window, |bar| {
                        if bar.editing {
                            if let Some(field) = bar.field.as_ref() {
                                field.send(message, wparam, lparam);
                            }
                        } else {
                            begin(window, bar);
                        }
                        bar.generation += 1;
                    });
                    crate::frame::update_appbar(window);
                    Some(0)
                }
                Some(other) => {
                    with_bar(window, |bar| bar.press = Some(other));
                    Some(0)
                }
                None => {
                    // A press anywhere else leaves the address as the page shows it.
                    if editing {
                        with_bar(window, |bar| end(bar));
                        crate::frame::update_appbar(window);
                    }
                    None
                }
            }
        }
        WM_LBUTTONUP => {
            let part = part_at(window, point_of(lparam));
            let pressed = with_bar(window, |bar| bar.press.take());
            if let Some(pressed) = pressed {
                if Some(pressed) == part {
                    act(window, pressed);
                }
                Some(0)
            } else if editing {
                with_bar(window, |bar| {
                    if let Some(field) = bar.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                    bar.generation += 1;
                });
                crate::frame::update_appbar(window);
                Some(0)
            } else {
                None
            }
        }
        _ if !editing => None,
        WM_KEYDOWN => match wparam as u16 {
            // Enter opens the picked suggestion, or the default one while its completion shows.
            0x0D if with_bar(window, |bar| {
                bar.selected.is_some() || (!bar.inline.is_empty() && !bar.suggestions.is_empty())
            }) =>
            {
                let index = with_bar(window, |bar| bar.selected.unwrap_or(0));
                open_suggestion(window, index);
                Some(0)
            }
            // Up and Down walk the suggestions, as in the browser's own dropdown.
            0x26 | 0x28 => {
                let down = wparam as u16 == 0x28;
                with_bar(window, |bar| {
                    let count = bar.suggestions.len();
                    if count > 0 {
                        let at = match (bar.selected, down) {
                            (None, true) => 0,
                            (None, false) => count - 1,
                            (Some(at), true) => (at + 1) % count,
                            (Some(at), false) => (at + count - 1) % count,
                        };
                        bar.selected = Some(at);
                        // The field shows the picked match as the browser's dropdown does, and
                        // typing goes on from it.
                        let fill = bar.suggestions[at].fill.clone();
                        if let (false, Some(field)) = (fill.is_empty(), bar.field.as_ref()) {
                            field.set_text(&fill);
                            let end = fill.encode_utf16().count();
                            field.send(EM_SETSEL, end, end as isize);
                            field.take_changed();
                            bar.queried = fill;
                            bar.inline.clear();
                        }
                        bar.generation += 1;
                    }
                });
                crate::frame::update_appbar(window);
                Some(0)
            }
            0x0D => {
                let text = with_bar(window, |bar| {
                    let text = bar
                        .field
                        .as_ref()
                        .map(|field| field.text())
                        .unwrap_or_default();
                    end(bar);
                    text
                });
                go(window, &text);
                crate::frame::update_appbar(window);
                Some(0)
            }
            0x1B => {
                // Escape first closes the suggestions and drops the inline completion, then puts
                // the page's address back, and a last one leaves the field.
                with_bar(window, |bar| {
                    if !bar.suggestions.is_empty() {
                        if let (false, Some(field)) = (bar.inline.is_empty(), bar.field.as_ref()) {
                            field.set_text(&bar.queried);
                            let end = bar.queried.encode_utf16().count();
                            field.send(EM_SETSEL, end, end as isize);
                        }
                        bar.inline.clear();
                        bar.suggestions.clear();
                        bar.selected = None;
                        bar.generation += 1;
                        return;
                    }
                    let field = bar.field.as_ref();
                    let now = field.map(|field| field.text()).unwrap_or_default();
                    if now != bar.original {
                        if let Some(field) = field {
                            field.set_text(&bar.original);
                            field.select_all();
                        }
                        bar.generation += 1;
                    } else {
                        end(bar);
                    }
                });
                crate::frame::update_appbar(window);
                Some(0)
            }
            _ => {
                // Ctrl+L and F6 belong to the browser's own address bar, which an app window lacks.
                if wparam as u16 == 0x4C && GetKeyState(0x11) < 0 {
                    with_bar(window, |bar| {
                        bar.field.as_ref().map(|field| field.select_all())
                    });
                    crate::frame::update_appbar(window);
                    return Some(0);
                }
                with_bar(window, |bar| {
                    bar.deleting = matches!(wparam as u16, 0x08 | 0x2E)
                });
                let used = forward(window, message, wparam, lparam);
                refresh_suggestions(window);
                crate::frame::update_appbar(window);
                used.or(Some(0))
            }
        },
        WM_CHAR => {
            if matches!(wparam, 0x0d | 0x1b | 0x09 | 0x0a) {
                return Some(0);
            }
            with_bar(window, |bar| bar.deleting = wparam == 0x08);
            let used = forward(window, message, wparam, lparam);
            refresh_suggestions(window);
            crate::frame::update_appbar(window);
            used.or(Some(0))
        }
        WM_KEYUP
        | WM_IME_STARTCOMPOSITION
        | WM_IME_COMPOSITION
        | WM_IME_ENDCOMPOSITION
        | WM_IME_NOTIFY
        | WM_IME_CHAR
        | WM_IME_REQUEST
        | WM_INPUTLANGCHANGE => {
            let used = forward(window, message, wparam, lparam);
            if matches!(message, WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION) {
                refresh_suggestions(window);
            }
            crate::frame::update_appbar(window);
            if message == WM_INPUTLANGCHANGE {
                None
            } else {
                used
            }
        }
        WM_IME_SETCONTEXT => {
            forward(window, message, wparam, lparam);
            None
        }
        WM_TIMER if (TIMER_BASE..=TIMER_LAST).contains(&wparam) => {
            // Most of the field's timers blink the caret, which moves only the caret's own visual.
            let before = with_bar(window, |bar| bar.generation);
            forward(window, message, wparam - TIMER_BASE, 0);
            let after = with_bar(window, |bar| bar.generation);
            redraw(window, redraw_after(editing, false, before != after));
            Some(0)
        }
        WM_KILLFOCUS => {
            forward(window, message, wparam, lparam);
            crate::frame::update_appbar(window);
            None
        }
        WM_SETFOCUS => {
            forward(window, message, wparam, lparam);
            crate::frame::update_appbar(window);
            None
        }
        _ => None,
    };
    result
}

unsafe fn forward(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let (used, caret) = with_bar(window, |bar| {
        let field = bar.field.as_ref()?;
        let used = field.send(message, wparam, lparam);
        if field.take_changed() {
            bar.generation += 1;
        }
        Some((used, field.caret()))
    })?;
    // Placing the windows makes the input method send notices of its own, which need no placing.
    if let Some(caret) =
        caret.filter(|_| !matches!(message, WM_IME_NOTIFY | WM_IME_SETCONTEXT | WM_IME_REQUEST))
    {
        place_input_method(window, caret);
    }
    used
}

/// Puts the input method's composition and candidate windows at the caret, below the bar's text,
/// as the browser does for a text field of a page.
unsafe fn place_input_method(window: HWND, caret: RECT) {
    use windows_sys::Win32::UI::Input::Ime::*;
    let context = ImmGetContext(window);
    if context.is_null() {
        return;
    }
    let mut composition: COMPOSITIONFORM = std::mem::zeroed();
    composition.dwStyle = CFS_POINT;
    composition.ptCurrentPos = POINT {
        x: caret.left,
        y: caret.top,
    };
    ImmSetCompositionWindow(context, &composition);
    // The candidate list opens below the caret and never covers the text being composed.
    let mut candidate: CANDIDATEFORM = std::mem::zeroed();
    candidate.dwIndex = 0;
    candidate.dwStyle = CFS_EXCLUDE;
    candidate.ptCurrentPos = POINT {
        x: caret.left,
        y: caret.bottom,
    };
    candidate.rcArea = caret;
    ImmSetCandidateWindow(context, &candidate);
    ImmReleaseContext(window, context);
}

unsafe fn act(window: HWND, part: Part) {
    let command: isize = match part {
        Part::Back => 1,
        Part::Forward => 2,
        Part::Reload => 3,
        Part::Chip => {
            let shown = crate::features::omnibox::page_info(window);
            crate::trace::trace(&format!("address bar: page info: {shown}"));
            return;
        }
        Part::Address => return,
    };
    // The browser's own commands, as the keyboard's browser keys send them.
    PostMessageW(window, WM_APPCOMMAND, window as usize, command << 16);
}

/// Starts editing as a click on the address would, for tests that may not move the pointer.
pub(crate) unsafe fn focus(window: HWND) -> bool {
    if !shown_in(window) {
        return false;
    }
    let done = with_bar(window, |bar| {
        begin(window, bar);
        bar.editing
    });
    crate::frame::update_appbar(window);
    done
}

/// The text of the field, for tests.
pub(crate) fn text(window: HWND) -> Option<String> {
    with_bar(window, |bar| bar.field.as_ref().map(|field| field.text()))
}

/// Forgets a closed window.
pub(crate) fn forget(window: HWND) {
    BARS.with(|bars| bars.borrow_mut().remove(&(window as isize)));
}

/// Types `text` as a composition of the Korean input method, as a person typing on its keyboard
/// would, and traces where the input method placed its windows. Tests cannot press keys without
/// taking the focus, so the input method is asked for the composition directly.
pub(crate) unsafe fn compose_for_test(window: HWND, text: &str) -> bool {
    use windows_sys::Win32::UI::Input::Ime::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        ActivateKeyboardLayout, LoadKeyboardLayoutW, KLF_ACTIVATE,
    };
    let layout: Vec<u16> = "00000412\0".encode_utf16().collect();
    let korean = LoadKeyboardLayoutW(layout.as_ptr(), KLF_ACTIVATE);
    ActivateKeyboardLayout(korean, 0);
    let context = ImmGetContext(window);
    let open = ImmSetOpenStatus(context, 1);
    ImmSetConversionStatus(context, IME_CMODE_NATIVE, IME_SMODE_NONE);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let set = ImmSetCompositionStringW(
        context,
        SCS_SETSTR,
        wide.as_ptr().cast(),
        (wide.len() * 2) as u32,
        std::ptr::null(),
        0,
    );
    let mut form: COMPOSITIONFORM = std::mem::zeroed();
    let placed = ImmGetCompositionWindow(context, &mut form);
    let mut candidate: CANDIDATEFORM = std::mem::zeroed();
    let candidate_placed = ImmGetCandidateWindow(context, 0, &mut candidate);
    crate::trace::trace(&format!(
        "address bar ime: layout {korean:?} context {context:?} open {open} set {set} composition window {placed} at {},{} style {:#x}, candidate {candidate_placed} at {},{} style {:#x}, caret {:?}",
        form.ptCurrentPos.x, form.ptCurrentPos.y, form.dwStyle,
        candidate.ptCurrentPos.x, candidate.ptCurrentPos.y, candidate.dwStyle,
        with_bar(window, |bar| bar.field.as_ref().and_then(|field| field.caret())).map(|caret| (caret.left, caret.top, caret.bottom)),
    ));
    ImmReleaseContext(window, context);
    crate::frame::update_appbar(window);
    set != 0
}

/// Ends a composition the way the input method does when a person presses Enter on it.
pub(crate) unsafe fn finish_composition_for_test(window: HWND) -> bool {
    use windows_sys::Win32::UI::Input::Ime::*;
    let context = ImmGetContext(window);
    let done = ImmNotifyIME(context, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
    ImmReleaseContext(window, context);
    crate::frame::update_appbar(window);
    done != 0
}

/// The text engine message an edit command of a test stands for: the one the engine runs for the
/// keyboard shortcut. A test cannot hold Ctrl down without taking the focus.
pub(crate) fn edit_message(word: &str) -> Option<(u32, usize, isize)> {
    match word {
        "cut" => Some((0x0300, 0, 0)),
        "copy" => Some((0x0301, 0, 0)),
        "paste" => Some((0x0302, 0, 0)),
        "undo" => Some((0x00C7, 0, 0)),
        "selectall" => Some((0x00B1, 0, -1)),
        _ => None,
    }
}

/// Runs an edit command on the field being edited, for tests, and traces its text afterwards.
pub(crate) unsafe fn edit_for_test(window: HWND, word: &str) -> bool {
    let Some((message, wparam, lparam)) = edit_message(word) else {
        return false;
    };
    let editing = with_bar(window, |bar| bar.editing && bar.field.is_some());
    if !editing {
        return false;
    }
    let used = forward(window, message, wparam, lparam).is_some();
    crate::trace::trace(&format!(
        "address bar edit {word}: {used} text {:?}",
        text(window)
    ));
    crate::frame::update_appbar(window);
    used
}

/// What a message to the bar needs drawn again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Redraw {
    Nothing,
    /// Only the caret moved or blinked: its own small visual is placed or hidden.
    Caret,
    /// The bar looks different: hover, text or selection.
    Bar,
}

fn redraw_after(editing: bool, hover_changed: bool, content_changed: bool) -> Redraw {
    if hover_changed || content_changed {
        Redraw::Bar
    } else if editing {
        Redraw::Caret
    } else {
        Redraw::Nothing
    }
}

unsafe fn redraw(window: HWND, redraw: Redraw) {
    match redraw {
        Redraw::Nothing => {}
        Redraw::Caret => crate::frame::update_address_caret(window),
        Redraw::Bar => crate::frame::update_appbar(window),
    }
}

/// The field's caret in client pixels while the address is edited and the caret is shown.
pub(crate) fn caret(window: HWND) -> Option<RECT> {
    if busy() {
        return None;
    }
    with_bar(window, |bar| {
        bar.editing
            .then(|| bar.field.as_ref().and_then(|field| field.caret()))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_edit_commands_are_the_text_engines_own_clipboard_and_undo_messages() {
        assert_eq!(edit_message("copy"), Some((0x0301, 0, 0)));
        assert_eq!(edit_message("cut"), Some((0x0300, 0, 0)));
        assert_eq!(edit_message("paste"), Some((0x0302, 0, 0)));
        assert_eq!(edit_message("undo"), Some((0x00C7, 0, 0)));
        assert_eq!(edit_message("selectall"), Some((0x00B1, 0, -1)));
        assert_eq!(edit_message("bold"), None);
    }

    #[test]
    fn fr12_inline_completion_is_asked_only_while_typing_at_the_end() {
        assert!(may_complete_inline("exa", (3, 3), false));
        assert!(!may_complete_inline("exa", (3, 3), true));
        assert!(!may_complete_inline("exa", (1, 1), false));
        assert!(!may_complete_inline("exa", (0, 3), false));
        // Positions are UTF-16 units, as the text engine counts them.
        assert!(may_complete_inline("검색", (2, 2), false));
    }

    #[test]
    fn fr12_a_search_suggestion_shows_the_magnifier_and_a_page_the_globe() {
        // What was typed, as the browser offers it first: no description.
        assert!(!is_search(
            "http://127.0.0.1:8080/suggest-a.",
            "",
            "http://127.0.0.1:8080/suggest-a."
        ));
        // A page from history: its title describes it and it opens the address it shows.
        assert!(!is_search(
            "127.0.0.1:8080/alpha.html",
            "alpha",
            "http://127.0.0.1:8080/alpha.html"
        ));
        assert!(!is_search(
            "example.com",
            "Example",
            "https://www.example.com/"
        ));
        // A search on the default engine.
        assert!(is_search(
            "http://127.0.0.1:8080/suggest-a.",
            "Google Search",
            "https://www.google.com/search?q=http%3A%2F%2F127.0.0.1"
        ));
        assert!(is_search(
            "rivet browser",
            "Bing",
            "https://www.bing.com/search?q=rivet+browser"
        ));
    }

    #[test]
    fn fr12_inline_completion_is_selected_after_the_typed_text() {
        assert_eq!(
            completed("exa", "mple.com"),
            ("example.com".to_string(), 3, 11)
        );
        assert_eq!(completed("검", "색어"), ("검색어".to_string(), 1, 3));
    }

    #[test]
    fn idle_caret_blink_redraws_only_the_caret() {
        assert_eq!(redraw_after(true, false, false), Redraw::Caret);
        assert_eq!(redraw_after(true, false, true), Redraw::Bar);
        assert_eq!(redraw_after(true, true, false), Redraw::Bar);
    }

    #[test]
    fn idle_pointer_moving_over_a_resting_bar_redraws_nothing() {
        assert_eq!(redraw_after(false, false, false), Redraw::Nothing);
        assert_eq!(redraw_after(false, true, false), Redraw::Bar);
    }

    #[test]
    fn fr12_a_bar_right_above_the_page_moves_below_the_address_bar_with_it() {
        // Brave's first tab shows an infobar right above the page. Only the page moved, so the
        // infobar sat under the address bar and an empty band of the bar's height was left above
        // the page, which put the first pane of a split below the second.
        let contents = [0, 56, 1164, 708];
        assert!(rides_above_page([0, 0, 1164, 56], contents));
        // The page itself, a view beside it and a view that ends above it stay.
        assert!(!rides_above_page(contents, contents));
        assert!(!rides_above_page([1164, 0, 40, 56], contents));
        assert!(!rides_above_page([0, 0, 1164, 40], contents));
        assert!(!rides_above_page([0, 56, 1164, 0], contents));
    }

    #[test]
    fn fr12_page_sits_directly_below_the_bar_where_the_browser_kept_a_title_row() {
        // Chrome's app window keeps a 23 DIP top container above the page once its client view
        // reaches the window's top edge. Rivet's bar replaces it, so the page takes its place.
        let (top, page, _) = page_over_top_row([0, 0, 1164, 23], [0, 23, 1164, 815], &[]).unwrap();
        assert_eq!(top, [0, 0, 1164, 0]);
        assert_eq!(page, [0, 0, 1164, 838]);
        // A top row that does not reach the page is left alone.
        assert_eq!(
            page_over_top_row([0, 0, 1164, 20], [0, 23, 1164, 815], &[]),
            None
        );
        // Brave's notice bar between the row and the page moves up with the page.
        let (_, page, moved) = page_over_top_row(
            [0, 0, 1164, 23],
            [0, 79, 1164, 759],
            &[[0, 23, 1164, 56], [1164, 0, 40, 9]],
        )
        .unwrap();
        assert_eq!(page, [0, 56, 1164, 782]);
        assert_eq!(moved, vec![[0, 0, 1164, 56], [1164, 0, 40, 9]]);
    }

    #[test]
    fn fr12_the_page_moves_below_the_bar_without_a_sidebar_layout_row() {
        // Edge 156.0.4314.58 and Whale 5.39.412.57 record no Sidebar layout, but both record the
        // second view, which moves the page view itself.
        let missing =
            || Err("this browser build has no recorded layout for the Sidebar".to_string());
        assert_eq!(offset_for(missing(), true), Offset::PageView);
        assert_eq!(
            offset_for(Ok(()), true),
            Offset::Layout,
            "a patched layout is used first"
        );
        assert_eq!(offset_for(Ok(()), false), Offset::Layout);
        assert!(matches!(offset_for(missing(), false), Offset::None(_)));
    }

    #[test]
    fn fr12_bar_has_back_forward_reload_and_an_address_left_of_the_caption_buttons() {
        let parts = parts(1200.0, true);
        let names: Vec<Part> = parts.iter().map(|p| p.0).collect();
        assert_eq!(
            names,
            [
                Part::Back,
                Part::Forward,
                Part::Reload,
                Part::Chip,
                Part::Address
            ]
        );
        let (_, x, w) = parts[4];
        assert!(x + w <= 1200.0 - CAPTION_ROOM);
        assert_eq!(parts[0].1, LEAD + TOGGLE_ROOM + BUTTON_GAP);
    }

    #[test]
    fn fr12_in_a_split_the_field_is_the_focused_panes_address_alone() {
        let parts = super::parts_of(300.0, true);
        let names: Vec<Part> = parts.iter().map(|part| part.0).collect();
        assert_eq!(
            names,
            [Part::Chip, Part::Address],
            "the pane's row draws back, forward and reload"
        );
        assert_eq!((parts[1].1, parts[1].2), (0.0, 300.0));
        assert_eq!(super::dropdown_of(300.0, true), Some((0.0, 300.0)));
        assert_eq!(super::parts_of(1000.0, false), super::parts(1000.0, true));
    }

    #[test]
    fn fr12_toggle_is_inline_before_back_forward_reload() {
        // The study: padding 8, then toggle, back, forward and reload 28 wide with 2 between.
        let parts = parts(1000.0, TOGGLE_OVER_BAR);
        let xs: Vec<f32> = parts.iter().take(3).map(|p| p.1).collect();
        assert_eq!(xs, [38.0, 68.0, 98.0]);
        // The toggle at 8..36 ends before back begins.
        assert!(8.0 + TOGGLE_ROOM < parts[0].1);
    }

    #[test]
    fn fr12_address_ends_where_the_studys_caption_margin_begins() {
        let (_, x, w) = *parts(1000.0, true)
            .iter()
            .find(|p| p.0 == Part::Address)
            .unwrap();
        // 4 DIP `.where` margin, 4 DIP `.caption` margin, then three 46 DIP buttons.
        assert_eq!(x + w, 1000.0 - 4.0 - 4.0 - 138.0);
    }

    #[test]
    fn fr11_developer_buttons_sit_between_the_address_and_the_caption_buttons() {
        let plain = super::parts_of(1000.0, false);
        let dev = super::parts_reserving(1000.0, false, 216.0);
        let address = |parts: &[(Part, f32, f32)]| {
            parts
                .iter()
                .find(|p| p.0 == Part::Address)
                .map(|p| p.1 + p.2)
        };
        assert_eq!(address(&dev).unwrap() + 216.0, address(&plain).unwrap());
        // Too narrow for both: the buttons keep their room and the address goes.
        let narrow = super::parts_reserving(330.0, false, 216.0);
        assert!(narrow
            .iter()
            .all(|p| p.0 != Part::Address && p.0 != Part::Chip));
        // A split pane's address carries no developer buttons.
        assert_eq!(
            super::parts_reserving(300.0, true, 216.0),
            super::parts_of(300.0, true)
        );
    }

    #[test]
    fn fr12_caption_buttons_fill_the_bar_right_end() {
        let buttons = caption_buttons(1000.0);
        assert_eq!(
            buttons.map(|b| b.0),
            [Caption::Minimize, Caption::Maximize, Caption::Close]
        );
        assert_eq!(buttons[0].1, 1000.0 - 138.0);
        assert_eq!(buttons[2].1 + buttons[2].2, 1000.0);
        assert_eq!(CAPTION_BUTTON * 3.0, CAPTION_ROOM);
        assert_eq!(caption_glyph(Caption::Maximize, false), '\u{E922}');
        assert_eq!(caption_glyph(Caption::Maximize, true), '\u{E923}');
    }

    #[test]
    fn fr12_start_page_shows_new_tab_and_rivet_start_in_the_bar() {
        let url = "file:///C:/Rivet%20files/start.html?space=Work&browser=Chrome";
        assert!(is_start_page(url));
        assert_eq!(
            shown("start", url),
            ("New tab".into(), " \u{b7} rivet://start".into())
        );
        assert!(!is_start_page("https://example.com/start.html"));
        assert!(!is_start_page("file:///D:/pages/page-top.html"));
    }

    #[test]
    fn fr12_narrow_bar_keeps_its_buttons_and_drops_the_address() {
        let names: Vec<Part> = parts(260.0, false).iter().map(|p| p.0).collect();
        assert_eq!(names, [Part::Back, Part::Forward, Part::Reload]);
    }

    #[test]
    fn fr12_address_shows_title_then_address_without_its_scheme() {
        assert_eq!(
            shown("Docs", "https://docs.example/a"),
            ("Docs".into(), " \u{b7} docs.example/a".into())
        );
        assert_eq!(
            shown("", "https://example.test/"),
            (String::new(), "example.test".into())
        );
        assert_eq!(
            shown("example.test", "https://example.test/"),
            (String::new(), "example.test".into())
        );
    }

    #[test]
    fn fr12_security_chip_follows_the_connection() {
        assert_eq!(Security::of("https://a.test/"), Security::Secure);
        assert_eq!(Security::of("http://a.test/"), Security::NotSecure);
        assert_eq!(Security::of("http://localhost:8080/"), Security::Local);
        assert_eq!(Security::of("file:///C:/a.html"), Security::Local);
        assert_eq!(Security::of("chrome://settings"), Security::Internal);
        assert_eq!(Security::NotSecure.look().1, "Not secure");
    }

    #[test]
    fn fr12_security_chip_takes_chromiums_level_over_the_address() {
        assert_eq!(
            Security::from_level(Some(3), "https://a.test/"),
            Security::Secure
        );
        // A certificate error leaves an https address dangerous.
        assert_eq!(
            Security::from_level(Some(5), "https://a.test/"),
            Security::NotSecure
        );
        // While an https page loads Chromium has no level yet; the chip must not say Not secure.
        assert_eq!(
            Security::from_level(Some(0), "https://a.test/"),
            Security::Internal
        );
        assert_eq!(
            Security::from_level(Some(0), "http://a.test/"),
            Security::NotSecure
        );
        assert_eq!(
            Security::from_level(Some(0), "file:///C:/a.html"),
            Security::Local
        );
        assert_eq!(
            Security::from_level(None, "http://a.test/"),
            Security::NotSecure
        );
    }

    #[test]
    fn fr12_typed_text_is_an_address_only_when_it_plainly_is_one() {
        for address in [
            "example.test",
            "localhost:3000",
            "a.b:8080/x",
            "file:///C:/x",
        ] {
            assert!(looks_like_address(address), "{address}");
        }
        for search in [
            "hello world",
            "rust",
            "한국어 검색",
            ".hidden",
            "what is a.b?",
        ] {
            assert!(!looks_like_address(search), "{search}");
        }
        assert_eq!(address_for("example.test"), "https://example.test");
        assert_eq!(address_for("localhost:3000"), "http://localhost:3000");
        assert_eq!(address_for("file:///C:/x"), "file:///C:/x");
    }
}
