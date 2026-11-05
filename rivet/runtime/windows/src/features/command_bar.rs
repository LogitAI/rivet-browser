//! The command bar: one field over the page for an address, a search, an open tab or a Rivet
//! command.
//!
//! Ctrl+T opens it when the setting says so, and the start page can ask for it. It is drawn on the
//! window's composition surface (see `appbar`), above a scrim over the page, and takes its input
//! from the window's own messages. The text goes into the Windows text engine
//! (`rivet_platform_windows::textservices`), which does editing, the clipboard and the input method,
//! Korean included. Results come from the browser's own autocomplete, the window's open tabs and
//! Rivet's commands. What is chosen opens in a new tab of the window, which is the current space.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use rivet_platform_windows::textservices::{Field, TIMER_BASE, TIMER_LAST};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::Ime::{ImmAssociateContextEx, IACE_DEFAULT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

/// What Ctrl+T opens in a Rivet window: the tab list's setting.
pub(crate) use crate::tab_sections::CtrlT;

/// Where the setting is read and written. The settings store installs these; until it does, the
/// choice lives in this process only and starts on the command bar, as the study shows it.
static READ_SETTING: OnceLock<fn() -> CtrlT> = OnceLock::new();
static WRITE_SETTING: OnceLock<fn(CtrlT)> = OnceLock::new();
static LOCAL_SETTING: AtomicU8 = AtomicU8::new(0);

/// Lets the settings store answer and keep what Ctrl+T opens.
pub(crate) fn set_setting_source(read: fn() -> CtrlT, write: fn(CtrlT)) {
    let _ = READ_SETTING.set(read);
    let _ = WRITE_SETTING.set(write);
}

pub(crate) fn ctrl_t_opens() -> CtrlT {
    match READ_SETTING.get() {
        Some(read) => read(),
        None if LOCAL_SETTING.load(Ordering::Relaxed) == 1 => CtrlT::NewTabPage,
        None => CtrlT::CommandBar,
    }
}

pub(crate) fn set_ctrl_t_opens(choice: CtrlT) {
    match WRITE_SETTING.get() {
        Some(write) => write(choice),
        None => LOCAL_SETTING.store((choice == CtrlT::NewTabPage) as u8, Ordering::Relaxed),
    }
}

/// Whether a key press is Ctrl+T alone, without Shift or Alt. Ctrl+Shift+T reopens a closed tab.
pub(crate) fn is_ctrl_t(key: u16, ctrl: bool, shift: bool, alt: bool) -> bool {
    key == 0x54 && ctrl && !shift && !alt
}

/// A Rivet command the bar can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    SplitView,
    CloseTab,
    LinkRules,
}

impl Command {
    pub(crate) const ALL: [Command; 3] =
        [Command::SplitView, Command::CloseTab, Command::LinkRules];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Command::SplitView => "Open beside this tab in Split View",
            Command::CloseTab => "Close this tab",
            Command::LinkRules => "Link Rules: where links from other apps open",
        }
    }

    fn glyph(self) -> char {
        match self {
            Command::SplitView => '\u{ECA5}',
            Command::CloseTab => '\u{E8BB}',
            Command::LinkRules => '\u{E71B}',
        }
    }
}

/// What choosing a result does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Opens an address the browser's autocomplete chose, in a new tab.
    Open(String),
    /// Opens typed text in a new tab as the browser's address bar would: an address, or a search
    /// with the profile's search engine.
    Typed(String),
    /// Makes an open tab of the window active.
    SwitchTab(usize),
    Run(Command),
}

/// One row of the results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub glyph: char,
    pub title: String,
    /// The key that runs the row, shown at its right end.
    pub key: &'static str,
    pub action: Action,
}

/// One suggestion of the browser: what it shows and where it goes.
pub(crate) struct Found<'a> {
    pub contents: &'a str,
    pub description: &'a str,
    pub destination: &'a str,
}

/// One open tab: strip index, title, address and whether it is the active one.
pub(crate) struct Open<'a> {
    pub index: usize,
    pub title: &'a str,
    pub url: &'a str,
    pub active: bool,
}

