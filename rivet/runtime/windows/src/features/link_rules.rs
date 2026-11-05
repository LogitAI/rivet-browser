//! Link rules in a Rivet window: where links from other apps and from pages open, and the panel
//! that shows and changes the rules.
//!
//! The rules live in one file of the state folder (`rivet_core::link_rules`), which the `rivet`
//! command edits too, so the file is read again each time a link arrives or the panel opens. A
//! link that matches no rule opens as a new tab of the current space.
//!
//! Links arrive two ways. The `rivet session open` command sends an address as copied data to the
//! window, and a page can open a link in a new window, which `link_tabs` takes into this window
//! and hands here. The panel is the study's `#rules` card, drawn on the window's composition
//! surface by `appbar`, with its input taken from the window's own messages.

use std::cell::RefCell;
use std::collections::HashMap;

use rivet_core::link_rules::{LinkRules, Pattern, Target};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

/// The rules file. A test profile's own debug folder holds its own rules, so tests that run side
/// by side never read or change the rules of the person at the machine.
fn rules_path() -> Option<std::path::PathBuf> {
    crate::frame::own_debug_directory()
        .map(|dir| dir.join("link-rules.json"))
        .or_else(|| rivet_core::paths::Layout::current().map(|layout| layout.link_rules_file()))
}

fn load() -> LinkRules {
    rules_path()
        .map(|path| LinkRules::load(&path))
        .unwrap_or_default()
}

fn save(rules: &LinkRules) {
    let Some(path) = rules_path() else {
        return;
    };
    if let Err(error) = rules.save(&path) {
        crate::trace::trace(&format!("link rules: could not save: {error}"));
    }
}

// --- Routing -----------------------------------------------------------------------------------

/// What to do with a link that a page opened in a window of its own, after its tab came into the
/// Rivet window as the active tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AfterMove {
    /// The tab stays where it is: a new tab of the current space.
    Stay,
    /// The tab floats as the Mini Window.
    Float,
    /// The tab leaves for this space.
    ToSpace(String),
}

/// What happens to a moved tab with address `url` under `rules`, when the window's current space
/// is `current`.
pub(crate) fn after_move(rules: &LinkRules, url: &str, current: &str) -> AfterMove {
    match rules.route(url) {
        Target::CurrentSpace => AfterMove::Stay,
        Target::Space(name) if name.eq_ignore_ascii_case(current) => AfterMove::Stay,
        Target::Space(name) => AfterMove::ToSpace(name),
        Target::MiniWindow => AfterMove::Float,
    }
}

/// Opens a link sent to the window, as its rules say.
pub(crate) unsafe fn open_request(window: HWND, url: &str) -> bool {
    let target = load().route(url);
    let done = deliver(window, url, &target);
    crate::trace::trace(&format!("link rules: open {url} to {target:?}: {done}"));
    done
}

/// Takes a `WM_COPYDATA` message that asks to open a link. `Some(1)` tells the sender the link
/// was taken; data of another kind is left to the browser.
pub(crate) unsafe fn copy_data(window: HWND, lparam: LPARAM) -> Option<LRESULT> {
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    let copy = (lparam as *const COPYDATASTRUCT).as_ref()?;
    if copy.dwData != rivet_protocol::window::OPEN_LINK_DATA {
        return None;
    }
    let length = copy.cbData as usize;
    if copy.lpData.is_null() || length > rivet_protocol::window::MAX_LINK_BYTES + 4 {
        return Some(0);
    }
    let bytes = std::slice::from_raw_parts(copy.lpData as *const u8, length);
    let Some(url) = rivet_protocol::window::decode_open_link(copy.dwData, bytes) else {
        return Some(0);
    };
    // The sender waits on this message. It is answered before the tab opens, so the browser's own
    // work that follows never runs while another process is blocked on it.
    windows_sys::Win32::UI::WindowsAndMessaging::ReplyMessage(1);
    open_request(window, &url);
    Some(1)
}

