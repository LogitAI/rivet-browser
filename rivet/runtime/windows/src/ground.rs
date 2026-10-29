//! The ground of a Rivet window: the frame colour, the space tint, and the shadows of the pages,
//! drawn as one composition visual of the browser window with a transparent rounded hole exactly
//! where each page panel sits. The browser's own frame paint can only approximate a gradient or a
//! blurred shadow with rectangles, so the appbar and the frame never matched. Here both are one
//! Direct2D drawing with the same gradient and the same shadow blur as the shell study's
//! `.window` and `.panel`.
use crate::chrome_layout::Rect;
use std::mem::ManuallyDrop;
use windows::core::{Interface, Result};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_FILL_MODE_ALTERNATE, D2D1_GRADIENT_STOP,
    D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Shadow, ID2D1DeviceContext, ID2D1Factory, ID2D1Geometry, ID2D1Image, ID2D1Layer, ID2D1RenderTarget,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2,
    D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1,
    D2D1_PROPERTY_TYPE_UNKNOWN, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT,
};
use windows_numerics::{Matrix3x2, Vector2};

/// The frame colour of the Work space.
pub const FRAME: u32 = 0xece8e2;
/// The paper of a page that has not painted yet.
pub const PAPER: u32 = 0xfbfaf7;
/// The radius of a page panel, in device-independent pixels.
pub const RADIUS: f32 = 12.0;

/// How high a page sits above the ground, which sets its shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Raise {
    /// A page that sits behind the page being worked in, in a split.
    Other,
    /// The only page of a window.
    Page,
    /// The page being worked in, in a split.
    Focused,
}

/// A page panel: where the ground leaves a hole, in device-independent pixels of the browser's
/// layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub rect: Rect,
    pub raise: Raise,
    /// A strip of the page's leading edge, up to this x in the browser's layout, that the browser
    /// has made room for but has not painted yet. The ground covers it with paper, so the
    /// browser's own frame never shows there.
    pub cover: Option<i32>,
}

/// How long a page's newly exposed leading strip is covered, which is about as long as the
/// browser takes to draw the page into its new width.
pub const COVER_MS: u64 = 80;

/// The leading edges a page panel had lately, to find the strip a layout exposed.
#[derive(Default)]
pub struct EdgeTrail(Vec<(u64, i32)>);

impl EdgeTrail {
    /// Records that the page's leading edge is at `left` at time `now`, in milliseconds, and
    /// returns how far right the strip between the edge and its recent positions reaches, when
    /// the edge moved left within the last [`COVER_MS`].
    pub fn push(&mut self, now: u64, left: i32) -> Option<i32> {
        self.0.retain(|(at, _)| now.saturating_sub(*at) < COVER_MS);
        let reach = self.0.iter().map(|(_, edge)| *edge).max().filter(|edge| *edge > left);
        self.0.push((now, left));
        reach
    }
}

/// A page's shadows as the shell study gives them: the ring's opacity, then `(offset_y, blur,
/// colour, opacity)` for each soft shadow.
struct Look {
    ring: f32,
    soft: &'static [(f32, f32, u32, f32)],
}

const fn look(raise: Raise) -> Look {
    match raise {
        // 0 0 0 1px rgba(0,0,0,.05), 0 1px 3px rgba(0,0,0,.05), 0 12px 32px rgba(40,28,30,.08)
        Raise::Page => Look { ring: 0.05, soft: &[(1.0, 3.0, 0x00_00_00, 0.05), (12.0, 32.0, 0x28_1c_1e, 0.08)] },
        // 0 0 0 1px rgba(0,0,0,.07), 0 2px 6px rgba(0,0,0,.06), 0 18px 44px rgba(40,28,30,.16)
        Raise::Focused => Look { ring: 0.07, soft: &[(2.0, 6.0, 0x00_00_00, 0.06), (18.0, 44.0, 0x28_1c_1e, 0.16)] },
        // 0 0 0 1px rgba(0,0,0,.04), 0 6px 18px rgba(40,28,30,.05)
        Raise::Other => Look { ring: 0.04, soft: &[(6.0, 18.0, 0x28_1c_1e, 0.05)] },
    }
}