/// The most rows the bar shows.
pub(crate) const ROWS_SHOWN: usize = 8;
const SUGGESTIONS_SHOWN: usize = 5;
const TABS_SHOWN: usize = 3;

fn matches(text: &str, value: &str) -> bool {
    value.to_lowercase().contains(&text.to_lowercase())
}

/// The rows for `text`: the browser's suggestions first, its default match leading as Enter, then
/// open tabs that match, then Rivet commands that match. Without suggestions from the browser,
/// the text itself is offered as an address or a search. An empty field lists other tabs and the
/// commands.
pub(crate) fn results(text: &str, found: &[Found], tabs: &[Open]) -> Vec<Item> {
    let text = text.trim();
    let mut out = Vec::new();
    if !text.is_empty() {
        if found.is_empty() {
            let address = crate::features::address_bar::looks_like_address(text);
            out.push(Item {
                glyph: if address { '\u{E774}' } else { '\u{E721}' },
                title: if address {
                    format!("Open {text}")
                } else {
                    format!("Search the web for \"{text}\"")
                },
                key: "",
                action: Action::Typed(text.to_owned()),
            });
        }
        for suggestion in found.iter().take(SUGGESTIONS_SHOWN) {
            let page = suggestion.contents.contains("://") || suggestion.description.is_empty();
            let title = if suggestion.description.is_empty() {
                suggestion.contents.to_owned()
            } else {
                format!("{} \u{b7} {}", suggestion.contents, suggestion.description)
            };
            out.push(Item {
                glyph: if page { '\u{E774}' } else { '\u{E721}' },
                title,
                key: "",
                action: Action::Open(suggestion.destination.to_owned()),
            });
        }
    }
    let mut first_tab = true;
    for tab in tabs
        .iter()
        .filter(|tab| {
            !tab.active && (text.is_empty() || matches(text, tab.title) || matches(text, tab.url))
        })
        .take(TABS_SHOWN)
    {
        let title = if tab.title.trim().is_empty() {
            tab.url
        } else {
            tab.title
        };
        out.push(Item {
            glyph: '\u{E737}',
            title: format!("Switch to tab: {title}"),
            key: if std::mem::take(&mut first_tab) {
                "Tab"
            } else {
                ""
            },
            action: Action::SwitchTab(tab.index),
        });
    }
    for command in Command::ALL {
        if text.is_empty() || matches(text, command.label()) {
            out.push(Item {
                glyph: command.glyph(),
                title: command.label().to_owned(),
                key: "",
                action: Action::Run(command),
            });
        }
    }
    out.truncate(ROWS_SHOWN);
    if let Some(first) = out.first_mut() {
        first.key = "Enter";
    }
    out
}

/// The study's `.command`: 580 wide or 90% of the page, 110 below its top, a 56 DIP field, rows
/// with 6 DIPs around the list and the line that says what Ctrl+T opens.
pub(crate) const CARD_WIDTH: f32 = 580.0;
pub(crate) const CARD_TOP: f32 = 110.0;
pub(crate) const CARD_RADIUS: f32 = 16.0;
pub(crate) const FIELD_HEIGHT: f32 = 56.0;
pub(crate) const LIST_PAD: f32 = 6.0;
pub(crate) const ROW_HEIGHT: f32 = 38.0;
pub(crate) const FOOTER_HEIGHT: f32 = 42.0;
pub(crate) const TEXT_SIZE: f32 = 16.0;
/// The field's paper, one colour so the text engine can draw on it.
pub(crate) const FIELD_GROUND: u32 = 0xfbfaf7;
const MAX_TEXT: u32 = 8192;

/// The card of the bar in DIPs from the scrim's top left corner: left, top, width and height.
pub(crate) fn card(width: f32, height: f32, rows: usize) -> [f32; 4] {
    let w = CARD_WIDTH.min(width * 0.9);
    let list = if rows == 0 {
        0.0
    } else {
        LIST_PAD * 2.0 + ROW_HEIGHT * rows as f32
    };
    let h = FIELD_HEIGHT + list + FOOTER_HEIGHT;
    let top = CARD_TOP.min((height - h).max(0.0) / 2.0);
    [((width - w) / 2.0).max(0.0), top, w, h]
}