/// Opens `url` at `target`. A space this window cannot show, such as one of another browser,
/// gets the link as a new tab of the current space rather than losing it.
unsafe fn deliver(window: HWND, url: &str, target: &Target) -> bool {
    let new_tab =
        || crate::features::tabs::new_tab(window) && crate::features::navigation::open(window, url);
    match target {
        Target::CurrentSpace => new_tab(),
        Target::MiniWindow => new_tab() && crate::features::mini_window::pop_out_active(window),
        Target::Space(name) => {
            if !crate::frame::show_space_named(window, name) {
                crate::trace::trace(&format!(
                    "link rules: space {name:?} is not in this window, opening here"
                ));
            }
            new_tab()
        }
    }
}

/// Applies the rules to a tab `link_tabs` just took from a window a page opened. The tab is the
/// window's active tab now.
pub(crate) unsafe fn after_link_moved(window: HWND, url: &str) {
    let current = crate::frame::current_space_name(window).unwrap_or_default();
    let what = after_move(&load(), url, &current);
    let done = match &what {
        AfterMove::Stay => return,
        AfterMove::Float => crate::features::mini_window::pop_out_active(window),
        AfterMove::ToSpace(name) => {
            // The page cannot move to a space's tab list as it is, so its address opens there
            // and the moved tab closes. A space this window cannot show keeps the tab here.
            let active = crate::features::tabs::tabs(window)
                .and_then(|tabs| tabs.into_iter().find(|tab| tab.active))
                .map(|tab| tab.index);
            match active {
                Some(tab) if crate::frame::has_space_named(window, name) => {
                    crate::features::tabs::close_tab(window, tab)
                        && deliver(window, url, &Target::Space(name.clone()))
                }
                _ => false,
            }
        }
    };
    crate::trace::trace(&format!("link rules: moved link {url} {what:?}: {done}"));
}

// --- The panel ---------------------------------------------------------------------------------

/// The study's `#rules`: 350 DIPs wide, 18 from the page's right edge, with 16 around its text.
pub(crate) const WIDTH: f32 = 350.0;
pub(crate) const RIGHT: f32 = 18.0;
pub(crate) const TOP: f32 = 12.0;
pub(crate) const PAD: f32 = 16.0;
pub(crate) const TITLE_HEIGHT: f32 = 26.0;
pub(crate) const NOTE_HEIGHT: f32 = 24.0;
pub(crate) const ROW_HEIGHT: f32 = 34.0;
pub(crate) const FOOTER_HEIGHT: f32 = 40.0;
pub(crate) const BUTTON_HEIGHT: f32 = 26.0;
pub(crate) const BUTTON_WIDTH: f32 = 112.0;
pub(crate) const RADIUS: f32 = 14.0;
/// The glyph box that removes a rule, at the right end of its row.
pub(crate) const REMOVE: f32 = 18.0;
/// The most rules the panel shows. The `rivet link-rules list` command shows every rule.
pub(crate) const ROWS_SHOWN: usize = 10;

/// What lies under a point of the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    /// The target of rule N, which a click moves to the next target.
    Target(usize),
    Remove(usize),
    AddSite,
    Inside,
    Outside,
}

/// The card's height for `rules` rule rows, with the row for everything else below them.
pub(crate) fn height(rules: usize) -> f32 {
    PAD + TITLE_HEIGHT
        + NOTE_HEIGHT
        + ROW_HEIGHT * (rules.min(ROWS_SHOWN) + 1) as f32
        + FOOTER_HEIGHT
        + PAD / 2.0
}

/// The card in DIPs from the page panel's top left corner, for a panel `width` wide.
pub(crate) fn card(width: f32, rules: usize) -> [f32; 4] {
    let w = WIDTH.min(width - 2.0 * RIGHT).max(0.0);
    [(width - RIGHT - w).max(0.0), TOP, w, height(rules)]
}

/// The top of rule row `index` inside the card.
pub(crate) fn row_top(index: usize) -> f32 {
    PAD + TITLE_HEIGHT + NOTE_HEIGHT + ROW_HEIGHT * index as f32
}

