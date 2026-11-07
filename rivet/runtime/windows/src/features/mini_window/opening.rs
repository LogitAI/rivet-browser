//! Opening a Mini Window as a Rivet app window of its own.
//!
//! The browser's move-tab-to-new-window command reads the app controller of an app window, and a
//! window opened with `--app=<url>` has none, so the command cannot run in a Rivet window. Rivet
//! asks the browser for one more app window instead, the way any second launch on a profile does:
//! it starts the browser with the same `--user-data-dir`, and the browser's process singleton hands
//! that command line to this process, which opens the window. The window's address is Rivet's start
//! page with a fragment that names this one request, so the runtime knows the window when its tab
//! appears. Then the page's tab moves into it with Chromium's own detach and insert, the functions
//! the way back uses, and the start page's tab closes. When a step cannot run, the new window
//! closes and the page stays a tab of its Rivet window.
//!
//! While the window is being made, the operating system layer creates it at the Mini Window's
//! place, keeps it hidden and refuses its activation, so the person keeps the keyboard where they
//! were and a test window never comes to the front. The opening reaches the browser and the
//! operating system only through their traits, so its tests run against test layers.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use crate::browser::{PageId, TabEntry, TabMoves};
use crate::os::{NewWindows, ScreenRect, WindowId};

/// The start of the fragment that names a Mini Window request.
pub(crate) const MARKER_PREFIX: &str = "rivet-mini-window-";
/// How often the new window is looked for, and the longest wait for it.
pub(crate) const TICK_MS: u32 = 50;
pub(crate) const LONGEST_WAIT_MS: u32 = 10_000;
/// The timer on the Rivet window that looks for the new window. It spells "RIMO".
pub(crate) const OPEN_TIMER: usize = 0x5249_4d4f;

/// The fragment that names request `request` of process `process`.
pub(crate) fn marker(process: u32, request: u32) -> String {
    format!("{MARKER_PREFIX}{process}-{request}")
}

/// The start page address `start` with `marker` as its fragment.
pub(crate) fn marker_url(start: &str, marker: &str) -> String {
    let page = start.split_once('#').map_or(start, |(page, _)| page);
    format!("{page}#{marker}")
}

/// Whether `url` is a start page that a Mini Window request opened.
pub(crate) fn is_marker_url(url: &str) -> bool {
    crate::features::address_bar::is_start_page(url)
        && url
            .split_once('#')
            .is_some_and(|(_, fragment)| fragment.starts_with(MARKER_PREFIX))
}

/// Whether `url` is the start page of request `marker`. The browser may write the rest of the
/// address in its own way, so only the fragment is compared.
pub(crate) fn names(url: &str, marker: &str) -> bool {
    is_marker_url(url) && url.split_once('#').map(|(_, fragment)| fragment) == Some(marker)
}

/// The `--profile-directory` among a process's arguments.
fn profile_directory_of(arguments: &[String]) -> Option<String> {
    arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("--profile-directory=")
            .map(|value| value.trim_matches('"').to_owned())
    })
}

/// The browser command line that asks the running browser of this profile for one more app window
/// at `url`. It names the same user data folder and profile as this process, so the browser's
/// process singleton hands it to this process. A browser started without a user data folder is not
/// Rivet's, and a second launch there could reach another browser, so it gets none.
pub(crate) fn window_arguments(
    own: impl IntoIterator<Item = String>,
    url: &str,
) -> Result<Vec<String>, String> {
    let own: Vec<String> = own.into_iter().collect();
    let directory = crate::spaces::user_data_dir_of(own.iter().cloned())
        .filter(|directory| !directory.is_empty())
        .ok_or("the browser was started without a --user-data-dir")?;
    let mut arguments = vec![format!("--user-data-dir={directory}")];
    if let Some(profile) = profile_directory_of(&own) {
        arguments.push(format!("--profile-directory={profile}"));
    }
    arguments.push(format!("--app={url}"));
    Ok(arguments)
}

/// A browser window of this process as one look sees it, with its tabs.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub window: WindowId,
    pub tabs: Vec<TabEntry>,
}

