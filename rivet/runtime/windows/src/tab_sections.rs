//! The tab list of a space, split into pinned tabs, folders and today's tabs.
//!
//! The browser keeps the tabs themselves. This list keeps what the browser has no word for: which
//! section a tab is in, the order the appbar shows, the folders, when a tab was last looked at,
//! the tabs closed recently and the ones archived. It knows a tab of the window by the identity of
//! its page while the window shows it, and by its address after a restart.
//!
//! Nothing here reads or writes browser profile data. The file holds addresses, titles, folder
//! names and times only.

use crate::tab_rows::{Group, Row};

/// A today's tab that has not been looked at for this long is archived, unless the user chose
/// another period.
pub const DEFAULT_ARCHIVE_SECS: u64 = 12 * 60 * 60;
/// The shortest and longest archive periods a setting can hold.
pub const MIN_ARCHIVE_SECS: u64 = 60 * 60;
pub const MAX_ARCHIVE_SECS: u64 = 30 * 24 * 60 * 60;
/// How many closed tabs Ctrl+Shift+T can bring back, newest first.
const MAX_CLOSED: usize = 25;
/// How many archived tabs a space keeps a record of.
const MAX_ARCHIVED: usize = 200;
/// The longest address, title or folder name the file keeps.
const MAX_TEXT: usize = 4096;
/// The tabs of one space the file keeps, at most.
const MAX_TABS: usize = 500;
const FORMAT: u64 = 1;

/// What Ctrl+T opens in a Rivet window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtrlT {
    CommandBar,
    NewTabPage,
}

impl CtrlT {
    pub fn key(self) -> &'static str {
        match self {
            Self::CommandBar => "command_bar",
            Self::NewTabPage => "new_tab_page",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::CommandBar, Self::NewTabPage].into_iter().find(|value| value.key() == key)
    }
}

/// The tab list settings of a browser profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Seconds after which an unseen today's tab is archived. `None` never archives.
    pub archive_after: Option<u64>,
    pub ctrl_t: CtrlT,
}

impl Default for Settings {
    fn default() -> Self {
        Self { archive_after: Some(DEFAULT_ARCHIVE_SECS), ctrl_t: CtrlT::CommandBar }
    }
}

impl Settings {
    /// Sets the archive period, kept inside the range a setting may hold.
    pub fn set_archive_after(&mut self, secs: Option<u64>) {
        self.archive_after = secs.map(|secs| secs.clamp(MIN_ARCHIVE_SECS, MAX_ARCHIVE_SECS));
    }
}

/// The section a tab is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Pinned,
    Folder(u32),
    Today,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub id: u32,
    pub name: String,
    pub collapsed: bool,
}

/// A tab of the list. `id` is the identity of the page the window shows for it, and zero while no
/// page of the window has been matched to it, for example right after a restart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub place: Place,
    /// When the tab was last the active one, in seconds since the Unix epoch.
    pub seen: u64,
}

/// A tab that was closed, with where it stood.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed {
    pub url: String,
    pub title: String,
    pub place: Place,
    /// Its position in the list when it was closed.
    pub position: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archived {
    pub url: String,
    pub title: String,
    pub at: u64,
}

/// A tab the window shows now, in the browser's own order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveTab {
    pub id: usize,
    pub index: usize,
    pub url: String,
    pub title: String,
    pub active: bool,
}

/// Where a dragged tab goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Into a folder, after its last tab.
    Folder(u32),
    /// Just before or after another tab, into that tab's section.
    Before(usize),
    After(usize),
    /// The first of today's tabs.
    TodayStart,
}

/// The tab list of one space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabList {
    pub entries: Vec<Entry>,
    pub folders: Vec<Folder>,
    pub closed: Vec<Closed>,
    pub archived: Vec<Archived>,
    /// A closed tab being brought back. The next new page of the window takes its place.
    pub restoring: Option<Closed>,
    /// Pages being archived. When they leave the window they are archived, not closed.
    pub archiving: Vec<usize>,
}

