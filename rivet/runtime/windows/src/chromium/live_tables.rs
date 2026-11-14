//! Reads the class tables of the views every browser window has from live objects, so a row needs
//! no class table address for them.
//!
//! A C++ object's first word is the table of its class. Every views class derives from
//! `views::View`, whose first base is `ui::LayerDelegate` (`ui/views/view.h`), so a view's first
//! word is its class's own table, the one whose slots hold the class's virtual functions. The views
//! are found by the class names their `ui::metadata` holds, through the view tree public Chromium
//! source describes:
//!
//! - `views::NonClientView` holds the window's frame view and its client view as children
//!   (`ui/views/window/non_client_view.h`), and the client view of a browser window is the
//!   `BrowserView` (`chrome/browser/ui/views/frame/browser_view.h`). So the frame view is the child of
//!   the browser view's parent whose class name names a frame view.
//! - The frame view's caption buttons are `WindowsCaptionButton` views below it
//!   (`chrome/browser/ui/views/frame/windows_caption_button.h`).
//!
//! A table is taken only when every live view of its class has the same one, it lies in read-only
//! data, its first slots lie in code, and, where the row also has the table, the two agree.

use crate::chromium::window_views::Memory;

/// The browser view's class name.
pub(crate) const BROWSER_VIEW: &str = "BrowserView";
/// The caption buttons' class name.
pub(crate) const CAPTION_BUTTON: &str = "WindowsCaptionButton";
/// The corner wedges' class name, where a browser has them.
pub(crate) const CORNER: &str = "CustomFloatingCorner";

/// How many views below the root are read at most.
const MOST_VIEWS: usize = 4000;
/// How many children one view may list.
const MOST_CHILDREN: usize = 256;

/// The views a view lists at `children`, a `std::vector<View*>`.
fn children<M: Memory>(memory: &M, view: usize, children: usize) -> Vec<usize> {
    let (Some(begin), Some(end)) = (
        memory.word(view + children),
        memory.word(view + children + 8),
    ) else {
        return Vec::new();
    };
    if begin == 0 || end < begin || (end - begin) % 8 != 0 || (end - begin) / 8 > MOST_CHILDREN {
        return Vec::new();
    }
    (begin..end)
        .step_by(8)
        .filter_map(|at| memory.word(at))
        .filter(|child| *child != 0)
        .collect()
}

/// Whether a class name names a browser frame view, as `BrowserFrameViewWin`, or a browser's own
/// subclass of one, as `EdgeBrowserFrameViewWin`, does.
pub(crate) fn is_frame_view(name: &str) -> bool {
    name.contains("FrameView")
}

/// Every view below `root`, the root included, whose class name `wanted` accepts, in tree order.
/// Only a child that names its parent at `parent` is followed, so a list read wrongly is not walked.
pub(crate) fn views_below<M: Memory>(
    memory: &M,
    root: usize,
    parent: usize,
    wanted: &dyn Fn(&str) -> bool,
) -> Vec<usize> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    let mut seen = 0;
    while let Some(view) = stack.pop() {
        seen += 1;
        if seen > MOST_VIEWS {
            break;
        }
        if memory.class_name(view).is_some_and(|name| wanted(&name)) {
            found.push(view);
        }
        let listed = children(memory, view, parent + 8);
        for child in listed.into_iter().rev() {
            if memory.word(child + parent) == Some(view) {
                stack.push(child);
            }
        }
    }
    found
}

/// The frame view of the window whose browser view is `browser_view`: the child of the browser
/// view's parent named as a frame view. `None` when there is not exactly one.
pub(crate) fn frame_view<M: Memory>(
    memory: &M,
    browser_view: usize,
    parent: usize,
) -> Option<usize> {
    let holder = memory.word(browser_view + parent).filter(|holder| *holder != 0)?;
    let frames: Vec<usize> = children(memory, holder, parent + 8)
        .into_iter()
        .filter(|child| *child != browser_view)
        .filter(|child| memory.word(child + parent) == Some(holder))
        .filter(|child| {
            memory
                .class_name(*child)
                .is_some_and(|name| is_frame_view(&name))
        })
        .collect();
    match frames.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// The class table every one of `views` begins with, when they all begin with the same one and
/// `is_table` accepts it. `None` for no views or views of different tables.
pub(crate) fn shared_table<M: Memory>(
    memory: &M,
    views: &[usize],
    is_table: &dyn Fn(usize) -> bool,
) -> Option<usize> {
    let mut tables = views.iter().map(|view| memory.word(*view));
    let first = tables.next()??;
    (tables.all(|table| table == Some(first)) && is_table(first)).then_some(first)
}

/// One class table from what the live views gave and what the row has, both as offsets in the
/// module. Both must agree when both are known; a row without one takes the live table, and a build
/// where no live view gave one keeps the row's.
pub(crate) fn settle_table(live: Option<usize>, row: Option<usize>) -> Option<usize> {
    match (live, row) {
        (Some(live), Some(row)) => (live == row).then_some(row),
        (Some(live), None) => Some(live),
        (None, row) => row,
    }
}

/// The class tables of the window's views, as addresses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tables {
    pub browser_view: Option<usize>,
    pub frame: Option<usize>,
    pub caption: Option<usize>,
    pub corner: Option<usize>,
}

