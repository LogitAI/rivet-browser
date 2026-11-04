//! The grid of a split: up to four pages of one window, arranged as a tree. A leaf shows one page
//! view, named by its slot: slot 0 is the window's own page view, and slots 1 to 3 are Rivet's
//! extra page views. A node holds two children side by side (`Row`) or stacked (`Column`), with
//! the first child's share of the room in thousandths. Everything here is plain geometry, so the
//! page views and the drawing follow one model.
use crate::features::split::Side;

/// The most pages one window shows at once.
pub(crate) const MAX_PANES: usize = 4;

/// How a node lays out its two children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    Row,
    Column,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Leaf(usize),
    Split { dir: Dir, share: u32, a: Box<Node>, b: Box<Node> },
}

/// A divider between two parts of the grid. `node` names the split it belongs to in pre-order,
/// `rect` is the strip that is dragged, and `span` is the area the split divides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Divider {
    pub node: usize,
    pub dir: Dir,
    pub rect: [i32; 4],
    pub span: [i32; 4],
}

/// The sizes the layout keeps: the gap between pages, the row above a page that is not on the top
/// line, and the narrowest a page gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Metrics {
    pub divider: i32,
    pub row: i32,
    pub min: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grid {
    pub root: Node,
}

impl Grid {
    /// The window's own page alone.
    pub(crate) fn single() -> Self {
        Grid { root: Node::Leaf(0) }
    }