/// The first bytes of a folder's row identity, which tells a Rivet folder from a browser tab group.
const FOLDER_MARK: [u8; 4] = *b"RVTF";

/// The row identity of a folder.
pub fn folder_key(id: u32) -> [u8; 16] {
    let mut key = [0u8; 16];
    key[..4].copy_from_slice(&FOLDER_MARK);
    key[4..8].copy_from_slice(&id.to_be_bytes());
    key
}

/// The folder a row identity stands for, or `None` for a browser tab group.
pub fn folder_of_key(key: &[u8; 16]) -> Option<u32> {
    (key[..4] == FOLDER_MARK && key[8..].iter().all(|byte| *byte == 0))
        .then(|| u32::from_be_bytes([key[4], key[5], key[6], key[7]]))
}

/// The tab to select when `closing` is closed, from the tabs in the order the appbar lists them:
/// the one just below it, else the one just above it.
pub fn select_after_close(order: &[usize], closing: usize) -> Option<usize> {
    let at = order.iter().position(|id| *id == closing)?;
    order.get(at + 1).or_else(|| at.checked_sub(1).and_then(|above| order.get(above))).copied()
}

impl TabList {
    fn position_of(&self, id: usize) -> Option<usize> {
        (id != 0).then(|| self.entries.iter().position(|entry| entry.id == id)).flatten()
    }

    fn folder_exists(&self, id: u32) -> bool {
        self.folders.iter().any(|folder| folder.id == id)
    }

    /// Brings the list up to date with the tabs the window shows. A matched tab that left the
    /// window was closed, or archived when it was being archived. A new page takes the place of a
    /// tab being restored, else of a saved tab with the same address, else goes last in today's
    /// tabs. The active tab counts as seen now.
    pub fn sync(&mut self, live: &[LiveTab], now: u64) {
        let mut index = 0;
        while index < self.entries.len() {
            let entry = &self.entries[index];
            if entry.id != 0 && !live.iter().any(|tab| tab.id == entry.id) {
                let entry = self.entries.remove(index);
                if let Some(at) = self.archiving.iter().position(|id| *id == entry.id) {
                    self.archiving.remove(at);
                    self.archived.insert(0, Archived { url: entry.url, title: entry.title, at: now });
                    self.archived.truncate(MAX_ARCHIVED);
                } else {
                    self.remember_closed(Closed { url: entry.url, title: entry.title, place: entry.place, position: index });
                }
                continue;
            }
            index += 1;
        }
        for tab in live {
            let at = match self.position_of(tab.id) {
                Some(at) => at,
                None => self.place_new(tab, now),
            };
            let entry = &mut self.entries[at];
            entry.url.clone_from(&tab.url);
            entry.title.clone_from(&tab.title);
            if tab.active {
                entry.seen = now;
            }
        }
        self.archiving.retain(|id| live.iter().any(|tab| tab.id == *id));
    }

    fn place_new(&mut self, tab: &LiveTab, now: u64) -> usize {
        if let Some(closed) = self.restoring.take() {
            let place = match closed.place {
                Place::Folder(id) if !self.folder_exists(id) => Place::Today,
                place => place,
            };
            let at = closed.position.min(self.entries.len());
            self.entries.insert(at, Entry { id: tab.id, url: tab.url.clone(), title: closed.title, place, seen: now });
            return at;
        }
        if let Some(at) = self.entries.iter().position(|entry| entry.id == 0 && !tab.url.is_empty() && entry.url == tab.url) {
            self.entries[at].id = tab.id;
            return at;
        }
        self.entries.push(Entry { id: tab.id, url: tab.url.clone(), title: tab.title.clone(), place: Place::Today, seen: now });
        self.entries.len() - 1
    }

    fn remember_closed(&mut self, closed: Closed) {
        self.closed.insert(0, closed);
        self.closed.truncate(MAX_CLOSED);
    }