/// The close glyph's box inside the card.
pub(crate) fn close_box(width: f32) -> [f32; 4] {
    [
        width - PAD - REMOVE,
        PAD + (TITLE_HEIGHT - REMOVE) / 2.0,
        REMOVE,
        REMOVE,
    ]
}

/// The "Add this site" button's box inside the card.
pub(crate) fn add_box(rules: usize) -> [f32; 4] {
    let top = row_top(rules.min(ROWS_SHOWN) + 1);
    [
        PAD,
        top + (FOOTER_HEIGHT - BUTTON_HEIGHT) / 2.0,
        BUTTON_WIDTH,
        BUTTON_HEIGHT,
    ]
}

fn inside(area: [f32; 4], x: f32, y: f32) -> bool {
    x >= area[0] && x < area[0] + area[2] && y >= area[1] && y < area[1] + area[3]
}

/// The hit at (`x`, `y`) DIPs from the page panel's corner, for a panel `width` wide.
pub(crate) fn hit(width: f32, rules: usize, x: f32, y: f32) -> Hit {
    let [cx, cy, cw, ch] = card(width, rules);
    if !inside([cx, cy, cw, ch], x, y) {
        return Hit::Outside;
    }
    let (x, y) = (x - cx, y - cy);
    if inside(close_box(cw), x, y) {
        return Hit::Close;
    }
    if inside(add_box(rules), x, y) {
        return Hit::AddSite;
    }
    for index in 0..rules.min(ROWS_SHOWN) {
        let top = row_top(index);
        if y < top || y >= top + ROW_HEIGHT {
            continue;
        }
        if x >= cw - PAD - REMOVE {
            return Hit::Remove(index);
        }
        if x >= cw / 2.0 {
            return Hit::Target(index);
        }
    }
    Hit::Inside
}

/// The host of a web address, the pattern "Add this site" makes.
pub(crate) fn site_of(url: &str) -> Option<Pattern> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = host.split(':').next()?;
    Pattern::parse(host.trim_start_matches("www.")).ok()
}

/// What the appbar draws: the card's place in client pixels and its rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Each rule's pattern and target, as shown.
    pub rows: Vec<(String, String)>,
    pub hover: Option<Hit>,
}

struct Panel {
    open: bool,
    rules: LinkRules,
    hover: Option<Hit>,
    press: Option<Hit>,
}

thread_local! {
    static PANELS: RefCell<HashMap<isize, Panel>> = RefCell::new(HashMap::new());
}

fn with_panel<R>(window: HWND, work: impl FnOnce(&mut Panel) -> R) -> R {
    PANELS.with(|panels| {
        let mut panels = panels.borrow_mut();
        let panel = panels.entry(window as isize).or_insert_with(|| Panel {
            open: false,
            rules: LinkRules::default(),
            hover: None,
            press: None,
        });
        work(panel)
    })
}

pub(crate) fn is_open(window: HWND) -> bool {
    PANELS.with(|panels| {
        panels
            .try_borrow()
            .map(|panels| {
                panels
                    .get(&(window as isize))
                    .is_some_and(|panel| panel.open)
            })
            .unwrap_or(false)
    })
}

fn factor(window: HWND) -> f32 {
    unsafe { crate::frame::layout_dpi(window) as f32 / 96.0 }
}

/// The page panel in DIPs: left, top, width and height.
fn page_panel(window: HWND) -> Option<[f32; 4]> {
    let panel = crate::features::frame_layout::panel_bounds(window)?;
    Some([
        panel.x as f32,
        panel.y as f32,
        panel.width as f32,
        panel.height as f32,
    ])
}

pub(crate) fn view(window: HWND) -> Option<View> {
    let [px, py, pw, _] = page_panel(window)?;
    let f = factor(window);
    with_panel(window, |panel| {
        if !panel.open {
            return None;
        }
        let count = panel.rules.rules.len();
        let [cx, cy, cw, ch] = card(pw, count);
        Some(View {
            x: (px + cx) * f,
            y: (py + cy) * f,
            width: cw * f,
            height: ch * f,
            rows: panel
                .rules
                .rules
                .iter()
                .take(ROWS_SHOWN)
                .map(|rule| (rule.pattern.label(), rule.target.label()))
                .collect(),
            hover: panel.hover,
        })
    })
}

