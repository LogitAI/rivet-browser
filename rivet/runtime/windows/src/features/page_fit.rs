//! The window's page fills the container the browser keeps it in.
//!
//! Some builds lay their page view out a few DIPs inside its container: about 3 DIPs at the top
//! and 4 to 8 at the sides and the bottom. Rivet's panel already rounds and shadows the page, so
//! that margin only shows as a band of the container's colour above the page, and it puts the
//! page lower than the row of a split's other pane. Rivet lays the page out over the margin.

/// The widest margin that is taken for a container's inset. Wider gaps are the browser's own
/// bars, which stay.
pub(crate) const MAX_INSET: i32 = 8;

/// The page view's bounds `page`, in its container, grown over a small inset of the container
/// whose size is `container`. Bounds that fill the container or leave a wider gap stay.
pub(crate) fn fill(page: [i32; 4], container: [i32; 2]) -> [i32; 4] {
    let [x, y, width, height] = page;
    let [container_width, container_height] = container;
    let margins = [x, y, container_width - x - width, container_height - y - height];
    if width <= 0 || height <= 0 || margins.iter().any(|margin| !(0..=MAX_INSET).contains(margin)) {
        return page;
    }
    if margins.iter().all(|margin| *margin == 0) {
        return page;
    }
    [0, 0, container_width, container_height]
}

#[cfg(test)]
mod tests {
    use super::fill;

    #[test]
    fn fr10_a_page_inset_by_its_container_fills_the_container() {
        // Samsung Internet 30.0.4.29 keeps its page 3 DIPs below the container's top and 4 to 8
        // in from the sides and the bottom, which left a 3 DIP band above the page.
        assert_eq!(fill([4, 3, 1152, 700], [1164, 711]), [0, 0, 1164, 711]);
        assert_eq!(fill([8, 3, 1148, 702], [1164, 713]), [0, 0, 1164, 713]);
    }

    #[test]
    fn fr10_a_page_that_fills_its_container_or_sits_below_a_bar_stays() {
        assert_eq!(fill([0, 0, 1164, 711], [1164, 711]), [0, 0, 1164, 711]);
        // A 56 DIP infobar above the page is the browser's own and keeps its place.
        assert_eq!(fill([0, 56, 1164, 655], [1164, 711]), [0, 56, 1164, 655]);
        // Bounds the container cannot hold are not touched.
        assert_eq!(fill([0, 0, 1200, 711], [1164, 711]), [0, 0, 1200, 711]);
        assert_eq!(fill([4, 3, 0, 700], [1164, 711]), [4, 3, 0, 700]);
    }
}