    /// Forgets which pages the tabs were, as when the window leaves the space. The tabs stay
    /// listed by address, so coming back matches them again.
    pub fn detach(&mut self) {
        for entry in &mut self.entries {
            entry.id = 0;
        }
        self.archiving.clear();
        self.restoring = None;
    }

    /// The most recently closed tab, taken off the list of closed tabs. The next new page of the
    /// window goes where it was.
    pub fn take_closed(&mut self) -> Option<Closed> {
        if self.closed.is_empty() {
            return None;
        }
        let closed = self.closed.remove(0);
        self.restoring = Some(closed.clone());
        Some(closed)
    }

    /// The pages of today's tabs that were not looked at for the archive period, other than the
    /// active one. They are marked as being archived.
    pub fn due_for_archive(&mut self, settings: &Settings, now: u64, active: Option<usize>) -> Vec<usize> {
        let Some(period) = settings.archive_after else {
            return Vec::new();
        };
        let due: Vec<usize> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.id != 0
                    && entry.place == Place::Today
                    && Some(entry.id) != active
                    && !self.archiving.contains(&entry.id)
                    && now.saturating_sub(entry.seen) >= period
            })
            .map(|entry| entry.id)
            .collect();
        self.archiving.extend(&due);
        due
    }

    pub fn set_pinned(&mut self, id: usize, pinned: bool) -> bool {
        let Some(at) = self.position_of(id) else { return false };
        self.entries[at].place = if pinned { Place::Pinned } else { Place::Today };
        true
    }

    /// Makes a folder that holds the tab `id`, and returns the folder's identity.
    pub fn new_folder(&mut self, id: usize, name: &str) -> Option<u32> {
        let at = self.position_of(id)?;
        let folder = self.folders.iter().map(|folder| folder.id).max().map_or(1, |last| last + 1);
        let name: String = name.trim().chars().take(200).collect();
        self.folders.push(Folder { id: folder, name: if name.is_empty() { "New folder".into() } else { name }, collapsed: false });
        self.entries[at].place = Place::Folder(folder);
        Some(folder)
    }

    #[allow(dead_code)]
    pub fn rename_folder(&mut self, folder: u32, name: &str) -> bool {
        let name = name.trim();
        match self.folders.iter_mut().find(|item| item.id == folder) {
            Some(item) if !name.is_empty() => {
                item.name = name.chars().take(200).collect();
                true
            }
            _ => false,
        }
    }

    pub fn toggle_folder(&mut self, folder: u32) -> bool {
        match self.folders.iter_mut().find(|item| item.id == folder) {
            Some(item) => {
                item.collapsed = !item.collapsed;
                true
            }
            None => false,
        }
    }

    /// Moves the tab `id` to `target`. A folder left empty stays, as a named place to drop into.
    pub fn move_tab(&mut self, id: usize, target: Target) -> bool {
        let Some(from) = self.position_of(id) else { return false };
        let (place, at) = match target {
            Target::Folder(folder) if self.folder_exists(folder) => {
                let last = self.entries.iter().rposition(|entry| entry.place == Place::Folder(folder) && entry.id != id);
                (Place::Folder(folder), last.map_or(self.entries.len(), |last| last + 1))
            }
            Target::Folder(_) => return false,
            Target::Before(other) | Target::After(other) => {
                if other == id {
                    return true;
                }
                let Some(near) = self.position_of(other) else { return false };
                let after = matches!(target, Target::After(_));
                (self.entries[near].place, if after { near + 1 } else { near })
            }
            Target::TodayStart => {
                let first = self.entries.iter().position(|entry| entry.place == Place::Today && entry.id != id);
                (Place::Today, first.unwrap_or(self.entries.len()))
            }
        };
        let mut entry = self.entries.remove(from);
        entry.place = place;
        let at = if at > from { at - 1 } else { at };
        self.entries.insert(at.min(self.entries.len()), entry);
        true
    }

    /// The pages in the order the appbar lists them: pinned tabs, then each folder's tabs, then
    /// today's tabs. Tabs inside a collapsed folder are listed as well, since closing the tab above
    /// one should not jump past it.
    pub fn order(&self, live: &[LiveTab]) -> Vec<usize> {
        let mut order = Vec::new();
        let listed = |entry: &&Entry| entry.id != 0 && live.iter().any(|tab| tab.id == entry.id);
        order.extend(self.entries.iter().filter(listed).filter(|entry| entry.place == Place::Pinned).map(|entry| entry.id));
        for folder in &self.folders {
            order.extend(
                self.entries.iter().filter(listed).filter(|entry| entry.place == Place::Folder(folder.id)).map(|entry| entry.id),
            );
        }
        order.extend(self.entries.iter().filter(listed).filter(|entry| entry.place == Place::Today).map(|entry| entry.id));
        for tab in live {
            if !order.contains(&tab.id) {
                order.push(tab.id);
            }
        }
        order
    }

    /// The appbar rows: pinned tabs, the folders with their tabs, the new tab row, then today's
    /// tabs. A folder row carries how many tabs it holds.
    pub fn rows(&self, live: &[LiveTab]) -> Vec<Row> {
        let tab_row = |id: usize, child: bool| {
            live.iter().find(|tab| tab.id == id).map(|tab| Row::Tab {
                index: tab.index,
                title: tab.title.clone(),
                active: tab.active,
                child,
            })
        };
        let in_place = |place: Place| -> Vec<usize> {
            self.entries
                .iter()
                .filter(|entry| entry.place == place && entry.id != 0 && live.iter().any(|tab| tab.id == entry.id))
                .map(|entry| entry.id)
                .collect()
        };
        let mut rows = Vec::new();
        rows.extend(in_place(Place::Pinned).into_iter().filter_map(|id| tab_row(id, false)));
        for folder in &self.folders {
            let tabs = in_place(Place::Folder(folder.id));
            rows.push(Row::Group(Group {
                id: folder_key(folder.id),
                title: folder.name.clone(),
                color: 0,
                collapsed: folder.collapsed,
                count: tabs.len(),
            }));
            if !folder.collapsed {
                rows.extend(tabs.into_iter().filter_map(|id| tab_row(id, true)));
            }
        }
        rows.push(Row::NewTab);
        rows.extend(in_place(Place::Today).into_iter().filter_map(|id| tab_row(id, false)));
        // Pages the list has not taken in yet still show, last.
        for tab in live {
            if !self.entries.iter().any(|entry| entry.id == tab.id) {
                rows.push(Row::Tab { index: tab.index, title: tab.title.clone(), active: tab.active, child: false });
            }
        }
        rows
    }
}

