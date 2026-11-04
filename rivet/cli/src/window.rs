//! The first setup window.
//!
//! It is the one Rivet window that exists before a browser window does, so it is a window of its
//! own: borderless, drawn with Direct2D and DirectWrite in the shell study's fonts and colours.
//! The profile name is typed into the Windows text engine (see `textservices`), which does the
//! editing, the clipboard and the Korean input method.

use crate::config::{self, Saved, Start};
use crate::detect::{Browser, Disk, Status};
use crate::model::{self, Hit, Layout, Model, Rect};
use crate::textservices::{Field, TIMER_BASE, TIMER_LAST};
use crate::{launch, system};
use std::collections::HashMap;
use std::path::PathBuf;
use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND as ComWindow;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection, IDWriteFontCollection1, IDWriteTextFormat3, IDWriteTextLayout1,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_TAG_OPTICAL_SIZE, DWRITE_FONT_AXIS_VALUE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::WindowsAndMessaging::HICON;
use windows_numerics::Matrix3x2;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, BeginPaint, EndPaint, InvalidateRect, ScreenToClient, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::MARGINS;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_RETURN,
    VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Fonts of the shell study, licensed under the SIL Open Font License 1.1.
const FONTS: [&[u8]; 4] = [
    include_bytes!("../../runtime/windows/fonts/Newsreader[opsz,wght].ttf"),
    include_bytes!("../../runtime/windows/fonts/Manrope[wght].ttf"),
    include_bytes!("../../runtime/windows/fonts/DMMono-Regular.ttf"),
    include_bytes!("../../runtime/windows/fonts/DMMono-Medium.ttf"),
];

mod palette {
    pub const FRAME: u32 = 0xece8e2;
    pub const PAPER: u32 = 0xfbfaf7;
    pub const INK: u32 = 0x1f2024;
    pub const SOFT_INK: u32 = 0x4a4850;
    pub const MUTED: u32 = 0x8a8690;
    pub const TINT: u32 = 0xd08a52;
    pub const ACCENT: u32 = 0x9a5424;
    pub const ACCENT_PRESSED: u32 = 0x7f4419;
    pub const WHITE: u32 = 0xffffff;
    pub const BERRY: u32 = 0x7c2d4f;
    pub const GREY_DOT: u32 = 0xb5b1ab;
}
use palette::*;

#[derive(Clone, Copy)]
struct Type {
    family: &'static str,
    size: f32,
    weight: i32,
    tracking: f32,
}

const fn style(family: &'static str, size: f32, weight: i32, tracking: f32) -> Type {
    Type { family, size, weight, tracking }
}

const TITLE: Type = style("Newsreader", 24.0, 600, -0.01);
const SUBTITLE: Type = style("Manrope", 13.0, 400, 0.0);
const LABEL: Type = style("DM Mono", 10.0, 500, 0.08);
const ROW_NAME: Type = style("Manrope", 13.0, 600, 0.0);
const ROW_META: Type = style("DM Mono", 10.0, 400, 0.0);
const BADGE: Type = style("DM Mono", 10.0, 500, 0.0);
const NOTE: Type = style("Manrope", 12.0, 400, 0.0);
const CHIP: Type = style("Manrope", 12.0, 500, 0.0);
const BUTTON: Type = style("Manrope", 13.0, 600, 0.0);
const PREVIEW: Type = style("DM Mono", 10.0, 400, 0.0);
const MESSAGE: Type = style("Manrope", 12.0, 500, 0.0);
const NOTICE: Type = style("Manrope", 12.0, 500, 0.0);
/// The size of the profile name the text engine draws, in DIPs.
const FIELD_TEXT: f32 = 14.0;
const ICONS: &str = "Segoe Fluent Icons";

