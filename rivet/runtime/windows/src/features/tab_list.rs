//! The appbar's tab list in a Rivet window: its sections, folders, drag and drop, archiving,
//! closing, reopening and what Ctrl+T opens.
//!
//! The list itself is `crate::tab_sections`. This module feeds it the tabs the window shows, acts
//! on the browser through the tab functions every supported build has, and keeps the file.

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::HWND;

use crate::tab_rows::Row;
use crate::tab_sections::{CtrlT, LiveTab, Store, Target};

struct State {
    store: Store,
    /// The window that reads and writes the file. Other windows of the process keep their list in
    /// memory only, as the spaces do.
    owner: isize,
    /// The file text last written, so an unchanged list is not written again.
    last: String,
    /// While the window swaps the tabs of two spaces, the pages that come and go are neither
    /// opened nor closed by the user.
    hold_until: Option<std::time::Instant>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn file() -> Option<(std::path::PathBuf, String)> {
    crate::frame::spaces_file().map(|(path, dir)| (crate::tab_sections::store_path(&path), dir))
}

fn with<R>(window: HWND, edit: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.get_or_insert_with(|| {
            // A missing or damaged file leaves the defaults. A damaged one is kept under another name.
            let store = match file() {
                Some((path, _)) => match std::fs::read_to_string(&path) {
                    Ok(text) => Store::from_json(&text).unwrap_or_else(|| {
                        let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                        Store::default()
                    }),
                    Err(_) => Store::default(),
                },
                None => Store::default(),
            };
            State { store, owner: window as isize, last: String::new(), hold_until: None }
        });
        edit(state)
    })
}

/// The tabs the window shows, as the list knows them.
unsafe fn live(window: HWND) -> Option<Vec<LiveTab>> {
    let tabs = crate::features::tabs::open_tabs(window)?;
    Some(
        tabs.into_iter()
            .enumerate()
            .map(|(index, tab)| LiveTab { id: tab.id, index, url: tab.saved.url, title: tab.saved.title, active: tab.saved.active })
            .collect(),
    )
}

fn held(state: &State) -> bool {
    state.hold_until.is_some_and(|until| std::time::Instant::now() < until)
}

/// Takes in the tabs the window shows now, unless it is swapping spaces.
unsafe fn sync(window: HWND) -> Option<Vec<LiveTab>> {
    let tabs = live(window)?;
    let space = crate::frame::space_name(window);
    with(window, |state| {
        if !held(state) {
            state.store.list(&space).sync(&tabs, now());
        }
    });
    Some(tabs)
}

/// The appbar rows in section order, or `None` when the window's tabs cannot be read.
pub(crate) unsafe fn rows(window: HWND) -> Option<Vec<Row>> {
    let tabs = sync(window)?;
    let space = crate::frame::space_name(window);
    Some(with(window, |state| state.store.list(&space).rows(&tabs)))
}

/// Runs on the window's timer: archives today's tabs past the period, moves links that opened in a
/// window of their own into this one, and writes the file when the list changed.
pub(crate) unsafe fn tick(window: HWND) {
    crate::features::link_tabs::tick(window);
    let Some(tabs) = sync(window) else { return };
    let space = crate::frame::space_name(window);
    let active = tabs.iter().find(|tab| tab.active).map(|tab| tab.id);
    let due = with(window, |state| {
        if held(state) {
            return Vec::new();
        }
        let settings = state.store.settings;
        state.store.list(&space).due_for_archive(&settings, now(), active)
    });
    for id in due {
        let closed = crate::features::tabs::close_tab_by_id(window, id);
        crate::trace::trace(&format!("tab list: archived page {id:#x}: {closed}"));
    }
    save(window);
}

unsafe fn save(window: HWND) {
    let Some((path, dir)) = file() else { return };
    let text = with(window, |state| (state.owner == window as isize).then(|| state.store.to_json(&dir)));
    let Some(text) = text else { return };
    if with(window, |state| state.last == text) {
        return;
    }
    match crate::spaces::write_atomic(&path, &text) {
        Ok(()) => with(window, |state| state.last = text),
        Err(error) => crate::trace::trace(&format!("tab list: could not save: {error}")),
    }
}

/// Before the window swaps its tabs for another space's: the tabs shown now stay this space's,
/// and the swap neither opens nor closes any of them.
pub(crate) fn leave_space(window: HWND) {
    let space = crate::frame::space_name(window);
    with(window, |state| {
        state.store.list(&space).detach();
        state.hold_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
    });
}

/// Closes the tab at the browser index `index`. When it is the selected tab, the one below it in
/// the appbar is selected first, else the one above, so the browser never picks another.
pub(crate) unsafe fn close(window: HWND, index: usize) -> bool {
    let Some(tabs) = sync(window) else {
        return crate::features::tabs::close_tab(window, index);
    };
    let Some(closing) = tabs.iter().find(|tab| tab.index == index) else { return false };
    if closing.active {
        let space = crate::frame::space_name(window);
        let order = with(window, |state| state.store.list(&space).order(&tabs));
        if let Some(next) = crate::tab_sections::select_after_close(&order, closing.id) {
            if let Some(next) = tabs.iter().find(|tab| tab.id == next) {
                crate::features::tabs::activate_tab(window, next.index);
            }
        }
    }
    let closed = crate::features::tabs::close_tab_by_id(window, closing.id);
    sync(window);
    closed
}