/// Where the tab list's rows start in the appbar, how tall a row is and the extra gap before the
/// new tab row when rows come before it. The appbar's drawing and hit test use the same numbers.
pub const ROWS_TOP: f32 = 138.0;
pub const ROW_HEIGHT: f32 = 34.0;
pub const DIVIDER: f32 = 17.0;

/// The row at `y` in appbar DIPs, and whether the point is in the row's lower half.
pub fn row_at(rows: &[Row], y: f32) -> Option<(usize, bool)> {
    let mut top = ROWS_TOP;
    for (index, row) in rows.iter().enumerate() {
        if matches!(row, Row::NewTab) && top > ROWS_TOP {
            top += DIVIDER;
        }
        if (top..top + ROW_HEIGHT).contains(&y) {
            return Some((index, y >= top + ROW_HEIGHT / 2.0));
        }
        top += ROW_HEIGHT;
    }
    None
}

/// Where a tab dropped on `row` goes, given the page each tab row stands for.
pub fn drop_target(rows: &[Row], row: usize, lower: bool, page_of: impl Fn(usize) -> Option<usize>) -> Option<Target> {
    match rows.get(row)? {
        Row::Group(group) => folder_of_key(&group.id).map(Target::Folder),
        Row::NewTab => Some(Target::TodayStart),
        Row::Tab { index, .. } => {
            let page = page_of(*index)?;
            Some(if lower { Target::After(page) } else { Target::Before(page) })
        }
    }
}