/// What one look at the opening finds.
#[derive(Clone, Debug)]
pub(crate) struct Look<'a> {
    pub waited_ms: u32,
    /// Whether the Rivet window the page pops out of still has Rivet's frame.
    pub owner_alive: bool,
    /// The page's strip index in the Rivet window, and whether that window shows it.
    pub page: Option<(usize, bool)>,
    pub owner_tabs: usize,
    /// The browser windows of this process that are neither Rivet windows nor Mini Windows.
    pub windows: &'a [Seen],
}

/// What the opening does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Next {
    Wait,
    /// Move the page into `window` and close its start page, `marker_tab`.
    Move {
        window: WindowId,
        marker_tab: PageId,
    },
    /// Stop, close `window` if it came, and keep the page where it is.
    GiveUp {
        window: Option<WindowId>,
        reason: String,
    },
}

/// The window made for request `marker`: the one whose only tab is that request's start page.
pub(crate) fn window_for(windows: &[Seen], marker: &str) -> Option<(WindowId, PageId)> {
    windows.iter().find_map(|seen| match seen.tabs.as_slice() {
        [tab] if names(&tab.url, marker) => Some((seen.window, tab.page)),
        _ => None,
    })
}

/// What the opening does after `look`.
pub(crate) fn next(look: &Look, marker: &str) -> Next {
    let found = window_for(look.windows, marker);
    let stop = |reason: &str| Next::GiveUp {
        window: found.map(|(window, _)| window),
        reason: reason.to_owned(),
    };
    if !look.owner_alive {
        return stop("its Rivet window closed");
    }
    match look.page {
        None => return stop("the page is no longer a tab of its Rivet window"),
        Some((_, true)) => return stop("its Rivet window shows the page now"),
        Some(_) if look.owner_tabs < 2 => {
            return stop("its Rivet window would be left without a tab");
        }
        Some(_) => {}
    }
    match found {
        Some((window, marker_tab)) => Next::Move { window, marker_tab },
        None if look.waited_ms >= LONGEST_WAIT_MS => {
            stop("the browser opened no window for the request in time")
        }
        None => Next::Wait,
    }
}

/// Whether the page and the new window's start page belong to the same profile, which a tab
/// needs to move between windows. A build that records no way to read the profile cannot say, so
/// the tab does not move.
pub(crate) fn same_profile(page: Option<usize>, start_page: Option<usize>) -> Result<(), String> {
    match (page, start_page) {
        (Some(page), Some(start_page)) if page == start_page => Ok(()),
        (Some(_), Some(_)) => Err("the new window has another profile".into()),
        _ => Err("this build records no way to read a page's profile".into()),
    }
}

/// What the Mini Window does when an opening ends: it takes `made`, the new window and its place,
/// when the page moved there, and shows the Rivet window's new state either way.
pub(crate) type Done = fn(owner: WindowId, made: Option<(WindowId, ScreenRect)>);

/// A request for a Mini Window.
pub(crate) struct Request {
    /// The Rivet window the page pops out of.
    pub owner: WindowId,
    pub page: PageId,
    /// Where the Mini Window goes on the screen.
    pub place: ScreenRect,
    /// The address of Rivet's start page for the owner's space.
    pub start_page: String,
    /// The browser program and the arguments it was started with.
    pub program: PathBuf,
    pub own_arguments: Vec<String>,
    /// Whether the owner shows the page again when the opening gives up, for a caller that took it
    /// off the screen to pop it out.
    pub show_again: bool,
    /// Whether a window still has Rivet's frame.
    pub is_rivet: fn(WindowId) -> bool,
    /// Whether a window is a Rivet window or a Mini Window already, so never the new one.
    pub is_taken: fn(WindowId) -> bool,
    pub done: Done,
}

/// The request that is opening. There is one at a time on the browser's UI thread.
struct Pending {
    owner: WindowId,
    page: PageId,
    place: ScreenRect,
    marker: String,
    show_again: bool,
    is_rivet: fn(WindowId) -> bool,
    is_taken: fn(WindowId) -> bool,
    done: Done,
    waited_ms: u32,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
    static REQUESTS: Cell<u32> = const { Cell::new(0) };
}

/// Whether `url` names the request that is opening now.
pub(crate) fn is_pending(url: &str) -> bool {
    PENDING.with(|pending| {
        pending.try_borrow().is_ok_and(|pending| {
            pending
                .as_ref()
                .is_some_and(|pending| names(url, &pending.marker))
        })
    })
}