/// Ctrl+Shift+T: opens the most recently closed tab again where it was.
pub(crate) unsafe fn reopen(window: HWND) -> bool {
    sync(window);
    let space = crate::frame::space_name(window);
    let Some(closed) = with(window, |state| state.store.list(&space).take_closed()) else {
        return false;
    };
    let opened = crate::features::tabs::new_tab(window) && crate::features::navigation::open(window, &closed.url);
    crate::trace::trace(&format!("tab list: reopened {:?} at {}: {opened}", closed.url, closed.position));
    sync(window);
    opened
}

/// A click on a folder row. False when the row is a browser tab group, which the caller toggles.
pub(crate) fn toggle_folder(window: HWND, key: &[u8; 16]) -> bool {
    let Some(folder) = crate::tab_sections::folder_of_key(key) else { return false };
    let space = crate::frame::space_name(window);
    with(window, |state| state.store.list(&space).toggle_folder(folder))
}

unsafe fn page_at(window: HWND, index: usize) -> Option<usize> {
    live(window)?.into_iter().find(|tab| tab.index == index).map(|tab| tab.id)
}

pub(crate) unsafe fn set_pinned(window: HWND, index: usize, pinned: bool) -> bool {
    sync(window);
    let Some(id) = page_at(window, index) else { return false };
    let space = crate::frame::space_name(window);
    with(window, |state| state.store.list(&space).set_pinned(id, pinned))
}

/// Pins the tab at `index`, or unpins it when it is pinned.
pub(crate) unsafe fn toggle_pinned(window: HWND, index: usize) -> bool {
    sync(window);
    let Some(id) = page_at(window, index) else { return false };
    let space = crate::frame::space_name(window);
    with(window, |state| {
        let list = state.store.list(&space);
        let pinned = list.entries.iter().any(|entry| entry.id == id && entry.place == crate::tab_sections::Place::Pinned);
        list.set_pinned(id, !pinned)
    })
}

pub(crate) unsafe fn new_folder(window: HWND, index: usize, name: &str) -> bool {
    sync(window);
    let Some(id) = page_at(window, index) else { return false };
    let space = crate::frame::space_name(window);
    with(window, |state| state.store.list(&space).new_folder(id, name).is_some())
}

/// A tab dropped at `y`, in appbar DIPs, on the tab list. True when the drop was on a row.
pub(crate) unsafe fn drop_on_list(window: HWND, index: usize, y: f32) -> bool {
    let Some(tabs) = sync(window) else { return false };
    let Some(id) = tabs.iter().find(|tab| tab.index == index).map(|tab| tab.id) else { return false };
    let space = crate::frame::space_name(window);
    // The rows as the appbar draws them, with the split's tabs together.
    let split = crate::features::split::shown_tabs(window);
    with(window, |state| {
        let list = state.store.list(&space);
        let rows = crate::tab_rows::group_split(list.rows(&tabs), &split);
        let Some((row, lower)) = crate::tab_sections::row_at(&rows, y) else { return false };
        let page_of = |index: usize| tabs.iter().find(|tab| tab.index == index).map(|tab| tab.id);
        let Some(target) = crate::tab_sections::drop_target(&rows, row, lower, page_of) else { return false };
        let moved = list.move_tab(id, target);
        crate::trace::trace(&format!("tab list: dropped page {id:#x} on {target:?}: {moved}"));
        moved
    })
}

/// Moves the tab at `index` without a pointer, for checked builds: `before`, `after` or `folder`.
unsafe fn move_by_command(window: HWND, index: usize, how: &str, other: usize) -> bool {
    sync(window);
    let (Some(id), Some(near)) = (page_at(window, index), page_at(window, other)) else { return false };
    let target = match how {
        "before" => Target::Before(near),
        "after" => Target::After(near),
        _ => return false,
    };
    let space = crate::frame::space_name(window);
    with(window, |state| state.store.list(&space).move_tab(id, target))
}

/// The command bar of another feature registers itself here. It returns false when it could not
/// open, and Ctrl+T then opens the new tab page.
pub(crate) type CommandBarHook = unsafe fn(HWND) -> bool;
static COMMAND_BAR: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn set_command_bar_hook(hook: CommandBarHook) {
    COMMAND_BAR.store(hook as usize, Ordering::Release);
}

/// Connects the command bar: Ctrl+T opens it, and the bar's own "Ctrl T opens" buttons read and
/// keep the tab list's setting, which is saved with the list.
pub(crate) fn connect_command_bar() {
    set_command_bar_hook(crate::features::command_bar::open);
    crate::features::command_bar::set_setting_source(ctrl_t_now, set_ctrl_t_now);
}

/// The setting as the loaded list holds it, or the default before the list is loaded.
fn ctrl_t_now() -> CtrlT {
    STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .map_or(crate::tab_sections::Settings::default().ctrl_t, |state| state.store.settings.ctrl_t)
    })
}

