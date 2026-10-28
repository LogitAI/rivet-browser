//! Composes and paints the Rivet appbar as part of the browser window's own surface.
//!
//! Chromium's top-level window has no GDI redirection surface, so GDI drawing never reaches the
//! screen. DirectComposition can still attach a visual tree to the window from inside the browser
//! process. Windows composes that visual with the browser's content as one window, so animations,
//! snapping, rounded corners and shadows treat the appbar and the browser as a single surface.
//! Direct2D and DirectWrite paint the visual with the shell study's fonts, which are embedded in
//! the runtime and loaded into a private font collection.

use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2,
    D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT,
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
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_RANGE,
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
const SPACE_DOTS: [u32; 3] = [TINT, 0x5b8bab, 0xc0577f];

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
    visual: IDCompositionVisual2,
    painted: Option<Painted>,
    panel: IDCompositionVisual2,
    panel_painted: Option<Painted>,
    tabs: Vec<crate::tab_rows::Row>,
    tint: u32,
    hover: Option<crate::frame::AppbarHit>,
    pressed: Option<crate::frame::AppbarHit>,
    menu_open: bool,
    /// False while the appbar is hidden: the column then only carries the ground of the frame.
    docked: bool,
    /// Whether the resizer is being dragged, which keeps its grip showing.
    dragging: bool,
    toggle: IDCompositionVisual2,
    toggle_painted: Option<(i32, u32, bool, bool)>,
    peek: IDCompositionVisual2,
    peek_painted: Option<Painted>,
}