/// The text of the field inside the card, after the 18 DIP padding, the 16 DIP icon and its 10 DIP gap.
pub(crate) fn field_box(card: [f32; 4]) -> [f32; 4] {
    let [x, y, w, _] = card;
    let left = x + 18.0 + 16.0 + 10.0;
    [
        left,
        y + 16.0,
        (x + w - 18.0 - left).max(0.0),
        FIELD_HEIGHT - 32.0,
    ]
}

/// The two choices of the footer as (choice, x, width) from the card's left edge.
pub(crate) fn mode_buttons() -> [(CtrlT, f32, f32); 2] {
    // "Ctrl T opens" at 16 DIPs in, then the two buttons 6 apart.
    let first = 16.0 + 66.0 + 6.0;
    [
        (CtrlT::CommandBar, first, 82.0),
        (CtrlT::NewTabPage, first + 82.0 + 6.0, 84.0),
    ]
}

/// What a point of the scrim is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Field,
    Row(usize),
    Mode(CtrlT),
    Card,
    Outside,
}

/// The part of the bar under a point in DIPs from the scrim's top left corner.
pub(crate) fn hit(card: [f32; 4], rows: usize, x: f32, y: f32) -> Hit {
    let [cx, cy, w, h] = card;
    if x < cx || x >= cx + w || y < cy || y >= cy + h {
        return Hit::Outside;
    }
    let (px, py) = (x - cx, y - cy);
    if py < FIELD_HEIGHT {
        return Hit::Field;
    }
    let list = if rows == 0 {
        0.0
    } else {
        LIST_PAD * 2.0 + ROW_HEIGHT * rows as f32
    };
    if py < FIELD_HEIGHT + list {
        let at = py - FIELD_HEIGHT - LIST_PAD;
        if at >= 0.0 && ((at / ROW_HEIGHT) as usize) < rows {
            return Hit::Row((at / ROW_HEIGHT) as usize);
        }
        return Hit::Card;
    }
    let top = (FOOTER_HEIGHT - 22.0) / 2.0;
    let fy = py - FIELD_HEIGHT - list;
    if fy >= top && fy < top + 22.0 {
        if let Some((choice, _, _)) = mode_buttons()
            .into_iter()
            .find(|(_, left, bw)| px >= *left && px < left + bw)
        {
            return Hit::Mode(choice);
        }
    }
    Hit::Card
}

/// A start page asks for the bar with a fragment that begins `#rivet-command`. Each ask carries a
/// new fragment, so the same address is not taken as a second ask.
pub(crate) fn asks_for_bar(url: &str) -> Option<&str> {
    let (_, fragment) = url.split_once('#')?;
    (crate::features::address_bar::is_start_page(url) && fragment.starts_with("rivet-command"))
        .then_some(fragment)
}

/// What the appbar draws of the bar. Positions are client pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    /// The scrim over the page.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rows: Vec<(char, String, &'static str)>,
    pub selected: usize,
    pub hover: Option<Hit>,
    pub ctrl_t: CtrlT,
    /// Whether the field is empty, which shows its placeholder.
    pub empty: bool,
    /// Bumped whenever the field draws something new.
    pub generation: u64,
}

struct Bar {
    field: Option<Field>,
    open: bool,
    items: Vec<Item>,
    selected: usize,
    hover: Option<Hit>,
    press: Option<Hit>,
    queried: Option<String>,
    generation: u64,
    /// The start page's last ask, so it is answered once.
    asked: String,
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
            open: false,
            items: Vec::new(),
            selected: 0,
            hover: None,
            press: None,
            queried: None,
            generation: 0,
            asked: String::new(),
        });
        work(bar)
    })
}

pub(crate) fn is_open(window: HWND) -> bool {
    BARS.with(|bars| {
        bars.try_borrow()
            .map(|bars| bars.get(&(window as isize)).is_some_and(|bar| bar.open))
            .unwrap_or(false)
    })
}

fn factor(window: HWND) -> f32 {
    unsafe { crate::frame::layout_dpi(window) as f32 / 96.0 }
}