const CLASS: *const u16 = windows_sys::w!("RivetSetup");
const CARET_TIMER: usize = 1;
const D2DERR_RECREATE_TARGET: i32 = 0x8899_000C_u32 as i32;
const WM_MOUSELEAVE: u32 = 0x02A3;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn color(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

fn mix(base: u32, over: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((base >> shift) & 0xff) as f32;
        let b = ((over >> shift) & 0xff) as f32;
        ((a * (1.0 - amount) + b * amount).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

fn d2d(rect: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: rect.x, top: rect.y, right: rect.right(), bottom: rect.bottom() }
}

/// The graphics objects of the window. The render target is rebuilt when Windows takes it away.
struct Gfx {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory5,
    fonts: IDWriteFontCollection1,
    wic: Option<IWICImagingFactory>,
    target: Option<ID2D1HwndRenderTarget>,
    icons: HashMap<String, Option<ID2D1Bitmap>>,
}

unsafe fn load_fonts(dwrite: &IDWriteFactory5) -> windows::core::Result<IDWriteFontCollection1> {
    let loader = dwrite.CreateInMemoryFontFileLoader()?;
    dwrite.RegisterFontFileLoader(&loader)?;
    let builder = dwrite.CreateFontSetBuilder()?;
    for font in FONTS {
        // The bytes are static, so DirectWrite can read them in place without an owner object.
        let file = loader.CreateInMemoryFontFileReference(dwrite, font.as_ptr().cast(), font.len() as u32, None)?;
        builder.AddFontFile(&file)?;
    }
    let set = builder.CreateFontSet()?;
    dwrite.CreateFontCollectionFromFontSet(&set)
}

impl Gfx {
    unsafe fn new() -> windows::core::Result<Self> {
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&dwrite)?;
        let wic = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok();
        Ok(Self { factory, dwrite, fonts, wic, target: None, icons: HashMap::new() })
    }

    unsafe fn target(&mut self, window: HWND, width: u32, height: u32, dpi: u32) -> windows::core::Result<ID2D1HwndRenderTarget> {
        if let Some(target) = &self.target {
            return Ok(target.clone());
        }
        let properties = D2D1_RENDER_TARGET_PROPERTIES::default();
        let hwnd = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: ComWindow(window),
            pixelSize: D2D_SIZE_U { width, height },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let target = self.factory.CreateHwndRenderTarget(&properties, &hwnd)?;
        target.SetDpi(dpi as f32, dpi as f32);
        self.icons.clear();
        self.target = Some(target.clone());
        Ok(target)
    }

    fn lose_target(&mut self) {
        self.target = None;
        self.icons.clear();
    }

    /// The icon of a program as a bitmap of the target, at the size that is asked for in pixels.
    unsafe fn icon(&mut self, target: &ID2D1RenderTarget, program: &std::path::Path, pixels: i32) -> Option<ID2D1Bitmap> {
        let key = format!("{}@{pixels}", program.display());
        if let Some(found) = self.icons.get(&key) {
            return found.clone();
        }
        let made = (|| {
            let wic = self.wic.as_ref()?;
            let file = wide(&program.to_string_lossy());
            let mut icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON = std::ptr::null_mut();
            let mut id = 0u32;
            let count = PrivateExtractIconsW(file.as_ptr(), 0, pixels, pixels, &mut icon, &mut id, 1, 0);
            if count == 0 || count == u32::MAX || icon.is_null() {
                return None;
            }
            let bitmap = wic.CreateBitmapFromHICON(HICON(icon.cast()));
            DestroyIcon(icon);
            // Direct2D draws premultiplied pixels, and an icon is not premultiplied.
            let converter = wic.CreateFormatConverter().ok()?;
            converter
                .Initialize(&bitmap.ok()?, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
                .ok()?;
            target.CreateBitmapFromWicBitmap(&converter, None).ok()
        })();
        self.icons.insert(key, made.clone());
        made
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
}

/// Drawing helpers over one render target, with the coordinates in DIPs.
struct Canvas<'a> {
    target: &'a ID2D1RenderTarget,
    dwrite: &'a IDWriteFactory5,
    fonts: &'a IDWriteFontCollection1,
}

impl Canvas<'_> {
    unsafe fn brush(&self, rgb: u32, alpha: f32) -> windows::core::Result<ID2D1SolidColorBrush> {
        self.target.CreateSolidColorBrush(&color(rgb, alpha), None)
    }

    unsafe fn fill(&self, area: Rect, radius: f32, rgb: u32, alpha: f32) {
        if let Ok(brush) = self.brush(rgb, alpha) {
            let shape = D2D1_ROUNDED_RECT { rect: d2d(area), radiusX: radius, radiusY: radius };
            self.target.FillRoundedRectangle(&shape, &brush);
        }
    }

    /// A one-pixel ring drawn inside a rounded box, like CSS `inset 0 0 0 1px`.
    unsafe fn ring(&self, area: Rect, radius: f32, rgb: u32, alpha: f32, width: f32) {
        if let Ok(brush) = self.brush(rgb, alpha) {
            let inset = width / 2.0;
            let shape = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: area.x + inset,
                    top: area.y + inset,
                    right: area.right() - inset,
                    bottom: area.bottom() - inset,
                },
                radiusX: radius - inset,
                radiusY: radius - inset,
            };
            self.target.DrawRoundedRectangle(&shape, &brush, width, None);
        }
    }

    unsafe fn dot(&self, x: f32, y: f32, size: f32, rgb: u32) {
        if let Ok(brush) = self.brush(rgb, 1.0) {
            let radius = size / 2.0;
            let ellipse = D2D1_ELLIPSE { point: windows_numerics::Vector2 { X: x + radius, Y: y + radius }, radiusX: radius, radiusY: radius };
            self.target.FillEllipse(&ellipse, &brush);
        }
    }

    /// The Rivet mark of the appbar: the tile, the ring and the pin.
    unsafe fn mark(&self, x: f32, y: f32, size: f32) {
        let unit = size / 96.0;
        self.fill(Rect { x, y, w: size, h: size }, 24.0 * unit, 0xf2eee5, 1.0);
        if let Ok(brush) = self.brush(INK, 1.0) {
            let start = windows_numerics::Vector2 { X: x + 58.0 * unit, Y: y + 58.0 * unit };
            let end = windows_numerics::Vector2 { X: x + 74.0 * unit, Y: y + 74.0 * unit };
            self.target.DrawLine(start, end, &brush, 13.0 * unit, None);
            self.dot(end.X - 6.5 * unit, end.Y - 6.5 * unit, 13.0 * unit, INK);
            let ring = D2D1_ELLIPSE {
                point: windows_numerics::Vector2 { X: x + 44.0 * unit, Y: y + 44.0 * unit },
                radiusX: 25.0 * unit,
                radiusY: 25.0 * unit,
            };
            self.target.DrawEllipse(&ring, &brush, 12.0 * unit, None);
        }
        self.dot(x + 35.5 * unit, y + 35.5 * unit, 17.0 * unit, BERRY);
    }

    unsafe fn format(&self, style: Type, family: Option<&str>) -> windows::core::Result<IDWriteTextFormat3> {
        let locale = HSTRING::from("en-us");
        let name = HSTRING::from(family.unwrap_or(style.family));
        let format = if family.is_some() {
            self.dwrite.CreateTextFormat(
                &name,
                None::<&IDWriteFontCollection>,
                DWRITE_FONT_WEIGHT(style.weight),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                PCWSTR(locale.as_ptr()),
            )?
        } else {
            self.dwrite.CreateTextFormat(
                &name,
                self.fonts,
                DWRITE_FONT_WEIGHT(style.weight),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                PCWSTR(locale.as_ptr()),
            )?
        };
        let format: IDWriteTextFormat3 = format.cast()?;
        if style.family == "Newsreader" {
            // A browser sets the optical size axis to the font size. DirectWrite keeps the default.
            format.SetFontAxisValues(&[DWRITE_FONT_AXIS_VALUE { axisTag: DWRITE_FONT_AXIS_TAG_OPTICAL_SIZE, value: style.size }])?;
        }
        Ok(format)
    }

    /// Draws text in a box: one line centred vertically and trimmed with an ellipsis, or wrapped
    /// from the top when `wrap` is set.
    unsafe fn text(&self, style: Type, area: Rect, value: &str, rgb: u32, align: Align, wrap: bool) {
        let _ = (|| -> windows::core::Result<()> {
            let format = self.format(style, None)?;
            format.SetWordWrapping(if wrap { DWRITE_WORD_WRAPPING_WRAP } else { DWRITE_WORD_WRAPPING_NO_WRAP })?;
            format.SetParagraphAlignment(if wrap { DWRITE_PARAGRAPH_ALIGNMENT_NEAR } else { DWRITE_PARAGRAPH_ALIGNMENT_CENTER })?;
            format.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            })?;
            if !wrap {
                let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
                let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                format.SetTrimming(&trimming, &sign)?;
            }
            let text: Vec<u16> = value.encode_utf16().collect();
            let layout = self.dwrite.CreateTextLayout(&text, &format, area.w, area.h)?;
            if style.tracking != 0.0 {
                let layout: IDWriteTextLayout1 = layout.cast()?;
                let range = DWRITE_TEXT_RANGE { startPosition: 0, length: text.len() as u32 };
                layout.SetCharacterSpacing(0.0, style.tracking * style.size, 0.0, range)?;
            }
            let brush = self.brush(rgb, 1.0)?;
            self.target.DrawTextLayout(
                windows_numerics::Vector2 { X: area.x, Y: area.y },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            Ok(())
        })();
    }

    /// The width a line of text takes.
    unsafe fn measure(&self, style: Type, value: &str) -> f32 {
        (|| -> windows::core::Result<f32> {
            let format = self.format(style, None)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let text: Vec<u16> = value.encode_utf16().collect();
            let layout = self.dwrite.CreateTextLayout(&text, &format, 4000.0, 100.0)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(metrics.widthIncludingTrailingWhitespace)
        })()
        .unwrap_or(60.0)
    }

    /// A Segoe Fluent icon glyph centred in a box.
    unsafe fn glyph(&self, glyph: char, area: Rect, size: f32, rgb: u32) {
        let _ = (|| -> windows::core::Result<()> {
            let format = self.format(style(ICONS, size, 400, 0.0), Some(ICONS))?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            let text: Vec<u16> = glyph.to_string().encode_utf16().collect();
            let layout = self.dwrite.CreateTextLayout(&text, &format, area.w, area.h)?;
            let brush = self.brush(rgb, 1.0)?;
            self.target.DrawTextLayout(windows_numerics::Vector2 { X: area.x, Y: area.y }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            Ok(())
        })();
    }
}