/// What a painted surface shows of the appbar.
#[derive(Clone, Copy)]
enum Surface {
    /// The whole appbar over the frame ground.
    Docked,
    /// The frame ground only, in the gap a hidden appbar leaves beside the page.
    Gap,
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
            visual,
            painted: None,
            panel,
            panel_painted: None,
            tabs: Vec::new(),
            tint: TINT,
            hover: None,
            pressed: None,
            menu_open: false,
            docked: true,
            dragging: false,
            toggle,
            toggle_painted: None,
            peek,
            peek_painted: None,
        })
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
        tint: u32,
        hover: Option<crate::frame::AppbarHit>,
        pressed: Option<crate::frame::AppbarHit>,
        menu_open: bool,
    ) {
        if self.menu_open != menu_open
            || self.tabs != tabs
            || self.tint != tint
            || self.hover != hover
            || self.pressed != pressed
        {
            self.menu_open = menu_open;
            self.tabs = tabs;
            self.tint = tint;
            self.hover = hover;
            self.pressed = pressed;
            self.painted = None;
            self.peek_painted = None;
            self.toggle_painted = None;
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
        let down = self.pressed == Some(crate::frame::AppbarHit::Toggle);
        if self.toggle_painted != Some((size, dpi, hot, down)) {
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
            let result = (|| {
                if hot || down {
                    canvas.fill_rounded(area, 8.0, rgba(INK, if down { 0.1 } else { 0.06 }))?;
                }
                canvas.icon('\u{E90C}', area, 18.0, SOFT_INK)
            })();
            surface.EndDraw()?;
            result?;
            self.toggle.SetContent(&surface)?;
            self.toggle_painted = Some((size, dpi, hot, down));
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
                let span = side as f32;
                canvas.draw(
                    side as f32,
                    height as f32,
                    span,
                    self.tint,
                    &self.tabs,
                    self.hover,
                    self.pressed,
                    self.menu_open,
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
        if !visible || width <= 0 || height <= 0 {
            self.visual.SetContent(None)?;
            self.painted = None;
            return self.device.Commit();
        }
        self.visual.SetOffsetX2(left as f32)?;
        self.visual.SetOffsetY2(top as f32)?;
        // The ground rises across the whole window, so its width is part of what was painted.
        let mut client = RECT::default();
        let _ = GetClientRect(self.window, &mut client);
        let span = client.right - client.left;
        let stale = self
            .painted
            .as_ref()
            .is_none_or(|p| (p.width, p.height, p.dpi, p.span) != (width, height, dpi, span));
        if stale {
            let surface = self.device.CreateSurface(
                width as u32,
                height as u32,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            self.paint(&surface, width, height, dpi, span)?;
            self.visual.SetContent(&surface)?;
            self.painted = Some(Painted {
                width,
                height,
                dpi,
                span,
            });
        }
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
        span: i32,
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
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let result = canvas.draw(
            width as f32 / scale,
            height as f32 / scale,
            span as f32 / scale,
            self.tint,
            &self.tabs,
            self.hover,
            self.pressed,
            self.menu_open,
            if self.docked {
                Surface::Docked
            } else {
                Surface::Gap
            },
        );
        surface.EndDraw()?;
        result
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

    /// The frame ground: the space colour rises from the lower left corner, as in the study.
    unsafe fn ground(&self, w: f32, h: f32, tint: u32) -> Result<()> {
        self.context.Clear(Some(&color(FRAME)));
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: rgba(tint, 0.34),
            },
            D2D1_GRADIENT_STOP {
                position: 0.7,
                color: rgba(tint, 0.0),
            },
        ];
        let target: &ID2D1RenderTarget = self.context;
        let collection =
            target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        let glow = self.context.CreateRadialGradientBrush(
            &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: Vector2 { X: 0.0, Y: h },
                gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                radiusX: w * 0.8,
                radiusY: h * 0.6,
            },
            None,
            &collection,
        )?;
        self.context.FillRectangle(&rect(0.0, 0.0, w, h), &glow);
        Ok(())
    }

    /// Paints the Appbar in device-independent pixels, after the shell study.
    unsafe fn draw(
        &self,
        w: f32,
        h: f32,
        span: f32,
        tint: u32,
        tabs: &[crate::tab_rows::Row],
        hover: Option<crate::frame::AppbarHit>,
        pressed: Option<crate::frame::AppbarHit>,
        menu_open: bool,
        surface: Surface,
    ) -> Result<()> {
        // A floating appbar is drawn over its own fill, and a hidden one leaves only the ground.
        match surface {
            Surface::Docked => self.ground(span, h, tint)?,
            Surface::Gap => return self.ground(span, h, tint),
            Surface::Floating => {}
        }
        let left = 10.0;
        let inner = w - 18.0;

        // The mark and the space menu.
        self.mark(left + 4.0, 15.0, 22.0)?;
        self.icon('\u{E70D}', rect(w - 36.0, 12.0, 28.0, 28.0), 16.0, SOFT_INK)?;

        // Pinned tabs.
        let tile = (inner - 18.0) / 4.0;
        for (index, (rgb, letter)) in PINNED.iter().enumerate() {
            let area = rect(left + index as f32 * (tile + 6.0), 52.0, tile, 42.0);
            let hot = hover == Some(crate::frame::AppbarHit::Pinned(index));
            let down = pressed == Some(crate::frame::AppbarHit::Pinned(index));
            let mut original = Matrix3x2::default();
            self.context.GetTransform(&mut original);
            if down {
                let center = Vector2 {
                    X: area.left + tile / 2.0,
                    Y: area.top + 21.0,
                };
                self.context
                    .SetTransform(&(Matrix3x2::scale_around(0.96, 0.96, center) * original));
            }
            self.fill_rounded(
                area,
                11.0,
                rgba(
                    WHITE,
                    if down {
                        0.72
                    } else if hot {
                        0.86
                    } else {
                        0.5
                    },
                ),
            )?;
            self.ring(area, 11.0, rgba(INK, 0.08))?;
            self.favicon(area.left + tile / 2.0 - 10.0, 63.0, 20.0, *rgb, letter)?;
            self.context.SetTransform(&original);
        }

        // The space, with the browser and profile it opens in.
        let space_index = SPACE_DOTS.iter().position(|rgb| *rgb == tint).unwrap_or(0);
        let names = ["Work", "Personal", "Research"];
        let profiles = ["Chrome · Work", "Chrome · Default", "Chrome · Default"];
        self.text(
            SPACE_NAME,
            rect(left + 8.0, 106.0, 120.0, 30.0),
            names[space_index],
            INK,
            false,
        )?;
        let badge_width = if space_index == 0 { 104.0 } else { 122.0 };
        let badge = rect(w - 8.0 - 8.0 - badge_width, 111.0, badge_width, 20.0);
        self.fill_rounded(badge, 6.0, rgba(WHITE, 0.5))?;
        self.dot(
            badge.left + 7.0,
            118.0,
            6.0,
            if space_index == 0 { ACCENT } else { tint },
        )?;
        self.text(
            BADGE,
            rect(badge.left + 18.0, 111.0, badge_width - 22.0, 20.0),
            profiles[space_index],
            SOFT_INK,
            false,
        )?;

        let mut top = 138.0;
        for item in tabs {
            use crate::frame::AppbarHit;
            use crate::tab_rows::Row as TabRow;
            if matches!(item, TabRow::NewTab) && top > 138.0 {
                self.fill(rect(left + 10.0, top + 8.0, inner - 20.0, 1.0), 0xd9d3cb)?;
                top += 17.0;
            }
            match item {
                TabRow::Group(group) => {
                    let hit = AppbarHit::Folder(group.id);
                    if hover == Some(hit) || pressed == Some(hit) {
                        self.fill_rounded(rect(left, top, inner, 34.0), 9.0, rgba(WHITE, 0.4))?;
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
                        SOFT_INK,
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
                    )?;
                }
                TabRow::NewTab => {
                    let row = row(Lead::Glyph('\u{E710}'), "New tab", "Ctrl T");
                    self.tab_row(
                        left,
                        inner,
                        top,
                        &row,
                        hover == Some(AppbarHit::NewTab),
                        pressed == Some(AppbarHit::NewTab),
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
                    self.tab_row_text(left,inner,top,&row,title,
                        matches!(hover,Some(AppbarHit::Tab(i)|AppbarHit::CloseTab(i)) if i==*index),
                        matches!(pressed,Some(AppbarHit::Tab(i)|AppbarHit::CloseTab(i)) if i==*index),
                        hover==Some(AppbarHit::CloseTab(*index)))?;
                }
            }
            top += 34.0;
        }

        // The foot: the library, the spaces and a new space.
        let foot = h - 38.0;
        self.icon('\u{E8F1}', rect(left, foot, 28.0, 28.0), 16.0, SOFT_INK)?;
        self.icon('\u{E710}', rect(w - 36.0, foot, 28.0, 28.0), 16.0, SOFT_INK)?;
        let dots = SPACE_DOTS.len() as f32 * 32.0 - 4.0;
        let mut x = left + 28.0 + (inner - 56.0 - dots) / 2.0;
        for (index, rgb) in SPACE_DOTS.iter().enumerate() {
            let active = tint == *rgb;
            let hot = hover == Some(crate::frame::AppbarHit::Space(index));
            let down = pressed == Some(crate::frame::AppbarHit::Space(index));
            let mut original = Matrix3x2::default();
            self.context.GetTransform(&mut original);
            if down {
                let center = Vector2 {
                    X: x + 14.0,
                    Y: foot + 14.0,
                };
                self.context
                    .SetTransform(&(Matrix3x2::scale_around(0.96, 0.96, center) * original));
            }
            if active || hot || down {
                let area = rect(x, foot, 28.0, 28.0);
                self.fill_rounded(area, 9.0, rgba(WHITE, if active { 0.86 } else { 0.5 }))?;
                if active {
                    let outer = rect(x - 1.0, foot - 1.0, 30.0, 30.0);
                    self.ring(outer, 10.0, rgba(INK, 0.08))?;
                }
            }
            let size = if active { 11.0 } else { 9.0 };
            let dot = rgba(*rgb, if active { 1.0 } else { 0.5 });
            let ellipse = D2D1_ELLIPSE {
                point: Vector2 {
                    X: x + 14.0,
                    Y: foot + 14.0,
                },
                radiusX: size / 2.0,
                radiusY: size / 2.0,
            };
            self.context.FillEllipse(&ellipse, &self.paint_brush(dot)?);
            self.context.SetTransform(&original);
            x += 32.0;
        }
        if menu_open {
            use crate::frame::AppbarHit;
            let area = rect(left, 138.0, inner, 246.0);
            self.fill_rounded(area, 11.0, color(WHITE))?;
            self.ring(area, 11.0, rgba(INK, 0.12))?;
            for index in 0..3 {
                let y = 138.0 + index as f32 * 48.0;
                let item = rect(left + 4.0, y + 2.0, inner - 8.0, 44.0);
                if hover == Some(AppbarHit::Space(index)) || index == space_index {
                    self.fill_rounded(
                        item,
                        8.0,
                        rgba(INK, if index == space_index { 0.06 } else { 0.04 }),
                    )?;
                }
                self.dot(left + 13.0, y + 13.0, 9.0, SPACE_DOTS[index])?;
                self.text(
                    ACTIVE,
                    rect(left + 30.0, y + 4.0, inner - 44.0, 19.0),
                    names[index],
                    INK,
                    false,
                )?;
                self.text(
                    META,
                    rect(left + 30.0, y + 23.0, inner - 44.0, 17.0),
                    profiles[index],
                    MUTED,
                    false,
                )?;
            }
            self.fill(rect(left + 10.0, 281.0, inner - 20.0, 1.0), 0xe5e1dc)?;
            for (index, (title, hit)) in [
                ("Profile settings", Some(AppbarHit::ProfileSettings)),
                ("Manage profiles", Some(AppbarHit::ManageProfiles)),
                ("Rivet settings", None),
            ]
            .iter()
            .enumerate()
            {
                let y = 282.0 + index as f32 * 34.0;
                if hit.is_some() && hover == *hit {
                    self.fill_rounded(
                        rect(left + 4.0, y, inner - 8.0, 34.0),
                        8.0,
                        rgba(INK, 0.04),
                    )?;
                }
                self.text(
                    TAB,
                    rect(left + 13.0, y, inner - 26.0, 34.0),
                    title,
                    if hit.is_some() { SOFT_INK } else { MUTED },
                    false,
                )?;
            }
        }
        // The grip of the resizer shows while the pointer is on the seam or drags it.
        if matches!(surface, Surface::Docked) && hover == Some(crate::frame::AppbarHit::Resizer) {
            self.fill_rounded(
                rect(w - 4.0, h / 2.0 - 18.0, 2.0, 36.0),
                1.0,
                rgba(INK, 0.18),
            )?;
        }
        Ok(())
    }

    /// The floating appbar's shape: a soft shadow, the fill and a hairline ring, like the study's
    /// `0 0 0 1px rgba(0,0,0,.07), 0 24px 60px rgba(40,28,30,.28)`.
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
        pressed: bool,
    ) -> Result<()> {
        self.tab_row_text(left, inner, top, item, item.title, hovered, pressed, false)
    }

    unsafe fn tab_row_text(
        &self,
        left: f32,
        inner: f32,
        top: f32,
        item: &Row,
        title: &str,
        hovered: bool,
        pressed: bool,
        close_hovered: bool,
    ) -> Result<()> {
        let area = rect(left, top, inner, 34.0);
        let mut original = Matrix3x2::default();
        self.context.GetTransform(&mut original);
        if pressed {
            let center = Vector2 {
                X: left + inner / 2.0,
                Y: top + 17.0,
            };
            self.context
                .SetTransform(&(Matrix3x2::scale_around(0.98, 0.98, center) * original));
        }
        if hovered || pressed {
            self.fill_rounded(area, 9.0, rgba(WHITE, if pressed { 0.65 } else { 0.4 }))?;
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
            Lead::Favicon(rgb, letter) => self.favicon(lead_x, top + 9.0, 16.0, rgb, letter)?,
        }
        let label_x = lead_x + 26.0;
        let (style, ink) = if item.active {
            (ACTIVE, INK)
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
        if item.active || hovered || pressed {
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
        self.context.SetTransform(&original);
        Ok(())
    }
}
