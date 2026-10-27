//! Composes and paints the Rivet appbar as part of the browser window's own surface.
//!
//! Chromium's top-level window has no GDI redirection surface, so GDI drawing never reaches the
//! screen. DirectComposition can still attach a visual tree to the window from inside the browser
//! process. Windows composes that visual with the browser's content as one window, so animations,
//! snapping, rounded corners and shadows treat the appbar and the browser as a single surface.
//! Direct2D and DirectWrite paint the visual with the shell study's fonts, which are embedded in
//! the runtime and loaded into a private font collection.

use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionSurface,
    IDCompositionTarget, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection1, IDWriteTextLayout1,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_RANGE,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
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

// The tab list shows the shell study's sample until the workspace model supplies real tabs.
const FOLDER: [Row; 3] = [
    row(Lead::Glyph('\u{E8B7}'), "Launch Atlas", "3"),
    Row {
        child: true,
        ..row(Lead::Favicon(0xc0577f, "L"), "Launch plan", "")
    },
    Row {
        child: true,
        ..row(Lead::Favicon(0x7a5bd1, "F"), "Design review", "")
    },
];
const TODAY: [Row; 6] = [
    row(Lead::Glyph('\u{E710}'), "New tab", "Ctrl T"),
    Row {
        active: true,
        ..row(Lead::Favicon(0x7c2d4f, "R"), "New tab", "")
    },
    row(Lead::Favicon(0xd07a3a, "E"), "Ember workspace", ""),
    row(Lead::Favicon(0x24292f, "G"), "Release notes", ""),
    row(Lead::Favicon(0x2f8f6b, "N"), "Launch notes", ""),
    row(Lead::Favicon(0x3a3d45, "</>"), "Local preview", ""),
];
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
}