/// How a pressed control sinks, as in the shell study: it shrinks a little about its centre.
unsafe fn press(target: &ID2D1RenderTarget, area: Rect, pressed: bool) {
    let scale = if pressed { 0.98 } else { 1.0 };
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    target.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: cx - cx * scale, M32: cy - cy * scale });
}

unsafe fn unpress(target: &ID2D1RenderTarget) {
    target.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 });
}

fn status_badge(status: &Status) -> (&'static str, u32) {
    match status {
        Status::Ready => ("Ready", ACCENT),
        Status::Kept { .. } => ("Kept build", TINT),
        Status::NotYet(_) => ("Not yet", GREY_DOT),
    }
}

/// Everything one open setup window holds.
struct App {
    window: HWND,
    dpi: u32,
    model: Model,
    layout: Layout,
    gfx: Gfx,
    field: Field,
    hover: Hit,
    pressed: Hit,
    caret_on: bool,
    tracking: bool,
    visible: usize,
}

impl App {
    fn to_dip(&self, pixels: i32) -> f32 {
        pixels as f32 * 96.0 / self.dpi as f32
    }

    fn to_px(&self, dip: f32) -> i32 {
        (dip * self.dpi as f32 / 96.0).round() as i32
    }

    fn repaint(&self) {
        unsafe { InvalidateRect(self.window, std::ptr::null(), 0) };
    }