/// Starts opening a Mini Window for `request`. Returns whether the browser was asked for the
/// window. The rest happens on the Rivet window's timer, and every outcome is traced.
pub(crate) fn start(os: &impl NewWindows, browser: &impl TabMoves, request: Request) -> bool {
    let refuse = |reason: &str| {
        crate::trace::trace(&format!("mini window: kept the tab, {reason}"));
        false
    };
    if PENDING.with(|pending| pending.borrow().is_some()) {
        return refuse("another Mini Window is still opening");
    }
    let number = REQUESTS.with(|count| {
        count.set(count.get().wrapping_add(1));
        count.get()
    });
    let marker = marker(std::process::id(), number);
    let url = marker_url(&request.start_page, &marker);
    let arguments = match window_arguments(request.own_arguments, &url) {
        Ok(arguments) => arguments,
        Err(reason) => return refuse(reason.as_str()),
    };
    if let Err(reason) = os.watch_new_windows(request.place) {
        return refuse(reason.as_str());
    }
    PENDING.with(|pending| {
        *pending.borrow_mut() = Some(Pending {
            owner: request.owner,
            page: request.page,
            place: request.place,
            marker: marker.clone(),
            show_again: request.show_again,
            is_rivet: request.is_rivet,
            is_taken: request.is_taken,
            done: request.done,
            waited_ms: 0,
        })
    });
    // The launch only hands its command line over and exits. The UI thread keeps running its
    // messages meanwhile, because the hand-over is a message to this process, so nothing waits here.
    if let Err(error) = os.launch(&request.program, &arguments) {
        give_up(
            os,
            browser,
            None,
            format!("the browser could not be started: {error}").as_str(),
        );
        return false;
    }
    os.start_ticking(request.owner, OPEN_TIMER, TICK_MS, tick);
    crate::trace::trace(&format!(
        "mini window: asked the browser for window {marker} at {:?}",
        request.place
    ));
    true
}

/// The opening's timer on the Rivet window, with the real layers.
fn tick(owner: WindowId) {
    step(&crate::frame::OS, &crate::frame::BROWSER, owner);
}

/// The browser windows of this process that may be the new window, with their tabs.
fn candidates(
    os: &impl NewWindows,
    browser: &impl TabMoves,
    owner: WindowId,
    is_taken: fn(WindowId) -> bool,
) -> Vec<Seen> {
    os.browser_windows()
        .into_iter()
        .filter(|window| *window != owner && !is_taken(*window))
        .map(|window| Seen {
            window,
            tabs: browser.tab_entries(window).unwrap_or_default(),
        })
        .collect()
}

/// The strip index of `page` in `window`, and whether `window` shows it.
fn place_of(browser: &impl TabMoves, window: WindowId, page: PageId) -> Option<(usize, bool)> {
    browser
        .tab_entries(window)?
        .iter()
        .enumerate()
        .find(|(_, tab)| tab.page == page)
        .map(|(index, tab)| (index, tab.active))
}

/// Looks for the new window once.
pub(crate) fn step(os: &impl NewWindows, browser: &impl TabMoves, owner: WindowId) {
    let Some((page, marker, waited_ms, is_rivet, is_taken)) = PENDING.with(|pending| {
        let mut pending = pending.try_borrow_mut().ok()?;
        let pending = pending.as_mut().filter(|pending| pending.owner == owner)?;
        pending.waited_ms = pending.waited_ms.saturating_add(TICK_MS);
        Some((
            pending.page,
            pending.marker.clone(),
            pending.waited_ms,
            pending.is_rivet,
            pending.is_taken,
        ))
    }) else {
        return;
    };
    let owner_alive = is_rivet(owner);
    let owner_tabs = if owner_alive {
        browser.tab_entries(owner).map_or(0, |tabs| tabs.len())
    } else {
        0
    };
    let windows = candidates(os, browser, owner, is_taken);
    let look = Look {
        waited_ms,
        owner_alive,
        page: owner_alive
            .then(|| place_of(browser, owner, page))
            .flatten(),
        owner_tabs,
        windows: &windows,
    };
    match next(&look, &marker) {
        Next::Wait => {
            // A window that could not be hidden as it was made is hidden as soon as it is seen.
            let made = os.watched();
            for seen in windows.iter().filter(|seen| made.contains(&seen.window)) {
                os.set_hidden(seen.window, true);
            }
        }
        Next::Move { window, marker_tab } => finish(os, browser, window, marker_tab),
        Next::GiveUp { window, reason } => give_up(os, browser, window, &reason),
    }
}