    /// The slots in reading order: left to right, then top to bottom within each part.
    pub(crate) fn leaves(&self) -> Vec<usize> {
        fn walk(node: &Node, out: &mut Vec<usize>) {
            match node {
                Node::Leaf(slot) => out.push(*slot),
                Node::Split { a, b, .. } => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    pub(crate) fn contains(&self, slot: usize) -> bool {
        self.leaves().contains(&slot)
    }

    /// The lowest extra slot no page uses, or none when the grid is full.
    pub(crate) fn free_slot(&self) -> Option<usize> {
        if self.leaves().len() >= MAX_PANES {
            return None;
        }
        (1..MAX_PANES).find(|slot| !self.contains(*slot))
    }

    /// Puts `slot` on `side` of `target`. Returns false when the grid is full, `target` is not in
    /// it, or `slot` already is. The shares become equal along each line of pages.
    pub(crate) fn insert(&mut self, target: usize, slot: usize, side: Side) -> bool {
        if self.leaves().len() >= MAX_PANES || self.contains(slot) || !self.contains(target) {
            return false;
        }
        fn put(node: &mut Node, target: usize, slot: usize, side: Side) {
            if *node == Node::Leaf(target) {
                let dir = match side {
                    Side::Left | Side::Right => Dir::Row,
                    Side::Top | Side::Bottom => Dir::Column,
                };
                let (a, b) = match side {
                    Side::Left | Side::Top => (Node::Leaf(slot), Node::Leaf(target)),
                    Side::Right | Side::Bottom => (Node::Leaf(target), Node::Leaf(slot)),
                };
                *node = Node::Split { dir, share: 500, a: Box::new(a), b: Box::new(b) };
            } else if let Node::Split { a, b, .. } = node {
                put(a, target, slot, side);
                put(b, target, slot, side);
            }
        }
        put(&mut self.root, target, slot, side);
        self.balance();
        true
    }

    /// Takes `slot` out. Its sibling takes its place. The last page is never taken out.
    pub(crate) fn remove(&mut self, slot: usize) -> bool {
        if !self.contains(slot) || self.leaves().len() < 2 {
            return false;
        }
        fn take(node: Node, slot: usize) -> Option<Node> {
            match node {
                Node::Leaf(at) if at == slot => None,
                Node::Leaf(at) => Some(Node::Leaf(at)),
                Node::Split { dir, share, a, b } => match (take(*a, slot), take(*b, slot)) {
                    (Some(a), Some(b)) => Some(Node::Split { dir, share, a: Box::new(a), b: Box::new(b) }),
                    (one, two) => one.or(two),
                },
            }
        }
        let root = std::mem::replace(&mut self.root, Node::Leaf(0));
        self.root = take(root, slot).unwrap_or(Node::Leaf(0));
        self.balance();
        true
    }

    /// Gives the page at slot `from` the name `to`.
    pub(crate) fn rename(&mut self, from: usize, to: usize) {
        fn walk(node: &mut Node, from: usize, to: usize) {
            match node {
                Node::Leaf(at) if *at == from => *at = to,
                Node::Leaf(_) => {}
                Node::Split { a, b, .. } => {
                    walk(a, from, to);
                    walk(b, from, to);
                }
            }
        }
        walk(&mut self.root, from, to);
    }

    /// The slot that takes over from `slot` when it leaves: the first page of its sibling.
    pub(crate) fn heir(&self, slot: usize) -> Option<usize> {
        fn first(node: &Node) -> usize {
            match node {
                Node::Leaf(at) => *at,
                Node::Split { a, .. } => first(a),
            }
        }
        fn find(node: &Node, slot: usize) -> Option<usize> {
            let Node::Split { a, b, .. } = node else { return None };
            if **a == Node::Leaf(slot) {
                return Some(first(b));
            }
            if **b == Node::Leaf(slot) {
                return Some(first(a));
            }
            find(a, slot).or_else(|| find(b, slot))
        }
        find(&self.root, slot)
    }

    /// Makes the pages of each line equal: a node's share is the number of pages its first child
    /// lays out along the node's direction, out of all of them.
    pub(crate) fn balance(&mut self) {
        fn along(node: &Node, dir: Dir) -> u32 {
            match node {
                Node::Split { dir: own, a, b, .. } if *own == dir => along(a, dir) + along(b, dir),
                _ => 1,
            }
        }
        fn walk(node: &mut Node) {
            if let Node::Split { dir, share, a, b } = node {
                let (one, two) = (along(a, *dir), along(b, *dir));
                *share = one * 1000 / (one + two);
                walk(a);
                walk(b);
            }
        }
        walk(&mut self.root);
    }

    /// Sets the share of the split `node`, numbered as `dividers` numbers them.
    pub(crate) fn set_share(&mut self, node: usize, value: u32) -> bool {
        fn walk(node: &mut Node, target: usize, next: &mut usize, value: u32) -> bool {
            let Node::Split { share, a, b, .. } = node else { return false };
            if *next == target {
                *share = value.min(1000);
                return true;
            }
            *next += 1;
            walk(a, target, next, value) || walk(b, target, next, value)
        }
        let mut next = 0;
        walk(&mut self.root, node, &mut next, value)
    }

    /// The share of the root split, for the window's first divider.
    pub(crate) fn set_root_share(&mut self, value: u32) -> bool {
        self.set_share(0, value)
    }

    /// Each slot's page in `area` as x, y, width and height. A page that has another page above it
    /// leaves `row` DIPs above it for its own row.
    pub(crate) fn cells(&self, area: [i32; 4], metrics: Metrics) -> Vec<(usize, [i32; 4])> {
        fn walk(node: &Node, area: [i32; 4], metrics: Metrics, out: &mut Vec<(usize, [i32; 4])>) {
            match node {
                Node::Leaf(slot) => out.push((*slot, area)),
                Node::Split { dir, share, a, b } => {
                    let (one, two, _) = divide(area, *dir, *share, metrics);
                    walk(a, one, metrics, out);
                    walk(b, two, metrics, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, area, metrics, &mut out);
        out
    }

    /// The dividers of the grid in `area`, in pre-order of their splits.
    pub(crate) fn dividers(&self, area: [i32; 4], metrics: Metrics) -> Vec<Divider> {
        fn walk(node: &Node, area: [i32; 4], metrics: Metrics, next: &mut usize, out: &mut Vec<Divider>) {
            let Node::Split { dir, share, a, b } = node else { return };
            let (one, two, rect) = divide(area, *dir, *share, metrics);
            out.push(Divider { node: *next, dir: *dir, rect, span: area });
            *next += 1;
            walk(a, one, metrics, next, out);
            walk(b, two, metrics, next, out);
        }
        let mut out = Vec::new();
        let mut next = 0;
        walk(&self.root, area, metrics, &mut next, &mut out);
        out
    }
}

/// The two parts of `area` a split gives its children, and the divider strip between them.
pub(crate) fn divide(area: [i32; 4], dir: Dir, share: u32, metrics: Metrics) -> ([i32; 4], [i32; 4], [i32; 4]) {
    let [x, y, w, h] = area;
    let Metrics { divider, row, min } = metrics;
    // A stacked page's row sits between the divider and its page, so the row is not room to share.
    let (length, kept) = match dir {
        Dir::Row => (w, divider),
        Dir::Column => (h, divider + row),
    };
    let room = (length - kept).max(0);
    let mut first = (room as i64 * share.min(1000) as i64 / 1000) as i32;
    if room >= 2 * min {
        first = first.clamp(min, room - min);
    }
    let second = room - first;
    match dir {
        Dir::Row => ([x, y, first, h], [x + first + divider, y, second, h], [x + first, y, divider, h]),
        Dir::Column => ([x, y, w, first], [x, y + first + kept, w, second], [x, y + first, w, divider]),
    }
}

/// The share a divider of `span` dragged to `position` (x for a row, y for a column) gives the
/// first part.
pub(crate) fn share_at(span: [i32; 4], dir: Dir, position: i32, metrics: Metrics) -> u32 {
    let (start, length, kept) = match dir {
        Dir::Row => (span[0], span[2], metrics.divider),
        Dir::Column => (span[1], span[3], metrics.divider + metrics.row),
    };
    let room = (length - kept).max(1) as f32;
    let first = (position - start) as f32 - metrics.divider as f32 / 2.0;
    ((first / room) * 1000.0).round().clamp(0.0, 1000.0) as u32
}

#[cfg(test)]
mod tests {
    use super::{share_at, Dir, Grid, Metrics, MAX_PANES};
    use crate::features::split::Side;

    const M: Metrics = Metrics { divider: 8, row: 44, min: 120 };

    #[test]
    fn fr9_a_drop_on_each_edge_splits_the_page_in_that_direction() {
        for (side, first) in [(Side::Left, 1), (Side::Right, 0), (Side::Top, 1), (Side::Bottom, 0)] {
            let mut grid = Grid::single();
            assert!(grid.insert(0, 1, side));
            let cells = grid.cells([0, 0, 1008, 1052], M);
            assert_eq!(cells[0].0, first, "{side:?}");
            let (a, b) = (cells[0].1, cells[1].1);
            match side {
                Side::Left | Side::Right => {
                    assert_eq!((a[1], b[1]), (0, 0), "side by side on one line");
                    assert_eq!((a[2], b[2]), (500, 500), "equal shares");
                }
                Side::Top | Side::Bottom => {
                    assert_eq!(a[0], b[0], "stacked in one column");
                    assert_eq!(b[1], a[1] + a[3] + 8 + 44, "the lower page keeps room for its row");
                    assert_eq!((a[3], b[3]), (500, 500), "equal shares");
                }
            }
        }
    }

    #[test]
    fn fr9_a_window_holds_at_most_four_pages_in_a_grid() {
        let mut grid = Grid::single();
        assert!(grid.insert(0, 1, Side::Right));
        assert!(grid.insert(0, 2, Side::Bottom));
        assert!(grid.insert(1, 3, Side::Bottom));
        assert_eq!(grid.leaves().len(), MAX_PANES);
        assert_eq!(grid.free_slot(), None);
        assert!(!grid.insert(0, 4, Side::Left), "a fifth page does not fit");
        let cells = grid.cells([0, 0, 1008, 1052], M);
        assert_eq!(cells.len(), 4);
        // A two by two grid: two columns of 500, each with two pages of 500.
        for (_, cell) in &cells {
            assert_eq!((cell[2], cell[3]), (500, 500), "{cells:?}");
        }
    }

    #[test]
    fn fr9_three_pages_in_a_line_get_equal_shares() {
        let mut grid = Grid::single();
        grid.insert(0, 1, Side::Right);
        grid.insert(1, 2, Side::Right);
        let cells = grid.cells([0, 0, 916, 600], M);
        let widths: Vec<i32> = cells.iter().map(|cell| cell.1[2]).collect();
        // Thousandths of the room leave at most a few DIPs between them.
        let (low, high) = (*widths.iter().min().unwrap(), *widths.iter().max().unwrap());
        assert!(high - low <= 3, "{widths:?}");
        assert_eq!(widths.iter().sum::<i32>() + 2 * 8, 916);
        assert_eq!(grid.leaves(), [0, 1, 2]);
    }

    #[test]
    fn fr9_no_page_of_the_grid_overlaps_another() {
        let mut grid = Grid::single();
        grid.insert(0, 1, Side::Top);
        grid.insert(1, 2, Side::Left);
        grid.insert(0, 3, Side::Right);
        let cells = grid.cells([10, 20, 1200, 800], M);
        for (i, (_, a)) in cells.iter().enumerate() {
            for (_, b) in &cells[i + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} {b:?}");
            }
            assert!(a[0] >= 10 && a[1] >= 20 && a[0] + a[2] <= 1210 && a[1] + a[3] <= 820);
        }
    }

    #[test]
    fn fr9_every_divider_can_be_dragged_and_keeps_the_narrowest_page() {
        let mut grid = Grid::single();
        grid.insert(0, 1, Side::Right);
        grid.insert(1, 2, Side::Bottom);
        let area = [0, 0, 1008, 1052];
        let dividers = grid.dividers(area, M);
        assert_eq!(dividers.len(), 2, "one divider per split");
        assert_eq!((dividers[0].dir, dividers[1].dir), (Dir::Row, Dir::Column));
        // Drag the column divider of the right half up to its top.
        let lower = dividers[1];
        let share = share_at(lower.span, lower.dir, lower.span[1], M);
        assert!(grid.set_share(lower.node, share));
        let cells = grid.cells(area, M);
        let right_top = cells.iter().find(|cell| cell.0 == 1).unwrap().1;
        assert_eq!(right_top[3], 120, "a page never gets smaller than the narrowest");
        // Drag the row divider to 300 DIPs.
        let row = grid.dividers(area, M)[0];
        assert!(grid.set_share(row.node, share_at(row.span, row.dir, 304, M)));
        assert_eq!(grid.cells(area, M)[0].1[2], 300);
    }

    #[test]
    fn fr9_a_closed_page_gives_its_place_to_its_sibling() {
        let mut grid = Grid::single();
        grid.insert(0, 1, Side::Right);
        grid.insert(1, 2, Side::Bottom);
        assert_eq!(grid.heir(2), Some(1));
        assert_eq!(grid.heir(0), Some(1));
        assert!(grid.remove(2));
        assert_eq!(grid.leaves(), [0, 1]);
        assert!(grid.remove(0));
        grid.rename(1, 0);
        assert_eq!(grid, Grid::single());
        assert!(!grid.remove(0), "the last page stays");
    }
}
