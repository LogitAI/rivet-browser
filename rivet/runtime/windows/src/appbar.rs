//! Composes and paints the Rivet appbar as part of the browser window's own surface.
//!
//! Chromium's top-level window has no GDI redirection surface, so GDI drawing never reaches the
//! screen. DirectComposition can still attach a visual tree to the window from inside the browser
//! process. Windows composes that visual with the browser's content as one window, so animations,
//! snapping, rounded corners and shadows treat the appbar and the browser as a single surface.
//! Direct2D and DirectWrite paint the visual with the shell study's fonts, which are embedded in
//! the runtime and loaded into a private font collection.

use crate::ground;
use crate::spaces::{SpaceInfo, Spaces};
use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT, D2D1_CAP_STYLE_FLAT,
    D2D1_DASH_STYLE_DASH, D2D1_LINE_JOIN_MITER, D2D1_STROKE_STYLE_PROPERTIES,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionSurface,
    IDCompositionTarget, IDCompositionVisual, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection1, IDWriteTextFormat3,
    IDWriteTextLayout1, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_TAG_OPTICAL_SIZE,
    DWRITE_FONT_AXIS_VALUE, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows_numerics::{Matrix3x2, Vector2};

/// Fonts of the shell study, licensed under the SIL Open Font License 1.1.
const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/Newsreader[opsz,wght].ttf"),
    include_bytes!("../fonts/Manrope[wght].ttf"),
    include_bytes!("../fonts/DMMono-Regular.ttf"),
    include_bytes!("../fonts/DMMono-Medium.ttf"),
];

/// Colours of the shell study for the Work space.
mod palette {
    pub const FRAME: u32 = 0xece8e2;
    pub const TINT: u32 = 0xd08a52;
    pub const ACCENT: u32 = 0x9a5424;
    pub const INK: u32 = 0x1f2024;
    pub const SOFT_INK: u32 = 0x4a4850;
    pub const MUTED: u32 = 0x8a8690;
    /// A top row button that has nothing to do, such as back on the first page.
    pub const WHITE: u32 = 0xffffff;
}
use palette::*;

/// A text style from the shell study: family, size, weight and letter spacing in em.
#[derive(Clone, Copy)]
struct Type {
    family: &'static str,
    size: f32,
    weight: i32,
    tracking: f32,
}

const fn style(family: &'static str, size: f32, weight: i32, tracking: f32) -> Type {
    Type {
        family,
        size,
        weight,
        tracking,
    }
}

const SPACE_NAME: Type = style("Newsreader", 14.0, 600, -0.01);
const BADGE: Type = style("DM Mono", 10.0, 500, 0.0);
const META: Type = style("DM Mono", 10.0, 400, 0.0);
const META_ROW: Type = style("Manrope", 12.0, 400, 0.0);
const TAB: Type = style("Manrope", 12.5, 400, 0.0);
const ACTIVE: Type = style("Manrope", 12.5, 600, 0.0);
const FAVICON: Type = style("Manrope", 9.0, 700, 0.0);
/// Interface icons come from the font Windows 11 draws its own caption buttons with.
const ICONS: &str = "Segoe Fluent Icons";
const PANEL_GAP: f32 = 8.0;
const PANEL_RADIUS: f32 = 12.0;

fn color(rgb: u32) -> D2D1_COLOR_F {
    rgba(rgb, 1.0)
}

fn rgba(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

/// What leads a row of the tab list.
#[derive(Clone, Copy)]
enum Lead {
    Glyph(char),
    Favicon(u32, &'static str),
}

/// One row of the tab list.
struct Row {
    lead: Lead,
    title: &'static str,
    meta: &'static str,
    child: bool,
    active: bool,
}

const fn row(lead: Lead, title: &'static str, meta: &'static str) -> Row {
    Row {
        lead,
        title,
        meta,
        child: false,
        active: false,
    }
}

const PINNED: [(u32, &str); 4] = [
    (0xd4513b, "M"),
    (0x3b6fd4, "31"),
    (0x2f8f6b, "D"),
    (0x1f2024, "N"),
];

fn rect(x: f32, y: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

/// The geometry the shown surface was painted for. The visual keeps the surface alive.
struct Painted {
    width: i32,
    height: i32,
    dpi: u32,
    span: i32,
}

/// The composition objects of one browser window's appbar.
pub struct Appbar {
    window: HWND,
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    fonts: IDWriteFontCollection1,
    // The target must outlive the visual tree it shows.
    _target: IDCompositionTarget,
    root: IDCompositionVisual2,
    visual: IDCompositionVisual2,
    painted: Option<Painted>,
    /// The ground of the whole window, with a hole for each page, and what it was last painted for.
    ground: IDCompositionVisual2,
    ground_painted: Option<(i32, i32, u32, u32, Vec<ground::Hole>)>,
    panel: IDCompositionVisual2,
    panel_painted: Option<Painted>,
    tabs: Vec<crate::tab_rows::Row>,
    spaces: Vec<SpaceInfo>,
    current: usize,
    hover: Option<crate::frame::AppbarHit>,
    /// The press feedback of the controls: scale and brightness amounts at this moment.
    effects: Vec<Effect>,
    menu: Option<crate::frame::Menu>,
    /// False while the appbar is hidden: the column then only carries the ground of the frame.
    docked: bool,
    /// Whether the resizer is being dragged, which keeps its grip showing.
    dragging: bool,
    toggle: IDCompositionVisual2,
    toggle_painted: Option<(i32, u32, bool, i32, i32)>,
    peek: IDCompositionVisual2,
    peek_painted: Option<Painted>,
    /// The drop target and the dragged tab's ghost, which exist only while a tab is dragged.
    drag: Option<DragVisuals>,
    /// The top rows of the pages of a split, which exist while the window shows one.
    rows: Vec<(IDCompositionVisual2, Option<(PaneRow, u32)>)>,
}

/// What one page's top row shows. `x`, `y`, `width` and `height` are window pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaneRow {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// How far the visual reaches into the gap beside the row, on each side, in pixels.
    pub reach_left: f32,
    pub reach_right: f32,
    /// The width at the row's right end that the native caption buttons keep, in pixels.
    pub caption: f32,
    pub active: bool,
    pub first: bool,
    pub last: bool,
    pub title: String,
    pub url: String,
    pub hover: Option<crate::chrome_split::RowButton>,
}

/// The visuals of a tab drag and what they were last painted for.
struct DragVisuals {
    zone: IDCompositionVisual2,
    zone_key: Option<(i32, i32, u32, String)>,
    ghost: IDCompositionVisual2,
    ghost_key: Option<(i32, u32, String, u32)>,
}

/// Where a dragged tab would land: the rectangle in window pixels and its label.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropView {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
}

/// The dragged tab following the pointer, with its top left corner in window pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GhostView {
    pub x: f32,
    pub y: f32,
    pub title: String,
    pub colour: u32,
    pub letter: String,
}

/// The scale and brightness amounts of one control's press at this moment.
pub(crate) type Effect = (crate::frame::AppbarHit, f32, f32);

/// What the appbar shows: the tab rows, the spaces, the pointer and the controls in motion.
#[derive(Clone, Copy)]
struct Scene<'a> {
    tabs: &'a [crate::tab_rows::Row],
    spaces: &'a [SpaceInfo],
    current: usize,
    hover: Option<crate::frame::AppbarHit>,
    effects: &'a [Effect],
    menu: Option<crate::frame::Menu>,
}

impl Scene<'_> {
    fn space(&self) -> SpaceInfo {
        self.spaces
            .get(self.current)
            .or(self.spaces.first())
            .cloned()
            .unwrap_or_else(|| Spaces::new().list()[0].clone())
    }

    fn tint(&self) -> u32 {
        self.spaces.get(self.current).map_or(TINT, |s| s.tint)
    }

    /// The press feedback of the first of `hits` that is in motion.
    fn effect(&self, hits: &[crate::frame::AppbarHit]) -> Option<(f32, f32)> {
        self.effects
            .iter()
            .find(|effect| hits.contains(&effect.0))
            .map(|effect| (effect.1, effect.2))
    }
}