/// The tab lists of every space of a browser profile, and its settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    pub settings: Settings,
    /// Each space's list, by the space's name.
    pub spaces: Vec<(String, TabList)>,
}

impl Store {
    pub fn list(&mut self, space: &str) -> &mut TabList {
        let at = match self.spaces.iter().position(|(name, _)| name == space) {
            Some(at) => at,
            None => {
                self.spaces.push((space.to_owned(), TabList::default()));
                self.spaces.len() - 1
            }
        };
        &mut self.spaces[at].1
    }

    /// The file text. Closed tabs are kept too, so Ctrl+Shift+T still works after a restart.
    pub fn to_json(&self, user_data_dir: &str) -> String {
        use serde_json::{json, Value};
        let place = |place: Place| -> Value {
            match place {
                Place::Pinned => json!("pinned"),
                Place::Today => json!("today"),
                Place::Folder(id) => json!({ "folder": id }),
            }
        };
        let spaces: Vec<Value> = self
            .spaces
            .iter()
            .map(|(name, list)| {
                json!({
                    "name": name,
                    "folders": list.folders.iter().map(|folder| json!({ "id": folder.id, "name": folder.name, "collapsed": folder.collapsed })).collect::<Vec<_>>(),
                    "tabs": list.entries.iter().take(MAX_TABS).map(|entry| json!({ "url": entry.url, "title": entry.title, "place": place(entry.place), "seen": entry.seen })).collect::<Vec<_>>(),
                    "closed": list.closed.iter().map(|closed| json!({ "url": closed.url, "title": closed.title, "place": place(closed.place), "position": closed.position })).collect::<Vec<_>>(),
                    "archived": list.archived.iter().map(|item| json!({ "url": item.url, "title": item.title, "at": item.at })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let root = json!({
            "version": FORMAT,
            "user_data_dir": user_data_dir,
            "settings": {
                "archive_after_secs": self.settings.archive_after,
                "ctrl_t": self.settings.ctrl_t.key(),
            },
            "spaces": spaces,
        });
        serde_json::to_string_pretty(&root).unwrap_or_default() + "\n"
    }

    /// Reads the file text. Anything that is not a whole, sane file gives `None`, and the window
    /// starts with an empty list and the default settings.
    pub fn from_json(text: &str) -> Option<Self> {
        use serde_json::Value;
        let root: Value = serde_json::from_str(text).ok()?;
        if root.get("version")?.as_u64()? != FORMAT {
            return None;
        }
        let text_of = |value: &Value, key: &str| -> Option<String> {
            let text = value.get(key)?.as_str()?;
            (text.len() <= MAX_TEXT).then(|| text.to_owned())
        };
        let place_of = |value: &Value| -> Option<Place> {
            match value {
                Value::String(word) if word == "pinned" => Some(Place::Pinned),
                Value::String(word) if word == "today" => Some(Place::Today),
                Value::Object(_) => Some(Place::Folder(u32::try_from(value.get("folder")?.as_u64()?).ok()?)),
                _ => None,
            }
        };
        let settings_value = root.get("settings")?;
        let mut settings = Settings {
            archive_after: None,
            ctrl_t: CtrlT::from_key(settings_value.get("ctrl_t")?.as_str()?)?,
        };
        match settings_value.get("archive_after_secs")? {
            Value::Null => {}
            value => settings.set_archive_after(Some(value.as_u64()?)),
        }
        let mut spaces = Vec::new();
        for space in root.get("spaces")?.as_array()?.iter().take(crate::spaces::MAX_SPACES * 4) {
            let name = text_of(space, "name")?;
            let mut list = TabList::default();
            for folder in space.get("folders")?.as_array()? {
                list.folders.push(Folder {
                    id: u32::try_from(folder.get("id")?.as_u64()?).ok()?,
                    name: text_of(folder, "name")?,
                    collapsed: folder.get("collapsed")?.as_bool()?,
                });
            }
            for tab in space.get("tabs")?.as_array()?.iter().take(MAX_TABS) {
                let place = match place_of(tab.get("place")?)? {
                    Place::Folder(id) if !list.folder_exists(id) => Place::Today,
                    place => place,
                };
                list.entries.push(Entry {
                    id: 0,
                    url: text_of(tab, "url")?,
                    title: text_of(tab, "title")?,
                    place,
                    seen: tab.get("seen")?.as_u64()?,
                });
            }
            for closed in space.get("closed")?.as_array()?.iter().take(MAX_CLOSED) {
                list.closed.push(Closed {
                    url: text_of(closed, "url")?,
                    title: text_of(closed, "title")?,
                    place: place_of(closed.get("place")?)?,
                    position: closed.get("position")?.as_u64()?.min(MAX_TABS as u64) as usize,
                });
            }
            for item in space.get("archived")?.as_array()?.iter().take(MAX_ARCHIVED) {
                list.archived.push(Archived { url: text_of(item, "url")?, title: text_of(item, "title")?, at: item.get("at")?.as_u64()? });
            }
            spaces.push((name, list));
        }
        Some(Self { settings, spaces })
    }
}

/// The tab list file of a profile, beside its spaces file: `<key>.tabs.json`.
pub fn store_path(spaces_file: &std::path::Path) -> std::path::PathBuf {
    spaces_file.with_extension("tabs.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(id: usize, url: &str, active: bool) -> LiveTab {
        LiveTab { id, index: id - 1, url: url.into(), title: url.into(), active }
    }

    fn three() -> (TabList, Vec<LiveTab>) {
        let tabs = vec![live(1, "a", true), live(2, "b", false), live(3, "c", false)];
        let mut list = TabList::default();
        list.sync(&tabs, 100);
        (list, tabs)
    }

    #[test]
    fn fr3_tab_list_has_pinned_folders_and_today_sections() {
        let (mut list, tabs) = three();
        assert!(list.set_pinned(3, true));
        let folder = list.new_folder(2, "Launch Atlas").unwrap();
        let rows = list.rows(&tabs);
        assert!(matches!(&rows[0], Row::Tab { index: 2, child: false, .. }), "pinned first: {rows:?}");
        match &rows[1] {
            Row::Group(group) => {
                assert_eq!(group.title, "Launch Atlas");
                assert_eq!(group.count, 1);
                assert_eq!(folder_of_key(&group.id), Some(folder));
            }
            other => panic!("folder second, got {other:?}"),
        }
        assert!(matches!(&rows[2], Row::Tab { index: 1, child: true, .. }));
        assert_eq!(rows[3], Row::NewTab);
        assert!(matches!(&rows[4], Row::Tab { index: 0, child: false, .. }), "today last");
    }

    #[test]
    fn fr3_folders_collapse_and_keep_their_count() {
        let (mut list, tabs) = three();
        let folder = list.new_folder(1, "Work").unwrap();
        assert!(list.move_tab(2, Target::Folder(folder)));
        assert!(list.toggle_folder(folder));
        let rows = list.rows(&tabs);
        match &rows[0] {
            Row::Group(group) => assert!(group.collapsed && group.count == 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(rows[1], Row::NewTab, "a collapsed folder hides its tabs");
    }

    #[test]
    fn fr3_drag_reorders_and_moves_into_folders() {
        let (mut list, tabs) = three();
        assert!(list.move_tab(3, Target::Before(1)));
        assert_eq!(list.order(&tabs), vec![3, 1, 2]);
        assert!(list.move_tab(3, Target::After(2)));
        assert_eq!(list.order(&tabs), vec![1, 2, 3]);
        let folder = list.new_folder(1, "F").unwrap();
        assert!(list.move_tab(3, Target::Folder(folder)));
        assert_eq!(list.order(&tabs), vec![1, 3, 2]);
        assert!(list.move_tab(3, Target::TodayStart));
        assert_eq!(list.order(&tabs), vec![1, 3, 2]);
        assert_eq!(list.entries.iter().find(|entry| entry.id == 3).unwrap().place, Place::Today);
        assert_eq!(list.order(&tabs).len(), 3);
        // A drop on a tab of another section takes that tab's section.
        assert!(list.move_tab(2, Target::After(1)));
        assert_eq!(list.entries.iter().find(|entry| entry.id == 2).unwrap().place, Place::Folder(folder));
    }

    #[test]
    fn fr3_a_drop_on_a_row_names_its_target() {
        let (mut list, tabs) = three();
        let folder = list.new_folder(1, "F").unwrap();
        let rows = list.rows(&tabs);
        let page = |index: usize| tabs.iter().find(|tab| tab.index == index).map(|tab| tab.id);
        let (row, lower) = row_at(&rows, ROWS_TOP + 5.0).unwrap();
        assert_eq!((row, lower), (0, false));
        assert_eq!(drop_target(&rows, row, lower, page), Some(Target::Folder(folder)));
        let (row, lower) = row_at(&rows, ROWS_TOP + ROW_HEIGHT + 30.0).unwrap();
        assert_eq!(drop_target(&rows, row, lower, page), Some(Target::After(1)));
        // The new tab row sits one divider lower.
        let (row, _) = row_at(&rows, ROWS_TOP + 2.0 * ROW_HEIGHT + DIVIDER + 2.0).unwrap();
        assert_eq!(rows[row], Row::NewTab);
        assert_eq!(drop_target(&rows, row, false, page), Some(Target::TodayStart));
    }

    #[test]
    fn fr3_sections_and_folders_survive_a_restart_through_the_file() {
        let (mut list, tabs) = three();
        list.set_pinned(1, true);
        let folder = list.new_folder(2, "Launch Atlas").unwrap();
        list.toggle_folder(folder);
        let mut store = Store::default();
        store.settings.set_archive_after(Some(3 * 3600));
        store.settings.ctrl_t = CtrlT::NewTabPage;
        store.spaces.push(("Work".into(), list));
        let text = store.to_json(r"C:\profile");
        assert!(text.contains(r#""user_data_dir": "C:\\profile""#), "prune reads the profile: {text}");
        let mut back = Store::from_json(&text).unwrap();
        assert_eq!(back.settings, store.settings);
        let list = back.list("Work");
        assert_eq!(list.folders, vec![Folder { id: folder, name: "Launch Atlas".into(), collapsed: true }]);
        assert!(list.entries.iter().all(|entry| entry.id == 0), "pages are matched again after a restart");
        list.sync(&[live(7, "b", false), live(8, "a", true), live(9, "c", false)], 200);
        assert_eq!(list.order(&[live(7, "b", false), live(8, "a", true), live(9, "c", false)]), vec![8, 7, 9]);
        assert_eq!(list.entries.iter().find(|entry| entry.id == 7).unwrap().place, Place::Folder(folder));
        let _ = tabs;
    }

    #[test]
    fn fr3_a_damaged_tab_list_file_is_not_guessed_at() {
        assert!(Store::from_json("{").is_none());
        assert!(Store::from_json(r#"{"version": 9}"#).is_none());
        let long = format!(
            r#"{{"version":1,"user_data_dir":"","settings":{{"archive_after_secs":null,"ctrl_t":"command_bar"}},"spaces":[{{"name":"W","folders":[],"tabs":[{{"url":"{}","title":"","place":"today","seen":0}}],"closed":[],"archived":[]}}]}}"#,
            "x".repeat(MAX_TEXT + 1)
        );
        assert!(Store::from_json(&long).is_none());
    }

    #[test]
    fn fr3_unpinned_tabs_archive_after_twelve_hours_by_default() {
        let settings = Settings::default();
        assert_eq!(settings.archive_after, Some(12 * 3600));
        let (mut list, tabs) = three();
        list.set_pinned(2, true);
        let now = 100 + 12 * 3600;
        assert!(list.due_for_archive(&settings, now - 1, Some(1)).is_empty());
        assert_eq!(list.due_for_archive(&settings, now, Some(1)), vec![3], "pinned and active tabs stay");
        assert!(list.due_for_archive(&settings, now, Some(1)).is_empty(), "asked once");
        list.sync(&tabs[..2], now);
        assert_eq!(list.archived.len(), 1);
        assert_eq!(list.archived[0].url, "c");
        assert!(list.closed.is_empty(), "an archived tab is not a closed tab");
    }

    #[test]
    fn fr3_the_archive_period_is_a_setting() {
        let mut settings = Settings::default();
        settings.set_archive_after(Some(2 * 3600));
        let (mut list, _) = three();
        assert_eq!(list.due_for_archive(&settings, 100 + 2 * 3600, Some(1)), vec![2, 3]);
        settings.set_archive_after(None);
        let (mut list, _) = three();
        assert!(list.due_for_archive(&settings, u64::MAX, None).is_empty());
        settings.set_archive_after(Some(1));
        assert_eq!(settings.archive_after, Some(MIN_ARCHIVE_SECS));
    }

    #[test]
    fn fr3_closing_the_selected_tab_selects_the_one_below_else_above() {
        assert_eq!(select_after_close(&[1, 2, 3], 2), Some(3));
        assert_eq!(select_after_close(&[1, 2, 3], 3), Some(2));
        assert_eq!(select_after_close(&[1], 1), None);
        assert_eq!(select_after_close(&[1, 2], 9), None);
        // Below means below in the appbar, not in the browser's own order.
        let (mut list, tabs) = three();
        list.set_pinned(3, true);
        assert_eq!(select_after_close(&list.order(&tabs), 3), Some(1));
    }

    #[test]
    fn fr3_ctrl_shift_t_restores_the_closed_tab_in_the_same_place() {
        let (mut list, tabs) = three();
        let folder = list.new_folder(2, "F").unwrap();
        list.sync(&[tabs[0].clone(), tabs[2].clone()], 150);
        assert_eq!(list.closed[0], Closed { url: "b".into(), title: "b".into(), place: Place::Folder(folder), position: 1 });
        let closed = list.take_closed().unwrap();
        assert_eq!(closed.url, "b");
        // The new page starts empty and loads the address afterwards.
        let reopened = LiveTab { id: 11, index: 2, url: String::new(), title: String::new(), active: true };
        list.sync(&[tabs[0].clone(), tabs[2].clone(), reopened.clone()], 160);
        assert_eq!(list.entries[1].id, 11);
        assert_eq!(list.entries[1].place, Place::Folder(folder));
        assert!(list.take_closed().is_none());
    }

    #[test]
    fn fr3_leaving_a_space_is_not_closing_its_tabs() {
        let (mut list, _) = three();
        list.detach();
        list.sync(&[], 200);
        assert!(list.closed.is_empty());
        assert_eq!(list.entries.len(), 3);
    }

    #[test]
    fn fr3_ctrl_t_opens_the_command_bar_or_the_new_tab_page_by_setting() {
        assert_eq!(Settings::default().ctrl_t, CtrlT::CommandBar);
        for value in [CtrlT::CommandBar, CtrlT::NewTabPage] {
            assert_eq!(CtrlT::from_key(value.key()), Some(value));
        }
        assert_eq!(CtrlT::from_key("other"), None);
    }

    #[test]
    fn fr3_folder_rows_are_told_from_browser_tab_groups() {
        assert_eq!(folder_of_key(&folder_key(42)), Some(42));
        assert_eq!(folder_of_key(&[7; 16]), None);
    }

    #[test]
    fn fr3_the_tab_list_file_sits_beside_the_spaces_file_where_prune_reads_it() {
        let path = store_path(std::path::Path::new(r"C:\Local\Rivet\state\spaces\abcd.json"));
        assert_eq!(path, std::path::Path::new(r"C:\Local\Rivet\state\spaces\abcd.tabs.json"));
    }
}