    unsafe fn canvas_measure(&mut self) -> Vec<f32> {
        let Ok(target) = self.gfx.target(self.window, 1, 1, self.dpi) else { return Vec::new() };
        let canvas = Canvas { target: &target.cast::<ID2D1RenderTarget>().unwrap(), dwrite: &self.gfx.dwrite, fonts: &self.gfx.fonts };
        let widths: Vec<f32> = self.model.profiles.iter().map(|name| canvas.measure(CHIP, name) + 28.0).collect();
        let fit = model::chips_that_fit(&widths);
        widths[..fit].to_vec()
    }

    /// Lays everything out again for the current model, and puts the text field in its place.
    unsafe fn relayout(&mut self) {
        let widths = self.canvas_measure();
        self.layout = model::layout(self.visible, self.model.reason.is_some(), &widths);
        self.model.reveal(self.visible);
        let text = self.layout.text;
        let client = RECT {
            left: self.to_px(text.x),
            top: self.to_px(text.y),
            right: self.to_px(text.x) + self.to_px(text.w),
            bottom: self.to_px(text.y) + self.to_px(text.h),
        };
        self.field.place(client, FIELD_TEXT, self.dpi);
    }

    /// Takes in what the text engine now holds as the profile name.
    unsafe fn sync_text(&mut self) {
        let text = self.field.text();
        if text != self.model.profile {
            self.model.profile = text;
            self.model.error = None;
        }
        self.caret_on = true;
        self.repaint();
    }

    unsafe fn paint(&mut self) {
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let (width, height) = ((client.right - client.left).max(1) as u32, (client.bottom - client.top).max(1) as u32);
        let Ok(target) = self.gfx.target(self.window, width, height, self.dpi) else { return };
        let render: ID2D1RenderTarget = match target.cast() {
            Ok(render) => render,
            Err(_) => return,
        };
        render.BeginDraw();
        render.Clear(Some(&color(FRAME, 1.0)));
        self.draw(&render);
        if let Err(error) = render.EndDraw(None, None) {
            if error.code().0 == D2DERR_RECREATE_TARGET {
                self.gfx.lose_target();
            }
        }
    }