/// What a painted surface shows of the appbar.
#[derive(Clone, Copy)]
enum Surface {
    /// The whole appbar over the frame ground.
    Docked,
    /// The appbar's content, on the fill of the floating appbar.
    Floating,
}

/// Room around the floating appbar for its shadow, in device-independent pixels.
const PEEK_PAD: f32 = 40.0;

/// The floating appbar's fill: the frame colour 88% over white, as in the shell study.
fn floating_fill() -> u32 {
    let mix = |shift: u32| {
        let channel = ((FRAME >> shift) & 0xff) as f32;
        ((channel * 0.88 + 255.0 * 0.12).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

impl Appbar {
    pub unsafe fn new(window: isize) -> Result<Self> {
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
        let dxgi: IDXGIDevice = d3d.expect("D3D11CreateDevice returned a device").cast()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d: ID2D1Device = factory.CreateDevice(&dxgi)?;
        let device: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
        let hwnd = HWND(window as _);
        // A window can carry one topmost and one bottom-most target. Chromium composes its web
        // content in a child window, so either is free. Prefer topmost.
        let target = device
            .CreateTargetForHwnd(hwnd, true)
            .or_else(|_| device.CreateTargetForHwnd(hwnd, false))?;
        let root = device.CreateVisual()?;
        let visual = device.CreateVisual()?;
        root.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
        // The ground lies just under the appbar, over the browser's content and the pages' top
        // rows. Its holes leave the pages showing.
        let ground = device.CreateVisual()?;
        root.AddVisual(&ground, false, &visual)?;
        let panel = device.CreateVisual()?;
        root.AddVisual(&panel, true, &visual)?;
        let toggle = device.CreateVisual()?;
        root.AddVisual(&toggle, true, &panel)?;
        // The floating appbar covers the toggle, as it covers the rest of the top row.
        let peek = device.CreateVisual()?;
        root.AddVisual(&peek, true, &toggle)?;
        target.SetRoot(&root)?;
        let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&dwrite)?;
        Ok(Self {
            window: hwnd,
            device,
            dwrite,
            fonts,
            _target: target,
            root,
            visual,
            painted: None,
            ground,
            ground_painted: None,
            panel,
            panel_painted: None,
            tabs: Vec::new(),
            spaces: Spaces::new().list().to_vec(),
            current: 0,
            hover: None,
            effects: Vec::new(),
            menu: None,
            docked: true,
            dragging: false,
            toggle,
            toggle_painted: None,
            peek,
            peek_painted: None,
            drag: None,
            rows: Vec::new(),
        })
    }

    /// The composition device, the root of the visual tree and the text factory, for other parts
    /// of the window's surface such as the Sidebar's rail.
    pub(crate) fn composition(
        &self,
    ) -> (IDCompositionDesktopDevice, IDCompositionVisual2, IDWriteFactory5) {
        (self.device.clone(), self.root.clone(), self.dwrite.clone())
    }

    /// Whether the column shows the whole appbar, and whether its resizer is being dragged.
    pub fn set_layout(&mut self, docked: bool, dragging: bool) {
        if self.docked != docked || self.dragging != dragging {
            self.docked = docked;
            self.dragging = dragging;
            self.painted = None;
            self.peek_painted = None;
        }
    }

    pub fn set_state(
        &mut self,
        tabs: Vec<crate::tab_rows::Row>,
        spaces: &[SpaceInfo],
        current: usize,
        hover: Option<crate::frame::AppbarHit>,
        effects: Vec<Effect>,
        menu: Option<crate::frame::Menu>,
    ) {
        if self.menu != menu
            || self.tabs != tabs
            || self.spaces != spaces
            || self.current != current
            || self.hover != hover
            || self.effects != effects
        {
            self.menu = menu;
            self.tabs = tabs;
            self.spaces = spaces.to_vec();
            self.current = current;
            self.hover = hover;
            self.effects = effects;
            self.painted = None;
            self.peek_painted = None;
            self.toggle_painted = None;
        }
    }

    fn scene(&self) -> Scene<'_> {
        Scene {
            tabs: &self.tabs,
            spaces: &self.spaces,
            current: self.current,
            hover: self.hover,
            effects: &self.effects,
            menu: self.menu,
        }
    }

    /// Draws the toggle button in the leading space of the toolbar row. `x` and `y` are physical
    /// pixels in the window's client area.
    pub unsafe fn update_toggle(
        &mut self,
        visible: bool,
        x: i32,
        y: i32,
        size: i32,
        dpi: u32,
    ) -> Result<()> {
        if !visible || size <= 0 {
            self.toggle.SetContent(None)?;
            self.toggle_painted = None;
            return self.device.Commit();
        }
        self.toggle.SetOffsetX2(x as f32)?;
        self.toggle.SetOffsetY2(y as f32)?;
        let hot = self.hover == Some(crate::frame::AppbarHit::Toggle);
        let effect = self.scene().effect(&[crate::frame::AppbarHit::Toggle]);
        let key = (
            size,
            dpi,
            hot,
            (effect.map_or(0.0, |e| e.0) * 1000.0) as i32,
            (effect.map_or(0.0, |e| e.1) * 1000.0) as i32,
        );
        if self.toggle_painted != Some(key) {
            let surface = self.device.CreateSurface(
                size as u32,
                size as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            let mut offset = POINT::default();
            let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
            let scale = dpi as f32 / 96.0;
            context.SetDpi(dpi as f32, dpi as f32);
            context.SetTransform(&Matrix3x2::translation(
                offset.x as f32 / scale,
                offset.y as f32 / scale,
            ));
            context.Clear(Some(&rgba(WHITE, 0.0)));
            let canvas = Canvas {
                context: &context,
                dwrite: &self.dwrite,
                fonts: &self.fonts,
            };
            let area = rect(0.0, 0.0, size as f32 / scale, size as f32 / scale);
            let result = canvas.pressed(area, 8.0, 0.96, effect, || {
                if hot || effect.is_some() {
                    canvas.fill_rounded(area, 8.0, rgba(INK, 0.06))?;
                }
                // The design's glyph sits above where DirectWrite centres it, and darkens to the
                // ink colour on hover like the other icons of the row.
                let (w, h) = (area.right - area.left, area.bottom - area.top);
                let glyph = rect(area.left, area.top - 0.6, w, h);
                let colour = if hot || effect.is_some() { INK } else { SOFT_INK };
                canvas.icon('\u{E90C}', glyph, crate::TOGGLE_GLYPH, colour)
            });
            surface.EndDraw()?;
            result?;
            self.toggle.SetContent(&surface)?;
            self.toggle_painted = Some(key);
        }
        self.device.Commit()
    }

    /// Draws the hidden appbar floating over the page. The floating rectangle starts `inset` DIPs
    /// from the window's left edge and from `top`. `slide` is 0 while it is out of the window and 1
    /// when it has arrived.
    pub unsafe fn update_peek(
        &mut self,
        visible: bool,
        top: i32,
        side: i32,
        height: i32,
        dpi: u32,
        slide: f32,
    ) -> Result<()> {
        let scale = dpi as f32 / 96.0;
        let inset = crate::FLOAT_INSET as f32;
        if !visible || side <= 0 || height <= 0 || slide <= 0.0 {
            self.peek.SetContent(None)?;
            self.peek_painted = None;
            return self.device.Commit();
        }
        let (w, h) = (side as f32 + PEEK_PAD * 2.0, height as f32 + PEEK_PAD * 2.0);
        let (pw, ph) = ((w * scale).round() as i32, (h * scale).round() as i32);
        // The visual leaves to the left by its whole width plus the gap, as the study's
        // translateX(-100% - 16px).
        let travel = (1.0 - slide) * (side as f32 + inset * 2.0) * scale;
        self.peek
            .SetOffsetX2(((inset - PEEK_PAD) * scale - travel).round())?;
        self.peek
            .SetOffsetY2(top as f32 + ((inset - PEEK_PAD) * scale).round())?;
        let stale = self
            .peek_painted
            .as_ref()
            .is_none_or(|p| (p.width, p.height, p.dpi) != (pw, ph, dpi));
        if stale {
            let surface = self.device.CreateSurface(
                pw as u32,
                ph as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            let mut offset = POINT::default();
            let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
            context.SetDpi(dpi as f32, dpi as f32);
            let origin = Matrix3x2::translation(offset.x as f32 / scale, offset.y as f32 / scale);
            context.SetTransform(&origin);
            context.Clear(Some(&rgba(WHITE, 0.0)));
            let canvas = Canvas {
                context: &context,
                dwrite: &self.dwrite,
                fonts: &self.fonts,
            };
            let result = (|| {
                canvas.floating(rect(PEEK_PAD, PEEK_PAD, side as f32, height as f32))?;
                context.SetTransform(&(Matrix3x2::translation(PEEK_PAD, PEEK_PAD) * origin));
                canvas.draw(
                    side as f32,
                    height as f32,
                    &self.scene(),
                    Surface::Floating,
                )
            })();
            surface.EndDraw()?;
            result?;
            self.peek.SetContent(&surface)?;
            self.peek_painted = Some(Painted {
                width: pw,
                height: ph,
                dpi,
                span: pw,
            });
        }
        self.device.Commit()
    }

    /// Positions the visual in window coordinates and repaints it when its size or DPI changed.
    pub unsafe fn update(
        &mut self,
        visible: bool,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        dpi: u32,
    ) -> Result<()> {
        // A hidden appbar leaves its column to the ground visual, which carries the same colours.
        if !visible || !self.docked || width <= 0 || height <= 0 {
            self.visual.SetContent(None)?;
            self.painted = None;
            return self.device.Commit();
        }
        self.visual.SetOffsetX2(left as f32)?;
        self.visual.SetOffsetY2(top as f32)?;
        let stale = self
            .painted
            .as_ref()
            .is_none_or(|p| (p.width, p.height, p.dpi) != (width, height, dpi));
        if stale {
            let surface = self.device.CreateSurface(
                width as u32,
                height as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            self.paint(&surface, width, height, dpi)?;
            self.visual.SetContent(&surface)?;
            self.painted = Some(Painted {
                width,
                height,
                dpi,
                span: width,
            });
        }
        self.device.Commit()
    }

    /// Shows the ground of the window, `width` by `height` pixels, with a hole for each page. It
    /// is repainted only when its size, scale, tint or holes change. No holes means the browser
    /// has not laid the pages out yet, so the ground stays away rather than cover them.
    pub(crate) unsafe fn update_ground(
        &mut self,
        visible: bool,
        width: i32,
        height: i32,
        dpi: u32,
        holes: &[ground::Hole],
    ) -> Result<()> {
        if !visible || holes.is_empty() || width <= 0 || height <= 0 {
            if self.ground_painted.take().is_some() {
                self.ground.SetContent(None)?;
                self.device.Commit()?;
            }
            return Ok(());
        }
        let tint = self.scene().tint();
        let key = (width, height, dpi, tint, holes.to_vec());
        if self.ground_painted.as_ref() == Some(&key) {
            return Ok(());
        }
        let surface = self.device.CreateSurface(
            width as u32,
            height as u32,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED,
        )?;
        let mut offset = POINT::default();
        let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
        // The ground is drawn in pixels, so the hole edges land on whole pixels.
        context.SetDpi(96.0, 96.0);
        context.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
        let result = ground::paint(&context, width as f32, height as f32, dpi as f32 / 96.0, tint, holes);
        surface.EndDraw()?;
        result?;
        self.ground.SetContent(&surface)?;
        self.ground_painted = Some(key);
        self.device.Commit()
    }

    /// Experiment: paints the frame around the page panel. `caption` is the width of the native
    /// caption buttons in DIPs, which the frame never covers.
    pub unsafe fn update_panel(
        &mut self,
        visible: bool,
        top: i32,
        width: i32,
        height: i32,
        dpi: u32,
        caption: f32,
    ) -> Result<()> {
        if !visible || width <= 0 || height <= 0 {
            self.panel.SetContent(None)?;
            self.panel_painted = None;
            return self.device.Commit();
        }
        self.panel.SetOffsetY2(top as f32)?;
        let stale = self
            .panel_painted
            .as_ref()
            .is_none_or(|p| (p.width, p.height, p.dpi) != (width, height, dpi));
        if stale {
            let surface = self.device.CreateSurface(
                width as u32,
                height as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            let mut offset = POINT::default();
            let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
            let scale = dpi as f32 / 96.0;
            context.SetDpi(dpi as f32, dpi as f32);
            context.SetTransform(&Matrix3x2::translation(
                offset.x as f32 / scale,
                offset.y as f32 / scale,
            ));
            let canvas = Canvas {
                context: &context,
                dwrite: &self.dwrite,
                fonts: &self.fonts,
            };
            let result = canvas.draw_panel(width as f32 / scale, height as f32 / scale, caption);
            surface.EndDraw()?;
            result?;
            self.panel.SetContent(&surface)?;
            self.panel_painted = Some(Painted {
                width,
                height,
                dpi,
                span: width,
            });
        }
        self.device.Commit()
    }

    unsafe fn paint(
        &self,
        surface: &IDCompositionSurface,
        width: i32,
        height: i32,
        dpi: u32,
    ) -> Result<()> {
        let mut offset = POINT::default();
        let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
        let scale = dpi as f32 / 96.0;
        context.SetDpi(dpi as f32, dpi as f32);
        // BeginDraw returns an atlas offset in physical pixels, while drawing uses DIPs.
        context.SetTransform(&Matrix3x2::translation(
            offset.x as f32 / scale,
            offset.y as f32 / scale,
        ));
        context.Clear(Some(&rgba(WHITE, 0.0)));
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let result = canvas.draw(
            width as f32 / scale,
            height as f32 / scale,
            &self.scene(),
            Surface::Docked,
        );
        surface.EndDraw()?;
        result
    }
}

const PAPER: u32 = 0xfbfaf7;
const HAIRLINE: u32 = 0xe9e9e6;

impl Appbar {
    /// Shows the top rows of the pages of a split. The active page's row is the browser's own, so
    /// only the buttons Rivet adds are drawn on it. An empty list removes the rows.
    pub(crate) unsafe fn update_rows(&mut self, rows: &[PaneRow], dpi: u32) -> Result<()> {
        while self.rows.len() < rows.len() {
            let visual = self.device.CreateVisual()?;
            // The rows lie under the ground, whose holes are the pages, so the ground rounds
            // their corners.
            self.root.AddVisual(&visual, false, &self.ground)?;
            self.rows.push((visual, None));
        }
        let scale = dpi as f32 / 96.0;
        let (device, dwrite, fonts) = (self.device.clone(), self.dwrite.clone(), self.fonts.clone());
        for (index, (visual, painted)) in self.rows.iter_mut().enumerate() {
            let Some(row) = rows.get(index) else {
                if painted.take().is_some() {
                    visual.SetContent(None)?;
                }
                continue;
            };
            // The visual starts at the window's top edge, so it also covers the frame above the row.
            visual.SetOffsetX2((row.x - row.reach_left).round())?;
            visual.SetOffsetY2(0.0)?;
            let (width, height) = (
                (row.reach_left + row.width - row.caption + row.reach_right).round() as i32,
                (row.y + row.height).round() as i32,
            );
            if painted.as_ref() == Some(&(row.clone(), dpi)) || width <= 0 || height <= 0 {
                continue;
            }
            let surface = device.CreateSurface(
                width as u32,
                height as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            let mut offset = POINT::default();
            let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
            context.SetDpi(dpi as f32, dpi as f32);
            context.SetTransform(&Matrix3x2::translation(
                offset.x as f32 / scale,
                offset.y as f32 / scale,
            ));
            context.Clear(Some(&rgba(WHITE, 0.0)));
            let canvas = Canvas { context: &context, dwrite: &dwrite, fonts: &fonts };
            let result = canvas.pane_row(row, width as f32 / scale, height as f32 / scale, scale);
            surface.EndDraw()?;
            result?;
            visual.SetContent(&surface)?;
            *painted = Some((row.clone(), dpi));
        }
        self.device.Commit()
    }
}

/// The width of one line of text in a style, in device-independent pixels.
unsafe fn measure(
    dwrite: &IDWriteFactory5,
    fonts: &IDWriteFontCollection1,
    style: Type,
    value: &str,
) -> Result<f32> {
    let locale = HSTRING::from("en-us");
    let format = dwrite.CreateTextFormat(
        &HSTRING::from(style.family),
        fonts,
        DWRITE_FONT_WEIGHT(style.weight),
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        style.size,
        PCWSTR(locale.as_ptr()),
    )?;
    format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    let text: Vec<u16> = value.encode_utf16().collect();
    let layout = dwrite.CreateTextLayout(&text, &format, 4096.0, 40.0)?;
    let mut metrics = DWRITE_TEXT_METRICS::default();
    layout.GetMetrics(&mut metrics)?;
    Ok(metrics.widthIncludingTrailingWhitespace.ceil())
}

/// Room around the ghost for its shadow, in device-independent pixels.
const GHOST_PAD: f32 = 20.0;
const GHOST_HEIGHT: f32 = 32.0;

impl Appbar {
    /// Shows or hides the drop target and the ghost of a dragged tab. Both are visuals of the
    /// window, on top of everything else the appbar draws.
    pub(crate) unsafe fn update_drag(
        &mut self,
        zone: Option<&DropView>,
        ghost: Option<&GhostView>,
        dpi: u32,
    ) -> Result<()> {
        if zone.is_none() && ghost.is_none() {
            if let Some(drag) = self.drag.as_mut() {
                drag.zone.SetContent(None)?;
                drag.ghost.SetContent(None)?;
                drag.zone_key = None;
                drag.ghost_key = None;
                self.device.Commit()?;
            }
            return Ok(());
        }
        if self.drag.is_none() {
            let zone = self.device.CreateVisual()?;
            self.root.AddVisual(&zone, true, None::<&IDCompositionVisual>)?;
            let ghost = self.device.CreateVisual()?;
            self.root.AddVisual(&ghost, true, None::<&IDCompositionVisual>)?;
            self.drag = Some(DragVisuals { zone, zone_key: None, ghost, ghost_key: None });
        }
        let scale = dpi as f32 / 96.0;
        let tint = self.scene().tint();
        let (device, dwrite, fonts) = (self.device.clone(), self.dwrite.clone(), self.fonts.clone());
        let drag = self.drag.as_mut().expect("created above");
        match zone {
            Some(zone) => {
                let (width, height) = (zone.width.round() as i32, zone.height.round() as i32);
                drag.zone.SetOffsetX2(zone.x.round())?;
                drag.zone.SetOffsetY2(zone.y.round())?;
                let key = (width, height, dpi, zone.label.clone());
                if drag.zone_key.as_ref() != Some(&key) && width > 0 && height > 0 {
                    let surface = device.CreateSurface(
                        width as u32,
                        height as u32,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_ALPHA_MODE_PREMULTIPLIED,
                    )?;
                    let mut offset = POINT::default();
                    let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
                    context.SetDpi(dpi as f32, dpi as f32);
                    context.SetTransform(&Matrix3x2::translation(
                        offset.x as f32 / scale,
                        offset.y as f32 / scale,
                    ));
                    context.Clear(Some(&rgba(WHITE, 0.0)));
                    let canvas = Canvas { context: &context, dwrite: &dwrite, fonts: &fonts };
                    let result =
                        canvas.drop_target(width as f32 / scale, height as f32 / scale, &zone.label, tint);
                    surface.EndDraw()?;
                    result?;
                    drag.zone.SetContent(&surface)?;
                    drag.zone_key = Some(key);
                }
            }
            None => {
                drag.zone.SetContent(None)?;
                drag.zone_key = None;
            }
        }
        match ghost {
            Some(ghost) => {
                let text = measure(&dwrite, &fonts, ACTIVE, &ghost.title)?;
                let width = (9.0 + 18.0 + 8.0 + text + 12.0).min(260.0);
                let (pw, ph) = (
                    ((width + 2.0 * GHOST_PAD) * scale).ceil() as i32,
                    ((GHOST_HEIGHT + 2.0 * GHOST_PAD) * scale).ceil() as i32,
                );
                drag.ghost.SetOffsetX2((ghost.x - GHOST_PAD * scale).round())?;
                drag.ghost.SetOffsetY2((ghost.y - GHOST_PAD * scale).round())?;
                let key = (pw, dpi, ghost.title.clone(), ghost.colour);
                if drag.ghost_key.as_ref() != Some(&key) {
                    let surface = device.CreateSurface(
                        pw as u32,
                        ph as u32,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_ALPHA_MODE_PREMULTIPLIED,
                    )?;
                    let mut offset = POINT::default();
                    let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
                    context.SetDpi(dpi as f32, dpi as f32);
                    context.SetTransform(&Matrix3x2::translation(
                        offset.x as f32 / scale,
                        offset.y as f32 / scale,
                    ));
                    context.Clear(Some(&rgba(WHITE, 0.0)));
                    let canvas = Canvas { context: &context, dwrite: &dwrite, fonts: &fonts };
                    let result = canvas.ghost(width, ghost);
                    surface.EndDraw()?;
                    result?;
                    drag.ghost.SetContent(&surface)?;
                    drag.ghost_key = Some(key);
                }
            }
            None => {
                drag.ghost.SetContent(None)?;
                drag.ghost_key = None;
            }
        }
        self.device.Commit()
    }
}

unsafe fn load_fonts(dwrite: &IDWriteFactory5) -> Result<IDWriteFontCollection1> {
    let loader = dwrite.CreateInMemoryFontFileLoader()?;
    dwrite.RegisterFontFileLoader(&loader)?;
    let builder = dwrite.CreateFontSetBuilder()?;
    for font in FONTS {
        // The bytes are static, so DirectWrite can read them in place without an owner object.
        let file = loader.CreateInMemoryFontFileReference(
            dwrite,
            font.as_ptr().cast(),
            font.len() as u32,
            None,
        )?;
        builder.AddFontFile(&file)?;
    }
    let set = builder.CreateFontSet()?;
    dwrite.CreateFontCollectionFromFontSet(&set)
}

struct Canvas<'a> {
    context: &'a ID2D1DeviceContext,
    dwrite: &'a IDWriteFactory5,
    fonts: &'a IDWriteFontCollection1,
}

impl Canvas<'_> {
    unsafe fn brush(&self, rgb: u32) -> Result<ID2D1SolidColorBrush> {
        self.context.CreateSolidColorBrush(&color(rgb), None)
    }

    unsafe fn fill(&self, area: D2D_RECT_F, rgb: u32) -> Result<()> {
        self.context.FillRectangle(&area, &self.brush(rgb)?);
        Ok(())
    }

    unsafe fn dot(&self, x: f32, y: f32, size: f32, rgb: u32) -> Result<()> {
        let radius = size / 2.0;
        let ellipse = D2D1_ELLIPSE {
            point: Vector2 {
                X: x + radius,
                Y: y + radius,
            },
            radiusX: radius,
            radiusY: radius,
        };
        self.context.FillEllipse(&ellipse, &self.brush(rgb)?);
        Ok(())
    }

    /// The Rivet mark uses the same tile, ring and pin as the shell study.
    unsafe fn mark(&self, x: f32, y: f32, size: f32) -> Result<()> {
        let unit = size / 96.0;
        self.fill_rounded(rect(x, y, size, size), 24.0 * unit, color(0xf2eee5))?;
        let start = Vector2 {
            X: x + 58.0 * unit,
            Y: y + 58.0 * unit,
        };
        let end = Vector2 {
            X: x + 74.0 * unit,
            Y: y + 74.0 * unit,
        };
        self.context
            .DrawLine(start, end, &self.brush(INK)?, 13.0 * unit, None);
        self.dot(end.X - 6.5 * unit, end.Y - 6.5 * unit, 13.0 * unit, INK)?;
        let ring = D2D1_ELLIPSE {
            point: Vector2 {
                X: x + 44.0 * unit,
                Y: y + 44.0 * unit,
            },
            radiusX: 25.0 * unit,
            radiusY: 25.0 * unit,
        };
        self.context
            .DrawEllipse(&ring, &self.brush(INK)?, 12.0 * unit, None);
        self.dot(x + 35.5 * unit, y + 35.5 * unit, 17.0 * unit, 0x7c2d4f)
    }

    /// Draws one line of text vertically centred in a box, optionally aligned to its right edge.
    unsafe fn text(
        &self,
        style: Type,
        area: D2D_RECT_F,
        value: &str,
        rgb: u32,
        right: bool,
    ) -> Result<()> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from(style.family),
            self.fonts,
            DWRITE_FONT_WEIGHT(style.weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.size,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        if style.family == "Newsreader" {
            // A browser sets the optical size axis to the font size. DirectWrite keeps the default.
            let format: IDWriteTextFormat3 = format.cast()?;
            let axes = [DWRITE_FONT_AXIS_VALUE {
                axisTag: DWRITE_FONT_AXIS_TAG_OPTICAL_SIZE,
                value: style.size,
            }];
            format.SetFontAxisValues(&axes)?;
        }
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetTextAlignment(if right {
            DWRITE_TEXT_ALIGNMENT_TRAILING
        } else {
            DWRITE_TEXT_ALIGNMENT_LEADING
        })?;
        let text: Vec<u16> = value.encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(
            &text,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
        if style.tracking != 0.0 {
            let layout: IDWriteTextLayout1 = layout.cast()?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: text.len() as u32,
            };
            layout.SetCharacterSpacing(0.0, style.tracking * style.size, 0.0, range)?;
        }
        self.context
            .PushAxisAlignedClip(&area, D2D1_ANTIALIAS_MODE_ALIASED);
        self.context.DrawTextLayout(
            Vector2 {
                X: area.left,
                Y: area.top,
            },
            &layout,
            &self.brush(rgb)?,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
        self.context.PopAxisAlignedClip();
        Ok(())
    }

    unsafe fn paint_brush(&self, value: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
        self.context.CreateSolidColorBrush(&value, None)
    }

    unsafe fn fill_rounded(
        &self,
        area: D2D_RECT_F,
        radius: f32,
        value: D2D1_COLOR_F,
    ) -> Result<()> {
        let shape = D2D1_ROUNDED_RECT {
            rect: area,
            radiusX: radius,
            radiusY: radius,
        };
        self.context
            .FillRoundedRectangle(&shape, &self.paint_brush(value)?);
        Ok(())
    }

    /// A one-pixel ring drawn inside a rounded box, like CSS `inset 0 0 0 1px`.
    unsafe fn ring(&self, area: D2D_RECT_F, radius: f32, value: D2D1_COLOR_F) -> Result<()> {
        let shape = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: area.left + 0.5,
                top: area.top + 0.5,
                right: area.right - 0.5,
                bottom: area.bottom - 0.5,
            },
            radiusX: radius - 0.5,
            radiusY: radius - 0.5,
        };
        self.context
            .DrawRoundedRectangle(&shape, &self.paint_brush(value)?, 1.0, None);
        Ok(())
    }

    /// A Segoe Fluent icon centred in a box, from the system font collection.
    unsafe fn icon(&self, glyph: char, area: D2D_RECT_F, size: f32, rgb: u32) -> Result<()> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from(ICONS),
            None,
            DWRITE_FONT_WEIGHT(400),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        let text: Vec<u16> = glyph.to_string().encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(
            &text,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
        self.context.DrawTextLayout(
            Vector2 {
                X: area.left,
                Y: area.top,
            },
            &layout,
            &self.brush(rgb)?,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
        Ok(())
    }

    /// A site icon: a small rounded tile in the site colour with its letter.
    unsafe fn favicon(&self, x: f32, y: f32, size: f32, rgb: u32, letter: &str) -> Result<()> {
        self.fill_rounded(rect(x, y, size, size), 4.0, color(rgb))?;
        let label = Type {
            size: if size > 18.0 { 10.0 } else { FAVICON.size },
            ..FAVICON
        };
        self.text_centred(label, rect(x, y, size, size), letter, WHITE)
    }

    unsafe fn text_centred(
        &self,
        style: Type,
        area: D2D_RECT_F,
        value: &str,
        rgb: u32,
    ) -> Result<()> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from(style.family),
            self.fonts,
            DWRITE_FONT_WEIGHT(style.weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.size,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        let text: Vec<u16> = value.encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(
            &text,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
        self.context.DrawTextLayout(
            Vector2 {
                X: area.left,
                Y: area.top,
            },
            &layout,
            &self.brush(rgb)?,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
        Ok(())
    }

    /// Runs `body` with a control's press feedback applied: the control sinks to `press` of its
    /// size about its centre and darkens by up to 4%, like `button.pressed` in the shell study.
    /// `effect` holds the scale and brightness amounts, where 1 is fully pressed and a negative
    /// scale amount is the spring's overshoot.
    unsafe fn pressed(
        &self,
        area: D2D_RECT_F,
        radius: f32,
        press: f32,
        effect: Option<(f32, f32)>,
        body: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let Some((sink, dim)) = effect else {
            return body();
        };
        let mut original = Matrix3x2::default();
        self.context.GetTransform(&mut original);
        let factor = 1.0 - (1.0 - press) * sink;
        let center = Vector2 {
            X: (area.left + area.right) / 2.0,
            Y: (area.top + area.bottom) / 2.0,
        };
        self.context
            .SetTransform(&(Matrix3x2::scale_around(factor, factor, center) * original));
        let mut result = body();
        if result.is_ok() && dim > 0.001 {
            // Black over opaque pixels is exactly `filter: brightness(.96)`.
            result = self.fill_rounded(area, radius, rgba(0x000000, 0.04 * dim));
        }
        self.context.SetTransform(&original);
        result
    }

    /// A 28 DIP icon button: glass under the pointer, and the ink darkens with it.
    unsafe fn icon_button(
        &self,
        scene: &Scene,
        hit: crate::frame::AppbarHit,
        area: D2D_RECT_F,
        enabled: bool,
        body: impl FnOnce(bool) -> Result<()>,
    ) -> Result<()> {
        let hot = enabled && scene.hover == Some(hit);
        let effect = scene.effect(&[hit]).filter(|_| enabled);
        self.pressed(area, 8.0, 0.96, effect, || {
            if hot || effect.is_some() {
                self.fill_rounded(area, 8.0, rgba(WHITE, 0.5))?;
            }
            body(hot)
        })
    }

    /// Paints the Appbar in device-independent pixels, after the shell study.
    unsafe fn draw(
        &self,
        w: f32,
        h: f32,
        scene: &Scene,
        surface: Surface,
    ) -> Result<()> {
        use crate::frame::AppbarHit;
        let tint = scene.tint();
        // The docked appbar has no fill of its own: the window's ground lies under it. A floating
        // appbar is drawn over its own fill.
        let left = 10.0;
        let inner = w - 18.0;
        let hover = scene.hover;

        // The mark, which opens the start page, and the space menu.
        self.icon_button(
            scene,
            AppbarHit::Mark,
            rect(left + 1.0, 12.0, 28.0, 28.0),
            true,
            |_| self.mark(left + 4.0, 15.0, 22.0),
        )?;
        self.icon_button(
            scene,
            AppbarHit::WorkspaceMenu,
            rect(w - 36.0, 12.0, 28.0, 28.0),
            true,
            |hot| {
                self.icon(
                    '\u{E70D}',
                    rect(w - 36.0, 12.0, 28.0, 28.0),
                    16.0,
                    if hot { INK } else { SOFT_INK },
                )
            },
        )?;

        // Pinned tabs.
        let tile = (inner - 18.0) / 4.0;
        for (index, (rgb, letter)) in PINNED.iter().enumerate() {
            let area = rect(left + index as f32 * (tile + 6.0), 52.0, tile, 42.0);
            let hit = AppbarHit::Pinned(index);
            let hot = hover == Some(hit);
            let effect = scene.effect(&[hit]);
            self.pressed(area, 11.0, 0.96, effect, || {
                let strong = hot || effect.is_some();
                self.fill_rounded(area, 11.0, rgba(WHITE, if strong { 0.86 } else { 0.5 }))?;
                self.ring(area, 11.0, rgba(INK, 0.08))?;
                self.favicon(area.left + tile / 2.0 - 10.0, 63.0, 20.0, *rgb, letter)
            })?;
        }

        // The space, with the browser and profile it opens in.
        let space = scene.space();
        self.text(
            SPACE_NAME,
            rect(left + 8.0, 106.0, 120.0, 30.0),
            &space.name,
            INK,
            false,
        )?;
        let badge_width = 26.0 + 6.0 * space.profile.chars().count() as f32;
        let badge = rect(w - 8.0 - 8.0 - badge_width, 111.0, badge_width, 20.0);
        self.fill_rounded(badge, 6.0, rgba(WHITE, 0.5))?;
        self.dot(
            badge.left + 7.0,
            118.0,
            6.0,
            if scene.current == 0 { ACCENT } else { tint },
        )?;
        self.text(
            BADGE,
            rect(badge.left + 18.0, 111.0, badge_width - 22.0, 20.0),
            &space.profile,
            SOFT_INK,
            false,
        )?;

        let mut top = 138.0;
        for item in scene.tabs {
            use crate::tab_rows::Row as TabRow;
            if matches!(item, TabRow::NewTab) && top > 138.0 {
                self.fill(rect(left + 10.0, top + 8.0, inner - 20.0, 1.0), 0xd9d3cb)?;
                top += 17.0;
            }
            match item {
                TabRow::Group(group) => {
                    let hit = AppbarHit::Folder(group.id);
                    let area = rect(left, top, inner, 34.0);
                    let effect = scene.effect(&[hit]);
                    self.pressed(area, 9.0, 0.98, effect, || {
                        if hover == Some(hit) || effect.is_some() {
                            self.fill_rounded(area, 9.0, rgba(WHITE, 0.5))?;
                        }
                        let colours = [
                            0x9aa0a6, 0x8ab4f8, 0xf28b82, 0xfdd663, 0x81c995, 0xf6aea9, 0xc58af9,
                            0x78d9ec, 0xffb26b,
                        ];
                        let color = colours
                            .get(group.color as usize)
                            .copied()
                            .unwrap_or(colours[0]);
                        self.dot(left + 10.0, top + 13.0, 8.0, color)?;
                        self.text(
                            TAB,
                            rect(left + 34.0, top, inner - 62.0, 34.0),
                            if group.title.is_empty() {
                                "Untitled group"
                            } else {
                                &group.title
                            },
                            if hover == Some(hit) { INK } else { SOFT_INK },
                            false,
                        )?;
                        self.icon(
                            if group.collapsed {
                                '\u{E76C}'
                            } else {
                                '\u{E70D}'
                            },
                            rect(left + inner - 26.0, top + 4.0, 24.0, 24.0),
                            12.0,
                            MUTED,
                        )
                    })?;
                }
                TabRow::NewTab => {
                    let row = row(Lead::Glyph('\u{E710}'), "New tab", "Ctrl T");
                    self.tab_row(
                        left,
                        inner,
                        top,
                        &row,
                        hover == Some(AppbarHit::NewTab),
                        scene.effect(&[AppbarHit::NewTab]),
                    )?;
                }
                TabRow::Tab {
                    index,
                    title,
                    active,
                    child,
                } => {
                    let row = Row {
                        lead: Lead::Favicon(0x7c2d4f, "R"),
                        title: "",
                        meta: "",
                        child: *child,
                        active: *active,
                    };
                    self.tab_row_text(
                        left,
                        inner,
                        top,
                        &row,
                        title,
                        matches!(hover, Some(AppbarHit::Tab(i) | AppbarHit::CloseTab(i)) if i == *index),
                        scene.effect(&[AppbarHit::Tab(*index), AppbarHit::CloseTab(*index)]),
                        hover == Some(AppbarHit::CloseTab(*index)),
                    )?;
                }
            }
            top += 34.0;
        }

        // The foot: the settings, the spaces and a new space.
        let foot = h - 38.0;
        self.icon_button(
            scene,
            AppbarHit::Settings,
            rect(left, foot, 28.0, 28.0),
            true,
            |hot| {
                self.icon(
                    '\u{E713}',
                    rect(left, foot, 28.0, 28.0),
                    16.0,
                    if hot { INK } else { SOFT_INK },
                )
            },
        )?;
        let can_add = scene.spaces.len() < crate::spaces::MAX_SPACES;
        self.icon_button(
            scene,
            AppbarHit::NewSpace,
            rect(w - 36.0, foot, 28.0, 28.0),
            can_add,
            |hot| {
                self.icon(
                    '\u{E710}',
                    rect(w - 36.0, foot, 28.0, 28.0),
                    16.0,
                    if !can_add {
                        // A disabled icon is faded, like `.icon:disabled`.
                        0xb9b6bb
                    } else if hot {
                        INK
                    } else {
                        SOFT_INK
                    },
                )
            },
        )?;
        let dots = scene.spaces.len() as f32 * 32.0 - 4.0;
        let mut x = left + 28.0 + (inner - 56.0 - dots) / 2.0;
        for (index, info) in scene.spaces.iter().enumerate() {
            let active = index == scene.current;
            let hit = AppbarHit::Space(index);
            let hot = hover == Some(hit);
            let area = rect(x, foot, 28.0, 28.0);
            let effect = scene.effect(&[hit]);
            self.pressed(area, 9.0, 0.96, effect, || {
                if active || hot || effect.is_some() {
                    self.fill_rounded(area, 9.0, rgba(WHITE, if active { 0.86 } else { 0.5 }))?;
                    if active {
                        let outer = rect(x - 1.0, foot - 1.0, 30.0, 30.0);
                        self.ring(outer, 10.0, rgba(INK, 0.08))?;
                    }
                }
                let size = if active { 11.0 } else { 9.0 };
                let dot = rgba(info.tint, if active { 1.0 } else { 0.5 });
                let ellipse = D2D1_ELLIPSE {
                    point: Vector2 {
                        X: x + 14.0,
                        Y: foot + 14.0,
                    },
                    radiusX: size / 2.0,
                    radiusY: size / 2.0,
                };
                self.context.FillEllipse(&ellipse, &self.paint_brush(dot)?);
                Ok(())
            })?;
            x += 32.0;
        }
        if let Some(menu) = scene.menu {
            self.menu(scene, menu, left, inner, h)?;
        }
        // The grip of the resizer shows while the pointer is on the seam or drags it.
        if matches!(surface, Surface::Docked) && hover == Some(AppbarHit::Resizer) {
            self.fill_rounded(
                rect(w - 4.0, h / 2.0 - 18.0, 2.0, 36.0),
                1.0,
                rgba(INK, 0.18),
            )?;
        }
        Ok(())
    }

    /// The workspace menu under the space header, or the settings menu above the footer.
    unsafe fn menu(
        &self,
        scene: &Scene,
        menu: crate::frame::Menu,
        left: f32,
        inner: f32,
        h: f32,
    ) -> Result<()> {
        use crate::frame::{AppbarHit, Menu};
        let (_, top, _, height) =
            crate::frame::menu_area(menu, inner + 18.0, h, scene.spaces.len());
        let area = rect(left, top, inner, height);
        self.fill_rounded(area, 11.0, color(WHITE))?;
        self.ring(area, 11.0, rgba(INK, 0.12))?;
        match menu {
            Menu::Workspace => {
                for (index, info) in scene.spaces.iter().enumerate() {
                    let y = top + index as f32 * 48.0;
                    let hit = AppbarHit::MenuSpace(index);
                    let current = index == scene.current;
                    let item = rect(left + 4.0, y + 2.0, inner - 8.0, 44.0);
                    self.pressed(item, 8.0, 0.98, scene.effect(&[hit]), || {
                        if scene.hover == Some(hit) || current {
                            self.fill_rounded(
                                item,
                                8.0,
                                rgba(INK, if current { 0.06 } else { 0.04 }),
                            )?;
                        }
                        self.dot(left + 13.0, y + 13.0, 9.0, info.tint)?;
                        self.text(
                            ACTIVE,
                            rect(left + 30.0, y + 4.0, inner - 44.0, 19.0),
                            &info.name,
                            INK,
                            false,
                        )?;
                        self.text(
                            META,
                            rect(left + 30.0, y + 23.0, inner - 44.0, 17.0),
                            &info.profile,
                            MUTED,
                            false,
                        )
                    })?;
                }
            }
            Menu::Tab { index: tab, .. } => {
                for (row, title) in crate::frame::TAB_ITEMS.iter().enumerate() {
                    let y = top + 4.0 + row as f32 * 34.0;
                    let item = rect(left + 4.0, y, inner - 8.0, 34.0);
                    let hit = AppbarHit::SplitTab(tab);
                    self.pressed(item, 8.0, 0.98, scene.effect(&[hit]), || {
                        if scene.hover == Some(hit) {
                            self.fill_rounded(item, 8.0, rgba(INK, 0.04))?;
                        }
                        self.text(
                            TAB,
                            rect(left + 13.0, y, inner - 26.0, 34.0),
                            title,
                            SOFT_INK,
                            false,
                        )
                    })?;
                }
            }
            Menu::Settings => {
                for (index, (title, hit)) in crate::frame::SETTINGS_ITEMS.iter().enumerate() {
                    let y = top + 4.0 + index as f32 * 34.0;
                    let item = rect(left + 4.0, y, inner - 8.0, 34.0);
                    let effect = hit.and_then(|hit| scene.effect(&[hit]));
                    self.pressed(item, 8.0, 0.98, effect, || {
                        if hit.is_some() && scene.hover == *hit {
                            self.fill_rounded(item, 8.0, rgba(INK, 0.04))?;
                        }
                        self.text(
                            TAB,
                            rect(left + 13.0, y, inner - 26.0, 34.0),
                            title,
                            if hit.is_some() { SOFT_INK } else { MUTED },
                            false,
                        )
                    })?;
                }
            }
        }
        Ok(())
    }

    /// The floating appbar's shape: a soft shadow, the fill and a hairline ring, like the study's
    /// `0 0 0 1px rgba(0,0,0,.07), 0 24px 60px rgba(40,28,30,.28)`.
    /// The drop target of a dragged tab, as in the shell study: a dashed rounded outline, a wash
    /// of the space tint and a label in a white pill.
    unsafe fn drop_target(&self, w: f32, h: f32, label: &str, tint: u32) -> Result<()> {
        let area = rect(0.75, 0.75, w - 1.5, h - 1.5);
        self.fill_rounded(area, PANEL_RADIUS, rgba(WHITE, 0.35))?;
        self.fill_rounded(area, PANEL_RADIUS, rgba(tint, 0.14))?;
        let factory = self.context.GetFactory()?;
        let dashes = factory.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_FLAT,
                endCap: D2D1_CAP_STYLE_FLAT,
                dashCap: D2D1_CAP_STYLE_FLAT,
                lineJoin: D2D1_LINE_JOIN_MITER,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_DASH,
                dashOffset: 0.0,
            },
            None,
        )?;
        let outline = D2D1_ROUNDED_RECT { rect: area, radiusX: PANEL_RADIUS, radiusY: PANEL_RADIUS };
        self.context.DrawRoundedRectangle(
            &outline,
            &self.paint_brush(rgba(ACCENT, 0.7))?,
            1.5,
            &dashes,
        );
        let text = measure(self.dwrite, self.fonts, ACTIVE, label)?;
        let (pill_w, pill_h) = (text + 24.0, 30.0);
        let pill = rect((w - pill_w) / 2.0, (h - pill_h) / 2.0, pill_w, pill_h);
        self.fill_rounded(
            D2D_RECT_F { top: pill.top + 3.0, bottom: pill.bottom + 3.0, ..pill },
            9.0,
            rgba(0x28_1c_1e, 0.08),
        )?;
        self.fill_rounded(pill, 9.0, rgba(WHITE, 0.9))?;
        self.text_centred(ACTIVE, pill, label, ACCENT)
    }

    /// The top row of one page of a split, as the shell study's `.toolbar`: grip, back, forward,
    /// reload, the address, then the buttons that move the page to a Mini Window or end the split.
    /// The active page's row is the browser's, so only the grip and those buttons are drawn on it.
    unsafe fn pane_row(&self, row: &PaneRow, vw: f32, vh: f32, scale: f32) -> Result<()> {
        use crate::chrome_split::{row_buttons, RowButton};
        // The visual holds the row at (ox, oy), with the frame above it and the gap beside it.
        let (ox, oy) = (row.reach_left / scale, row.y / scale);
        let w = row.width / scale;
        let h = vh - oy;
        let clip_w = vw - ox;
        self.fill(rect(0.0, oy, ox, h), PAPER)?;
        self.fill(rect(vw - row.reach_right / scale, oy, row.reach_right / scale, h), PAPER)?;
        let buttons = row_buttons(w, row.first, row.last);
        if row.active {
            // The browser's top row is under the middle of the row. Its sides are bare.
            let popout = buttons.iter().find(|b| b.0 == RowButton::Popout).map_or(clip_w, |b| b.1 - 2.0);
            if row.first {
                self.fill(rect(ox, oy, crate::chrome_split::TOGGLE_ROOM, h), PAPER)?;
            }
            self.fill(rect(ox + popout, oy, vw - ox - popout, h), PAPER)?;
            self.fill(rect(ox + popout, oy + h - 1.0, vw - ox - popout, 1.0), HAIRLINE)?;
        } else {
            self.fill(rect(ox, oy, clip_w, h), PAPER)?;
            self.fill(rect(ox, oy + h - 1.0, clip_w, 1.0), HAIRLINE)?;
        }
        for (button, x, width) in buttons {
            let area = rect(ox + x, oy + (h - 28.0) / 2.0, width, 28.0);
            let own = matches!(button, RowButton::Grip | RowButton::Popout | RowButton::Unsplit);
            if row.active && !own {
                continue;
            }
            let hot = row.hover == Some(button);
            match button {
                RowButton::Address => {
                    if hot {
                        self.fill_rounded(area, 8.0, rgba(INK, 0.05))?;
                    }
                    let shown = if row.url.is_empty() {
                        row.title.clone()
                    } else {
                        format!("{} \u{b7} {}", row.title, row.url)
                    };
                    self.text(META_ROW, rect(area.left + 10.0, area.top, (width - 20.0).max(0.0), 28.0), &shown, MUTED, false)?;
                }
                _ => {
                    if hot {
                        self.fill_rounded(area, 8.0, rgba(INK, 0.06))?;
                    }
                    let (glyph, size, ink) = match button {
                        RowButton::Grip => ('\u{E7C2}', 13.0, SOFT_INK),
                        RowButton::Back => ('\u{E72B}', 16.0, SOFT_INK),
                        RowButton::Forward => ('\u{E72A}', 16.0, MUTED),
                        RowButton::Reload => ('\u{E72C}', 16.0, SOFT_INK),
                        RowButton::Popout => ('\u{EE47}', 13.0, SOFT_INK),
                        _ => ('\u{E711}', 12.0, SOFT_INK),
                    };
                    self.icon(glyph, area, size, ink)?;
                }
            }
        }
        Ok(())
    }

    /// The tab that follows the pointer while it is dragged.
    unsafe fn ghost(&self, width: f32, ghost: &GhostView) -> Result<()> {
        let body = rect(GHOST_PAD, GHOST_PAD, width, GHOST_HEIGHT);
        for layer in (1..=6).rev() {
            let grow = layer as f32 * 2.0;
            let reach = D2D_RECT_F {
                left: body.left - grow,
                top: body.top - grow + 8.0,
                right: body.right + grow,
                bottom: body.bottom + grow + 8.0,
            };
            self.fill_rounded(reach, 9.0 + grow, rgba(0x28_1c_1e, 0.03))?;
        }
        self.fill_rounded(body, 9.0, color(WHITE))?;
        self.ring(body, 9.0, rgba(0x000000, 0.06))?;
        self.favicon(body.left + 9.0, body.top + 7.0, 18.0, ghost.colour, &ghost.letter)?;
        let label = rect(body.left + 35.0, body.top, width - 47.0, GHOST_HEIGHT);
        self.text(ACTIVE, label, &ghost.title, INK, false)
    }

    unsafe fn floating(&self, area: D2D_RECT_F) -> Result<()> {
        const LAYERS: i32 = 10;
        for layer in (1..=LAYERS).rev() {
            let grow = (layer as f32 / LAYERS as f32) * (PEEK_PAD - 4.0);
            let fade = 1.0 - (layer - 1) as f32 / LAYERS as f32;
            let reach = D2D_RECT_F {
                left: area.left - grow,
                top: area.top - grow + 12.0,
                right: area.right + grow,
                bottom: area.bottom + grow + 12.0,
            };
            self.fill_rounded(reach, 14.0 + grow, rgba(0x28_1c_1e, 0.028 * fade * fade))?;
        }
        self.fill_rounded(area, 14.0, color(floating_fill()))?;
        self.ring(area, 14.0, rgba(0x000000, 0.07))
    }

    /// Experiment: the frame colour around a rounded page panel. A rounded stroke of twice the
    /// gap, centred one gap outside the panel, covers the gaps and leaves exactly the panel's
    /// rounded shape clear. It is clipped away from the appbar and the native caption buttons.
    unsafe fn draw_panel(&self, w: f32, h: f32, caption: f32) -> Result<()> {
        self.context.Clear(Some(&rgba(FRAME, 0.0)));
        let left = crate::APPBAR_WIDTH as f32;
        let panel = rect(left, 0.0, w - PANEL_GAP - left, h - PANEL_GAP);
        let outer = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: panel.left - PANEL_GAP,
                top: panel.top - PANEL_GAP,
                right: panel.right + PANEL_GAP,
                bottom: panel.bottom + PANEL_GAP,
            },
            radiusX: PANEL_RADIUS + PANEL_GAP,
            radiusY: PANEL_RADIUS + PANEL_GAP,
        };
        let ring = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: panel.left + 0.5,
                top: panel.top + 0.5,
                right: panel.right - 0.5,
                bottom: panel.bottom - 0.5,
            },
            radiusX: PANEL_RADIUS,
            radiusY: PANEL_RADIUS,
        };
        let frame = self.brush(FRAME)?;
        let hairline = self.paint_brush(rgba(INK, 0.08))?;
        // The top row runs to the window edge under the caption buttons, so the frame starts
        // below it on the right.
        let band = (crate::CAPTION_HEIGHT + crate::chrome_layout::PANEL_TOP) as f32;
        for clip in [
            rect(left, band, w - left, h - band),
            rect(left, 0.0, w - caption - left, band),
        ] {
            self.context
                .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
            self.context
                .DrawRoundedRectangle(&outer, &frame, PANEL_GAP * 2.0, None);
            self.context
                .DrawRoundedRectangle(&ring, &hairline, 1.0, None);
            self.context.PopAxisAlignedClip();
        }
        Ok(())
    }

    unsafe fn tab_row(
        &self,
        left: f32,
        inner: f32,
        top: f32,
        item: &Row,
        hovered: bool,
        effect: Option<(f32, f32)>,
    ) -> Result<()> {
        self.tab_row_text(left, inner, top, item, item.title, hovered, effect, false)
    }

    unsafe fn tab_row_text(
        &self,
        left: f32,
        inner: f32,
        top: f32,
        item: &Row,
        title: &str,
        hovered: bool,
        effect: Option<(f32, f32)>,
        close_hovered: bool,
    ) -> Result<()> {
        let area = rect(left, top, inner, 34.0);
        let pressed = effect.is_some();
        self.pressed(area, 9.0, 0.98, effect, || {
            if (hovered || pressed) && !item.active {
                self.fill_rounded(area, 9.0, rgba(WHITE, 0.5))?;
            }
            if item.active {
                // The selected tab is a white pill with a hairline ring and a soft shadow.
                self.fill_rounded(rect(left, top + 1.0, inner, 34.0), 9.0, rgba(INK, 0.06))?;
                self.fill_rounded(area, 9.0, color(WHITE))?;
                // The selected tab's ring sits outside its box, like a CSS box shadow with a 1px spread.
                let outer = rect(left - 1.0, top - 1.0, inner + 2.0, 36.0);
                self.ring(outer, 10.0, rgba(INK, 0.08))?;
            }
            let lead_x = left + if item.child { 34.0 } else { 8.0 };
            match item.lead {
                Lead::Glyph(glyph) => {
                    self.icon(glyph, rect(lead_x, top, 16.0, 34.0), 16.0, SOFT_INK)?
                }
                Lead::Favicon(rgb, letter) => {
                    self.favicon(lead_x, top + 9.0, 16.0, rgb, letter)?
                }
            }
            let label_x = lead_x + 26.0;
            let (style, ink) = if item.active {
                (ACTIVE, INK)
            } else if hovered || pressed {
                (TAB, INK)
            } else {
                (TAB, SOFT_INK)
            };
            self.text(
                style,
                rect(label_x, top, left + inner - label_x - 30.0, 34.0),
                title,
                ink,
                false,
            )?;
            if !item.meta.is_empty() {
                self.text(
                    META,
                    rect(left + inner - 70.0, top, 62.0, 34.0),
                    item.meta,
                    MUTED,
                    true,
                )?;
            }
            let closable = matches!(item.lead, Lead::Favicon(..));
            if closable && (item.active || hovered || pressed) {
                if close_hovered {
                    self.fill_rounded(
                        rect(left + inner - 26.0, top + 6.0, 22.0, 22.0),
                        6.0,
                        rgba(INK, 0.08),
                    )?;
                }
                self.icon(
                    '\u{E711}',
                    rect(left + inner - 26.0, top + 6.0, 22.0, 22.0),
                    10.0,
                    if close_hovered { INK } else { MUTED },
                )?;
            }
            Ok(())
        })
    }
}
