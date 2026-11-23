//! A typed look at `views::View`, instead of raw offsets scattered through the features.
//!
//! The offsets are those of the Chromium 156 builds, where `views::View` keeps its parent at
//! `+0x198` and its bounds as x, y, width and height at `+0x1C8`. A build whose layout differs
//! records its own offsets in its row and reads them from there.

/// A view of the browser, by address. Reading one is only as safe as the address: callers hold
/// views the browser handed them, or check with `readable` first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct View(usize);

impl View {
    /// Where a view keeps its parent.
    pub(crate) const PARENT: usize = 0x198;

    /// Where a view keeps its parent, from where its build keeps the bounds. In `views::View` the
    /// parent pointer lies 0x30 before the bounds: `parent_`, the `children_` vector, two flags and
    /// the optional preferred size come between (`ui/views/view.h`). `None` for a build that records
    /// no bounds offset.
    pub(crate) fn parent_offset(bounds_offset: usize) -> Option<usize> {
        (bounds_offset >= 0x30).then(|| bounds_offset - 0x30)
    }

    /// Where a view keeps its children, from where it keeps its parent: `children_`, a
    /// `std::vector` of views, comes right after `parent_` in `ui/views/view.h`. The view walk of
    /// `reflect` finds the same pair at run time, each child naming the view 8 bytes before its list.
    pub(crate) const fn children_offset(parent_offset: usize) -> usize {
        parent_offset + 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr3_a_views_parent_lies_0x30_before_its_bounds() {
        // Chrome 156 and later, Brave and Edge record their bounds at 0x1C8: the parent at 0x198.
        assert_eq!(View::parent_offset(0x1C8), Some(View::PARENT));
        // Whale 5.39 and Chrome 150 record 0x1C0; the view walk found their parent at 0x190.
        assert_eq!(View::parent_offset(0x1C0), Some(0x190));
        // A build that records no bounds offset has no parent offset either.
        assert_eq!(View::parent_offset(0), None);
    }
}