/// Moves the page into the new window, closes its start page and hands the window to the Mini
/// Window.
fn finish(os: &impl NewWindows, browser: &impl TabMoves, window: WindowId, marker_tab: PageId) {
    let Some((owner, page)) = PENDING.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .map(|pending| (pending.owner, pending.page))
    }) else {
        return;
    };
    if let Err(reason) = same_profile(browser.profile_of(page), browser.profile_of(marker_tab)) {
        return give_up(os, browser, Some(window), &reason);
    }
    if !browser.can_move_tabs() {
        return give_up(
            os,
            browser,
            Some(window),
            "this build cannot move a tab between windows",
        );
    }
    let Some((index, _)) = place_of(browser, owner, page) else {
        return give_up(
            os,
            browser,
            Some(window),
            "the page is no longer a tab of its Rivet window",
        );
    };
    // The watch ends before the move, because moving and closing tabs may make windows of their
    // own.
    let Some(ended) = end(os) else {
        return;
    };
    let moved = browser.move_tab(owner, index, window);
    // The new window closes only while the page is still in its Rivet window, so a page that did
    // leave it is never closed with the window.
    if place_of(browser, owner, page).is_some() {
        let reason = if moved {
            "the page stayed in its Rivet window after the move"
        } else {
            "the tab could not be detached and inserted"
        };
        return conclude(os, browser, Some(ended), Some(window), reason);
    }
    if !browser.close_page(window, marker_tab) {
        crate::trace::trace("mini window: the start page tab of the new window did not close");
    }
    let (pending, made) = ended;
    for other in made.into_iter().filter(|made| *made != window) {
        os.set_hidden(other, false);
    }
    (pending.done)(owner, Some((window, pending.place)));
    crate::trace::trace(&format!(
        "mini window: open tab {index} in window {:#x} at {:?}",
        window.raw(),
        pending.place
    ));
}

/// Ends the opening: the watch and the timer stop, and the request and the windows the watch saw
/// made are returned.
fn end(os: &impl NewWindows) -> Option<(Pending, Vec<WindowId>)> {
    let pending = PENDING.with(|pending| pending.borrow_mut().take())?;
    let made = os.stop_watching();
    os.stop_ticking(pending.owner, OPEN_TIMER);
    Some((pending, made))
}

/// Stops the opening for `reason` and keeps the page in its Rivet window.
fn give_up(os: &impl NewWindows, browser: &impl TabMoves, window: Option<WindowId>, reason: &str) {
    let ended = end(os);
    conclude(os, browser, ended, window, reason);
}

/// Closes the new window if it came, shows any other window the watch hid, and shows the page
/// again when the opening took it from the screen.
fn conclude(
    os: &impl NewWindows,
    browser: &impl TabMoves,
    ended: Option<(Pending, Vec<WindowId>)>,
    window: Option<WindowId>,
    reason: &str,
) {
    if let Some(window) = window {
        os.close_window(window);
    }
    if let Some((pending, made)) = ended {
        for other in made.into_iter().filter(|made| Some(*made) != window) {
            os.set_hidden(other, false);
        }
        if pending.show_again && (pending.is_rivet)(pending.owner) {
            if let Some((index, _)) = place_of(browser, pending.owner, pending.page) {
                browser.show_tab(pending.owner, index);
            }
        }
        (pending.done)(pending.owner, None);
    }
    crate::trace::trace(&format!("mini window: kept the tab, {reason}"));
}

/// The Rivet window `owner` is going away. An opening from it gives up.
pub(crate) fn forget(os: &impl NewWindows, browser: &impl TabMoves, owner: WindowId) {
    let Some((marker, is_taken)) = PENDING.with(|pending| {
        pending.try_borrow().ok().and_then(|pending| {
            pending
                .as_ref()
                .filter(|pending| pending.owner == owner)
                .map(|pending| (pending.marker.clone(), pending.is_taken))
        })
    }) else {
        return;
    };
    let windows = candidates(os, browser, owner, is_taken);
    let found = window_for(&windows, &marker).map(|(window, _)| window);
    give_up(os, browser, found, "its Rivet window closed");
}