fn rgba(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
}

/// A hole in whole pixels of a window laid out at `scale` device pixels per DIP. Both edges are
/// rounded, so a hole is as wide as the layer the browser snaps the page to.
pub fn hole_pixels(hole: Rect, scale: f32) -> D2D_RECT_F {
    let edge = |value: i32| (value as f32 * scale).round();
    rect(edge(hole.x), edge(hole.y), edge(hole.x + hole.width), edge(hole.y + hole.height))
}

/// The holes of a window's pages, with the page being worked in last so its deeper shadow is not
/// dimmed by the others.
pub fn stacked(holes: &[Hole]) -> Vec<Hole> {
    let mut order = holes.to_vec();
    order.sort_by_key(|hole| hole.raise);
    order
}

/// The pages of a split as holes: every page with its top row, the one being worked in raised.
pub fn split_holes(pages: &[Rect], active: usize, row: i32) -> Vec<Hole> {
    pages
        .iter()
        .enumerate()
        .map(|(index, page)| Hole {
            rect: Rect { x: page.x, y: page.y - row, width: page.width, height: page.height + row },
            raise: if index == active { Raise::Focused } else { Raise::Other },
            cover: None,
        })
        .collect()
}

/// Fills `area` with the ground of the whole window: the frame colour, a faint white glow from the
/// top right corner and the space's tint rising from the lower left, as the study's `.window`
/// background. The surface starts at `origin` of the `window` size, so a part of the window gets
/// exactly the colours it has in the whole.
unsafe fn background(
    context: &ID2D1DeviceContext,
    area: D2D_RECT_F,
    origin: (f32, f32),
    window: (f32, f32),
    tint: u32,
) -> Result<()> {
    let (width, height) = window;
    let target: &ID2D1RenderTarget = context;
    context.FillRectangle(&area, &context.CreateSolidColorBrush(&rgba(FRAME, 1.0), None)?);
    let fade = |colour: u32, peak: f32| -> Result<_> {
        let stops = [
            D2D1_GRADIENT_STOP { position: 0.0, color: rgba(colour, peak) },
            D2D1_GRADIENT_STOP { position: 0.7, color: rgba(colour, 0.0) },
        ];
        target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    };
    let radial = |x: f32, y: f32, rx: f32, ry: f32, stops| {
        context.CreateRadialGradientBrush(
            &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: Vector2 { X: x - origin.0, Y: y - origin.1 },
                gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                radiusX: rx,
                radiusY: ry,
            },
            None,
            stops,
        )
    };
    let white = fade(0xffffff, 0.55)?;
    let glow = radial(width, 0.0, width * 0.6, height * 0.5, &white)?;
    context.FillRectangle(&area, &glow);
    let warm = fade(tint, 0.34)?;
    let rise = radial(0.0, height, width * 0.8, height * 0.6, &warm)?;
    context.FillRectangle(&area, &rise);
    Ok(())
}