    unsafe fn draw(&mut self, render: &ID2D1RenderTarget) {
        let layout = self.layout.clone();
        let canvas = Canvas { target: render, dwrite: &self.gfx.dwrite.clone(), fonts: &self.gfx.fonts.clone() };
        let size = Rect { x: 0.0, y: 0.0, w: model::WIDTH, h: layout.height };
        // The window is the frame colour with one paper panel inside it, as in the shell study.
        let panel = Rect { x: model::FRAME, y: model::FRAME, w: size.w - 2.0 * model::FRAME, h: size.h - 2.0 * model::FRAME };
        canvas.fill(panel, 12.0, PAPER, 1.0);
        canvas.ring(panel, 12.0, 0x000000, 0.06, 1.0);

        canvas.mark(model::PAD, 30.0, 44.0);
        canvas.text(TITLE, Rect { x: model::PAD + 58.0, y: 28.0, w: 360.0, h: 30.0 }, "Set up Rivet", INK, Align::Left, false);
        canvas.text(
            SUBTITLE,
            Rect { x: model::PAD + 58.0, y: 58.0, w: 400.0, h: 18.0 },
            "Choose the browser Rivet works in, and a profile for it.",
            MUTED,
            Align::Left,
            false,
        );
        let close = layout.close;
        if self.hover == Hit::Close {
            canvas.fill(close, 8.0, INK, 0.06);
        }
        canvas.glyph('\u{E711}', close, 11.0, SOFT_INK);

        if let (Some(notice), Some(reason)) = (layout.notice, self.model.reason.clone()) {
            canvas.fill(notice, 10.0, mix(PAPER, TINT, 0.16), 1.0);
            canvas.ring(notice, 10.0, TINT, 0.5, 1.0);
            canvas.text(NOTICE, Rect { x: notice.x + 14.0, y: notice.y + 6.0, w: notice.w - 28.0, h: notice.h - 12.0 }, &reason, ACCENT, Align::Left, true);
        }

        canvas.text(LABEL, layout.browser_label, "DETECTED BROWSERS", MUTED, Align::Left, false);
        canvas.fill(layout.group, 12.0, WHITE, 0.5);
        canvas.ring(layout.group, 12.0, INK, 0.08, 1.0);
        for (position, row) in layout.rows.iter().enumerate() {
            let index = self.model.scroll + position;
            let Some(browser) = self.model.browsers.get(index).cloned() else { break };
            self.draw_row(&canvas, render, *row, &browser, index, position);
        }
        let manual = layout.manual;
        let hot = self.hover == Hit::Manual;
        press(render, manual, self.pressed == Hit::Manual && hot);
        if hot {
            canvas.fill(manual, 10.0, WHITE, 0.7);
        }
        canvas.glyph('\u{E8E5}', Rect { x: manual.x + 12.0, y: manual.y + 8.0, w: 28.0, h: 28.0 }, 15.0, ACCENT);
        canvas.text(ROW_NAME, Rect { x: manual.x + 52.0, y: manual.y + 5.0, w: manual.w - 64.0, h: 18.0 }, "Choose a browser file", INK, Align::Left, false);
        canvas.text(
            ROW_META,
            Rect { x: manual.x + 52.0, y: manual.y + 23.0, w: manual.w - 64.0, h: 14.0 },
            "Any Chromium browser program, picked by hand",
            MUTED,
            Align::Left,
            false,
        );
        unpress(render);

        let note_color = if self.model.can_finish() { MUTED } else { BERRY };
        canvas.text(NOTE, layout.note, &self.model.note(), note_color, Align::Left, true);

        canvas.text(LABEL, layout.profile_label, "RIVET PROFILE", MUTED, Align::Left, false);
        let field = layout.field;
        canvas.fill(field, 12.0, WHITE, 1.0);
        canvas.ring(field, 12.0, ACCENT, 0.55, 1.5);
        self.draw_field(render, &canvas);

        if layout.chips.is_empty() {
            let hint = if self.model.profiles.is_empty() { "No Rivet profile for this browser yet. It is made when you finish." } else { "" };
            canvas.text(NOTE, layout.chip_hint, hint, MUTED, Align::Left, false);
        } else {
            let typed = config::clean_profile(&self.model.profile);
            for (index, chip) in layout.chips.iter().enumerate() {
                let name = &self.model.profiles[index];
                let current = name.eq_ignore_ascii_case(&typed);
                let hot = self.hover == Hit::Chip(index);
                press(render, *chip, self.pressed == Hit::Chip(index) && hot);
                canvas.fill(*chip, 8.0, if current { mix(PAPER, TINT, 0.2) } else { WHITE }, if current { 1.0 } else if hot { 0.9 } else { 0.5 });
                canvas.ring(*chip, 8.0, if current { ACCENT } else { INK }, if current { 0.5 } else { 0.08 }, 1.0);
                canvas.text(CHIP, *chip, name, if current { ACCENT } else { SOFT_INK }, Align::Center, false);
                unpress(render);
            }
        }
        canvas.text(PREVIEW, layout.preview, &self.model.preview(), MUTED, Align::Left, false);

        canvas.text(LABEL, layout.credentials_label, "PASSWORDS AND PASSKEYS", MUTED, Align::Left, false);
        for (index, provider) in rivet_core::CredentialProvider::ALL.iter().enumerate() {
            let option = layout.credentials[index];
            let current = self.model.credentials == *provider;
            let hot = self.hover == Hit::Credentials(index);
            press(render, option, self.pressed == Hit::Credentials(index) && hot);
            canvas.fill(option, 10.0, if current { mix(PAPER, TINT, 0.2) } else { WHITE }, if current { 1.0 } else if hot { 0.9 } else { 0.5 });
            canvas.ring(option, 10.0, if current { ACCENT } else { INK }, if current { 0.5 } else { 0.08 }, 1.0);
            canvas.text(CHIP, option, provider.label(), if current { ACCENT } else { SOFT_INK }, Align::Center, false);
            unpress(render);
        }

        let message = self.model.error.clone().unwrap_or_default();
        canvas.text(MESSAGE, layout.message, &message, BERRY, Align::Left, true);
        let finish = layout.finish;
        let enabled = self.model.can_finish();
        let hot = enabled && self.hover == Hit::Finish;
        press(render, finish, enabled && self.pressed == Hit::Finish && hot);
        let fill = if !enabled { mix(PAPER, TINT, 0.22) } else if hot { ACCENT_PRESSED } else { ACCENT };
        canvas.fill(finish, 12.0, fill, 1.0);
        canvas.text(BUTTON, finish, "Finish", if enabled { WHITE } else { MUTED }, Align::Center, false);
        unpress(render);
    }