/// Reads the tables from the window whose browser view is `browser_view`.
pub(crate) fn tables_of<M: Memory>(
    memory: &M,
    browser_view: usize,
    parent: usize,
    is_table: &dyn Fn(usize) -> bool,
) -> Tables {
    let named = |name: &str| memory.class_name(browser_view).as_deref() == Some(name);
    if !named(BROWSER_VIEW) {
        return Tables::default();
    }
    let frame = frame_view(memory, browser_view, parent);
    let below = |class: &'static str| {
        frame
            .map(|frame| views_below(memory, frame, parent, &|name| name == class))
            .unwrap_or_default()
    };
    Tables {
        browser_view: shared_table(memory, &[browser_view], is_table),
        frame: frame.and_then(|frame| shared_table(memory, &[frame], is_table)),
        caption: shared_table(memory, &below(CAPTION_BUTTON), is_table),
        corner: shared_table(memory, &below(CORNER), is_table),
    }
}

/// The tables the live views of this process gave, as offsets in the module, once read.
static FOUND: std::sync::Mutex<Option<Tables>> = std::sync::Mutex::new(None);

/// Reads the window's class tables once per process from the browser view the view walk found,
/// as offsets in the module at `base`.
pub(crate) fn note<M: Memory>(
    memory: &M,
    browser_view: usize,
    parent: usize,
    base: usize,
    is_table: &dyn Fn(usize) -> bool,
) -> Tables {
    let mut found = FOUND.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(tables) = *found {
        return tables;
    }
    let live = tables_of(memory, browser_view, parent, is_table);
    let offset = |table: Option<usize>| table.map(|table| table - base);
    let tables = Tables {
        browser_view: offset(live.browser_view),
        frame: offset(live.frame),
        caption: offset(live.caption),
        corner: offset(live.corner),
    };
    if tables != Tables::default() {
        *found = Some(tables);
    }
    tables
}

/// The class table a feature uses, as an offset in the module: the row's and the live one settled
/// together, or the address the build does not have when they disagree or neither is known.
pub(crate) fn table(row: usize, live: Option<usize>) -> usize {
    let row = (row != crate::builds::ABSENT).then_some(row);
    settle_table(live, row).unwrap_or(crate::builds::ABSENT)
}