/// The scrim in DIPs: the window's page panel.
fn scrim(window: HWND) -> Option<[f32; 4]> {
    let panel = crate::features::frame_layout::panel_bounds(window)?;
    Some([
        panel.x as f32,
        panel.y as f32,
        panel.width as f32,
        panel.height as f32,
    ])
}

fn card_of(window: HWND, rows: usize) -> Option<[f32; 4]> {
    let [_, _, w, h] = scrim(window)?;
    Some(card(w, h, rows))
}

unsafe fn field_rect(window: HWND, rows: usize) -> Option<RECT> {
    let [sx, sy, _, _] = scrim(window)?;
    let [x, y, w, h] = field_box(card_of(window, rows)?);
    let f = factor(window);
    Some(RECT {
        left: ((sx + x) * f).round() as i32,
        top: ((sy + y) * f).round() as i32,
        right: ((sx + x + w) * f).round() as i32,
        bottom: ((sy + y + h) * f).round() as i32,
    })
}

/// Lets the appbar draw the field's pixels at their place, in client pixels.
pub(crate) unsafe fn with_field_pixels<R>(
    window: HWND,
    work: impl FnOnce(RECT, &[u8], i32, i32, Option<RECT>) -> R,
) -> Option<R> {
    with_bar(window, |bar| {
        if !bar.open {
            return None;
        }
        let rect = field_rect(window, bar.items.len())?;
        let field = bar.field.as_mut()?;
        let caret = field.caret();
        let pixels = field.render(FIELD_GROUND)?;
        Some(work(rect, pixels.bytes, pixels.width, pixels.height, caret))
    })
}

pub(crate) unsafe fn view(window: HWND) -> Option<View> {
    let [x, y, width, height] = scrim(window)?;
    let f = factor(window);
    with_bar(window, |bar| {
        if !bar.open {
            return None;
        }
        Some(View {
            x: x * f,
            y: y * f,
            width: width * f,
            height: height * f,
            rows: bar
                .items
                .iter()
                .map(|item| (item.glyph, item.title.clone(), item.key))
                .collect(),
            selected: bar.selected,
            hover: bar.hover,
            ctrl_t: ctrl_t_opens(),
            empty: bar
                .field
                .as_ref()
                .is_none_or(|field| field.text().is_empty()),
            generation: bar.generation,
        })
    })
}

/// Opens the bar with an empty field.
pub(crate) unsafe fn open(window: HWND) -> bool {
    if !crate::frame::attached_window(window) {
        return false;
    }
    let opened = with_bar(window, |bar| {
        if bar.field.is_none() {
            crate::appbar::make_fonts_known_to_gdi();
            match Field::new(window, "Manrope", 0x1f2024) {
                Ok(field) => {
                    field.set_max_length(MAX_TEXT);
                    bar.field = Some(field);
                }
                Err(error) => {
                    crate::trace::trace(&format!("command bar: {error}"));
                    return false;
                }
            }
        }
        bar.open = true;
        bar.queried = None;
        bar.selected = 0;
        bar.generation += 1;
        true
    });
    if !opened {
        return false;
    }
    refresh(window);
    let placed = with_bar(window, |bar| {
        let rect = field_rect(window, bar.items.len())?;
        let field = bar.field.as_mut()?;
        field.place(rect, TEXT_SIZE, crate::frame::layout_dpi(window));
        // The browser turns the input method off while its page has no text field focused. The
        // bar is a text field, so it gets the window's default input context back.
        ImmAssociateContextEx(window, std::ptr::null_mut(), IACE_DEFAULT);
        field.activate();
        field.set_text("");
        Some(())
    });
    crate::trace::trace(&format!("command bar: open: {}", placed.is_some()));
    crate::frame::update_appbar(window);
    placed.is_some()
}

pub(crate) unsafe fn close(window: HWND) {
    with_bar(window, |bar| {
        if let Some(field) = bar.field.as_ref() {
            field.send(0x0008, 0, 0); // WM_KILLFOCUS
        }
        bar.open = false;
        bar.items.clear();
        bar.hover = None;
        bar.press = None;
        bar.generation += 1;
    });
    crate::frame::update_appbar(window);
}