    unsafe fn draw_row(&mut self, canvas: &Canvas<'_>, render: &ID2D1RenderTarget, row: Rect, browser: &Browser, index: usize, position: usize) {
        let selected = self.model.selected == Some(index);
        let hot = self.hover == Hit::Row(position);
        press(render, row, self.pressed == Hit::Row(position) && hot);
        if selected {
            canvas.fill(row, 10.0, mix(WHITE, TINT, 0.12), 1.0);
            canvas.ring(row, 10.0, ACCENT, 0.6, 1.5);
        } else if hot {
            canvas.fill(row, 10.0, WHITE, 0.7);
        }
        let icon_box = Rect { x: row.x + 12.0, y: row.y + 10.0, w: 28.0, h: 28.0 };
        let pixels = self.to_px(icon_box.w);
        match self.gfx.icon(render, &browser.path, pixels) {
            Some(bitmap) => {
                let dest = d2d(icon_box);
                render.DrawBitmap(&bitmap, Some(&dest), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
            }
            None => canvas.glyph('\u{E774}', icon_box, 18.0, ACCENT),
        }
        let (badge, dot) = status_badge(&browser.status);
        let badge_width = canvas.measure(BADGE, badge) + 28.0;
        let badge_box = Rect { x: row.right() - 12.0 - badge_width, y: row.y + 14.0, w: badge_width, h: 20.0 };
        canvas.fill(badge_box, 6.0, WHITE, 0.6);
        canvas.ring(badge_box, 6.0, INK, 0.08, 1.0);
        canvas.dot(badge_box.x + 8.0, badge_box.y + 7.0, 6.0, dot);
        canvas.text(BADGE, Rect { x: badge_box.x + 20.0, y: badge_box.y, w: badge_box.w - 24.0, h: badge_box.h }, badge, SOFT_INK, Align::Left, false);
        let text_width = badge_box.x - (row.x + 52.0) - 10.0;
        let name_color = if browser.status.can_open() { INK } else { SOFT_INK };
        canvas.text(ROW_NAME, Rect { x: row.x + 52.0, y: row.y + 6.0, w: text_width, h: 18.0 }, &browser.name, name_color, Align::Left, false);
        let meta = format!("{}  {}", browser.version, browser.path.display());
        canvas.text(ROW_META, Rect { x: row.x + 52.0, y: row.y + 25.0, w: text_width, h: 14.0 }, &meta, MUTED, Align::Left, false);
        unpress(render);
    }

    unsafe fn draw_field(&mut self, render: &ID2D1RenderTarget, canvas: &Canvas<'_>) {
        let text = self.layout.text;
        let empty = self.model.profile.is_empty();
        let (left, top) = (self.to_px(text.x), self.to_px(text.y));
        let (dpi, to_dip) = (self.dpi, |pixels: i32| pixels as f32 * 96.0 / self.dpi as f32);
        if let Some(pixels) = self.field.render(WHITE) {
            let properties = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
            };
            let size = D2D_SIZE_U { width: pixels.width as u32, height: pixels.height as u32 };
            if let Ok(bitmap) = render.CreateBitmap(size, Some(pixels.bytes.as_ptr().cast()), pixels.width as u32 * 4, &properties) {
                let dest = D2D_RECT_F {
                    left: to_dip(left),
                    top: to_dip(top),
                    right: to_dip(left + pixels.width),
                    bottom: to_dip(top + pixels.height),
                };
                render.DrawBitmap(&bitmap, Some(&dest), 1.0, windows::Win32::Graphics::Direct2D::D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None);
            }
        }
        if empty {
            canvas.text(
                style("Manrope", FIELD_TEXT, 400, 0.0),
                Rect { x: text.x, y: text.y, w: text.w, h: text.h },
                config::DEFAULT_PROFILE,
                0xb3b0b8,
                Align::Left,
                false,
            );
        }
        if self.caret_on {
            if let Some(caret) = self.field.caret() {
                let rect = Rect {
                    x: self.to_dip(caret.left),
                    y: self.to_dip(caret.top),
                    w: self.to_dip((caret.right - caret.left).max(1)).max(1.2),
                    h: self.to_dip(caret.bottom - caret.top),
                };
                canvas.fill(rect, 0.0, INK, 1.0);
            }
        }
    }

    unsafe fn hit_at(&self, lparam: LPARAM) -> (Hit, f32, f32) {
        let x = self.to_dip((lparam & 0xffff) as i16 as i32);
        let y = self.to_dip(((lparam >> 16) & 0xffff) as i16 as i32);
        (model::hit(&self.layout, x, y), x, y)
    }

    unsafe fn pick_file(&mut self) {
        let mut buffer = [0u16; 1024];
        let filter: Vec<u16> = "Programs (*.exe)\0*.exe\0All files\0*.*\0\0".encode_utf16().collect();
        let title = wide("Choose a Chromium browser");
        let mut dialog: OPENFILENAMEW = std::mem::zeroed();
        dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = self.window;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.lpstrFile = buffer.as_mut_ptr();
        dialog.nMaxFile = buffer.len() as u32;
        dialog.lpstrTitle = title.as_ptr();
        dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
        if GetOpenFileNameW(&mut dialog) == 0 {
            return;
        }
        let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        let path = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
        match system::inspect(&path) {
            Ok(browser) => {
                self.model.add_by_hand(browser);
                self.model.reveal(self.visible);
            }
            Err(rejection) => self.model.error = Some(format!("{}: {rejection}.", path.display())),
        }
        self.relayout();
        self.repaint();
    }

    unsafe fn finish(&mut self) {
        let saved = match self.model.finish() {
            Ok(saved) => saved,
            Err(error) => {
                self.model.error = Some(error);
                self.repaint();
                return;
            }
        };
        let outcome = config::data_root().and_then(|root| {
            config::save(&root, &saved)?;
            launch::start(&saved, &root)
        });
        match outcome {
            Ok(()) => {
                DestroyWindow(self.window);
            }
            Err(error) => {
                self.model.error = Some(error);
                self.repaint();
            }
        }
    }

    unsafe fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Close => {
                DestroyWindow(self.window);
            }
            Hit::Row(position) => {
                self.model.select(self.model.scroll + position);
                self.relayout();
                self.repaint();
            }
            Hit::Manual => self.pick_file(),
            Hit::Chip(index) => {
                if let Some(name) = self.model.profiles.get(index).cloned() {
                    self.field.set_text(&name);
                    self.field.select_all();
                    self.sync_text();
                    self.relayout();
                }
            }
            Hit::Credentials(index) => {
                if let Some(provider) = rivet_core::CredentialProvider::ALL.get(index) {
                    self.model.credentials = *provider;
                    self.repaint();
                }
            }
            Hit::Finish => self.finish(),
            Hit::Field | Hit::Caption | Hit::None => {}
        }
    }
}

