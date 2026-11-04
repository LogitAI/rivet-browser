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
    /// Where a view keeps its bounds in its parent.
    pub(crate) const BOUNDS: usize = 0x1C8;

    pub(crate) const fn at(address: usize) -> Self {
        Self(address)
    }

    /// The address of the parent, or zero for a root.
    pub(crate) unsafe fn parent_address(self) -> usize {
        *((self.0 + Self::PARENT) as *const usize)
    }

    /// x, y, width and height in the parent.
    pub(crate) unsafe fn bounds(self) -> [i32; 4] {
        *((self.0 + Self::BOUNDS) as *const [i32; 4])
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
            assert_eq!(view.bounds(), [10, 20, 300, 40]);
            assert_eq!(view.parent_address(), parent_at);
            assert_eq!(View::at(parent_at).parent_address(), 0);
        }
    }
}
