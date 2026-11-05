//! Rivet settings: one card over the page with every setting Rivet keeps itself.
//!
//! The appbar's settings menu opens it. Like the command bar it is drawn on the window's own
//! composition surface (see `appbar`), above a scrim over the page, and takes its input from the
//! window's own messages, so it never makes a window of its own. Text goes into the Windows text
//! engine (`rivet_platform_windows::textservices`), which does editing, the clipboard and the input
//! method, Korean included.
//!
//! Each setting is read from and written to the store that already owns it: the tab list's file
//! for the archive period and Ctrl+T, the setup file for the password and passkey provider, and the
//! spaces file for the appbar and the spaces. Nothing here keeps a copy of its own.

use std::cell::RefCell;
use std::collections::HashMap;

use rivet_core::CredentialProvider;
use rivet_platform_windows::textservices::{Field, TIMER_BASE, TIMER_LAST};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::Ime::{ImmAssociateContextEx, IACE_DEFAULT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

use crate::spaces::{BrowserKind, SpaceInfo, TINTS};
use crate::tab_sections::CtrlT;

/// One setting of the card, in the order it is drawn and walked with the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Row {
    Archive,
    CtrlT,
    Credentials,
    Width,
    Hidden,
    /// Which space the rows below it edit.
    Space,
    SpaceName,
    SpaceColour,
    SpaceBrowser,
    SpaceProfile,
}

impl Row {
    pub(crate) const ALL: [Row; 10] = [
        Row::Archive,
        Row::CtrlT,
        Row::Credentials,
        Row::Width,
        Row::Hidden,
        Row::Space,
        Row::SpaceName,
        Row::SpaceColour,
        Row::SpaceBrowser,
        Row::SpaceProfile,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Row::Archive => "Archive unused tabs after",
            Row::CtrlT => "Ctrl T opens",
            Row::Credentials => "Passwords and passkeys",
            Row::Width => "Appbar width",
            Row::Hidden => "Appbar",
            Row::Space => "Space",
            Row::SpaceName => "Name",
            Row::SpaceColour => "Colour",
            Row::SpaceBrowser => "Browser",
            Row::SpaceProfile => "Profile",
        }
    }

    pub(crate) fn kind(self) -> Kind {
        match self {
            Row::CtrlT | Row::Credentials | Row::Hidden => Kind::Choice,
            Row::SpaceName | Row::SpaceProfile => Kind::Text,
            _ => Kind::Stepper,
        }
    }

    /// The heading drawn above the row, for the first row of each group.
    pub(crate) fn heading(self) -> Option<&'static str> {
        match self {
            Row::Archive => Some("TABS"),
            Row::Credentials => Some("PASSWORDS AND PASSKEYS"),
            Row::Width => Some("APPBAR"),
            Row::Space => Some("SPACES"),
            _ => None,
        }
    }

    /// The line under the label, for a setting that does not apply at once.
    pub(crate) fn note(self) -> Option<&'static str> {
        (self == Row::Credentials).then_some("Applies from the next start of Rivet")
    }
}

/// How a row's value is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Two buttons, the chosen one dark, as the command bar's "Ctrl T opens".
    Choice,
    /// A value between a back and a forward arrow.
    Stepper,
    /// A text field.
    Text,
}

/// The archive periods offered, in hours. `None` never archives.
pub(crate) const ARCHIVE_HOURS: [Option<u64>; 8] = [
    Some(1),
    Some(4),
    Some(12),
    Some(24),
    Some(72),
    Some(168),
    Some(720),
    None,
];
/// The appbar width moves in steps of this many DIPs.
pub(crate) const WIDTH_STEP: i32 = 20;
pub(crate) const MAX_NAME: usize = 32;
pub(crate) const MAX_PROFILE: usize = 64;
const TINT_NAMES: [&str; 6] = ["Amber", "Blue", "Rose", "Green", "Violet", "Gold"];

/// What the card shows, read from the stores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Values {
    pub archive_hours: Option<u64>,
    pub ctrl_t: CtrlT,
    pub provider: CredentialProvider,
    pub width: i32,
    pub hidden: bool,
    pub spaces: Vec<SpaceInfo>,
    /// The space this window shows. Its browser and profile are the window's own.
    pub current: usize,
    /// The browsers a space can be based on here: the installed ones. Empty in a window of a
    /// browser no space can be based on, which shows every space itself.
    pub browsers: Vec<BrowserKind>,
}

/// A change the card asks a store for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    Archive(Option<u64>),
    CtrlT(CtrlT),
    Provider(CredentialProvider),
    Width(i32),
    Hidden(bool),
    Name(usize, String),
    Tint(usize, u32),
    Browser(usize, BrowserKind),
    Profile(usize, String),
}

/// Whether a row can be changed for space `space`. The current space's browser and profile are the
/// window's own, and a window of a browser no space can be based on has no browser to choose.
pub(crate) fn enabled(values: &Values, space: usize, row: Row) -> bool {
    match row {
        Row::SpaceBrowser => space != values.current && values.browsers.len() > 1,
        Row::SpaceProfile => space != values.current && !values.browsers.is_empty(),
        Row::Space => values.spaces.len() > 1,
        _ => true,
    }
}

/// The row the keyboard moves to from `from`, skipping rows that cannot be changed.
pub(crate) fn next_row(values: &Values, space: usize, from: Row, down: bool) -> Row {
    let count = Row::ALL.len();
    let at = Row::ALL.iter().position(|row| *row == from).unwrap_or(0);
    for step in 1..=count {
        let index = if down {
            (at + step) % count
        } else {
            (at + count * 2 - step) % count
        };
        let row = Row::ALL[index];
        if enabled(values, space, row) {
            return row;
        }
    }
    from
}

fn nearest_archive(hours: Option<u64>) -> usize {
    match hours {
        None => ARCHIVE_HOURS.len() - 1,
        Some(hours) => ARCHIVE_HOURS
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.map(|value| (index, value.abs_diff(hours))))
            .min_by_key(|(_, distance)| *distance)
            .map_or(2, |(index, _)| index),
    }
}