/// Draws the whole ground of a `width` by `height` pixel window at `scale` pixels per DIP onto a
/// transparent surface drawn in pixels: the background and the pages' shadows, with a hole for
/// every page. The context's world transform may carry the surface's offset in its atlas.
pub unsafe fn paint(
    context: &ID2D1DeviceContext,
    width: f32,
    height: f32,
    scale: f32,
    tint: u32,
    holes: &[Hole],
) -> Result<()> {
    context.Clear(Some(&rgba(0, 0.0)));
    let factory = context.GetFactory()?;
    let radius = RADIUS * scale;
    let shape = |area: D2D_RECT_F, grow: f32| D2D1_ROUNDED_RECT {
        rect: rect(area.left - grow, area.top - grow, area.right + grow, area.bottom + grow),
        radiusX: radius + grow,
        radiusY: radius + grow,
    };
    let stacked = stacked(holes);
    let areas: Vec<D2D_RECT_F> = stacked.iter().map(|hole| hole_pixels(hole.rect, scale)).collect();

    // Everything but the pages: the outer rectangle with the pages cut out of it.
    let mut parts: Vec<Option<ID2D1Geometry>> =
        vec![Some(factory.CreateRectangleGeometry(&rect(-1.0, -1.0, width + 1.0, height + 1.0))?.cast()?)];
    for area in &areas {
        parts.push(Some(factory.CreateRoundedRectangleGeometry(&shape(*area, 0.0))?.cast()?));
    }
    let ground: ID2D1Geometry = factory.CreateGeometryGroup(D2D1_FILL_MODE_ALTERNATE, &parts)?.cast()?;
    let mut layer = D2D1_LAYER_PARAMETERS1 {
        contentBounds: rect(-1.0e6, -1.0e6, 1.0e6, 1.0e6),
        geometricMask: ManuallyDrop::new(Some(ground)),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity: 1.0,
        opacityBrush: ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
    };
    context.PushLayer(&layer, None::<&ID2D1Layer>);
    ManuallyDrop::drop(&mut layer.geometricMask);
    let result = (|| -> Result<()> {
        background(context, rect(0.0, 0.0, width, height), (0.0, 0.0), (width, height), tint)?;
        for (hole, area) in stacked.iter().zip(&areas) {
            let page = look(hole.raise);
            let outline = context.CreateSolidColorBrush(&rgba(0, page.ring), None)?;
            context.DrawRoundedRectangle(&shape(*area, 0.5 * scale), &outline, scale, None);
            for &(offset, blur, colour, peak) in page.soft {
                soft_shadow(context, &shape(*area, 0.0), offset * scale, blur * scale, colour, peak)?;
            }
        }
        Ok(())
    })();
    context.PopLayer();
    result?;
    cover_strips(context, &factory, &stacked, &areas, scale, radius)
}