/// Works out the rows again when the text changed since the last time, and moves the field, whose
/// card grows with the rows.
unsafe fn refresh(window: HWND) {
    let Some(text) = with_bar(window, |bar| {
        let text = bar
            .field
            .as_ref()
            .map(|field| field.text())
            .unwrap_or_default();
        (bar.open && bar.queried.as_deref() != Some(text.as_str())).then_some(text)
    }) else {
        return;
    };
    let suggestions = if text.trim().is_empty() {
        Vec::new()
    } else {
        crate::features::omnibox::suggest(window, &text).unwrap_or_default()
    };
    let found: Vec<Found> = suggestions
        .iter()
        .map(|s| Found {
            contents: &s.contents,
            description: &s.description,
            destination: &s.destination,
        })
        .collect();
    let rows = crate::features::tabs::tabs(window).unwrap_or_default();
    let saved = crate::features::tabs::open_tabs(window).unwrap_or_default();
    let open: Vec<Open> = rows
        .iter()
        .map(|tab| Open {
            index: tab.index,
            title: &tab.title,
            url: saved
                .get(tab.index)
                .map_or("", |open| open.saved.url.as_str()),
            active: tab.active,
        })
        .collect();
    let items = results(&text, &found, &open);
    with_bar(window, |bar| {
        bar.items = items;
        bar.selected = 0;
        bar.queried = Some(text);
        bar.generation += 1;
        let rows = bar.items.len();
        if let (Some(rect), Some(field)) = (field_rect(window, rows), bar.field.as_mut()) {
            field.place(rect, TEXT_SIZE, crate::frame::layout_dpi(window));
        }
    });
}

/// Runs a row: the bar closes first, then the row's action.
unsafe fn run(window: HWND, index: usize) -> bool {
    let Some(action) = with_bar(window, |bar| {
        bar.items.get(index).map(|item| item.action.clone())
    }) else {
        return false;
    };
    close(window);
    let done = match &action {
        Action::Open(url) => {
            crate::features::tabs::new_tab(window) && crate::features::navigation::open(window, url)
        }
        Action::Typed(text) => {
            crate::features::tabs::new_tab(window)
                && match crate::features::omnibox::open_typed(window, text) {
                    Some(opened) => opened,
                    None if crate::features::address_bar::looks_like_address(text) => {
                        crate::features::navigation::open(
                            window,
                            &crate::features::address_bar::address_for(text),
                        )
                    }
                    // A build without the browser's classifier does not guess a search engine.
                    None => false,
                }
        }
        Action::SwitchTab(tab) => crate::features::tabs::activate_tab(window, *tab),
        Action::Run(Command::SplitView) => crate::features::split::split_active(window),
        Action::Run(Command::CloseTab) => crate::features::tabs::tabs(window)
            .and_then(|tabs| tabs.into_iter().find(|tab| tab.active))
            .is_some_and(|tab| crate::features::tabs::close_tab(window, tab.index)),
        Action::Run(Command::LinkRules) => crate::features::link_rules::open(window),
    };
    crate::trace::trace(&format!("command bar: run {action:?}: {done}"));
    crate::frame::update_appbar(window);
    done
}

/// Opens the bar when the active page is the start page asking for it. Called on the window's
/// periodic tab check.
pub(crate) unsafe fn tick(window: HWND) {
    let Some(contents) = crate::features::navigation::active_contents(window) else {
        return;
    };
    let url = crate::features::tabs::page_address(contents);
    let Some(ask) = asks_for_bar(&url) else {
        return;
    };
    let new = with_bar(window, |bar| {
        let new = bar.asked != ask;
        bar.asked = ask.to_owned();
        new
    });
    if new && !is_open(window) {
        open(window);
    }
}