fn cycle<T: Copy + PartialEq>(list: &[T], now: T, forward: bool) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    let at = list.iter().position(|value| *value == now).unwrap_or(0);
    let count = list.len();
    Some(
        list[if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        }],
    )
}

/// What one step back or forward on `row` changes, for space `space`. The space row itself moves
/// the card to another space and changes nothing; the caller handles it.
pub(crate) fn step(values: &Values, space: usize, row: Row, forward: bool) -> Option<Change> {
    if !enabled(values, space, row) {
        return None;
    }
    let info = values.spaces.get(space);
    match row {
        Row::Archive => {
            let at = nearest_archive(values.archive_hours);
            let to = if forward {
                (at + 1).min(ARCHIVE_HOURS.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            (to != at || ARCHIVE_HOURS[at] != values.archive_hours)
                .then(|| Change::Archive(ARCHIVE_HOURS[to]))
        }
        Row::CtrlT => Some(Change::CtrlT(match values.ctrl_t {
            CtrlT::CommandBar => CtrlT::NewTabPage,
            CtrlT::NewTabPage => CtrlT::CommandBar,
        })),
        Row::Credentials => Some(Change::Provider(values.provider.other())),
        Row::Hidden => Some(Change::Hidden(!values.hidden)),
        Row::Width => {
            let delta = if forward { WIDTH_STEP } else { -WIDTH_STEP };
            let to = (values.width + delta).clamp(crate::APPBAR_MIN_WIDTH, crate::APPBAR_MAX_WIDTH);
            (to != values.width).then_some(Change::Width(to))
        }
        Row::SpaceColour => {
            let info = info?;
            cycle(&TINTS, info.tint, forward).map(|tint| Change::Tint(space, tint))
        }
        Row::SpaceBrowser => {
            let info = info?;
            cycle(&values.browsers, info.browser, forward)
                .filter(|browser| *browser != info.browser)
                .map(|browser| Change::Browser(space, browser))
        }
        Row::Space | Row::SpaceName | Row::SpaceProfile => None,
    }
}

/// The space the card edits after a step on the space row.
pub(crate) fn step_space(values: &Values, space: usize, forward: bool) -> usize {
    let count = values.spaces.len().max(1);
    if forward {
        (space + 1) % count
    } else {
        (space + count - 1) % count
    }
}

/// What a press on button `choice` of a choice row changes. A press on the chosen one changes
/// nothing.
pub(crate) fn choose(values: &Values, row: Row, choice: usize) -> Option<Change> {
    let change = match (row, choice) {
        (Row::CtrlT, 0) => Change::CtrlT(CtrlT::CommandBar),
        (Row::CtrlT, 1) => Change::CtrlT(CtrlT::NewTabPage),
        (Row::Credentials, index) => Change::Provider(*CredentialProvider::ALL.get(index)?),
        (Row::Hidden, 0) => Change::Hidden(false),
        (Row::Hidden, 1) => Change::Hidden(true),
        _ => return None,
    };
    (Some(choice) != chosen(values, row)).then_some(change)
}

/// The two buttons of a choice row.
pub(crate) fn choices(row: Row) -> [&'static str; 2] {
    match row {
        Row::CtrlT => ["Command Bar", "New tab page"],
        Row::Credentials => ["Browser", "Windows"],
        _ => ["Shown", "Hidden"],
    }
}

/// Which button of a choice row is the chosen one.
pub(crate) fn chosen(values: &Values, row: Row) -> Option<usize> {
    match row {
        Row::CtrlT => Some((values.ctrl_t == CtrlT::NewTabPage) as usize),
        Row::Credentials => CredentialProvider::ALL
            .iter()
            .position(|provider| *provider == values.provider),
        Row::Hidden => Some(values.hidden as usize),
        _ => None,
    }
}

/// The value a stepper or a text row shows.
pub(crate) fn shown(values: &Values, space: usize, row: Row) -> String {
    let info = values.spaces.get(space);
    match row {
        Row::Archive => match values.archive_hours {
            None => "Never".to_owned(),
            Some(1) => "1 hour".to_owned(),
            Some(hours) if hours % 24 == 0 && hours >= 48 => format!("{} days", hours / 24),
            Some(hours) => format!("{hours} hours"),
        },
        Row::Width => format!("{} px", values.width),
        Row::Space => info.map_or_else(String::new, |info| info.name.clone()),
        Row::SpaceName => info.map_or_else(String::new, |info| info.name.clone()),
        Row::SpaceColour => info
            .and_then(|info| TINTS.iter().position(|tint| *tint == info.tint))
            .map_or("Custom", |index| TINT_NAMES[index])
            .to_owned(),
        Row::SpaceBrowser => match (info, values.browsers.is_empty()) {
            (_, true) => "This browser".to_owned(),
            (Some(info), false) => info.browser.label().to_owned(),
            (None, false) => String::new(),
        },
        Row::SpaceProfile => info.map_or_else(String::new, |info| info.profile.clone()),
        Row::CtrlT | Row::Credentials | Row::Hidden => {
            let index = chosen(values, row).unwrap_or(0);
            choices(row)[index].to_owned()
        }
    }
}

/// Checks typed text for a text row and gives the change it makes, or the reason it cannot.
/// A space name is the key its tab list is kept under, so two spaces cannot share one.
pub(crate) fn commit_text(
    values: &Values,
    space: usize,
    row: Row,
    text: &str,
) -> Result<Option<Change>, &'static str> {
    let text = text.trim();
    let info = values.spaces.get(space).ok_or("no such space")?;
    match row {
        Row::SpaceName => {
            if text.is_empty() {
                return Err("A space needs a name");
            }
            if text.chars().count() > MAX_NAME {
                return Err("A space name is at most 32 characters");
            }
            let taken = values
                .spaces
                .iter()
                .enumerate()
                .any(|(index, other)| index != space && other.name.eq_ignore_ascii_case(text));
            if taken {
                return Err("Another space has this name");
            }
            Ok((text != info.name).then(|| Change::Name(space, text.to_owned())))
        }
        Row::SpaceProfile => {
            if !enabled(values, space, row) {
                return Ok(None);
            }
            if text.is_empty() {
                return Err("A space needs a profile");
            }
            if text.chars().count() > MAX_PROFILE {
                return Err("A profile name is at most 64 characters");
            }
            if !text
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
            {
                return Err("A profile name has letters, digits, spaces, - and _ only");
            }
            Ok((text != info.profile).then(|| Change::Profile(space, text.to_owned())))
        }
        _ => Ok(None),
    }
}

// --- Layout ---------------------------------------------------------------------------------

pub(crate) const CARD_WIDTH: f32 = 540.0;
pub(crate) const CARD_TOP: f32 = 64.0;
pub(crate) const CARD_RADIUS: f32 = 16.0;
pub(crate) const HEAD_HEIGHT: f32 = 64.0;
pub(crate) const HEADING_HEIGHT: f32 = 28.0;
pub(crate) const ROW_HEIGHT: f32 = 38.0;
pub(crate) const NOTE_ROW_HEIGHT: f32 = 50.0;
pub(crate) const FOOTER_HEIGHT: f32 = 40.0;
pub(crate) const PAD: f32 = 20.0;
pub(crate) const VALUE_WIDTH: f32 = 236.0;
pub(crate) const CONTROL_HEIGHT: f32 = 26.0;
pub(crate) const ARROW_WIDTH: f32 = 26.0;
pub(crate) const CLOSE_SIZE: f32 = 28.0;
pub(crate) const TEXT_SIZE: f32 = 13.0;
/// The card's paper, one colour so the text engine can draw on it.
pub(crate) const FIELD_GROUND: u32 = 0xfbfaf7;

fn row_height(row: Row) -> f32 {
    if row.note().is_some() {
        NOTE_ROW_HEIGHT
    } else {
        ROW_HEIGHT
    }
}

/// Each row's top inside the card, in DIPs.
pub(crate) fn row_tops() -> [(Row, f32); 10] {
    let mut top = HEAD_HEIGHT;
    Row::ALL.map(|row| {
        if row.heading().is_some() {
            top += HEADING_HEIGHT;
        }
        let at = top;
        top += row_height(row);
        (row, at)
    })
}

fn body_height() -> f32 {
    let (row, top) = row_tops()[Row::ALL.len() - 1];
    top + row_height(row)
}

/// The card in DIPs from the scrim's top left corner: left, top, width and height.
pub(crate) fn card(width: f32, height: f32) -> [f32; 4] {
    let w = CARD_WIDTH.min(width * 0.92);
    let h = body_height() + FOOTER_HEIGHT;
    let top = CARD_TOP.min((height - h).max(0.0) / 2.0);
    [((width - w) / 2.0).max(0.0), top, w, h]
}

/// A row's area inside the card, from the card's top left corner: left, top, width and height.
pub(crate) fn row_box(card: [f32; 4], row: Row) -> [f32; 4] {
    let top = row_tops()
        .iter()
        .find(|(each, _)| *each == row)
        .map_or(0.0, |(_, top)| *top);
    [
        card[0] + 8.0,
        card[1] + top,
        card[2] - 16.0,
        row_height(row),
    ]
}

/// The control at the right end of a row.
pub(crate) fn value_box(card: [f32; 4], row: Row) -> [f32; 4] {
    let [_, y, _, h] = row_box(card, row);
    let width = VALUE_WIDTH.min(card[2] * 0.5);
    [
        card[0] + card[2] - PAD - width,
        y + (h - CONTROL_HEIGHT) / 2.0,
        width,
        CONTROL_HEIGHT,
    ]
}

/// The text of a text row's field, inside its box.
pub(crate) fn text_box(card: [f32; 4], row: Row) -> [f32; 4] {
    let [x, y, w, h] = value_box(card, row);
    [x + 10.0, y + 4.0, (w - 20.0).max(0.0), h - 8.0]
}

/// The close button at the right end of the card's head.
pub(crate) fn close_box(card: [f32; 4]) -> [f32; 4] {
    [
        card[0] + card[2] - PAD - CLOSE_SIZE + 6.0,
        card[1] + 16.0,
        CLOSE_SIZE,
        CLOSE_SIZE,
    ]
}

/// What a point of the scrim is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    /// Button 0 or 1 of a choice row.
    Choice(Row, usize),
    /// The back (`false`) or forward (`true`) arrow of a stepper.
    Step(Row, bool),
    Text(Row),
    /// The rest of a row, which gives it the keyboard.
    Row(Row),
    Card,
    Outside,
}