/// The tables read so far, as offsets in the module.
pub(crate) fn found() -> Tables {
    FOUND
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A view tree in plain memory: objects whose first word is a table, with parent and child
    /// list fields where `views::View` keeps them, and class names by object.
    #[derive(Default)]
    struct Tree {
        words: HashMap<usize, usize>,
        names: HashMap<usize, String>,
        next_list: usize,
    }

    const PARENT: usize = 0x198;

    impl Tree {
        fn view(&mut self, at: usize, table: usize, name: &str) {
            self.words.insert(at, table);
            self.names.insert(at, name.to_owned());
        }

        fn adopt(&mut self, parent: usize, children: &[usize]) {
            self.next_list += 0x1000;
            let list = 0x90_0000 + self.next_list;
            for (index, child) in children.iter().enumerate() {
                self.words.insert(list + index * 8, *child);
                self.words.insert(child + PARENT, parent);
            }
            self.words.insert(parent + PARENT + 8, list);
            self.words.insert(parent + PARENT + 16, list + children.len() * 8);
        }
    }

    impl Memory for Tree {
        fn word(&self, address: usize) -> Option<usize> {
            Some(self.words.get(&address).copied().unwrap_or(0))
        }
        fn class_name(&self, object: usize) -> Option<String> {
            self.names.get(&object).cloned()
        }
    }

    const BASE: usize = 0x1_8000_0000;
    const NON_CLIENT: usize = 0x10_0000;
    const BROWSER: usize = 0x20_0000;
    const FRAME: usize = 0x30_0000;
    const CONTAINER: usize = 0x40_0000;
    const BUTTONS: [usize; 3] = [0x50_0000, 0x51_0000, 0x52_0000];

    fn is_table(table: usize) -> bool {
        table >= BASE + 0x100_0000 && table < BASE + 0x200_0000
    }

    /// A browser window's tree: a non-client view holding the frame view and the browser view,
    /// and three caption buttons in a container of the frame view.
    fn window(frame_name: &str, button_tables: [usize; 3]) -> Tree {
        let mut tree = Tree::default();
        tree.view(NON_CLIENT, BASE + 0x100_1000, "NonClientView");
        tree.view(BROWSER, BASE + 0x100_2000, BROWSER_VIEW);
        tree.view(FRAME, BASE + 0x100_3000, frame_name);
        tree.view(CONTAINER, BASE + 0x100_4000, "WindowsCaptionButtonContainer");
        for (button, table) in BUTTONS.iter().zip(button_tables) {
            tree.view(*button, table, CAPTION_BUTTON);
        }
        tree.adopt(NON_CLIENT, &[FRAME, BROWSER]);
        tree.adopt(FRAME, &[CONTAINER]);
        tree.adopt(CONTAINER, &BUTTONS);
        tree
    }

    #[test]
    fn nfr6_a_views_class_table_is_read_from_its_live_views() {
        let caption = BASE + 0x100_5000;
        let tree = window("EdgeBrowserFrameViewWin", [caption; 3]);
        let tables = tables_of(&tree, BROWSER, PARENT, &is_table);
        assert_eq!(tables.browser_view, Some(BASE + 0x100_2000));
        assert_eq!(tables.frame, Some(BASE + 0x100_3000));
        assert_eq!(tables.caption, Some(caption));
        // The window has no corner wedges, so their table stays the row's.
        assert_eq!(tables.corner, None);
        let buttons = views_below(&tree, FRAME, PARENT, &|name| name == CAPTION_BUTTON);
        assert_eq!(buttons, BUTTONS);
    }

    #[test]
    fn nfr6_live_views_of_one_class_must_share_their_table() {
        let (one, other) = (BASE + 0x100_5000, BASE + 0x100_6000);
        let tree = window("BrowserFrameViewWin", [one, one, other]);
        assert_eq!(tables_of(&tree, BROWSER, PARENT, &is_table).caption, None);
        // A first word outside the module's tables is no class table.
        let tree = window("BrowserFrameViewWin", [0x1234; 3]);
        assert_eq!(tables_of(&tree, BROWSER, PARENT, &is_table).caption, None);
        // Only a view named as a frame view is the frame view.
        let tree = window("OpaqueTitleBar", [BASE + 0x100_5000; 3]);
        let tables = tables_of(&tree, BROWSER, PARENT, &is_table);
        assert_eq!((tables.frame, tables.caption), (None, None));
        // The walk starts only from a view named as the browser view.
        let tree = window("BrowserFrameViewWin", [BASE + 0x100_5000; 3]);
        assert_eq!(tables_of(&tree, FRAME, PARENT, &is_table), Tables::default());
    }

    #[test]
    fn nfr6_a_row_without_a_class_table_takes_the_live_one() {
        let absent = crate::builds::ABSENT;
        assert_eq!(table(absent, Some(0x1185_ff40)), 0x1185_ff40);
        assert_eq!(table(0x1185_ff40, Some(0x1185_ff40)), 0x1185_ff40);
        assert_eq!(table(0x1185_ff40, Some(0x11b5_2528)), absent);
        assert_eq!(table(0x1185_ff40, None), 0x1185_ff40);
        assert_eq!(table(absent, None), absent);
    }

    #[test]
    fn nfr6_a_live_table_and_the_rows_must_agree() {
        assert_eq!(settle_table(Some(0x10), Some(0x10)), Some(0x10));
        assert_eq!(settle_table(Some(0x10), Some(0x18)), None);
        assert_eq!(settle_table(Some(0x10), None), Some(0x10));
        assert_eq!(settle_table(None, Some(0x18)), Some(0x18));
        assert_eq!(settle_table(None, None), None);
    }
}
