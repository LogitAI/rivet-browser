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
    /// Where a Chromium 156 view keeps its bounds in its parent, for the tests of the typed reads.
    #[cfg(test)]
    pub(crate) const BOUNDS: usize = 0x1C8;

    /// Where a view keeps its parent, from where its build keeps the bounds. In `views::View` the
    /// parent pointer lies 0x30 before the bounds: `parent_`, the `children_` vector, two flags and
    /// the optional preferred size come between (`ui/views/view.h`). `None` for a build that records
    /// no bounds offset.
    pub(crate) fn parent_offset(bounds_offset: usize) -> Option<usize> {
        (bounds_offset >= 0x30).then(|| bounds_offset - 0x30)
    }

    pub(crate) const fn at(address: usize) -> Self {
        Self(address)
    }

    /// The address of the parent kept at `offset`, or zero for a root.
    pub(crate) unsafe fn parent_address(self, offset: usize) -> usize {
        *((self.0 + offset) as *const usize)
    }

    /// x, y, width and height in the parent, kept at `offset`.
    pub(crate) unsafe fn bounds(self, offset: usize) -> [i32; 4] {
        *((self.0 + offset) as *const [i32; 4])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two fake views in plain memory: a child inside a parent.
    #[test]
    fn nfr3_a_view_reads_its_parent_and_bounds_at_the_recorded_offsets() {
        let mut parent = vec![0u8; 0x300];
        let mut child = vec![0u8; 0x300];
        let parent_at = parent.as_mut_ptr() as usize;
        child[View::PARENT..View::PARENT + 8].copy_from_slice(&parent_at.to_le_bytes());
        for (index, value) in [10i32, 20, 300, 40].into_iter().enumerate() {
            child[View::BOUNDS + index * 4..View::BOUNDS + index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
        for (index, value) in [5i32, 6, 900, 800].into_iter().enumerate() {
            parent[View::BOUNDS + index * 4..View::BOUNDS + index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
        let view = View::at(child.as_ptr() as usize);
        unsafe {
            assert_eq!(view.bounds(View::BOUNDS), [10, 20, 300, 40]);
            assert_eq!(view.parent_address(View::PARENT), parent_at);
            assert_eq!(View::at(parent_at).parent_address(View::PARENT), 0);
        }
    }

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