fn inside([x, y, w, h]: [f32; 4], px: f32, py: f32) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// The part of the card under a point in DIPs from the scrim's top left corner.
pub(crate) fn hit(card: [f32; 4], x: f32, y: f32) -> Hit {
    if !inside(card, x, y) {
        return Hit::Outside;
    }
    if inside(close_box(card), x, y) {
        return Hit::Close;
    }
    for row in Row::ALL {
        if !inside(row_box(card, row), x, y) {
            continue;
        }
        let value = value_box(card, row);
        if !inside(value, x, y) {
            return Hit::Row(row);
        }
        return match row.kind() {
            Kind::Choice => Hit::Choice(row, (x >= value[0] + value[2] / 2.0) as usize),
            Kind::Text => Hit::Text(row),
            Kind::Stepper if x < value[0] + ARROW_WIDTH => Hit::Step(row, false),
            Kind::Stepper if x >= value[0] + value[2] - ARROW_WIDTH => Hit::Step(row, true),
            Kind::Stepper => Hit::Row(row),
        };
    }
    Hit::Card
}

// --- The card in a window -------------------------------------------------------------------

/// What the appbar draws of the card. Positions are client pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    /// The scrim over the page.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub values: Values,
    /// The space the space rows show.
    pub space: usize,
    /// The row that has the keyboard.
    pub focus: Row,
    /// The text row being typed in, whose field draws itself.
    pub editing: Option<Row>,
    pub hover: Option<Hit>,
    /// Why the last typed text was not taken.
    pub notice: Option<&'static str>,
    /// Bumped whenever the field draws something new.
    pub generation: u64,
}

