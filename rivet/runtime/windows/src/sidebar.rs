//! Composes and paints the Rivet sidebar as part of the browser window's own surface.
//!
//! Chromium's top-level window has no GDI redirection surface, so GDI drawing never reaches the
//! screen. DirectComposition can still attach a visual tree to the window from inside the browser
//! process. Windows composes that visual with the browser's content as one window, so animations,
//! snapping, rounded corners and shadows treat the sidebar and the browser as a single surface.
//! Direct2D and DirectWrite paint the visual with the shell study's fonts, which are embedded in
//! the runtime and loaded into a private font collection.

use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
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
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
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

/// Colours of the Rivet shell, with translucent layers already composed over `PAPER_DEEP`.
mod palette {
    pub const PAPER_DEEP: u32 = 0xe7e0d3;
    pub const LINE: u32 = 0xcdc7bc;
    pub const INK: u32 = 0x1f2024;
    pub const MUTED: u32 = 0x77746f;
    pub const BODY: u32 = 0x515056;
    pub const SPACE: u32 = 0x7c2d4f;
    pub const RESEARCH: u32 = 0x405f73;
    pub const PERSONAL: u32 = 0xbd7444;
    pub const SITE: u32 = 0xd6c9d0;
    pub const BUTTON: u32 = 0xf1ede5;
    pub const ACTIVE_TAB: u32 = 0xf8f6f2;
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

const BRAND: Type = style("Newsreader", 25.0, 400, -0.04);
const NOTE: Type = style("Newsreader", 16.0, 400, 0.0);
const CAPTION: Type = style("DM Mono", 9.0, 500, 0.08);
const LABEL: Type = style("DM Mono", 9.0, 500, 0.12);
const KEY: Type = style("DM Mono", 10.0, 400, 0.0);
const BUTTON_TEXT: Type = style("Manrope", 11.0, 600, 0.0);
const TAB: Type = style("Manrope", 11.0, 400, 0.0);
const ACTIVE: Type = style("Manrope", 11.0, 600, 0.0);
const SMALL: Type = style("Manrope", 10.0, 400, 0.0);

fn color(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

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

/// The composition objects of one browser window's sidebar.
pub struct Sidebar {
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    fonts: IDWriteFontCollection1,
    // The target must outlive the visual tree it shows.
    _target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    painted: Option<Painted>,
}

impl Sidebar {
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

    unsafe fn rounded(
        &self,
        area: D2D_RECT_F,
        radius: f32,
        fill: u32,
        border: Option<u32>,
    ) -> Result<()> {
        let shape = D2D1_ROUNDED_RECT {
            rect: area,
            radiusX: radius,
            radiusY: radius,
        };
        self.context
            .FillRoundedRectangle(&shape, &self.brush(fill)?);
        if let Some(rgb) = border {
            // Keep the one-pixel border inside the box, as CSS border-box sizing does.
            let inner = D2D1_ROUNDED_RECT {
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
                .DrawRoundedRectangle(&inner, &self.brush(rgb)?, 1.0, None);
        }
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

    /// Paints the sidebar in device-independent pixels.
    unsafe fn draw(&self, w: f32, h: f32) -> Result<()> {
        self.context.Clear(Some(&color(PAPER_DEEP)));
        self.fill(rect(w - 1.0, 0.0, 1.0, h), LINE)?;

        // Brand row.
        self.mark(22.0, 24.0, 24.0, 5.0, PAPER_DEEP)?;
        self.text(BRAND, rect(56.0, 24.0, 150.0, 25.0), "Rivet", INK, false)?;
        self.text(CAPTION, rect(w - 80.0, 24.0, 58.0, 25.0), "01", MUTED, true)?;

        // Space switcher. The active space is the long bar.
        self.fill(rect(18.0, 74.0, 76.0, 7.0), SPACE)?;
        self.fill(rect(98.0, 74.0, 36.0, 7.0), RESEARCH)?;
        self.fill(rect(138.0, 74.0, 36.0, 7.0), PERSONAL)?;

        // New tab.
        self.rounded(rect(15.0, 104.0, w - 30.0, 33.0), 7.0, BUTTON, Some(LINE))?;
        self.text(
            BUTTON_TEXT,
            rect(27.0, 104.0, 120.0, 33.0),
            "New tab",
            INK,
            false,
        )?;
        self.text(
            KEY,
            rect(w - 120.0, 104.0, 94.0, 33.0),
            "Ctrl T",
            MUTED,
            true,
        )?;

        let today = [
            ("Agent brief", true),
            ("Ember workspace", false),
            ("Release notes", false),
        ];
        self.section(w, 161.0, "TODAY", &today)?;
        let saved = [("Research queue", false), ("Design references", false)];
        self.section(w, 305.0, "SAVED", &saved)?;

        // Agent note, pinned to the bottom.
        let note = h - 100.0;
        let text = w - 56.0;
        self.fill(rect(15.0, note, w - 30.0, 2.0), INK)?;
        self.text(
            NOTE,
            rect(28.0, note + 15.0, text, 17.0),
            "One browser,",
            INK,
            false,
        )?;
        self.text(
            NOTE,
            rect(28.0, note + 32.0, text, 17.0),
            "wherever you work.",
            INK,
            false,
        )?;
        self.text(
            SMALL,
            rect(28.0, note + 55.0, text, 14.0),
            "RIVET / EMBER LINK",
            MUTED,
            false,
        )
    }

    unsafe fn section(&self, w: f32, top: f32, label: &str, tabs: &[(&str, bool)]) -> Result<()> {
        self.text(
            LABEL,
            rect(23.0, top - 4.0, 120.0, 17.0),
            label,
            MUTED,
            false,
        )?;
        for (index, (title, active)) in tabs.iter().enumerate() {
            let row = top + 17.0 + 34.0 * index as f32;
            if *active {
                self.rounded(rect(15.0, row, w - 30.0, 34.0), 5.0, ACTIVE_TAB, None)?;
                self.text(
                    KEY,
                    rect(w - 60.0, row, 37.0, 34.0),
                    "\u{2197}",
                    MUTED,
                    true,
                )?;
            }
            self.dot(23.0, row + 13.0, 8.0, if *active { SPACE } else { SITE })?;
            let (style, ink) = if *active { (ACTIVE, INK) } else { (TAB, BODY) };
            self.text(style, rect(40.0, row, w - 90.0, 34.0), title, ink, false)?;
        }
        Ok(())
    }
}