/// Closes `window` when it holds only the start page of a Mini Window request that already gave
/// up. Returns whether it did.
pub(crate) fn close_if_late(os: &impl NewWindows, window: WindowId, urls: &[String]) -> bool {
    let late = matches!(urls, [url] if is_marker_url(url) && !is_pending(url));
    if late {
        os.close_window(window);
        crate::trace::trace(&format!(
            "mini window: closed window {:#x} of a request that gave up",
            window.raw()
        ));
    }
    late
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = "file:///C:/Rivet/start.html?space=Work&browser=Chrome&profile=Work&tint=d08a52&accent=9a5424";
    const OWNER: WindowId = WindowId::from_raw(0x100);
    const NEW: WindowId = WindowId::from_raw(0x300);
    const PAGE: PageId = PageId::from_raw(0x5000);
    const OTHER_PAGE: PageId = PageId::from_raw(0x5100);
    const START_TAB: PageId = PageId::from_raw(0x9000);

    fn tab(page: PageId, url: &str, active: bool) -> TabEntry {
        TabEntry {
            page,
            url: url.to_owned(),
            active,
        }
    }

    /// An operating system layer that records what the opening asks of it.
    #[derive(Default)]
    struct TestOs {
        launched: RefCell<Vec<(PathBuf, Vec<String>)>>,
        launch_fails: bool,
        watching: Cell<bool>,
        made: RefCell<Vec<WindowId>>,
        hidden: RefCell<Vec<(WindowId, bool)>>,
        closed: RefCell<Vec<WindowId>>,
        windows: RefCell<Vec<WindowId>>,
        ticking: Cell<bool>,
    }

    impl TestOs {
        /// The browser makes `window` while the watch runs, which hides it.
        fn browser_makes(&self, window: WindowId) {
            if self.watching.get() {
                self.made.borrow_mut().push(window);
                self.hidden.borrow_mut().push((window, true));
            }
            self.windows.borrow_mut().push(window);
        }
    }

    impl NewWindows for TestOs {
        fn launch(&self, program: &std::path::Path, arguments: &[String]) -> Result<(), String> {
            if self.launch_fails {
                return Err("no such program".into());
            }
            self.launched
                .borrow_mut()
                .push((program.to_path_buf(), arguments.to_vec()));
            Ok(())
        }
        fn watch_new_windows(&self, _place: ScreenRect) -> Result<(), String> {
            self.watching.set(true);
            Ok(())
        }
        fn stop_watching(&self) -> Vec<WindowId> {
            self.watching.set(false);
            std::mem::take(&mut *self.made.borrow_mut())
        }
        fn watched(&self) -> Vec<WindowId> {
            self.made.borrow().clone()
        }
        fn set_hidden(&self, window: WindowId, hidden: bool) {
            self.hidden.borrow_mut().push((window, hidden));
        }
        fn close_window(&self, window: WindowId) {
            self.closed.borrow_mut().push(window);
        }
        fn browser_windows(&self) -> Vec<WindowId> {
            self.windows.borrow().clone()
        }
        fn start_ticking(&self, _window: WindowId, timer: usize, _every_ms: u32, _: fn(WindowId)) {
            assert_eq!(timer, OPEN_TIMER);
            self.ticking.set(true);
        }
        fn stop_ticking(&self, _window: WindowId, _timer: usize) {
            self.ticking.set(false);
        }
    }

    /// A browser access layer with tabs kept in memory, where a move is a real move of the entry.
    struct TestBrowser {
        tabs: RefCell<Vec<(WindowId, Vec<TabEntry>)>>,
        profiles: Vec<(PageId, usize)>,
        movable: bool,
        shown: RefCell<Vec<(WindowId, usize)>>,
    }

    impl TestBrowser {
        /// A Rivet window that shows another page and holds the page to pop out.
        fn new() -> Self {
            Self {
                tabs: RefCell::new(vec![(
                    OWNER,
                    vec![
                        tab(OTHER_PAGE, "https://example.com/", true),
                        tab(PAGE, "https://example.org/", false),
                    ],
                )]),
                profiles: vec![(PAGE, 0x700), (OTHER_PAGE, 0x700), (START_TAB, 0x700)],
                movable: true,
                shown: RefCell::new(Vec::new()),
            }
        }

        fn add_window(&self, window: WindowId, tabs: Vec<TabEntry>) {
            self.tabs.borrow_mut().push((window, tabs));
        }

        fn pages_of(&self, window: WindowId) -> Vec<PageId> {
            self.tab_entries(window)
                .unwrap_or_default()
                .into_iter()
                .map(|tab| tab.page)
                .collect()
        }
    }

    impl TabMoves for TestBrowser {
        fn tab_entries(&self, window: WindowId) -> Option<Vec<TabEntry>> {
            self.tabs
                .borrow()
                .iter()
                .find(|(known, _)| *known == window)
                .map(|(_, tabs)| tabs.clone())
        }
        fn profile_of(&self, page: PageId) -> Option<usize> {
            self.profiles
                .iter()
                .find(|(known, _)| *known == page)
                .map(|(_, profile)| *profile)
        }
        fn can_move_tabs(&self) -> bool {
            self.movable
        }
        fn move_tab(&self, from: WindowId, index: usize, to: WindowId) -> bool {
            if !self.movable {
                return false;
            }
            let mut all = self.tabs.borrow_mut();
            let Some(source) = all.iter().position(|(known, _)| *known == from) else {
                return false;
            };
            let Some(target) = all.iter().position(|(known, _)| *known == to) else {
                return false;
            };
            if index >= all[source].1.len() {
                return false;
            }
            let mut moved = all[source].1.remove(index);
            moved.active = true;
            for other in &mut all[target].1 {
                other.active = false;
            }
            all[target].1.push(moved);
            true
        }
        fn close_page(&self, window: WindowId, page: PageId) -> bool {
            let mut all = self.tabs.borrow_mut();
            let Some(tabs) = all.iter_mut().find(|(known, _)| *known == window) else {
                return false;
            };
            let before = tabs.1.len();
            tabs.1.retain(|tab| tab.page != page);
            tabs.1.len() < before
        }
        fn show_tab(&self, window: WindowId, index: usize) -> bool {
            self.shown.borrow_mut().push((window, index));
            true
        }
    }

    /// A finished opening: the owner window and the Mini Window with its place, if one opened.
    type Done = (WindowId, Option<(WindowId, ScreenRect)>);

    thread_local! {
        static DONE: RefCell<Vec<Done>> =
            const { RefCell::new(Vec::new()) };
    }

    fn record_done(owner: WindowId, made: Option<(WindowId, ScreenRect)>) {
        DONE.with(|done| done.borrow_mut().push((owner, made)));
    }

    fn taken_done() -> Vec<(WindowId, Option<(WindowId, ScreenRect)>)> {
        DONE.with(|done| std::mem::take(&mut *done.borrow_mut()))
    }

    fn only_owner_is_rivet(window: WindowId) -> bool {
        window == OWNER
    }

    fn request(show_again: bool) -> Request {
        Request {
            owner: OWNER,
            page: PAGE,
            place: [9000, 1000, 672, 444],
            start_page: START.to_owned(),
            program: PathBuf::from(r"C:\Browser\chrome.exe"),
            own_arguments: [
                r"C:\Browser\chrome.exe",
                "--no-first-run",
                r"--user-data-dir=C:\Users\a\AppData\Local\Rivet\profiles\chrome\Work",
                "--app=file:///C:/Rivet/start.html",
            ]
            .map(str::to_owned)
            .to_vec(),
            show_again,
            is_rivet: only_owner_is_rivet,
            is_taken: only_owner_is_rivet,
            done: record_done,
        }
    }

    /// The browser opens the window the launch asked for, with its start page tab.
    fn browser_opens_the_window(os: &TestOs, browser: &TestBrowser) {
        let url = os.launched.borrow()[0]
            .1
            .iter()
            .find_map(|argument| argument.strip_prefix("--app=").map(str::to_owned))
            .expect("the launch names the app window's address");
        os.browser_makes(NEW);
        browser.add_window(NEW, vec![tab(START_TAB, &url, true)]);
    }

    #[test]
    fn fr9_mini_window_opens_a_rivet_app_window_and_moves_the_tab() {
        let os = TestOs::default();
        let browser = TestBrowser::new();
        assert!(start(&os, &browser, request(false)));
        // The browser is asked for one more app window on the same profile, so its process
        // singleton hands the request to this process. The window's address is the start page
        // with a fragment that names the request.
        let (program, arguments) = os.launched.borrow()[0].clone();
        assert_eq!(program, PathBuf::from(r"C:\Browser\chrome.exe"));
        assert_eq!(
            arguments[0],
            r"--user-data-dir=C:\Users\a\AppData\Local\Rivet\profiles\chrome\Work"
        );
        let app = arguments[1].strip_prefix("--app=").expect("an app window");
        assert!(app.starts_with(START) && is_marker_url(app) && is_pending(app));
        assert_eq!(arguments.len(), 2);
        assert!(os.watching.get() && os.ticking.get());
        // Until the window is there, the opening waits, and another window is never taken.
        let stranger = WindowId::from_raw(0x200);
        os.windows.borrow_mut().push(stranger);
        browser.add_window(
            stranger,
            vec![tab(PageId::from_raw(1), "https://example.net/", true)],
        );
        step(&os, &browser, OWNER);
        assert!(taken_done().is_empty());
        // The window comes: the page moves in with the detach and insert, the start page closes,
        // and the window becomes the Mini Window at its place.
        browser_opens_the_window(&os, &browser);
        step(&os, &browser, OWNER);
        assert_eq!(browser.pages_of(NEW), vec![PAGE]);
        assert_eq!(browser.pages_of(OWNER), vec![OTHER_PAGE]);
        assert_eq!(browser.pages_of(stranger), vec![PageId::from_raw(1)]);
        assert_eq!(
            taken_done(),
            vec![(OWNER, Some((NEW, [9000, 1000, 672, 444])))]
        );
        assert!(os.closed.borrow().is_empty());
        assert!(!os.watching.get() && !os.ticking.get());
        // The new window was hidden as it was made, and only the Mini Window shows it again.
        assert!(!os.hidden.borrow().contains(&(NEW, false)));
        // The profile folder of the session goes along.
        let own = [
            "c.exe",
            "--user-data-dir=D:\\P",
            "--profile-directory=Profile 1",
        ]
        .map(str::to_owned);
        assert_eq!(
            window_arguments(own, "file:///x").unwrap()[1],
            "--profile-directory=Profile 1"
        );
        // The browser may write the address its own way; the fragment names the request.
        let marker = marker(4242, 7);
        let seen = [Seen {
            window: NEW,
            tabs: vec![tab(
                START_TAB,
                "file:///C:/Rivet/start.html?space=Work#rivet-mini-window-4242-7",
                true,
            )],
        }];
        assert_eq!(window_for(&seen, &marker), Some((NEW, START_TAB)));
        assert_eq!(window_for(&seen, "rivet-mini-window-4242-6"), None);
        assert!(!is_marker_url("https://example.com/#rivet-mini-window-1-1"));
        assert!(!is_marker_url(&format!("{START}#rivet-command-3")));
    }

    #[test]
    fn fr9_mini_window_keeps_the_tab_when_the_move_fails() {
        // A build whose detach and insert are not there: the new window closes, and the page stays
        // a tab of its Rivet window and is shown again.
        let os = TestOs::default();
        let mut browser = TestBrowser::new();
        browser.movable = false;
        assert!(start(&os, &browser, request(true)));
        browser_opens_the_window(&os, &browser);
        step(&os, &browser, OWNER);
        assert_eq!(*os.closed.borrow(), vec![NEW]);
        assert_eq!(browser.pages_of(OWNER), vec![OTHER_PAGE, PAGE]);
        assert_eq!(*browser.shown.borrow(), vec![(OWNER, 1)]);
        assert_eq!(taken_done(), vec![(OWNER, None)]);
        assert!(!os.watching.get() && !os.ticking.get());

        // Another profile, or a build that cannot read one, keeps the tab where it was.
        for profiles in [vec![(PAGE, 0x700), (START_TAB, 0x800)], vec![(PAGE, 0x700)]] {
            let os = TestOs::default();
            let mut browser = TestBrowser::new();
            browser.profiles = profiles;
            assert!(start(&os, &browser, request(false)));
            browser_opens_the_window(&os, &browser);
            step(&os, &browser, OWNER);
            assert_eq!(*os.closed.borrow(), vec![NEW]);
            assert_eq!(browser.pages_of(OWNER), vec![OTHER_PAGE, PAGE]);
            assert!(browser.shown.borrow().is_empty());
            assert_eq!(taken_done(), vec![(OWNER, None)]);
        }

        // No window in time: nothing to close, and the page stays.
        let os = TestOs::default();
        let browser = TestBrowser::new();
        assert!(start(&os, &browser, request(false)));
        for _ in 0..LONGEST_WAIT_MS / TICK_MS {
            step(&os, &browser, OWNER);
        }
        assert!(os.closed.borrow().is_empty());
        assert!(!os.ticking.get());
        assert_eq!(browser.pages_of(OWNER), vec![OTHER_PAGE, PAGE]);
        assert_eq!(taken_done(), vec![(OWNER, None)]);
        // A window of a request that gave up, coming late, is closed when it is seen.
        let late = marker_url(START, &marker(std::process::id(), 4));
        assert!(close_if_late(&os, NEW, &[late]));
        assert_eq!(*os.closed.borrow(), vec![NEW]);

        // A browser that cannot be started: nothing waits and nothing moves.
        let os = TestOs {
            launch_fails: true,
            ..TestOs::default()
        };
        assert!(!start(&os, &TestBrowser::new(), request(false)));
        assert!(!os.watching.get() && !os.ticking.get());
        assert_eq!(taken_done(), vec![(OWNER, None)]);

        // A browser without a Rivet profile folder is never asked for a window.
        let os = TestOs::default();
        let mut bare = request(false);
        bare.own_arguments = vec![r"C:\Browser\chrome.exe".to_owned()];
        assert!(!start(&os, &TestBrowser::new(), bare));
        assert!(os.launched.borrow().is_empty() && !os.watching.get());
        assert!(window_arguments(
            ["chrome.exe".to_owned(), "--user-data-dir=".to_owned()],
            "file:///x"
        )
        .is_err());
    }

    /// A look at an opening whose Rivet window still holds the page in the background.
    fn look(windows: &[Seen]) -> Look<'_> {
        Look {
            waited_ms: 200,
            owner_alive: true,
            page: Some((1, false)),
            owner_tabs: 2,
            windows,
        }
    }

    #[test]
    fn fr9_mini_window_opening_gives_up_when_its_rivet_window_changes() {
        let marker = marker(1, 1);
        let made = [Seen {
            window: NEW,
            tabs: vec![tab(START_TAB, &marker_url(START, &marker), true)],
        }];

        let closes_new = |next: Next| {
            assert!(
                matches!(next, Next::GiveUp { window: Some(window), .. } if window == NEW),
                "{next:?}"
            )
        };
        closes_new(next(
            &Look {
                owner_alive: false,
                ..look(&made)
            },
            &marker,
        ));
        closes_new(next(
            &Look {
                page: None,
                ..look(&made)
            },
            &marker,
        ));
        closes_new(next(
            &Look {
                page: Some((0, true)),
                ..look(&made)
            },
            &marker,
        ));
        // The Rivet window keeps at least one tab.
        closes_new(next(
            &Look {
                owner_tabs: 1,
                ..look(&made)
            },
            &marker,
        ));
        // A window with more than the start page was not made for the request alone.
        let busy = [Seen {
            window: NEW,
            tabs: vec![
                tab(START_TAB, &marker_url(START, &marker), true),
                tab(PAGE, "https://example.com/", false),
            ],
        }];
        assert_eq!(next(&look(&busy), &marker), Next::Wait);
        assert_eq!(
            next(&look(&made), &marker),
            Next::Move {
                window: NEW,
                marker_tab: START_TAB
            }
        );
        assert_eq!(same_profile(Some(0x500), Some(0x500)), Ok(()));
        assert!(same_profile(None, None).is_err());
    }
}