struct Card {
    field: Option<Field>,
    open: bool,
    values: Option<Values>,
    space: usize,
    focus: Row,
    editing: Option<Row>,
    hover: Option<Hit>,
    press: Option<Hit>,
    notice: Option<&'static str>,
    generation: u64,
}

thread_local! {
    static CARDS: RefCell<HashMap<isize, Card>> = RefCell::new(HashMap::new());
}

/// Whether the card is inside one of its own calls now.
pub(crate) fn busy() -> bool {
    CARDS.with(|cards| cards.try_borrow_mut().is_err())
}

fn with_card<R>(window: HWND, work: impl FnOnce(&mut Card) -> R) -> R {
    CARDS.with(|cards| {
        let mut cards = cards.borrow_mut();
        let card = cards.entry(window as isize).or_insert_with(|| Card {
            field: None,
            open: false,
            values: None,
            space: 0,
            focus: Row::Archive,
            editing: None,
            hover: None,
            press: None,
            notice: None,
            generation: 0,
        });
        work(card)
    })
}

pub(crate) fn is_open(window: HWND) -> bool {
    CARDS.with(|cards| {
        cards
            .try_borrow()
            .map(|cards| cards.get(&(window as isize)).is_some_and(|card| card.open))
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

fn card_of(window: HWND) -> Option<[f32; 4]> {
    let [_, _, w, h] = scrim(window)?;
    Some(card(w, h))
}

fn field_rect(window: HWND, row: Row) -> Option<RECT> {
    let [sx, sy, _, _] = scrim(window)?;
    let [x, y, w, h] = text_box(card_of(window)?, row);
    let f = factor(window);
    Some(RECT {
        left: ((sx + x) * f).round() as i32,
        top: ((sy + y) * f).round() as i32,
        right: ((sx + x + w) * f).round() as i32,
        bottom: ((sy + y + h) * f).round() as i32,
    })
}

/// Reads every value from its store.
unsafe fn read_values(window: HWND) -> Values {
    let (width, hidden) = crate::frame::appbar_settings(window);
    let (spaces, current) = crate::frame::spaces_snapshot(window);
    let browsers = if crate::installed_browsers::current().is_none() {
        Vec::new()
    } else {
        BrowserKind::ALL
            .into_iter()
            .filter(|browser| crate::installed_browsers::installed(*browser).is_some())
            .collect()
    };
    Values {
        archive_hours: crate::features::tab_list::archive_hours(window),
        ctrl_t: crate::features::tab_list::ctrl_t_setting(window),
        provider: crate::frame::saved_credentials(),
        width,
        hidden,
        spaces,
        current,
        browsers,
    }
}

/// Reads the values again after a store changed, keeping the browsers found on open, which are a
/// registry search.
unsafe fn reread(window: HWND) {
    let mut fresh = read_values(window);
    with_card(window, |card| {
        if let Some(old) = card.values.take() {
            fresh.browsers = old.browsers;
        }
        card.space = card.space.min(fresh.spaces.len().saturating_sub(1));
        card.values = Some(fresh);
        card.generation += 1;
    });
}

/// The field's caret in client pixels while a text row is edited and the caret is shown.
pub(crate) fn caret(window: HWND) -> Option<RECT> {
    if busy() {
        return None;
    }
    with_card(window, |card| {
        card.editing
            .filter(|_| card.open)
            .and_then(|_| card.field.as_ref().and_then(|field| field.caret()))
    })
}

/// Lets the appbar draw the field's pixels at their place, in client pixels.
pub(crate) unsafe fn with_field_pixels<R>(
    window: HWND,
    work: impl FnOnce(RECT, &[u8], i32, i32, Option<RECT>) -> R,
) -> Option<R> {
    with_card(window, |card| {
        let row = card.editing.filter(|_| card.open)?;
        let rect = field_rect(window, row)?;
        let field = card.field.as_mut()?;
        let caret = field.caret();
        let pixels = field.render(FIELD_GROUND)?;
        Some(work(rect, pixels.bytes, pixels.width, pixels.height, caret))
    })
}

pub(crate) unsafe fn view(window: HWND) -> Option<View> {
    let [x, y, width, height] = scrim(window)?;
    let f = factor(window);
    with_card(window, |card| {
        if !card.open {
            return None;
        }
        // A change made without a key, such as a drag that selects, counts too.
        if card
            .field
            .as_ref()
            .is_some_and(|field| field.take_changed())
        {
            card.generation += 1;
        }
        Some(View {
            x: x * f,
            y: y * f,
            width: width * f,
            height: height * f,
            values: card.values.clone()?,
            space: card.space,
            focus: card.focus,
            editing: card.editing,
            hover: card.hover,
            notice: card.notice,
            generation: card.generation,
        })
    })
}

/// Opens the card on the space the window shows.
pub(crate) unsafe fn open(window: HWND) -> bool {
    if !crate::frame::attached_window(window) {
        return false;
    }
    if crate::features::command_bar::is_open(window) {
        crate::features::command_bar::close(window);
    }
    let values = read_values(window);
    let made = with_card(window, |card| {
        if card.field.is_none() {
            crate::appbar::make_fonts_known_to_gdi();
            match Field::new(window, "Manrope", 0x1f2024) {
                Ok(field) => {
                    field.set_max_length(MAX_PROFILE as u32);
                    card.field = Some(field);
                }
                Err(error) => {
                    crate::trace::trace(&format!("settings: {error}"));
                    return false;
                }
            }
        }
        card.open = true;
        card.space = values.current;
        card.values = Some(values);
        card.focus = Row::Archive;
        card.editing = None;
        card.hover = None;
        card.press = None;
        card.notice = None;
        card.generation += 1;
        true
    });
    crate::trace::trace(&format!("settings: open: {made}"));
    if made {
        trace_values(window);
    }
    crate::frame::update_appbar(window);
    made
}

/// Closes the card. Text that is being typed and was not taken with Enter is dropped.
pub(crate) unsafe fn close(window: HWND) {
    with_card(window, |card| {
        if let Some(field) = card.field.as_ref() {
            field.send(0x0008, 0, 0); // WM_KILLFOCUS
        }
        card.open = false;
        card.editing = None;
        card.hover = None;
        card.press = None;
        card.notice = None;
        card.generation += 1;
    });
    crate::trace::trace("settings: closed");
    crate::frame::update_appbar(window);
}

/// Writes a change to the store that owns it.
unsafe fn apply(window: HWND, change: Change) {
    let done = match &change {
        Change::Archive(hours) => {
            crate::features::tab_list::set_archive_hours(window, *hours);
            true
        }
        Change::CtrlT(value) => {
            crate::features::tab_list::set_ctrl_t(window, *value);
            true
        }
        Change::Provider(provider) => {
            // The browser reads the provider's switches and preferences when a Rivet profile
            // starts, so the choice applies from the next start, as the card says.
            match rivet_core::paths::Layout::current().map(|layout| layout.setup_file()) {
                Some(path) => rivet_core::credentials::save_to_setup(&path, *provider).is_ok(),
                None => false,
            }
        }
        Change::Width(width) => {
            crate::frame::set_appbar_width(window, *width);
            true
        }
        Change::Hidden(hidden) => {
            crate::frame::set_appbar_hidden(window, *hidden);
            true
        }
        Change::Name(space, name) => {
            let old = crate::frame::spaces_snapshot(window)
                .0
                .get(*space)
                .map(|info| info.name.clone());
            let renamed = crate::frame::edit_space(window, *space, |info| info.name = name.clone());
            if let (true, Some(old)) = (renamed, old) {
                crate::features::tab_list::rename_space(window, &old, name);
            }
            renamed
        }
        Change::Tint(space, tint) => {
            crate::frame::edit_space(window, *space, |info| info.tint = *tint)
        }
        Change::Browser(space, browser) => {
            crate::frame::edit_space(window, *space, |info| info.browser = *browser)
        }
        Change::Profile(space, profile) => {
            crate::frame::edit_space(window, *space, |info| info.profile = profile.clone())
        }
    };
    crate::trace::trace(&format!("settings: {change:?}: {done}"));
    reread(window);
    trace_values(window);
    crate::frame::update_appbar(window);
}

fn trace_values(window: HWND) {
    let line = with_card(window, |card| {
        let values = card.values.as_ref()?;
        Some(format!(
            "settings values archive={:?} ctrl_t={} credentials={} width={} hidden={} space={} spaces={:?}",
            values.archive_hours,
            values.ctrl_t.key(),
            values.provider.as_str(),
            values.width,
            values.hidden,
            card.space,
            values
                .spaces
                .iter()
                .map(|info| format!("{}|{:06x}|{}|{}", info.name, info.tint, info.browser.key(), info.profile))
                .collect::<Vec<_>>()
        ))
    });
    if let Some(line) = line {
        crate::trace::trace(&line);
    }
}

/// Starts typing in a text row: the field takes the row's value and the keyboard.
unsafe fn begin_edit(window: HWND, row: Row) {
    let placed = with_card(window, |card| {
        let values = card.values.as_ref()?;
        if row.kind() != Kind::Text || !enabled(values, card.space, row) {
            return None;
        }
        let text = shown(values, card.space, row);
        let rect = field_rect(window, row)?;
        let field = card.field.as_mut()?;
        field.place(rect, TEXT_SIZE, crate::frame::layout_dpi(window));
        // The browser turns the input method off while its page has no text field focused. A
        // text row is a text field, so it gets the window's default input context back.
        ImmAssociateContextEx(window, std::ptr::null_mut(), IACE_DEFAULT);
        field.activate();
        field.set_text(&text);
        field.select_all();
        card.editing = Some(row);
        card.focus = row;
        card.notice = None;
        card.generation += 1;
        Some(())
    });
    if placed.is_none() {
        with_card(window, |card| card.editing = None);
    }
}

/// Takes what was typed in the text row being edited. False when the text was refused, and the
/// row then stays in typing with the reason shown.
unsafe fn end_edit(window: HWND) -> bool {
    let Some((row, space, text, values)) = with_card(window, |card| {
        let row = card.editing?;
        let text = card.field.as_ref()?.text();
        Some((row, card.space, text, card.values.clone()?))
    }) else {
        return true;
    };
    match commit_text(&values, space, row, &text) {
        Ok(change) => {
            with_card(window, |card| {
                if let Some(field) = card.field.as_ref() {
                    field.send(0x0008, 0, 0); // WM_KILLFOCUS
                }
                card.editing = None;
                card.notice = None;
                card.generation += 1;
            });
            if let Some(change) = change {
                apply(window, change);
            }
            true
        }
        Err(reason) => {
            with_card(window, |card| {
                card.notice = Some(reason);
                card.generation += 1;
            });
            false
        }
    }
}

/// Moves the keyboard to another row, taking any text typed first.
unsafe fn move_focus(window: HWND, down: bool) {
    if !end_edit(window) {
        return;
    }
    let row = with_card(window, |card| {
        let values = card.values.as_ref()?;
        let row = next_row(values, card.space, card.focus, down);
        card.focus = row;
        card.generation += 1;
        Some(row)
    });
    if let Some(row) = row.filter(|row| row.kind() == Kind::Text) {
        begin_edit(window, row);
    }
}

/// One step back or forward on a row, from an arrow key or an arrow button.
unsafe fn step_row(window: HWND, row: Row, forward: bool) {
    if row == Row::Space {
        with_card(window, |card| {
            if let Some(values) = card.values.as_ref() {
                card.space = step_space(values, card.space, forward);
                card.generation += 1;
            }
        });
        trace_values(window);
        crate::frame::update_appbar(window);
        return;
    }
    let change = with_card(window, |card| {
        card.values
            .as_ref()
            .and_then(|values| step(values, card.space, row, forward))
    });
    if let Some(change) = change {
        apply(window, change);
    }
}

unsafe fn press_choice(window: HWND, row: Row, choice: usize) {
    let change = with_card(window, |card| {
        card.values
            .as_ref()
            .and_then(|values| choose(values, row, choice))
    });
    if let Some(change) = change {
        apply(window, change);
    }
}

/// The hit under a client point, in the card's own terms.
fn hit_at(window: HWND, point: POINT) -> Hit {
    let (Some([sx, sy, _, _]), Some(card)) = (scrim(window), card_of(window)) else {
        return Hit::Outside;
    };
    let f = factor(window);
    hit(card, point.x as f32 / f - sx, point.y as f32 / f - sy)
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
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
const WM_IME_COMPOSITION: u32 = 0x010F;
const WM_IME_SETCONTEXT: u32 = 0x0281;
const WM_IME_NOTIFY: u32 = 0x0282;
const WM_IME_CHAR: u32 = 0x0286;
const WM_IME_REQUEST: u32 = 0x0288;
const WM_INPUTLANGCHANGE: u32 = 0x0051;

const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;

/// A key press while the card is open and no text row is being typed in.
unsafe fn key(window: HWND, vk: u16, shift: bool) {
    let focus = with_card(window, |card| card.focus);
    match vk {
        VK_ESCAPE => close(window),
        VK_UP | VK_DOWN => move_focus(window, vk == VK_DOWN),
        VK_TAB => move_focus(window, !shift),
        VK_LEFT | VK_RIGHT => {
            let forward = vk == VK_RIGHT;
            match focus.kind() {
                Kind::Choice => press_choice(window, focus, forward as usize),
                Kind::Stepper => step_row(window, focus, forward),
                Kind::Text => begin_edit(window, focus),
            }
        }
        VK_RETURN | VK_SPACE => match focus.kind() {
            Kind::Text => begin_edit(window, focus),
            Kind::Choice => step_row(window, focus, true),
            Kind::Stepper => step_row(window, focus, true),
        },
        _ => {}
    }
    crate::frame::update_appbar(window);
}

/// A key press while a text row is being typed in. `None` leaves it to the field.
unsafe fn editing_key(window: HWND, vk: u16, shift: bool) -> Option<()> {
    match vk {
        VK_RETURN => {
            end_edit(window);
        }
        VK_ESCAPE => {
            // The typed text is dropped and the row shows its value again.
            with_card(window, |card| {
                if let Some(field) = card.field.as_ref() {
                    field.send(WM_KILLFOCUS, 0, 0);
                }
                card.editing = None;
                card.notice = None;
                card.generation += 1;
            });
        }
        VK_UP | VK_DOWN => move_focus(window, vk == VK_DOWN),
        VK_TAB => move_focus(window, !shift),
        _ => return None,
    }
    crate::frame::update_appbar(window);
    Some(())
}

/// Handles a message of the browser window for the card. `Some` means the card used it and the
/// browser does not see it. While the card is open it takes the keyboard and the pointer over the
/// page, as the command bar does.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if busy() || !is_open(window) {
        return None;
    }
    let editing = with_card(window, |card| card.editing.is_some());
    match message {
        WM_MOUSEMOVE => {
            let now = hit_at(window, point_of(lparam));
            let changed = with_card(window, |card| {
                if editing
                    && (now == card.editing.map_or(Hit::Outside, Hit::Text) || wparam & 1 != 0)
                {
                    if let Some(field) = card.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                }
                let changed = card.hover != Some(now);
                card.hover = Some(now);
                card.generation += changed as u64;
                changed
            });
            if changed || editing {
                crate::frame::update_appbar(window);
            }
            Some(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let now = hit_at(window, point_of(lparam));
            let into_field = with_card(window, |card| {
                card.press = Some(now);
                editing && card.editing.map(Hit::Text) == Some(now)
            });
            if into_field {
                with_card(window, |card| {
                    if let Some(field) = card.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                    card.generation += 1;
                });
            }
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_LBUTTONUP => {
            let now = hit_at(window, point_of(lparam));
            let (pressed, in_field) = with_card(window, |card| {
                let in_field = editing && card.editing.map(Hit::Text) == Some(now);
                if editing {
                    if let Some(field) = card.field.as_ref() {
                        field.send(message, wparam, lparam);
                    }
                }
                (card.press.take(), in_field)
            });
            if !in_field {
                if let Some(pressed) = pressed.filter(|pressed| *pressed == now) {
                    // A press elsewhere takes the typed text first; refused text keeps the row.
                    if end_edit(window) {
                        match pressed {
                            Hit::Close | Hit::Outside => close(window),
                            Hit::Choice(row, choice) => {
                                with_card(window, |card| card.focus = row);
                                press_choice(window, row, choice);
                            }
                            Hit::Step(row, forward) => {
                                with_card(window, |card| card.focus = row);
                                step_row(window, row, forward);
                            }
                            Hit::Text(row) => begin_edit(window, row),
                            Hit::Row(row) => with_card(window, |card| {
                                let can = card
                                    .values
                                    .as_ref()
                                    .is_some_and(|values| enabled(values, card.space, row));
                                if can {
                                    card.focus = row;
                                    card.generation += 1;
                                }
                            }),
                            Hit::Card => {}
                        }
                    }
                }
            }
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_MOUSEWHEEL | WM_RBUTTONDOWN | WM_RBUTTONUP => Some(0),
        WM_KEYDOWN => {
            let shift = GetKeyState(0x10) < 0;
            let vk = wparam as u16;
            if !editing {
                key(window, vk, shift);
                return Some(0);
            }
            if editing_key(window, vk, shift).is_some() {
                return Some(0);
            }
            let used = forward(window, message, wparam, lparam);
            crate::frame::update_appbar(window);
            used.or(Some(0))
        }
        WM_CHAR => {
            if !editing || matches!(wparam, 0x0d | 0x1b | 0x09 | 0x0a) {
                return Some(0);
            }
            let used = forward(window, message, wparam, lparam);
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
            if !editing {
                return if message == WM_INPUTLANGCHANGE {
                    None
                } else {
                    Some(0)
                };
            }
            let used = forward(window, message, wparam, lparam);
            crate::frame::update_appbar(window);
            if message == WM_INPUTLANGCHANGE {
                None
            } else {
                used
            }
        }
        WM_IME_SETCONTEXT => {
            if editing {
                forward(window, message, wparam, lparam);
            }
            None
        }
        WM_TIMER if (TIMER_BASE..=TIMER_LAST).contains(&wparam) => {
            // Most of the field's timers blink the caret, which moves only the caret's own visual.
            let before = with_card(window, |card| card.generation);
            forward(window, message, wparam - TIMER_BASE, 0);
            let after = with_card(window, |card| card.generation);
            if crate::features::field_caret::only_caret(before, after) {
                crate::frame::update_card_caret(window, crate::appbar::caret::Which::Settings);
            } else {
                crate::frame::update_appbar(window);
            }
            Some(0)
        }
        WM_KILLFOCUS | WM_SETFOCUS => {
            if editing {
                forward(window, message, wparam, lparam);
                crate::frame::update_appbar(window);
            }
            None
        }
        _ => None,
    }
}

unsafe fn forward(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let (used, caret) = with_card(window, |card| {
        let field = card.field.as_ref()?;
        let used = field.send(message, wparam, lparam);
        if field.take_changed() {
            card.generation += 1;
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

/// Puts the input method's composition and candidate windows at the caret, as the command bar does.
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

/// Presses a key on the open card as the keyboard would, for tests that may not press keys.
/// `text` puts text in the field of a row being typed in.
pub(crate) unsafe fn key_for_test(window: HWND, name: &str, text: &str) -> bool {
    if !is_open(window) {
        return false;
    }
    let vk = match name {
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "enter" => VK_RETURN,
        "esc" => VK_ESCAPE,
        "tab" => VK_TAB,
        "type" => {
            let typed = with_card(window, |card| {
                card.editing?;
                card.field.as_ref().map(|field| field.set_text(text))
            });
            crate::frame::update_appbar(window);
            return typed.is_some();
        }
        _ => return false,
    };
    let editing = with_card(window, |card| card.editing.is_some());
    if !editing || editing_key(window, vk, false).is_none() {
        key(window, vk, false);
    }
    let focus = with_card(window, |card| card.focus);
    crate::trace::trace(&format!("settings: key {name}: focus {focus:?}"));
    trace_values(window);
    true
}

/// Forgets a closed window.
pub(crate) fn forget(window: HWND) {
    CARDS.with(|cards| cards.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space(name: &str, tint: u32, browser: BrowserKind, profile: &str) -> SpaceInfo {
        SpaceInfo {
            name: name.into(),
            tint,
            browser,
            profile: profile.into(),
        }
    }

    fn values() -> Values {
        Values {
            archive_hours: Some(12),
            ctrl_t: CtrlT::CommandBar,
            provider: CredentialProvider::Browser,
            width: 276,
            hidden: false,
            spaces: vec![
                space("Work", TINTS[0], BrowserKind::Chrome, "Work"),
                space("Research", TINTS[1], BrowserKind::Edge, "Lab"),
            ],
            current: 0,
            browsers: vec![BrowserKind::Chrome, BrowserKind::Edge, BrowserKind::Brave],
        }
    }

    #[test]
    fn fr3_archive_period_steps_through_the_offered_periods_and_never() {
        let mut v = values();
        assert_eq!(shown(&v, 0, Row::Archive), "12 hours", "the default period");
        assert_eq!(
            step(&v, 0, Row::Archive, true),
            Some(Change::Archive(Some(24)))
        );
        assert_eq!(
            step(&v, 0, Row::Archive, false),
            Some(Change::Archive(Some(4)))
        );
        v.archive_hours = Some(720);
        assert_eq!(shown(&v, 0, Row::Archive), "30 days");
        assert_eq!(step(&v, 0, Row::Archive, true), Some(Change::Archive(None)));
        v.archive_hours = None;
        assert_eq!(shown(&v, 0, Row::Archive), "Never");
        assert_eq!(
            step(&v, 0, Row::Archive, true),
            None,
            "never is the last period"
        );
        v.archive_hours = Some(1);
        assert_eq!(
            step(&v, 0, Row::Archive, false),
            None,
            "one hour is the shortest"
        );
        // A period saved by another build snaps to the nearest offered one.
        v.archive_hours = Some(30);
        assert_eq!(
            step(&v, 0, Row::Archive, true),
            Some(Change::Archive(Some(72)))
        );
    }

    #[test]
    fn fr3_ctrl_t_setting_is_chosen_in_rivet_settings() {
        let v = values();
        assert_eq!(chosen(&v, Row::CtrlT), Some(0));
        assert_eq!(
            choose(&v, Row::CtrlT, 1),
            Some(Change::CtrlT(CtrlT::NewTabPage))
        );
        assert_eq!(
            choose(&v, Row::CtrlT, 0),
            None,
            "the chosen one changes nothing"
        );
        assert_eq!(
            step(&v, 0, Row::CtrlT, true),
            Some(Change::CtrlT(CtrlT::NewTabPage))
        );
    }

    #[test]
    fn nfr8_rivet_settings_choose_the_provider_and_say_it_applies_from_the_next_start() {
        let v = values();
        assert_eq!(chosen(&v, Row::Credentials), Some(0));
        assert_eq!(
            choose(&v, Row::Credentials, 1),
            Some(Change::Provider(CredentialProvider::System))
        );
        assert_eq!(choices(Row::Credentials), ["Browser", "Windows"]);
        assert!(Row::Credentials
            .note()
            .is_some_and(|note| note.contains("next start")));
    }

    #[test]
    fn fr3_appbar_width_steps_inside_200_to_420_and_hides() {
        let mut v = values();
        assert_eq!(step(&v, 0, Row::Width, true), Some(Change::Width(296)));
        v.width = 420;
        assert_eq!(step(&v, 0, Row::Width, true), None);
        v.width = 210;
        assert_eq!(step(&v, 0, Row::Width, false), Some(Change::Width(200)));
        assert_eq!(choose(&v, Row::Hidden, 1), Some(Change::Hidden(true)));
        assert_eq!(step(&v, 0, Row::Hidden, true), Some(Change::Hidden(true)));
    }

    #[test]
    fn fr3_each_space_gets_a_name_colour_browser_and_profile() {
        let v = values();
        assert_eq!(step_space(&v, 0, true), 1);
        assert_eq!(step_space(&v, 0, false), 1);
        assert_eq!(
            step(&v, 1, Row::SpaceColour, true),
            Some(Change::Tint(1, TINTS[2]))
        );
        assert_eq!(
            step(&v, 1, Row::SpaceBrowser, true),
            Some(Change::Browser(1, BrowserKind::Brave))
        );
        assert_eq!(
            commit_text(&v, 1, Row::SpaceName, "  연구  "),
            Ok(Some(Change::Name(1, "연구".into())))
        );
        assert_eq!(
            commit_text(&v, 1, Row::SpaceProfile, "Lab 2"),
            Ok(Some(Change::Profile(1, "Lab 2".into())))
        );
        assert_eq!(commit_text(&v, 1, Row::SpaceName, "Research"), Ok(None));
    }

    #[test]
    fn fr3_current_space_keeps_the_windows_browser_and_profile() {
        let v = values();
        assert!(!enabled(&v, 0, Row::SpaceBrowser));
        assert!(!enabled(&v, 0, Row::SpaceProfile));
        assert_eq!(step(&v, 0, Row::SpaceBrowser, true), None);
        assert!(enabled(&v, 0, Row::SpaceName));
        // A window of a browser no space can be based on chooses no browser for any space.
        let mut sole = values();
        sole.browsers.clear();
        assert!(!enabled(&sole, 1, Row::SpaceBrowser));
        assert_eq!(shown(&sole, 1, Row::SpaceBrowser), "This browser");
    }

    #[test]
    fn fr3_space_names_are_unique_and_profiles_are_folder_safe() {
        let v = values();
        assert_eq!(
            commit_text(&v, 1, Row::SpaceName, "work"),
            Err("Another space has this name")
        );
        assert!(commit_text(&v, 1, Row::SpaceName, "   ").is_err());
        assert!(commit_text(&v, 1, Row::SpaceName, &"x".repeat(MAX_NAME + 1)).is_err());
        assert!(commit_text(&v, 1, Row::SpaceProfile, "..\\other").is_err());
        assert!(commit_text(&v, 1, Row::SpaceProfile, "").is_err());
    }

    #[test]
    fn nfr5_settings_are_walked_with_the_keyboard_past_rows_that_cannot_change() {
        let v = values();
        assert_eq!(next_row(&v, 0, Row::Archive, true), Row::CtrlT);
        assert_eq!(next_row(&v, 0, Row::Archive, false), Row::SpaceColour);
        assert_eq!(
            next_row(&v, 0, Row::SpaceColour, true),
            Row::Archive,
            "the current space's browser and profile are skipped"
        );
        assert_eq!(next_row(&v, 1, Row::SpaceColour, true), Row::SpaceBrowser);
        let mut one = values();
        one.spaces.truncate(1);
        assert_eq!(next_row(&one, 0, Row::Hidden, true), Row::SpaceName);
    }

    #[test]
    fn nfr5_settings_card_hit_finds_every_control() {
        let c = card(1200.0, 900.0);
        assert_eq!(c[2], CARD_WIDTH);
        assert!(c[1] + c[3] <= 900.0);
        let centre = |row: Row| {
            let [x, y, w, h] = value_box(c, row);
            (x, y + h / 2.0, w)
        };
        let (x, y, w) = centre(Row::CtrlT);
        assert_eq!(hit(c, x + 2.0, y), Hit::Choice(Row::CtrlT, 0));
        assert_eq!(hit(c, x + w - 2.0, y), Hit::Choice(Row::CtrlT, 1));
        let (x, y, w) = centre(Row::Width);
        assert_eq!(hit(c, x + 2.0, y), Hit::Step(Row::Width, false));
        assert_eq!(hit(c, x + w - 2.0, y), Hit::Step(Row::Width, true));
        let (x, y, _) = centre(Row::SpaceName);
        assert_eq!(hit(c, x + 40.0, y), Hit::Text(Row::SpaceName));
        assert_eq!(hit(c, c[0] + 30.0, y), Hit::Row(Row::SpaceName));
        let [bx, by, bw, bh] = close_box(c);
        assert_eq!(hit(c, bx + bw / 2.0, by + bh / 2.0), Hit::Close);
        assert_eq!(hit(c, 2.0, 2.0), Hit::Outside);
        // Rows never overlap.
        let tops = row_tops();
        for pair in tops.windows(2) {
            assert!(pair[1].1 >= pair[0].1 + ROW_HEIGHT);
        }
    }
}