fn set_ctrl_t_now(value: CtrlT) {
    STATE.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            state.store.settings.ctrl_t = value;
        }
    });
}

/// What Ctrl+T opens in this window by the setting.
pub(crate) fn ctrl_t_setting(window: HWND) -> CtrlT {
    with(window, |state| state.store.settings.ctrl_t)
}

/// Ctrl+T: opens the command bar when the setting asks for it and a command bar is there. False
/// means the caller opens the new tab page.
pub(crate) unsafe fn open_command_bar(window: HWND) -> bool {
    if ctrl_t_setting(window) != CtrlT::CommandBar {
        return false;
    }
    let hook = COMMAND_BAR.load(Ordering::Acquire);
    if hook == 0 {
        return false;
    }
    let hook: CommandBarHook = std::mem::transmute::<usize, CommandBarHook>(hook);
    hook(window)
}

pub(crate) fn set_ctrl_t(window: HWND, value: CtrlT) {
    with(window, |state| state.store.settings.ctrl_t = value);
}

/// Sets the archive period in hours. `None` never archives.
pub(crate) fn set_archive_hours(window: HWND, hours: Option<u64>) {
    with(window, |state| state.store.settings.set_archive_after(hours.map(|hours| hours * 3600)));
}

/// Archives as if the tabs were last seen `secs` ago, for checked builds that cannot wait hours.
fn age_by(window: HWND, secs: u64) -> bool {
    let space = crate::frame::space_name(window);
    with(window, |state| {
        for entry in &mut state.store.list(&space).entries {
            entry.seen = entry.seen.saturating_sub(secs);
        }
    });
    true
}

/// Traces the sections, for checked builds: `tab sections pinned [..] folders [..] today [..]`.
unsafe fn trace_sections(window: HWND) -> bool {
    let Some(tabs) = sync(window) else { return false };
    let space = crate::frame::space_name(window);
    let text = with(window, |state| {
        let settings = state.store.settings;
        let list = state.store.list(&space);
        let title = |id: usize| tabs.iter().find(|tab| tab.id == id).map(|tab| tab.title.clone()).unwrap_or_default();
        let pick = |place: crate::tab_sections::Place| -> Vec<String> {
            list.entries.iter().filter(|entry| entry.place == place && entry.id != 0).map(|entry| title(entry.id)).collect()
        };
        let folders: Vec<String> = list
            .folders
            .iter()
            .map(|folder| {
                let inside = pick(crate::tab_sections::Place::Folder(folder.id));
                format!("{}:{}:{}:{:?}", folder.name, inside.len(), if folder.collapsed { "collapsed" } else { "open" }, inside)
            })
            .collect();
        let order: Vec<String> = list.order(&tabs).into_iter().map(&title).collect();
        format!(
            "tab sections pinned {:?} folders {:?} today {:?} order {:?} closed {} archived {} archive_after {:?} ctrl_t {}",
            pick(crate::tab_sections::Place::Pinned),
            folders,
            pick(crate::tab_sections::Place::Today),
            order,
            list.closed.len(),
            list.archived.len(),
            settings.archive_after,
            settings.ctrl_t.key()
        )
    });
    crate::trace::trace(&text);
    true
}

/// One tab list command of `rivet-tab.txt`, or `None` when the word is not one of them.
pub(crate) unsafe fn command(window: HWND, text: &str) -> Option<bool> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let number = |at: usize| words.get(at).and_then(|word| word.parse::<usize>().ok());
    let done = match words.first().copied()? {
        "sections" => trace_sections(window),
        "pin" => set_pinned(window, number(1)?, true),
        "unpin" => set_pinned(window, number(1)?, false),
        "folder" => new_folder(window, number(1)?, &words[2..].join(" ")),
        "foldertoggle" => {
            let space = crate::frame::space_name(window);
            let id = with(window, |state| state.store.list(&space).folders.first().map(|folder| folder.id))?;
            toggle_folder(window, &crate::tab_sections::folder_key(id))
        }
        "into" => {
            sync(window);
            let id = page_at(window, number(1)?)?;
            let space = crate::frame::space_name(window);
            with(window, |state| {
                let list = state.store.list(&space);
                let folder = list.folders.first().map(|folder| folder.id);
                folder.is_some_and(|folder| list.move_tab(id, Target::Folder(folder)))
            })
        }
        "before" | "after" => move_by_command(window, number(1)?, words[0], number(2)?),
        "closesel" => close(window, number(1)?),
        "reopen" => reopen(window),
        "archive" => {
            match words.get(1).copied() {
                Some("never") => set_archive_hours(window, None),
                Some(hours) => set_archive_hours(window, Some(hours.parse().ok()?)),
                None => return Some(false),
            }
            true
        }
        "age" => age_by(window, number(1)? as u64 * 3600),
        "ctrlt" => {
            set_ctrl_t(window, if words.get(1) == Some(&"newtab") { CtrlT::NewTabPage } else { CtrlT::CommandBar });
            true
        }
        _ => return None,
    };
    Some(done)
}
