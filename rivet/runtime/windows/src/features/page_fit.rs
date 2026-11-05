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
    let margins = [
        x,
        y,
        container_width - x - width,
        container_height - y - height,
    ];
    if width <= 0
        || height <= 0
        || margins
            .iter()
            .any(|margin| !(0..=MAX_INSET).contains(margin))
    {
        return page;
    }
    if margins.iter().all(|margin| *margin == 0) {
        return page;
    }
    [0, 0, container_width, container_height]
}

/// An extra page view's bounds `rect` grown by the inset the window's page had, the page bounds
/// `page` before `fill` grew them to `filled`. The browser keeps the same inset inside every page
/// view it makes, so Rivet's own views take it back the same way and their pages line up.
pub(crate) fn outset(rect: [i32; 4], page: [i32; 4], filled: [i32; 4]) -> [i32; 4] {
    if page == filled {
        return rect;
    }
    let left = page[0] - filled[0];
    let top = page[1] - filled[1];
    let right = (filled[0] + filled[2]) - (page[0] + page[2]);
    let bottom = (filled[1] + filled[3]) - (page[1] + page[3]);
    [
        rect[0] - left,
        rect[1] - top,
        rect[2] + left + right,
        rect[3] + top + bottom,
    ]
}

/// The border a page view keeps around the one view that holds its page, as left, top, right and
/// bottom, from the page view's size `view` and its children's bounds. Some builds give their own
/// page view a border of a few DIPs, which shows as a band inside Rivet's panel. A view with other
/// children, or a margin outside `0..=MAX_INSET`, has no such border.
pub(crate) fn holder_inset(view: [i32; 2], children: &[[i32; 4]]) -> [i32; 4] {
    let [only] = children else {
        return [0; 4];
    };
    let [x, y, width, height] = *only;
    let margins = [x, y, view[0] - x - width, view[1] - y - height];
    if width <= 0 || height <= 0 || margins.iter().any(|m| !(0..=MAX_INSET).contains(m)) {
        return [0; 4];
    }
    margins
}

/// The bounds `rect` grown by `inset` on each side, so the page inside a bordered view fills the
/// place Rivet meant for the view.
pub(crate) fn grow(rect: [i32; 4], inset: [i32; 4]) -> [i32; 4] {
    let [left, top, right, bottom] = inset;
    [
        rect[0] - left,
        rect[1] - top,
        rect[2] + left + right,
        rect[3] + top + bottom,
    ]
}

/// The place of the page inside a view at `rect` that keeps it `inset` from each side, the
/// reverse of [`grow`]. Rows and hit areas follow the page, not its grown view.
pub(crate) fn shrink(rect: [i32; 4], inset: [i32; 4]) -> [i32; 4] {
    let [left, top, right, bottom] = inset;
    [
        rect[0] + left,
        rect[1] + top,
        rect[2] - left - right,
        rect[3] - top - bottom,
    ]
}

#[cfg(test)]
mod tests {
    use super::{fill, grow, holder_inset, outset, shrink};

    #[test]
    fn fr9_a_grown_page_view_keeps_its_row_over_its_page() {
        // Samsung Internet 30.0.4.29: the right pane's view grew by its 2 DIP border, so its row
        // sat 2 DIPs above the left one and the panes were read right to left.
        let inset = [2, 2, 2, 2];
        let page = [578, 14, 570, 790];
        assert_eq!(shrink(grow(page, inset), inset), page);
        let pages = [
            (shrink([574, 10, 574, 794], inset), true, Some(0)),
            ([4, 12, 570, 790], false, Some(1)),
        ];
        let panes = crate::features::second_view::grid_panes(&pages, 1160, None);
        assert_eq!(panes[0].x, 4);
        assert_eq!(panes[0].row_y, panes[1].row_y);
    }

    #[test]
    fn fr9_a_page_view_with_a_border_grows_so_its_page_lines_up() {
        // Samsung Internet 30.0.4.29 keeps the page 2 DIPs inside its own page view, and Rivet's
        // second view has no border, so the split's panes started 2.4 DIP apart.
        let inset = holder_inset([570, 790], &[[2, 2, 566, 786]]);
        assert_eq!(inset, [2, 2, 2, 2]);
        assert_eq!(grow([578, 14, 570, 790], inset), [576, 12, 574, 794]);
        // The page fills the view: nothing to take back.
        assert_eq!(holder_inset([570, 790], &[[0, 0, 570, 790]]), [0; 4]);
        // Two children or a wide gap are the browser's own layout.
        assert_eq!(
            holder_inset([570, 790], &[[2, 2, 566, 786], [0, 0, 1, 1]]),
            [0; 4]
        );
        assert_eq!(holder_inset([570, 790], &[[0, 56, 570, 734]]), [0; 4]);
        assert_eq!(holder_inset([570, 790], &[]), [0; 4]);
    }

    #[test]
    fn fr9_an_extra_page_takes_back_the_inset_its_view_keeps() {
        // Samsung Internet 30.0.4.29 drew the split's second page 3 DIPs lower than the first.
        let page = [4, 3, 1152, 700];
        let filled = fill(page, [1164, 711]);
        assert_eq!(
            outset([600, 0, 500, 711], page, filled),
            [596, -3, 512, 722]
        );
        assert_eq!(
            outset([600, 0, 500, 711], filled, filled),
            [600, 0, 500, 711]
        );
    }

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