/// The composition objects of one browser window's appbar.
pub struct Appbar {
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    fonts: IDWriteFontCollection1,
    // The target must outlive the visual tree it shows.
    _target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    painted: Option<Painted>,
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
        let visual = device.CreateVisual()?;
        target.SetRoot(&visual)?;
        let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&dwrite)?;
        Ok(Self {
            device,
            dwrite,
            fonts,
            _target: target,
            visual,
            painted: None,
        })
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
            self.painted = Some(Painted { width, height, dpi });
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
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let result = canvas.draw(width as f32 / scale, height as f32 / scale);
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

    /// The Rivet mark: a heavy ring whose lower right is cut by a rotated rivet tail.
    unsafe fn mark(&self, x: f32, y: f32, size: f32, stroke: f32, ground: u32) -> Result<()> {
        let radius = (size - stroke) / 2.0;
        let ring = D2D1_ELLIPSE {
            point: Vector2 {
                X: x + size / 2.0,
                Y: y + size / 2.0,
            },
            radiusX: radius,
            radiusY: radius,
        };
        self.context
            .DrawEllipse(&ring, &self.brush(INK)?, stroke, None);
        // The study places an 11px square 19.5px into the 24px mark and rotates it by -45
        // degrees. Its left edge is ink and the rest is the ground colour, which opens the ring.
        let unit = size / 24.0;
        let tail = 11.0 * unit;
        let centre = Vector2 {
            X: x + 19.5 * unit,
            Y: y + 19.5 * unit,
        };
        let mut base = Matrix3x2::default();
        self.context.GetTransform(&mut base);
        self.context
            .SetTransform(&(Matrix3x2::rotation_around(-45.0, centre) * base));
        let left = centre.X - tail / 2.0;
        let top = centre.Y - tail / 2.0;
        self.fill(rect(left, top, tail, tail), ground)?;
        self.fill(rect(left, top, stroke, tail), INK)?;
        self.context.SetTransform(&base);
        Ok(())
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
    unsafe fn ground(&self, w: f32, h: f32) -> Result<()> {
        self.context.Clear(Some(&color(FRAME)));
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: rgba(TINT, 0.34),
            },
            D2D1_GRADIENT_STOP {
                position: 0.7,
                color: rgba(TINT, 0.0),
            },
        ];
        let target: &ID2D1RenderTarget = self.context;
        let collection =
            target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        let glow = self.context.CreateRadialGradientBrush(
            &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: Vector2 { X: 0.0, Y: h },
                gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                radiusX: w * 1.6,
                radiusY: h * 0.6,
            },
            None,
            &collection,
        )?;
        self.context.FillRectangle(&rect(0.0, 0.0, w, h), &glow);
        Ok(())
    }

    /// Paints the Appbar in device-independent pixels, after the shell study.
    unsafe fn draw(&self, w: f32, h: f32) -> Result<()> {
        self.ground(w, h)?;
        let left = 10.0;
        let inner = w - 18.0;

        // The mark and the space menu.
        self.mark(left + 4.0, 15.0, 22.0, 4.6, FRAME)?;
        self.icon('\u{E70D}', rect(w - 36.0, 12.0, 28.0, 28.0), 16.0, SOFT_INK)?;

        // Pinned tabs.
        let tile = (inner - 18.0) / 4.0;
        for (index, (rgb, letter)) in PINNED.iter().enumerate() {
            let area = rect(left + index as f32 * (tile + 6.0), 52.0, tile, 42.0);
            self.fill_rounded(area, 11.0, rgba(WHITE, 0.5))?;
            self.ring(area, 11.0, rgba(INK, 0.08))?;
            self.favicon(area.left + tile / 2.0 - 10.0, 63.0, 20.0, *rgb, letter)?;
        }

        // The space, with the browser and profile it opens in.
        self.text(
            SPACE_NAME,
            rect(left + 8.0, 106.0, 120.0, 30.0),
            "Work",
            INK,
            false,
        )?;
        let badge_width = 104.0;
        let badge = rect(w - 8.0 - 8.0 - badge_width, 111.0, badge_width, 20.0);
        self.fill_rounded(badge, 6.0, rgba(WHITE, 0.5))?;
        self.dot(badge.left + 7.0, 118.0, 6.0, ACCENT)?;
        self.text(
            BADGE,
            rect(badge.left + 18.0, 111.0, badge_width - 22.0, 20.0),
            "Chrome · Work",
            SOFT_INK,
            false,
        )?;

        // Tabs: a folder, then today's tabs below New tab.
        let mut top = 138.0;
        for item in &FOLDER {
            self.tab_row(left, inner, top, item)?;
            top += 34.0;
        }
        self.fill(rect(left + 10.0, top + 8.0, inner - 20.0, 1.0), 0xd9d3cb)?;
        top += 17.0;
        for item in &TODAY {
            self.tab_row(left, inner, top, item)?;
            top += 34.0;
        }

        // The foot: the library, the spaces and a new space.
        let foot = h - 38.0;
        self.icon('\u{E8F1}', rect(left, foot, 28.0, 28.0), 16.0, SOFT_INK)?;
        self.icon('\u{E710}', rect(w - 36.0, foot, 28.0, 28.0), 16.0, SOFT_INK)?;
        let dots = SPACE_DOTS.len() as f32 * 32.0 - 4.0;
        let mut x = left + 28.0 + (inner - 56.0 - dots) / 2.0;
        for (index, rgb) in SPACE_DOTS.iter().enumerate() {
            let active = index == 0;
            if active {
                let area = rect(x, foot, 28.0, 28.0);
                self.fill_rounded(area, 9.0, rgba(WHITE, 0.86))?;
                self.ring(area, 9.0, rgba(INK, 0.08))?;
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
            x += 32.0;
        }
        Ok(())
    }

    unsafe fn tab_row(&self, left: f32, inner: f32, top: f32, item: &Row) -> Result<()> {
        let area = rect(left, top, inner, 34.0);
        if item.active {
            // The selected tab is a white pill with a hairline ring and a soft shadow.
            self.fill_rounded(rect(left, top + 1.0, inner, 34.0), 9.0, rgba(INK, 0.06))?;
            self.fill_rounded(area, 9.0, color(WHITE))?;
            self.ring(area, 9.0, rgba(INK, 0.08))?;
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
            item.title,
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
        if item.active {
            self.icon(
                '\u{E711}',
                rect(left + inner - 26.0, top + 6.0, 22.0, 22.0),
                10.0,
                MUTED,
            )?;
        }
        Ok(())
    }
}