/// Paints the paper over the leading strip of every page that has one, inside the page's own
/// rounded corners.
unsafe fn cover_strips(
    context: &ID2D1DeviceContext,
    factory: &ID2D1Factory,
    holes: &[Hole],
    areas: &[D2D_RECT_F],
    scale: f32,
    radius: f32,
) -> Result<()> {
    for (hole, area) in holes.iter().zip(areas) {
        let Some(reach) = hole.cover else { continue };
        let right = (reach as f32 * scale).round().min(area.right);
        if right <= area.left {
            continue;
        }
        let page: ID2D1Geometry = factory
            .CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT { rect: *area, radiusX: radius, radiusY: radius })?
            .cast()?;
        let mut layer = D2D1_LAYER_PARAMETERS1 {
            contentBounds: rect(-1.0e6, -1.0e6, 1.0e6, 1.0e6),
            geometricMask: ManuallyDrop::new(Some(page)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: 1.0,
            opacityBrush: ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        context.PushLayer(&layer, None::<&ID2D1Layer>);
        ManuallyDrop::drop(&mut layer.geometricMask);
        let strip = rect(area.left, area.top, right, area.bottom);
        context.FillRectangle(&strip, &context.CreateSolidColorBrush(&rgba(PAPER, 1.0), None)?);
        context.PopLayer();
    }
    Ok(())
}

/// One CSS box shadow of a rounded rectangle: the shape blurred by a Gaussian of half the blur
/// radius, moved down by `offset` pixels.
unsafe fn soft_shadow(
    context: &ID2D1DeviceContext,
    shape: &D2D1_ROUNDED_RECT,
    offset: f32,
    blur: f32,
    colour: u32,
    peak: f32,
) -> Result<()> {
    let previous = context.GetTarget()?;
    let list = context.CreateCommandList()?;
    context.SetTarget(&list);
    context.FillRoundedRectangle(shape, &context.CreateSolidColorBrush(&rgba(0, 1.0), None)?);
    let closed = list.Close();
    context.SetTarget(&previous);
    closed?;
    let effect = context.CreateEffect(&CLSID_D2D1Shadow)?;
    effect.SetInput(0, &list.cast::<ID2D1Image>()?, true);
    effect.SetValue(0, D2D1_PROPERTY_TYPE_UNKNOWN, &(blur / 2.0).to_ne_bytes())?;
    let tone = rgba(colour, peak);
    let mut value = [0u8; 16];
    for (slot, part) in value.chunks_mut(4).zip([tone.r, tone.g, tone.b, tone.a]) {
        slot.copy_from_slice(&part.to_ne_bytes());
    }
    effect.SetValue(1, D2D1_PROPERTY_TYPE_UNKNOWN, &value)?;
    // The best quality, so a wide blur is not drawn at a lower resolution.
    effect.SetValue(2, D2D1_PROPERTY_TYPE_UNKNOWN, &2u32.to_ne_bytes())?;
    let image: ID2D1Image = effect.cast()?;
    context.DrawImage(
        &image,
        Some(&Vector2 { X: 0.0, Y: offset }),
        None,
        D2D1_INTERPOLATION_MODE_LINEAR,
        D2D1_COMPOSITE_MODE_SOURCE_OVER,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Rect = Rect { x: 268, y: 8, width: 960, height: 760 };

    #[test]
    fn fr12_hole_edges_are_whole_pixels_at_a_fractional_scale() {
        let area = hole_pixels(Rect { x: 267, y: 7, width: 101, height: 33 }, 1.25);
        assert_eq!((area.left, area.top, area.right, area.bottom), (334.0, 9.0, 460.0, 50.0));
    }

    #[test]
    fn fr12_split_pages_each_get_a_hole_with_their_top_row_and_the_active_one_is_raised() {
        let pages = [PAGE, Rect { x: 1236, ..PAGE }];
        let holes = split_holes(&pages, 1, 44);
        assert_eq!(holes.len(), 2);
        assert_eq!(holes[0].rect, Rect { x: 268, y: -36, width: 960, height: 804 });
        assert_eq!((holes[0].raise, holes[1].raise), (Raise::Other, Raise::Focused));
    }

    #[test]
    fn fr12_a_page_that_grows_leftward_covers_the_strip_it_exposed_until_the_browser_paints() {
        let mut trail = EdgeTrail::default();
        assert_eq!(trail.push(0, 300), None);
        assert_eq!(trail.push(16, 296), Some(300));
        assert_eq!(trail.push(32, 280), Some(300));
        assert_eq!(trail.push(60, 280), Some(300));
        // Once the older positions are past the cover time, only the later ones count.
        assert_eq!(trail.push(90, 280), Some(296));
        assert_eq!(trail.push(100, 280), None);
    }

    #[test]
    fn fr12_a_page_that_shrinks_or_rests_covers_nothing() {
        let mut trail = EdgeTrail::default();
        assert_eq!(trail.push(0, 280), None);
        assert_eq!(trail.push(16, 296), None);
        assert_eq!(trail.push(32, 296), None);
    }

    #[test]
    fn fr12_the_raised_page_is_painted_last() {
        let holes = [
            Hole { rect: PAGE, raise: Raise::Focused, cover: None },
            Hole { rect: Rect { x: 0, ..PAGE }, raise: Raise::Other, cover: None },
        ];
        assert_eq!(stacked(&holes).last().map(|hole| hole.raise), Some(Raise::Focused));
    }

    #[test]
    fn fr12_a_page_is_deeper_than_the_pages_behind_it() {
        let reach = |raise| look(raise).soft.iter().map(|(offset, blur, _, _)| offset + blur).fold(0.0, f32::max);
        assert!(reach(Raise::Focused) > reach(Raise::Page));
        assert!(reach(Raise::Page) > reach(Raise::Other));
    }
}