/// Opens the panel with the rules as the file holds them now.
pub(crate) unsafe fn open(window: HWND) -> bool {
    if !crate::frame::attached_window(window) {
        return false;
    }
    let rules = load();
    with_panel(window, |panel| {
        panel.open = true;
        panel.rules = rules;
        panel.hover = None;
        panel.press = None;
    });
    crate::trace::trace("link rules: open");
    crate::frame::update_appbar(window);
    true
}

pub(crate) unsafe fn close(window: HWND) {
    with_panel(window, |panel| {
        panel.open = false;
        panel.hover = None;
        panel.press = None;
    });
    crate::frame::update_appbar(window);
}

pub(crate) fn forget(window: HWND) {
    PANELS.with(|panels| panels.borrow_mut().remove(&(window as isize)));
}

/// Runs what a click on `hit` asks for, then saves the rules and redraws.
unsafe fn act(window: HWND, hit: Hit) {
    match hit {
        Hit::Close => return close(window),
        Hit::Target(index) => {
            let spaces = crate::frame::space_names(window);
            with_panel(window, |panel| {
                if let Some(rule) = panel.rules.rules.get_mut(index) {
                    rule.target = rule.target.next(&spaces);
                }
            });
        }
        Hit::Remove(index) => with_panel(window, |panel| {
            if index < panel.rules.rules.len() {
                panel.rules.rules.remove(index);
            }
        }),
        Hit::AddSite => {
            let url = crate::features::navigation::active_contents(window)
                .map(|contents| crate::features::tabs::page_address(contents))
                .unwrap_or_default();
            let Some(pattern) = site_of(&url) else {
                crate::trace::trace(&format!("link rules: {url:?} is not a site"));
                return;
            };
            let target = crate::frame::current_space_name(window)
                .map_or(Target::CurrentSpace, Target::Space);
            let added = with_panel(window, |panel| panel.rules.set(pattern, target));
            if let Err(error) = added {
                crate::trace::trace(&format!("link rules: {error}"));
                return;
            }
        }
        Hit::Inside | Hit::Outside => return,
    }
    let rules = with_panel(window, |panel| panel.rules.clone());
    save(&rules);
    crate::trace::trace(&format!(
        "link rules: {hit:?}, now {} rules",
        rules.rules.len()
    ));
    crate::frame::update_appbar(window);
}

/// Runs a click on `hit` as the tests drive it.
pub(crate) unsafe fn click_for_test(window: HWND, hit: Hit) -> bool {
    if !is_open(window) {
        return false;
    }
    act(window, hit);
    true
}