/// The hit under a client point, in the bar's own terms.
unsafe fn hit_at(window: HWND, point: POINT) -> Hit {
    let Some([sx, sy, _, _]) = scrim(window) else {
        return Hit::Outside;
    };
    let rows = with_bar(window, |bar| bar.items.len());
    let Some(card) = card_of(window, rows) else {
        return Hit::Outside;
    };
    let f = factor(window);
    hit(card, rows, point.x as f32 / f - sx, point.y as f32 / f - sy)
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
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
const WM_IME_COMPOSITION: u32 = 0x010F;
const WM_IME_SETCONTEXT: u32 = 0x0281;
const WM_IME_NOTIFY: u32 = 0x0282;
const WM_IME_CHAR: u32 = 0x0286;
const WM_IME_REQUEST: u32 = 0x0288;
const WM_INPUTLANGCHANGE: u32 = 0x0051;

/// Handles a message of the browser window for the bar. `Some` means the bar used it and the
/// browser does not see it. While the bar is open it takes the keyboard and the pointer over the
/// page, as the study's scrim does.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if busy() {
        return None;
    }
    let open = is_open(window);
    if message == WM_KEYDOWN {
        let ctrl = GetKeyState(0x11) < 0;
        let shift = GetKeyState(0x10) < 0;
        let alt = GetKeyState(0x12) < 0;
        if is_ctrl_t(wparam as u16, ctrl, shift, alt) {
            if open {
                close(window);
                return Some(0);
            }
            // The other choice leaves Ctrl+T to the browser, whose new tab opens the start page.
            if ctrl_t_opens() == CtrlT::CommandBar && open_bar(window) {
                return Some(0);
            }
            return None;
        }
    }
    if !open {
        return None;
    }
    let result = match message {
        WM_MOUSEMOVE => {
            let now = hit_at(window, point_of(lparam));
            let changed = with_bar(window, |bar| {
                if let Hit::Row(row) = now {
                    bar.selected = row;
                }
                if now == Hit::Field || (wparam & 1) != 0 {
                    if let Some(field) = bar.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                }
                let changed = bar.hover != Some(now);
                bar.hover = Some(now);
                bar.generation += changed as u64;
                changed
            });
            if changed || now == Hit::Field {
                crate::frame::update_appbar(window);
            }
            Some(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let now = hit_at(window, point_of(lparam));
            with_bar(window, |bar| {
                bar.press = Some(now);
                if now == Hit::Field {
                    if let Some(field) = bar.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                    bar.generation += 1;
                }
            });
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_LBUTTONUP => {
            let now = hit_at(window, point_of(lparam));
            let pressed = with_bar(window, |bar| {
                if let Some(field) = bar.field.as_ref() {
                    field.send(message, wparam, lparam);
                }
                bar.press.take()
            });
            match pressed.filter(|pressed| *pressed == now) {
                Some(Hit::Row(row)) => {
                    run(window, row);
                }
                Some(Hit::Mode(choice)) => {
                    set_ctrl_t_opens(choice);
                    with_bar(window, |bar| bar.generation += 1);
                    crate::trace::trace(&format!("command bar: Ctrl+T opens {choice:?}"));
                }
                // A press on the scrim closes the bar, as in the study.
                Some(Hit::Outside) => close(window),
                _ => {}
            }
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_MOUSEWHEEL | WM_RBUTTONDOWN | WM_RBUTTONUP => Some(0),
        WM_KEYDOWN => match wparam as u16 {
            0x0D => {
                let index = with_bar(window, |bar| bar.selected);
                run(window, index);
                Some(0)
            }
            0x1B => {
                close(window);
                Some(0)
            }
            // Tab switches to the first matching tab, as its row says.
            0x09 => {
                let tab = with_bar(window, |bar| {
                    bar.items
                        .iter()
                        .position(|item| matches!(item.action, Action::SwitchTab(_)))
                });
                if let Some(index) = tab {
                    run(window, index);
                }
                Some(0)
            }
            0x26 | 0x28 => {
                let down = wparam as u16 == 0x28;
                with_bar(window, |bar| {
                    let count = bar.items.len();
                    if count > 0 {
                        bar.selected = if down {
                            (bar.selected + 1) % count
                        } else {
                            (bar.selected + count - 1) % count
                        };
                        bar.generation += 1;
                    }
                });
                crate::frame::update_appbar(window);
                Some(0)
            }
            _ => {
                let used = forward(window, message, wparam, lparam);
                refresh(window);
                crate::frame::update_appbar(window);
                used.or(Some(0))
            }
        },
        WM_CHAR => {
            if matches!(wparam, 0x0d | 0x1b | 0x09 | 0x0a) {
                return Some(0);
            }
            let used = forward(window, message, wparam, lparam);
            refresh(window);
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
                refresh(window);
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
            forward(window, message, wparam - TIMER_BASE, 0);
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_KILLFOCUS | WM_SETFOCUS => {
            forward(window, message, wparam, lparam);
            crate::frame::update_appbar(window);
            None
        }
        _ => None,
    };
    result
}

/// Opens the bar from Ctrl+T. A window whose bar cannot start leaves the key to the browser.
unsafe fn open_bar(window: HWND) -> bool {
    open(window)
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
    if let Some(caret) =
        caret.filter(|_| !matches!(message, WM_IME_NOTIFY | WM_IME_SETCONTEXT | WM_IME_REQUEST))
    {
        place_input_method(window, caret);
    }
    used
}

/// Puts the input method's composition and candidate windows at the caret, as the address bar does.
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

/// Puts `text` in the open bar's field as typing would, and traces the rows, for tests that may
/// not press keys.
pub(crate) unsafe fn type_for_test(window: HWND, text: &str) -> bool {
    if !is_open(window) {
        return false;
    }
    with_bar(window, |bar| {
        bar.field.as_ref().map(|field| field.set_text(text))
    });
    refresh(window);
    let rows: Vec<String> = with_bar(window, |bar| {
        bar.items.iter().map(|item| item.title.clone()).collect()
    });
    crate::trace::trace(&format!("command bar rows {rows:?}"));
    crate::frame::update_appbar(window);
    true
}

/// Runs row `index` of the open bar, as a click on it would.
pub(crate) unsafe fn run_for_test(window: HWND, index: usize) -> bool {
    is_open(window) && run(window, index)
}

/// Forgets a closed window.
pub(crate) fn forget(window: HWND) {
    BARS.with(|bars| bars.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab<'a>(index: usize, title: &'a str, url: &'a str, active: bool) -> Open<'a> {
        Open {
            index,
            title,
            url,
            active,
        }
    }

    #[test]
    fn fr3_ctrl_t_is_ctrl_and_t_alone() {
        assert!(is_ctrl_t(0x54, true, false, false));
        assert!(
            !is_ctrl_t(0x54, true, true, false),
            "Ctrl+Shift+T reopens a closed tab"
        );
        assert!(!is_ctrl_t(0x54, false, false, false));
        assert!(!is_ctrl_t(0x54, true, false, true));
        assert!(!is_ctrl_t(0x4C, true, false, false));
    }

    #[test]
    fn fr3_ctrl_t_setting_chooses_command_bar_or_new_tab_page() {
        assert_eq!(
            ctrl_t_opens(),
            CtrlT::CommandBar,
            "the study starts on the command bar"
        );
        set_ctrl_t_opens(CtrlT::NewTabPage);
        assert_eq!(ctrl_t_opens(), CtrlT::NewTabPage);
        set_ctrl_t_opens(CtrlT::CommandBar);
        assert_eq!(ctrl_t_opens(), CtrlT::CommandBar);
    }

    #[test]
    fn fr3_command_bar_lists_browser_suggestions_then_tabs_then_commands() {
        let found = [
            Found {
                contents: "launch plan",
                description: "Google Search",
                destination: "https://www.google.com/search?q=launch+plan",
            },
            Found {
                contents: "https://launch.example/",
                description: "",
                destination: "https://launch.example/",
            },
        ];
        let tabs = [
            tab(0, "Ember workspace", "https://ember.example/launch", false),
            tab(1, "Launch notes", "https://notes.example/", true),
        ];
        let rows = results("launch", &found, &tabs);
        assert_eq!(
            rows[0].action,
            Action::Open("https://www.google.com/search?q=launch+plan".into())
        );
        assert_eq!(rows[0].key, "Enter");
        assert_eq!(
            rows[1].action,
            Action::Open("https://launch.example/".into())
        );
        assert_eq!(
            rows[2].action,
            Action::SwitchTab(0),
            "the active tab is not offered"
        );
        assert_eq!(rows[2].key, "Tab");
        assert_eq!(rows[2].title, "Switch to tab: Ember workspace");
        assert_eq!(rows.len(), 3, "no command matches \"launch\"");
    }

    #[test]
    fn fr3_command_bar_without_suggestions_offers_the_text_as_an_address_or_a_search() {
        let rows = results("example.test", &[], &[]);
        assert_eq!(rows[0].action, Action::Typed("example.test".into()));
        assert_eq!(rows[0].title, "Open example.test");
        let rows = results("한국어 검색", &[], &[]);
        assert_eq!(rows[0].title, "Search the web for \"한국어 검색\"");
        assert_eq!(rows[0].key, "Enter");
    }

    #[test]
    fn fr3_command_bar_finds_rivet_commands_and_lists_them_when_empty() {
        let rows = results("split", &[], &[]);
        assert!(rows
            .iter()
            .any(|row| row.action == Action::Run(Command::SplitView)));
        let tabs = [
            tab(0, "A", "https://a.test/", true),
            tab(1, "B", "https://b.test/", false),
        ];
        let rows = results("", &[], &tabs);
        assert_eq!(rows[0].action, Action::SwitchTab(1));
        assert_eq!(rows.len(), 1 + Command::ALL.len());
    }

    #[test]
    fn fr3_command_bar_shows_at_most_eight_rows() {
        let found: Vec<Found> = (0..10)
            .map(|_| Found {
                contents: "x",
                description: "",
                destination: "https://x.test/",
            })
            .collect();
        let titles: Vec<String> = (0..10).map(|i| format!("x {i}")).collect();
        let tabs: Vec<Open> = titles
            .iter()
            .enumerate()
            .map(|(i, t)| tab(i, t, "", false))
            .collect();
        assert_eq!(
            results("x", &found, &tabs).len(),
            SUGGESTIONS_SHOWN + TABS_SHOWN
        );
        assert!(results("x", &found, &tabs).len() <= ROWS_SHOWN);
    }

    #[test]
    fn fr3_command_bar_card_is_the_studys_580_wide_110_below_the_top() {
        let [x, y, w, h] = card(1200.0, 800.0, 4);
        assert_eq!((x, y, w), (310.0, 110.0, 580.0));
        assert_eq!(
            h,
            FIELD_HEIGHT + LIST_PAD * 2.0 + ROW_HEIGHT * 4.0 + FOOTER_HEIGHT
        );
        // A narrow page keeps 90% of its width.
        assert_eq!(card(400.0, 800.0, 0)[2], 360.0);
        // A short page keeps the card inside it.
        let [_, top, _, height] = card(1200.0, 300.0, 4);
        assert!(top + height <= 300.0);
    }

    #[test]
    fn fr3_command_bar_hit_finds_field_rows_mode_and_scrim() {
        let c = card(1200.0, 800.0, 3);
        assert_eq!(hit(c, 3, 600.0, 120.0), Hit::Field);
        let row_y = c[1] + FIELD_HEIGHT + LIST_PAD + ROW_HEIGHT * 1.5;
        assert_eq!(hit(c, 3, 600.0, row_y), Hit::Row(1));
        let footer_y = c[1] + c[3] - FOOTER_HEIGHT / 2.0;
        let [(_, left, _), (_, second, _)] = mode_buttons();
        assert_eq!(
            hit(c, 3, c[0] + left + 1.0, footer_y),
            Hit::Mode(CtrlT::CommandBar)
        );
        assert_eq!(
            hit(c, 3, c[0] + second + 1.0, footer_y),
            Hit::Mode(CtrlT::NewTabPage)
        );
        assert_eq!(hit(c, 3, 10.0, 10.0), Hit::Outside);
        let field = field_box(c);
        assert!(field[0] > c[0] && field[0] + field[2] < c[0] + c[2]);
    }

    #[test]
    fn fr3_start_page_asks_for_the_command_bar_with_a_fragment() {
        let start = "file:///C:/Rivet/start.html?space=Work#rivet-command-17";
        assert_eq!(asks_for_bar(start), Some("rivet-command-17"));
        assert_eq!(asks_for_bar("file:///C:/Rivet/start.html?space=Work"), None);
        assert_eq!(
            asks_for_bar("https://example.test/#rivet-command"),
            None,
            "only the start page may ask"
        );
    }
}