unsafe fn app<'a>(window: HWND) -> Option<&'a mut App> {
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut App;
    pointer.as_mut()
}

fn low(value: usize) -> i32 {
    (value & 0xffff) as i16 as i32
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(app) = app(window) else {
        return DefWindowProcW(window, message, wparam, lparam);
    };
    match message {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(window, &mut paint);
            app.paint();
            EndPaint(window, &paint);
            0
        }
        WM_SIZE => {
            if let Some(target) = &app.gfx.target {
                let _ = target.Resize(&D2D_SIZE_U { width: (lparam & 0xffff) as u32, height: ((lparam >> 16) & 0xffff) as u32 });
            }
            app.repaint();
            0
        }
        WM_DPICHANGED => {
            app.dpi = (wparam & 0xffff) as u32;
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(window, std::ptr::null_mut(), suggested.left, suggested.top, suggested.right - suggested.left, suggested.bottom - suggested.top, SWP_NOACTIVATE | SWP_NOZORDER);
            if let Some(target) = &app.gfx.target {
                target.SetDpi(app.dpi as f32, app.dpi as f32);
            }
            app.gfx.icons.clear();
            app.relayout();
            app.repaint();
            0
        }
        WM_NCHITTEST => {
            let mut point = POINT { x: low(lparam as usize), y: ((lparam >> 16) & 0xffff) as i16 as i32 };
            ScreenToClient(window, &mut point);
            let x = app.to_dip(point.x);
            let y = app.to_dip(point.y);
            match model::hit(&app.layout, x, y) {
                Hit::Caption => HTCAPTION as LRESULT,
                _ => HTCLIENT as LRESULT,
            }
        }
        WM_SETCURSOR if low(lparam as usize) as u32 == HTCLIENT => {
            let mut point: POINT = std::mem::zeroed();
            GetCursorPos(&mut point);
            ScreenToClient(window, &mut point);
            let (x, y) = (app.to_dip(point.x), app.to_dip(point.y));
            let cursor = match model::hit(&app.layout, x, y) {
                Hit::Field => IDC_IBEAM,
                Hit::Row(_) | Hit::Manual | Hit::Chip(_) | Hit::Credentials(_) | Hit::Close => IDC_HAND,
                Hit::Finish if app.model.can_finish() => IDC_HAND,
                _ => IDC_ARROW,
            };
            SetCursor(LoadCursorW(std::ptr::null_mut(), cursor));
            1
        }
        WM_MOUSEMOVE => {
            if !app.tracking {
                let mut track = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: window, dwHoverTime: 0 };
                TrackMouseEvent(&mut track);
                app.tracking = true;
            }
            let (hit, _, _) = app.hit_at(lparam);
            if hit == Hit::Field || (wparam & 1) != 0 {
                app.field.send(message, wparam, lparam);
            }
            if hit != app.hover {
                app.hover = hit;
            }
            app.repaint();
            0
        }
        WM_MOUSELEAVE => {
            app.tracking = false;
            app.hover = Hit::None;
            app.repaint();
            0
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let (hit, _, _) = app.hit_at(lparam);
            app.pressed = hit;
            if hit == Hit::Field {
                app.field.send(message, wparam, lparam);
                app.sync_text();
            }
            app.repaint();
            0
        }
        WM_LBUTTONUP => {
            let (hit, _, _) = app.hit_at(lparam);
            let pressed = std::mem::replace(&mut app.pressed, Hit::None);
            if pressed == Hit::Field || hit == Hit::Field {
                app.field.send(message, wparam, lparam);
                app.sync_text();
            }
            if hit == pressed && hit != Hit::Field {
                app.activate(hit);
            }
            app.repaint();
            0
        }
        WM_MOUSEWHEEL => {
            let rows = if ((wparam >> 16) as i16) > 0 { -1 } else { 1 };
            app.model.scroll_by(rows, app.visible);
            app.repaint();
            0
        }
        WM_KEYDOWN => {
            match wparam as u16 {
                VK_RETURN => app.finish(),
                VK_ESCAPE => {
                    DestroyWindow(window);
                }
                VK_UP | VK_DOWN if GetKeyState(VK_CONTROL as i32) >= 0 => {
                    app.model.step(wparam as u16 == VK_DOWN);
                    app.model.reveal(app.visible);
                    app.relayout();
                    app.repaint();
                }
                _ => {
                    return match app.field.send(message, wparam, lparam) {
                        Some(result) => {
                            app.sync_text();
                            result
                        }
                        None => DefWindowProcW(window, message, wparam, lparam),
                    };
                }
            }
            0
        }
        WM_CHAR => {
            // Enter, Escape and Tab are the window's, not text.
            if matches!(wparam, 0x0d | 0x1b | 0x09 | 0x0a) {
                return 0;
            }
            let result = app.field.send(message, wparam, lparam).unwrap_or(0);
            app.sync_text();
            result
        }
        WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION
        | WM_INPUTLANGCHANGE | WM_CONTEXTMENU => match app.field.send(message, wparam, lparam) {
            Some(result) => {
                app.sync_text();
                result
            }
            None => DefWindowProcW(window, message, wparam, lparam),
        },
        WM_SETFOCUS => {
            app.field.send(message, wparam, lparam);
            app.caret_on = true;
            app.repaint();
            0
        }
        WM_KILLFOCUS => {
            app.field.send(message, wparam, lparam);
            app.repaint();
            0
        }
        WM_TIMER if wparam == CARET_TIMER => {
            app.caret_on = !app.caret_on;
            app.repaint();
            0
        }
        WM_TIMER if (TIMER_BASE..=TIMER_LAST).contains(&wparam) => {
            app.field.send(message, wparam - TIMER_BASE, 0);
            app.sync_text();
            0
        }
        WM_DESTROY => {
            KillTimer(window, CARET_TIMER);
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut App;
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            if !pointer.is_null() {
                drop(Box::from_raw(pointer));
            }
            DefWindowProcW(window, message, wparam, lparam)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// Where to put the window when a test asks for a place, as `x,y`. Such a window opens without
/// taking the focus.
fn requested_position() -> Option<(i32, i32)> {
    let value = std::env::var("RIVET_SETUP_POSITION").ok()?;
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Shows the setup window over the browsers it found and returns when it closes.
pub fn open(reason: Option<String>) -> Result<(), String> {
    let root = config::data_root()?;
    let saved = config::load(&root).ok().flatten();
    let browsers = system::detect_all();
    let model = Model::new(browsers, root, reason, saved.as_ref());
    show(model)
}

fn show(model: Model) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        for font in FONTS {
            let mut count = 0u32;
            AddFontMemResourceEx(font.as_ptr().cast(), font.len() as u32, std::ptr::null(), &mut count);
        }
        let instance = GetModuleHandleW(std::ptr::null());
        let mut class: WNDCLASSW = std::mem::zeroed();
        class.style = CS_DROPSHADOW | CS_DBLCLKS;
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        class.lpszClassName = CLASS;
        RegisterClassW(&class);

        let title = wide("Rivet Setup");
        let window = CreateWindowExW(
            0,
            CLASS,
            title.as_ptr(),
            WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU,
            0,
            0,
            100,
            100,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            return Err("could not create the Rivet setup window".into());
        }
        let dpi = GetDpiForWindow(window).max(96);
        let mut work: RECT = std::mem::zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
        let available = (work.bottom - work.top) as f32 * 96.0 / dpi as f32 - 40.0;
        let visible = model::visible_rows(model.browsers.len().max(1), available, model.reason.is_some());
        let field = Field::new(window, "Manrope", INK).map_err(|error| {
            DestroyWindow(window);
            error
        })?;
        let gfx = Gfx::new().map_err(|error| {
            DestroyWindow(window);
            format!("could not start the graphics of the setup window: {error}")
        })?;
        let layout = model::layout(visible, model.reason.is_some(), &[]);
        let initial = model.profile.clone();
        let boxed = Box::new(App {
            window,
            dpi,
            model,
            layout,
            gfx,
            field,
            hover: Hit::None,
            pressed: Hit::None,
            caret_on: true,
            tracking: false,
            visible,
        });
        let pointer = Box::into_raw(boxed);
        SetWindowLongPtrW(window, GWLP_USERDATA, pointer as isize);
        let app = &mut *pointer;
        app.relayout();
        app.field.activate();
        app.field.set_text(&initial);
        app.field.select_all();
        app.relayout();

        let width = app.to_px(model::WIDTH);
        let height = app.to_px(app.layout.height);
        let (x, y) = requested_position().unwrap_or((
            work.left + ((work.right - work.left) - width) / 2,
            work.top + ((work.bottom - work.top) - height) / 2,
        ));
        let corners = 2i32;
        DwmSetWindowAttribute(window, 33, (&corners as *const i32).cast(), 4);
        let margins = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
        DwmExtendFrameIntoClientArea(window, &margins);
        SetWindowPos(window, std::ptr::null_mut(), x, y, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
        SetTimer(window, CARET_TIMER, 530, None);
        if requested_position().is_some() {
            ShowWindow(window, SW_SHOWNOACTIVATE);
        } else {
            ShowWindow(window, SW_SHOW);
            SetForegroundWindow(window);
            SetFocus(window);
        }
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

/// What the launcher does: open the saved browser and profile at once, or show the setup window
/// when nothing is saved or the saved browser is gone.
pub fn run_launcher() -> Result<(), String> {
    let root = config::data_root()?;
    match config::start(&root, &Disk) {
        Start::Open(saved) => open_saved(&saved, &root),
        Start::Setup { reason } => open(reason),
    }
}

fn open_saved(saved: &Saved, root: &std::path::Path) -> Result<(), String> {
    launch::keep_used_builds(root, &system::detect_all());
    launch::start(saved, root)
}

/// Tells a person why nothing opened. The launcher has no terminal.
pub fn show_error(text: &str) {
    let caption = wide("Rivet Browser");
    let body = wide(text);
    unsafe { MessageBoxW(std::ptr::null_mut(), body.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR) };
}