/// The rules as the panel holds them, for the trace the tests read.
pub(crate) fn describe(window: HWND) -> String {
    with_panel(window, |panel| {
        panel
            .rules
            .rules
            .iter()
            .map(|rule| format!("{} -> {}", rule.pattern.label(), rule.target.label()))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

const WM_KEYDOWN: u32 = 0x0100;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const VK_ESCAPE: usize = 0x1B;

/// Handles a message of the browser window for the panel. `Some` means the panel used it and
/// the browser does not see it. A press outside the card closes the panel and is used up, so the
/// page under it does not take a click the person meant for the panel.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if !is_open(window) {
        return None;
    }
    if message == WM_KEYDOWN && wparam == VK_ESCAPE {
        close(window);
        return Some(0);
    }
    if !matches!(
        message,
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK
    ) {
        return None;
    }
    let [px, py, pw, _] = page_panel(window)?;
    let f = factor(window);
    let x = (lparam & 0xffff) as i16 as f32 / f - px;
    let y = ((lparam >> 16) & 0xffff) as i16 as f32 / f - py;
    let count = with_panel(window, |panel| panel.rules.rules.len());
    let at = hit(pw, count, x, y);
    match message {
        WM_MOUSEMOVE => {
            let shown = (at != Hit::Outside && at != Hit::Inside).then_some(at);
            if with_panel(window, |panel| std::mem::replace(&mut panel.hover, shown)) != shown {
                crate::frame::update_appbar(window);
            }
            (at != Hit::Outside).then_some(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            if at == Hit::Outside {
                close(window);
            } else {
                with_panel(window, |panel| panel.press = Some(at));
            }
            Some(0)
        }
        _ => {
            let pressed = with_panel(window, |panel| panel.press.take());
            if pressed == Some(at) {
                act(window, at);
            }
            (pressed.is_some() || at != Hit::Outside).then_some(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(list: &[(&str, Target)]) -> LinkRules {
        let mut rules = LinkRules::default();
        for (pattern, target) in list {
            rules
                .set(Pattern::parse(pattern).unwrap(), target.clone())
                .unwrap();
        }
        rules
    }

    #[test]
    fn fr3_a_new_window_link_follows_its_rule_and_otherwise_stays_a_tab_of_this_space() {
        let list = rules(&[
            ("github.com", Target::Space("Work".into())),
            ("calendar.google.com", Target::MiniWindow),
            ("example.com", Target::CurrentSpace),
        ]);
        assert_eq!(
            after_move(&list, "https://other.example/", "Research"),
            AfterMove::Stay
        );
        assert_eq!(
            after_move(&list, "https://example.com/", "Research"),
            AfterMove::Stay
        );
        assert_eq!(
            after_move(&list, "https://github.com/x", "Research"),
            AfterMove::ToSpace("Work".into())
        );
        assert_eq!(
            after_move(&list, "https://github.com/x", "work"),
            AfterMove::Stay,
            "a link for the space the window shows stays where it is"
        );
        assert_eq!(
            after_move(&list, "https://calendar.google.com/", "Work"),
            AfterMove::Float
        );
    }

    #[test]
    fn fr3_link_rules_panel_sits_at_the_pages_right_like_the_study() {
        let [x, y, w, h] = card(1200.0, 2);
        assert_eq!((x, y, w), (1200.0 - 18.0 - 350.0, TOP, 350.0));
        assert_eq!(h, height(2));
        assert!(height(3) > height(2));
        assert_eq!(height(50), height(ROWS_SHOWN), "the card stops growing");
        let [_, _, narrow, _] = card(300.0, 0);
        assert_eq!(
            narrow,
            300.0 - 36.0,
            "a narrow page keeps the card inside it"
        );
    }

    #[test]
    fn fr3_link_rules_panel_hits_targets_removes_and_the_add_button() {
        let width = 1200.0;
        let [cx, cy, cw, _] = card(width, 2);
        let row = |index: usize| cy + row_top(index) + ROW_HEIGHT / 2.0;
        assert_eq!(hit(width, 2, cx - 5.0, row(0)), Hit::Outside);
        assert_eq!(
            hit(width, 2, cx + 20.0, row(0)),
            Hit::Inside,
            "the pattern is not a button"
        );
        assert_eq!(hit(width, 2, cx + cw - 80.0, row(1)), Hit::Target(1));
        assert_eq!(hit(width, 2, cx + cw - PAD - 4.0, row(0)), Hit::Remove(0));
        assert_eq!(
            hit(width, 2, cx + cw - 80.0, row(2)),
            Hit::Inside,
            "everything else is fixed"
        );
        let [ax, ay, _, ah] = add_box(2);
        assert_eq!(
            hit(width, 2, cx + ax + 4.0, cy + ay + ah / 2.0),
            Hit::AddSite
        );
        let [bx, by, _, _] = close_box(cw);
        assert_eq!(hit(width, 2, cx + bx + 2.0, cy + by + 2.0), Hit::Close);
    }

    #[test]
    fn fr3_add_this_site_makes_a_host_rule_from_the_page() {
        assert_eq!(
            site_of("https://www.github.com:443/LogitAI?x=1"),
            Some(Pattern::Host("github.com".into()))
        );
        assert_eq!(
            site_of("http://user@notion.so/"),
            Some(Pattern::Host("notion.so".into()))
        );
        assert_eq!(site_of("chrome://settings/"), None);
        assert_eq!(site_of("file:///C:/x.html"), None);
    }
}
