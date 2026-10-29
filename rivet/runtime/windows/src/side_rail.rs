//! The Sidebar's rail and its open button, drawn as visuals of the browser window.
//!
//! The rail lies in the column the browser's layout leaves free at the right of the page area
//! (see `chrome_side_panel`), so it covers nothing of the browser. The open button of a folded
//! Sidebar appears over the page's lower right corner while the pointer is near it. Both take
//! their input from the window's own message queue, because Chromium's page child window would
//! otherwise swallow clicks that land on a visual above it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{GENERIC_READ, POINT};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1DeviceContext, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDesktopDevice, IDCompositionVisual, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory5, IDWriteTextLayout, DWRITE_FONT_STRETCH_NORMAL, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows_numerics::{Matrix3x2, Vector2};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT as SysPoint, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetCursorPos, IsIconic, IsWindowVisible, SetTimer,
    SetWindowsHookExW, GA_ROOT, MSG, PM_REMOVE, WH_GETMESSAGE, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_NULL,
};

use crate::appbar::Appbar;
use crate::chrome_side_panel::{self as panel, PageGeometry, PANEL_WIDTH, RAIL_WIDTH};

const PAPER: u32 = 0xfbfaf7;
/// The tint of a window before it has a space, and the accent that does not follow the space.
const TINT: u32 = 0xd08a52;
const ACCENT: u32 = 0x9a5424;
const INK: u32 = 0x1f2024;
const SOFT_INK: u32 = 0x4a4850;
const ICONS: &str = "Segoe Fluent Icons";
const PANEL_RADIUS: f32 = 12.0;

/// The glyph the rail shows for a side panel entry. Extension panels share one.
fn glyph_for(id: u32) -> char {
    match id {
        0 => '\u{E7BC}',
        1 => '\u{E736}',
        2 | 3 => '\u{E81C}',
        4 => '\u{E8E9}',
        _ => '\u{EA86}',
    }
}
/// What a rail button shows for an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Look {
    /// One of Chrome's own entries, drawn with its icon glyph.
    Glyph(char),
    /// An extension's own icon, from the file its manifest names.
    Picture(PathBuf),
    /// An extension whose icon is not on disk: its initial.
    Letter(char),
}

/// A rail entry as the person sees it: the label that shows on hover, and the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryLook {
    name: String,
    look: Look,
}

/// How an entry of Chrome's side panel looks in the rail.
fn entry_look(key: &panel::EntryKey, extensions: &mut HashMap<String, Option<panel::ExtensionLook>>) -> EntryLook {
    let Some(id) = panel::extension_id(key) else {
        let id = panel::entry_id(key);
        return EntryLook { name: panel::builtin_name(id).to_owned(), look: Look::Glyph(glyph_for(id)) };
    };
    let found = extensions.entry(id.clone()).or_insert_with(|| panel::extension_look(&id)).clone();
    let name = found.as_ref().map_or_else(|| id.clone(), |found| found.name.clone());
    let look = match found.and_then(|found| found.icon) {
        Some(path) => Look::Picture(path),
        None => Look::Letter(name.chars().next().map_or('?', |c| c.to_uppercase().next().unwrap_or(c))),
    };
    EntryLook { name, look }
}

const ADD_GLYPH: char = '\u{E710}';
const FOLD_GLYPH: char = '\u{E76C}';
const OPEN_GLYPH: char = '\u{E76B}';

/// The rail's buttons are 30 DIPs square, 2 apart, and the rail pads its ends by 6.
const BUTTON: f32 = 30.0;
const BUTTON_GAP: f32 = 2.0;
const RAIL_PAD: f32 = 6.0;
/// The area at the page's lower right corner in which the pointer shows the open button, and the
/// button's place in it.
const HOT_AREA: f32 = 72.0;
const OPEN_RIGHT: f32 = 4.0;
const OPEN_BOTTOM: f32 = 6.0;
/// The timer that follows the pointer, in milliseconds.
const POLL_MS: u32 = 70;
const POLL_TIMER: usize = 0x5249_5253;
/// The label over a hovered button: 26 DIPs tall, padded by 10, 6 away from the rail.
const TIP_HEIGHT: f32 = 26.0;
const TIP_PAD_X: f32 = 10.0;
const TIP_GAP: f32 = 6.0;
/// The open panel's header is 40 DIPs tall, pads its left by 14 and its right by 6, and puts the
/// icon and the name 8 apart. Its close button is 28 DIPs square.
const HEADER_HEIGHT: f32 = 40.0;
const HEADER_PAD_LEFT: f32 = 14.0;
const HEADER_PAD_RIGHT: f32 = 6.0;
const HEADER_GAP: f32 = 8.0;
const CLOSE_BUTTON: f32 = 28.0;
const CLOSE_GLYPH: char = '\u{E8BB}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailHit {
    Entry(usize),
    Add,
    Fold,
}

/// What a point of the rail, in its own DIPs, is. `height` is the rail's height.
pub fn rail_hit(x: f32, y: f32, height: f32, entries: usize) -> Option<RailHit> {
    let inside = |top: f32| {
        x >= (RAIL_WIDTH as f32 - BUTTON) / 2.0
            && x < (RAIL_WIDTH as f32 + BUTTON) / 2.0
            && y >= top && y < top + BUTTON
    };
    for index in 0..entries {
        if inside(RAIL_PAD + index as f32 * (BUTTON + BUTTON_GAP)) {
            return Some(RailHit::Entry(index));
        }
    }
    if inside(add_top(entries)) {
        return Some(RailHit::Add);
    }
    inside(height - RAIL_PAD - BUTTON).then_some(RailHit::Fold)
}

/// Where the add button begins: after the entries, a rule and its margins.
fn add_top(entries: usize) -> f32 {
    RAIL_PAD + entries as f32 * (BUTTON + BUTTON_GAP) + 4.0 + 1.0 + 4.0 + BUTTON_GAP
}

/// The open button's rectangle as (left, top) in page-area DIPs measured from the page's right
/// and bottom edges, and whether a point is in the area that shows it.
fn open_button(geometry: PageGeometry) -> (f32, f32) {
    (
        geometry.right as f32 - OPEN_RIGHT - BUTTON,
        geometry.bottom as f32 - OPEN_BOTTOM - BUTTON,
    )
}

fn in_hot_area(geometry: PageGeometry, x: f32, y: f32) -> bool {
    x >= geometry.right as f32 - HOT_AREA
        && x < geometry.right as f32
        && y >= geometry.bottom as f32 - HOT_AREA
        && y < geometry.bottom as f32
}

fn on_open_button(geometry: PageGeometry, x: f32, y: f32) -> bool {
    let (left, top) = open_button(geometry);
    (left..left + BUTTON).contains(&x) && (top..top + BUTTON).contains(&y)
}

/// The rail's rectangle in window DIPs as (left, top, height).
fn rail_rect(geometry: PageGeometry) -> (f32, f32, f32) {
    (
        (geometry.right - RAIL_WIDTH) as f32,
        geometry.top as f32,
        (geometry.bottom - geometry.top) as f32,
    )
}

/// The opacity the open button takes: invisible away from the corner, faint near it and solid
/// under the pointer.
fn open_opacity(near: bool, over: bool) -> f32 {
    match (near, over) {
        (_, true) => 1.0,
        (true, false) => 0.6,
        _ => 0.0,
    }
}

/// What one frame of a fade does: moves `now` toward `target` by at most `step`.
fn fade(now: f32, target: f32, step: f32) -> f32 {
    if (target - now).abs() <= step {
        target
    } else {
        now + step * (target - now).signum()
    }
}

fn rgb(value: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

fn mix(under: u32, top: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((under >> shift) & 0xff) as f32;
        let b = ((top >> shift) & 0xff) as f32;
        ((a + (b - a) * amount).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// What the rail's surface was painted for.
#[derive(Debug, Clone, PartialEq)]
struct ColumnKey {
    width: i32,
    height: i32,
    dpi: u32,
    hover: Option<RailHit>,
    pressed: Option<RailHit>,
    looks: Vec<EntryLook>,
    tint: u32,
    showing: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct OpenKey {
    dpi: u32,
    opacity: i32,
    over: bool,
}

/// What the label over a hovered button was painted for.
#[derive(Debug, Clone, PartialEq)]
struct TipKey {
    text: String,
    dpi: u32,
}

/// What the open panel's header was painted for.
#[derive(Debug, Clone, PartialEq)]
struct PanelKey {
    width: i32,
    height: i32,
    dpi: u32,
    look: EntryLook,
    close_hover: bool,
}

struct Rail {
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    column: IDCompositionVisual2,
    open: IDCompositionVisual2,
    tip: IDCompositionVisual2,
    /// The open extension's panel: its left hairline and its header, over the browser's own.
    panel: IDCompositionVisual2,
    panel_key: Option<PanelKey>,
    /// The index in `looks` of the entry the side panel shows, if any.
    shown: Option<usize>,
    close_hover: bool,
    column_key: Option<ColumnKey>,
    open_key: Option<OpenKey>,
    tip_key: Option<TipKey>,
    /// The icons already decoded, by file.
    pictures: HashMap<PathBuf, Option<IWICBitmapSource>>,
    /// What was found out about each extension, by id, so the disk is read once.
    extensions: HashMap<String, Option<panel::ExtensionLook>>,
    looks: Vec<EntryLook>,
    /// The entries Chrome's side panel registry listed at the last update.
    entries: Vec<panel::EntryKey>,
    hover: Option<RailHit>,
    pressed: Option<RailHit>,
    /// The pointer is near the corner, and on the open button.
    near: bool,
    over: bool,
    opacity: f32,
    opening: bool,
    /// The geometry of the last update, for hit testing in physical pixels.
    scale: f32,
    geometry: Option<PageGeometry>,
}

thread_local! {
    static RAILS: RefCell<HashMap<isize, Rail>> = RefCell::new(HashMap::new());
    static HOOK: RefCell<isize> = const { RefCell::new(0) };
}

impl Rail {
    unsafe fn new(appbar: &Appbar) -> Result<Self> {
        let (device, root, dwrite) = appbar.composition();
        let column = device.CreateVisual()?;
        root.AddVisual(&column, true, None::<&IDCompositionVisual>)?;
        let panel = device.CreateVisual()?;
        root.AddVisual(&panel, true, None::<&IDCompositionVisual>)?;
        let open = device.CreateVisual()?;
        root.AddVisual(&open, true, None::<&IDCompositionVisual>)?;
        let tip = device.CreateVisual()?;
        root.AddVisual(&tip, true, None::<&IDCompositionVisual>)?;
        Ok(Self {
            device,
            dwrite,
            column,
            open,
            tip,
            panel,
            panel_key: None,
            shown: None,
            close_hover: false,
            column_key: None,
            open_key: None,
            tip_key: None,
            pictures: HashMap::new(),
            extensions: HashMap::new(),
            looks: Vec::new(),
            entries: Vec::new(),
            hover: None,
            pressed: None,
            near: false,
            over: false,
            opacity: 0.0,
            opening: false,
            scale: 1.0,
            geometry: None,
        })
    }

    unsafe fn icon(&self, context: &ID2D1DeviceContext, glyph: char, area: D2D_RECT_F, size: f32, colour: D2D1_COLOR_F) -> Result<()> {
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
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.DrawTextLayout(
            Vector2 { X: area.left, Y: area.top },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
        Ok(())
    }

    /// Draws an extension's icon, 16 DIPs square in the middle of a button.
    unsafe fn picture(&self, context: &ID2D1DeviceContext, path: &PathBuf, area: D2D_RECT_F, ink: u32) -> Result<()> {
        let Some(Some(source)) = self.pictures.get(path) else {
            return self.letter(context, '?', area, ink);
        };
        let bitmap = context.CreateBitmapFromWicBitmap(source, None)?;
        let (x, y) = ((area.left + area.right) / 2.0 - 8.0, (area.top + area.bottom) / 2.0 - 8.0);
        let target = rect(x, y, 16.0, 16.0);
        context.DrawBitmap(&bitmap, Some(&target), 1.0, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, None, None);
        Ok(())
    }

    /// The initial of an extension that has no icon on disk, in a small rounded square.
    unsafe fn letter(&self, context: &ID2D1DeviceContext, letter: char, area: D2D_RECT_F, ink: u32) -> Result<()> {
        let (x, y) = ((area.left + area.right) / 2.0 - 8.0, (area.top + area.bottom) / 2.0 - 8.0);
        self.rounded(context, rect(x, y, 16.0, 16.0), 5.0, rgb(INK, 0.08))?;
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from("Segoe UI Variable Text"),
            None,
            DWRITE_FONT_WEIGHT(700),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            10.0,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        let text: Vec<u16> = letter.to_string().encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(&text, &format, 16.0, 16.0)?;
        let brush = context.CreateSolidColorBrush(&rgb(ink, 1.0), None)?;
        context.DrawTextLayout(Vector2 { X: x, Y: y }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        Ok(())
    }

    /// The width and height in DIPs of a label's bubble.
    unsafe fn tip_size(&self, text: &str) -> Result<(f32, f32)> {
        let layout = self.tip_layout(text, 400.0)?;
        let mut metrics = Default::default();
        layout.GetMetrics(&mut metrics)?;
        Ok((metrics.width.ceil() + 2.0 * TIP_PAD_X, TIP_HEIGHT))
    }

    unsafe fn tip_layout(&self, text: &str, width: f32) -> Result<IDWriteTextLayout> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from("Segoe UI Variable Text"),
            None,
            DWRITE_FONT_WEIGHT(500),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            12.0,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let text: Vec<u16> = text.encode_utf16().collect();
        self.dwrite.CreateTextLayout(&text, &format, width, TIP_HEIGHT)
    }

    /// The label that names a button, drawn like the mock-up's tooltip: ink with light text.
    unsafe fn draw_tip(&self, context: &ID2D1DeviceContext, text: &str, width: f32) -> Result<()> {
        self.rounded(context, rect(0.0, 0.0, width, TIP_HEIGHT), 8.0, rgb(INK, 0.94))?;
        let layout = self.tip_layout(text, width - 2.0 * TIP_PAD_X)?;
        let brush = context.CreateSolidColorBrush(&rgb(0xffffff, 1.0), None)?;
        context.DrawTextLayout(Vector2 { X: TIP_PAD_X, Y: 0.0 }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        Ok(())
    }

    unsafe fn rounded(&self, context: &ID2D1DeviceContext, area: D2D_RECT_F, radius: f32, colour: D2D1_COLOR_F) -> Result<()> {
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT { rect: area, radiusX: radius, radiusY: radius },
            &brush,
        );
        Ok(())
    }

    /// Paints into a fresh surface of `width` by `height` pixels at `dpi` and shows it in `visual`.
    unsafe fn paint(
        &self,
        visual: &IDCompositionVisual2,
        width: i32,
        height: i32,
        dpi: u32,
        draw: impl FnOnce(&ID2D1DeviceContext) -> Result<()>,
    ) -> Result<()> {
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
        context.SetTransform(&Matrix3x2::translation(offset.x as f32 / scale, offset.y as f32 / scale));
        context.Clear(Some(&rgb(PAPER, 0.0)));
        let result = draw(&context);
        surface.EndDraw()?;
        result?;
        visual.SetContent(&surface)
    }

    unsafe fn draw_column(&self, context: &ID2D1DeviceContext, width: f32, height: f32, key: &ColumnKey) -> Result<()> {
        let ground = mix(PAPER, key.tint, 0.04);
        // The page panel rounds its lower right corner. The rail is cut from the same shape: its
        // top edge lies below the top row, so only the lower corner shows.
        let square = context.CreateSolidColorBrush(&rgb(ground, 1.0), None)?;
        context.FillRectangle(&rect(0.0, 0.0, width - PANEL_RADIUS, height), &square);
        self.rounded(
            context,
            rect(width - 2.0 * PANEL_RADIUS, -PANEL_RADIUS, 2.0 * PANEL_RADIUS, height + PANEL_RADIUS),
            PANEL_RADIUS,
            rgb(ground, 1.0),
        )?;
        context.FillRectangle(&rect(0.0, 0.0, 1.0, height), &context.CreateSolidColorBrush(&rgb(INK, 0.08), None)?);
        let left = (width - BUTTON) / 2.0;
        let button = |top: f32, hit: RailHit, look: &Look, on: bool| -> Result<()> {
            let area = rect(left, top, BUTTON, BUTTON);
            let pressed = key.pressed == Some(hit) && key.hover == Some(hit);
            if on {
                self.rounded(context, area, 8.0, rgb(mix(0xffffff, key.tint, 0.14), 1.0))?;
            } else if pressed || key.hover == Some(hit) {
                self.rounded(context, area, 8.0, rgb(INK, if pressed { 0.1 } else { 0.06 }))?;
            }
            let ink = if on { ACCENT } else if key.hover == Some(hit) { INK } else { SOFT_INK };
            match look {
                Look::Glyph(glyph) => self.icon(context, *glyph, area, 14.0, rgb(ink, 1.0)),
                Look::Picture(path) => self.picture(context, path, area, ink),
                Look::Letter(letter) => self.letter(context, *letter, area, ink),
            }
        };
        for (index, entry) in key.looks.iter().enumerate() {
            let top = RAIL_PAD + index as f32 * (BUTTON + BUTTON_GAP);
            let on = key.showing & (1 << index.min(31)) != 0;
            button(top, RailHit::Entry(index), &entry.look, on)?;
        }
        let rule = RAIL_PAD + key.looks.len() as f32 * (BUTTON + BUTTON_GAP) + 4.0;
        context.FillRectangle(
            &rect((width - 18.0) / 2.0, rule, 18.0, 1.0),
            &context.CreateSolidColorBrush(&rgb(INK, 0.08), None)?,
        );
        button(add_top(key.looks.len()), RailHit::Add, &Look::Glyph(ADD_GLYPH), false)?;
        button(height - RAIL_PAD - BUTTON, RailHit::Fold, &Look::Glyph(FOLD_GLYPH), false)
    }

    /// The header of an open extension panel, drawn like the mock-up's `.ext-panel header`: the
    /// accent icon, the name in 13 DIP semibold and a close button, over a hairline-bordered column.
    unsafe fn draw_panel(&self, context: &ID2D1DeviceContext, width: f32, height: f32, key: &PanelKey) -> Result<()> {
        let hairline = context.CreateSolidColorBrush(&rgb(INK, 0.08), None)?;
        context.FillRectangle(&rect(0.0, 0.0, 1.0, height), &hairline);
        context.FillRectangle(&rect(1.0, 0.0, width - 1.0, HEADER_HEIGHT), &context.CreateSolidColorBrush(&rgb(PAPER, 1.0), None)?);
        context.FillRectangle(&rect(1.0, HEADER_HEIGHT - 1.0, width - 1.0, 1.0), &hairline);
        let icon = rect(1.0 + HEADER_PAD_LEFT, 0.0, 16.0, HEADER_HEIGHT - 1.0);
        match &key.look.look {
            Look::Glyph(glyph) => self.icon(context, *glyph, icon, 15.0, rgb(ACCENT, 1.0))?,
            Look::Picture(path) => self.picture(context, path, icon, ACCENT)?,
            Look::Letter(letter) => self.letter(context, *letter, icon, ACCENT)?,
        }
        let close = rect(width - HEADER_PAD_RIGHT - CLOSE_BUTTON, (HEADER_HEIGHT - 1.0 - CLOSE_BUTTON) / 2.0, CLOSE_BUTTON, CLOSE_BUTTON);
        if key.close_hover {
            self.rounded(context, close, 8.0, rgb(INK, 0.06))?;
        }
        self.icon(context, CLOSE_GLYPH, close, 10.0, rgb(if key.close_hover { INK } else { SOFT_INK }, 1.0))?;
        let name_left = 1.0 + HEADER_PAD_LEFT + 16.0 + HEADER_GAP;
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from("Segoe UI Variable Text"),
            None,
            DWRITE_FONT_WEIGHT(600),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            13.0,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let text: Vec<u16> = key.look.name.encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(&text, &format, close.left - HEADER_GAP - name_left, HEADER_HEIGHT - 1.0)?;
        let brush = context.CreateSolidColorBrush(&rgb(INK, 1.0), None)?;
        context.DrawTextLayout(Vector2 { X: name_left, Y: 0.0 }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        Ok(())
    }

    unsafe fn draw_open(&self, context: &ID2D1DeviceContext, opacity: f32, over: bool) -> Result<()> {
        let area = rect(0.0, 0.0, BUTTON, BUTTON);
        let ground = if over { rgb(INK, 0.06 * opacity) } else { rgb(0xffffff, 0.4 * opacity) };
        self.rounded(context, area, 8.0, ground)?;
        let ink = if over { INK } else { SOFT_INK };
        self.icon(context, OPEN_GLYPH, area, 14.0, rgb(ink, opacity))
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: x, top: y, right: x + width, bottom: y + height }
}

/// Which of the entries Chrome's side panel shows, as a bit mask over the listed ones.
unsafe fn showing_mask(window: HWND, entries: &[panel::EntryKey]) -> u32 {
    entries
        .iter()
        .enumerate()
        .filter(|(index, key)| *index < 32 && panel::entry_showing(window, key))
        .fold(0, |mask, (index, _)| mask | 1 << index)
}

/// Places and paints the rail and the open button of a window. Called with the other visuals of
/// the window's surface.
pub unsafe fn update(window: HWND, appbar: &Appbar, visible: bool, dpi: u32) {
    let geometry = panel::page_geometry(window).filter(|_| visible);
    RAILS.with(|rails| {
        let mut rails = rails.borrow_mut();
        if !rails.contains_key(&(window as isize)) {
            let Ok(rail) = Rail::new(appbar) else { return };
            rails.insert(window as isize, rail);
            // The page area is known once the browser has laid out with the patched layout.
            if let Err(error) = panel::install_layout() {
                crate::vertical_tabs::trace(&format!("side rail: {error}"));
            }
            if let Err(error) = panel::install_page_colours() {
                crate::vertical_tabs::trace(&format!("side rail: {error}"));
            }
            SetTimer(window, POLL_TIMER, POLL_MS, Some(poll));
            install_hook();
        }
        let rail = rails.get_mut(&(window as isize)).expect("inserted above");
        if let Err(error) = refresh(window, rail, geometry, dpi) {
            crate::vertical_tabs::trace(&format!("side rail: {error}"));
        }
    });
}

unsafe fn refresh(window: HWND, rail: &mut Rail, geometry: Option<PageGeometry>, dpi: u32) -> Result<()> {
    let scale = dpi as f32 / 96.0;
    rail.scale = scale;
    rail.geometry = geometry;
    let unfolded = panel::is_unfolded(window);
    let (Some(geometry), true) = (geometry, unfolded) else {
        rail.column.SetContent(None)?;
        rail.column_key = None;
        rail.panel.SetContent(None)?;
        rail.panel_key = None;
        rail.shown = None;
        rail.tip.SetContent(None)?;
        rail.tip_key = None;
        return refresh_open(rail, geometry.filter(|_| !unfolded), dpi);
    };
    rail.entries = panel::listed_entries(window);
    let looks: Vec<EntryLook> = rail.entries.iter().map(|key| entry_look(key, &mut rail.extensions)).collect();
    rail.looks = looks;
    rail.looks.extend(FAKE.with(|fake| fake.borrow().clone()));
    for look in rail.looks.clone() {
        if let Look::Picture(path) = &look.look {
            if !rail.pictures.contains_key(path) {
                rail.pictures.insert(path.clone(), load_picture(path));
            }
        }
    }
    let (left, top, height) = rail_rect(geometry);
    let (width_px, height_px) = ((RAIL_WIDTH as f32 * scale).round() as i32, (height * scale).round() as i32);
    rail.column.SetOffsetX2((left * scale).round())?;
    rail.column.SetOffsetY2((top * scale).round())?;
    let key = ColumnKey {
        width: width_px,
        height: height_px,
        dpi,
        hover: rail.hover,
        pressed: rail.pressed,
        looks: rail.looks.clone(),
        tint: FORCED_TINT.with(|forced| forced.get()).unwrap_or_else(|| crate::frame::space_tint(window)),
        showing: showing_mask(window, &rail.entries),
    };
    rail.shown = (0..rail.entries.len().min(32)).find(|index| key.showing & (1 << index) != 0);
    if rail.column_key.as_ref() != Some(&key) && width_px > 0 && height_px > 0 {
        let shown = key.clone();
        rail.paint(&rail.column, width_px, height_px, dpi, |context| {
            rail.draw_column(context, RAIL_WIDTH as f32, height, &shown)
        })?;
        rail.column_key = Some(key);
    }
    refresh_open(rail, None, dpi)?;
    refresh_panel(rail, geometry, height, dpi)?;
    refresh_tip(rail, geometry, height, dpi)?;
    rail.device.Commit()
}

/// The open panel's left edge in window DIPs, which is where its header starts.
fn panel_left(geometry: PageGeometry) -> f32 {
    (geometry.right - RAIL_WIDTH - PANEL_WIDTH) as f32
}

/// The close button's rectangle in window DIPs as (left, top).
fn close_button(geometry: PageGeometry) -> (f32, f32) {
    (
        geometry.right as f32 - RAIL_WIDTH as f32 - HEADER_PAD_RIGHT - CLOSE_BUTTON,
        geometry.top as f32 + (HEADER_HEIGHT - 1.0 - CLOSE_BUTTON) / 2.0,
    )
}

/// Whether a window point is on the header of an open panel, and whether on its close button.
fn on_header(geometry: PageGeometry, x: f32, y: f32) -> (bool, bool) {
    let left = panel_left(geometry);
    let inside = x >= left
        && x < left + PANEL_WIDTH as f32
        && y >= geometry.top as f32
        && y < geometry.top as f32 + HEADER_HEIGHT;
    let (close_left, close_top) = close_button(geometry);
    let on_close = (close_left..close_left + CLOSE_BUTTON).contains(&x) && (close_top..close_top + CLOSE_BUTTON).contains(&y);
    (inside, inside && on_close)
}

/// Places and paints the header of the panel the browser shows, or removes it when none shows.
unsafe fn refresh_panel(rail: &mut Rail, geometry: PageGeometry, height: f32, dpi: u32) -> Result<()> {
    let Some(look) = rail.shown.and_then(|index| rail.looks.get(index)).cloned() else {
        rail.panel.SetContent(None)?;
        rail.panel_key = None;
        return Ok(());
    };
    let scale = dpi as f32 / 96.0;
    rail.panel.SetOffsetX2((panel_left(geometry) * scale).round())?;
    rail.panel.SetOffsetY2((geometry.top as f32 * scale).round())?;
    let (width_px, height_px) = ((PANEL_WIDTH as f32 * scale).round() as i32, (height * scale).round() as i32);
    let key = PanelKey { width: width_px, height: height_px, dpi, look, close_hover: rail.close_hover };
    if rail.panel_key.as_ref() != Some(&key) && width_px > 0 && height_px > 0 {
        let shown = key.clone();
        rail.paint(&rail.panel, width_px, height_px, dpi, |context| {
            rail.draw_panel(context, PANEL_WIDTH as f32, height, &shown)
        })?;
        rail.panel_key = Some(key);
    }
    Ok(())
}

/// What a rail button is called.
fn hit_name(rail: &Rail, hit: RailHit) -> String {
    match hit {
        RailHit::Entry(index) => rail.looks.get(index).map(|look| look.name.clone()).unwrap_or_default(),
        RailHit::Add => "Get extensions".to_owned(),
        RailHit::Fold => "Hide the Sidebar".to_owned(),
    }
}

/// Where a hit's button is in the rail, from its top.
fn hit_top(hit: RailHit, height: f32, entries: usize) -> f32 {
    match hit {
        RailHit::Entry(index) => RAIL_PAD + index as f32 * (BUTTON + BUTTON_GAP),
        RailHit::Add => add_top(entries),
        RailHit::Fold => height - RAIL_PAD - BUTTON,
    }
}

/// Shows the name of the hovered button to its left, or nothing when no button is hovered.
unsafe fn refresh_tip(rail: &mut Rail, geometry: PageGeometry, height: f32, dpi: u32) -> Result<()> {
    let Some(hit) = rail.hover.filter(|hit| match hit {
        RailHit::Entry(index) => *index < rail.looks.len(),
        _ => true,
    }) else {
        rail.tip.SetContent(None)?;
        rail.tip_key = None;
        return Ok(());
    };
    let text = hit_name(rail, hit);
    let scale = dpi as f32 / 96.0;
    let (width, tall) = rail.tip_size(&text)?;
    let (left, top, _) = rail_rect(geometry);
    let button = top + hit_top(hit, height, rail.looks.len());
    rail.tip.SetOffsetX2(((left - TIP_GAP - width) * scale).round())?;
    rail.tip.SetOffsetY2(((button + (BUTTON - tall) / 2.0) * scale).round())?;
    let key = TipKey { text: text.clone(), dpi };
    if rail.tip_key.as_ref() != Some(&key) {
        rail.paint(&rail.tip, (width * scale).ceil() as i32, (tall * scale).ceil() as i32, dpi, |context| {
            rail.draw_tip(context, &text, width)
        })?;
        rail.tip_key = Some(key);
    }
    Ok(())
}

/// Decodes an icon file into pixels Direct2D can draw.
unsafe fn load_picture(path: &PathBuf) -> Option<IWICBitmapSource> {
    let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
    let name = HSTRING::from(path.to_string_lossy().as_ref());
    let decoder = factory
        .CreateDecoderFromFilename(&name, None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
        .ok()?;
    let frame = decoder.GetFrame(0).ok()?;
    let converter = factory.CreateFormatConverter().ok()?;
    converter
        .Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut)
        .ok()?;
    converter.cast().ok()
}

unsafe fn refresh_open(rail: &mut Rail, geometry: Option<PageGeometry>, dpi: u32) -> Result<()> {
    let Some(geometry) = geometry.filter(|_| rail.opacity > 0.01) else {
        rail.open.SetContent(None)?;
        rail.open_key = None;
        return rail.device.Commit();
    };
    let (left, top) = open_button(geometry);
    let scale = dpi as f32 / 96.0;
    rail.open.SetOffsetX2((left * scale).round())?;
    rail.open.SetOffsetY2((top * scale).round())?;
    let key = OpenKey { dpi, opacity: (rail.opacity * 20.0).round() as i32, over: rail.over };
    if rail.open_key.as_ref() != Some(&key) {
        let size = (BUTTON * scale).round() as i32;
        let (opacity, over) = (rail.opacity, rail.over);
        rail.paint(&rail.open, size, size, dpi, |context| rail.draw_open(context, opacity, over))?;
        rail.open_key = Some(key);
    }
    rail.device.Commit()
}

/// The pointer in window DIPs, from a client point in pixels.
fn to_dips(point: SysPoint, scale: f32) -> (f32, f32) {
    (point.x as f32 / scale, point.y as f32 / scale)
}

/// What the pointer is on, in window DIPs: the rail button, or the open button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Rail(RailHit),
    Open,
    /// The close button of the open panel's header.
    PanelClose,
    /// The rest of that header, which the browser's own header lies under.
    PanelHeader,
}

fn target_at(rail: &Rail, unfolded: bool, x: f32, y: f32) -> Option<Target> {
    let geometry = rail.geometry?;
    if unfolded {
        if rail.shown.is_some() {
            match on_header(geometry, x, y) {
                (_, true) => return Some(Target::PanelClose),
                (true, false) => return Some(Target::PanelHeader),
                _ => {}
            }
        }
        let (left, top, height) = rail_rect(geometry);
        return rail_hit(x - left, y - top, height, rail.entries.len()).map(Target::Rail);
    }
    (rail.opacity > 0.0 && on_open_button(geometry, x, y)).then_some(Target::Open)
}

/// Follows the pointer: the open button fades in near the corner, and hover marks follow.
unsafe extern "system" fn poll(window: HWND, _: u32, _: usize, _: u32) {
    let mut pointer = SysPoint { x: 0, y: 0 };
    if GetCursorPos(&mut pointer) == 0 || IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
        return;
    }
    ScreenToClient(window, &mut pointer);
    let known = RAILS.with(|rails| rails.borrow().get(&(window as isize)).is_some_and(|rail| rail.geometry.is_some()));
    if !known {
        // Not laid out since the layout hook went in. Lay out outside any layout of the browser's.
        panel::relayout_page_area(window);
        crate::frame::update_appbar(window);
        return;
    }
    let changed = RAILS.with(|rails| {
        let mut rails = rails.borrow_mut();
        let Some(rail) = rails.get_mut(&(window as isize)) else { return false };
        let unfolded = panel::is_unfolded(window);
        let (x, y) = to_dips(pointer, rail.scale);
        let before = (rail.hover, rail.near, rail.over, rail.opacity, rail.close_hover);
        let forced = FORCED_POINTER.with(|forced| forced.get());
        rail.near = !unfolded
            && forced.map_or_else(
                || rail.geometry.is_some_and(|g| in_hot_area(g, x, y)),
                |(near, _)| near,
            );
        rail.over = !unfolded
            && forced.map_or_else(
                || rail.geometry.is_some_and(|g| on_open_button(g, x, y)),
                |(_, over)| over,
            );
        rail.hover = FORCED_HOVER.with(|forced| forced.get()).filter(|_| unfolded).or(match target_at(rail, unfolded, x, y) {
            Some(Target::Rail(hit)) => Some(hit),
            _ => None,
        });
        rail.close_hover = (FORCED_CLOSE.with(|forced| forced.get()) && rail.shown.is_some())
            || (unfolded && target_at(rail, unfolded, x, y) == Some(Target::PanelClose));
        rail.opacity = fade(rail.opacity, open_opacity(rail.near, rail.over), 0.2);
        before != (rail.hover, rail.near, rail.over, rail.opacity, rail.close_hover)
    });
    if changed {
        crate::frame::update_appbar(window);
    }
}

unsafe fn install_hook() {
    HOOK.with(|hook| {
        if *hook.borrow() == 0 {
            let installed = SetWindowsHookExW(WH_GETMESSAGE, Some(mouse_hook), std::ptr::null_mut(), GetCurrentThreadId());
            *hook.borrow_mut() = installed as isize;
        }
    });
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam == PM_REMOVE as usize {
        let message = &mut *(lparam as *mut MSG);
        if matches!(message.message, WM_LBUTTONDOWN | WM_LBUTTONUP) && click(message) {
            message.message = WM_NULL;
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Handles a button message that lands on the rail or the open button. Returns true when it did,
/// so the browser never sees it.
unsafe fn click(message: &MSG) -> bool {
    let window = GetAncestor(message.hwnd, GA_ROOT);
    if window.is_null() {
        return false;
    }
    let mut pointer = message.pt;
    ScreenToClient(window, &mut pointer);
    let action = RAILS.with(|rails| {
        let mut rails = rails.borrow_mut();
        let rail = rails.get_mut(&(window as isize))?;
        let unfolded = panel::is_unfolded(window);
        let (x, y) = to_dips(pointer, rail.scale);
        let target = target_at(rail, unfolded, x, y);
        if message.message == WM_LBUTTONDOWN {
            rail.pressed = match target {
                Some(Target::Rail(hit)) => Some(hit),
                _ => None,
            };
            return target.map(|_| None);
        }
        let pressed = rail.pressed.take();
        match target {
            Some(Target::Rail(hit)) => Some((pressed == Some(hit)).then_some(Target::Rail(hit))),
            Some(Target::Open) => Some(Some(Target::Open)),
            Some(Target::PanelClose) => Some(Some(Target::PanelClose)),
            Some(Target::PanelHeader) => Some(None),
            None => None,
        }
    });
    let Some(action) = action else { return false };
    if let Some(target) = action {
        perform(window, target);
    }
    crate::frame::update_appbar(window);
    true
}

/// The entries the rail lists for a window.
fn entries_of(window: HWND) -> Vec<panel::EntryKey> {
    RAILS.with(|rails| rails.borrow().get(&(window as isize)).map(|rail| rail.entries.clone()).unwrap_or_default())
}

/// What a click on a target does.
unsafe fn perform(window: HWND, target: Target) {
    match target {
        Target::Open => panel::set_unfolded(window, true),
        Target::PanelHeader => {}
        Target::PanelClose => {
            for key in entries_of(window) {
                if panel::entry_showing(window, &key) {
                    panel::toggle_entry(window, &key);
                }
            }
        }
        Target::Rail(RailHit::Entry(index)) => {
            if let Some(key) = entries_of(window).get(index) {
                panel::toggle_entry(window, key);
            }
        }
        Target::Rail(RailHit::Add) => {
            if crate::chrome_tabs::new_tab(window) {
                crate::chrome_navigation::open(window, "chrome://extensions");
            }
        }
        Target::Rail(RailHit::Fold) => {
            for key in entries_of(window) {
                if panel::entry_showing(window, &key) {
                    panel::toggle_entry(window, &key);
                }
            }
            panel::set_unfolded(window, false);
        }
    }
}

thread_local! {
    /// A test can show the rail in the tint of a space the window is not in.
    static FORCED_TINT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    /// Entries a test adds to the rail, to see how an extension's entry looks without one.
    static FAKE: RefCell<Vec<EntryLook>> = const { RefCell::new(Vec::new()) };
    /// A test can hold the pointer over the open panel's close button.
    static FORCED_CLOSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A test can hold the pointer over a rail button.
    static FORCED_HOVER: std::cell::Cell<Option<RailHit>> = const { std::cell::Cell::new(None) };
    /// A test can hold the pointer near the corner without a real pointer: (near, over).
    static FORCED_POINTER: std::cell::Cell<Option<(bool, bool)>> = const { std::cell::Cell::new(None) };
}

/// Drives the rail like a click or a pointer would. Used by the test flag file, so a checked
/// build needs neither the real pointer nor focus. Commands: `open`, `fold`, `entry <n>`, `add`,
/// `near`, `over` and `away`.
pub unsafe fn simulate(window: HWND, command: &str) {
    let mut words = command.split_whitespace();
    let target = match (words.next(), words.next().and_then(|n| n.parse::<usize>().ok())) {
        (Some("open"), _) => Some(Target::Open),
        (Some("close"), _) => Some(Target::PanelClose),
        (Some("fold"), _) => Some(Target::Rail(RailHit::Fold)),
        (Some("add"), _) => Some(Target::Rail(RailHit::Add)),
        (Some("entry"), Some(n)) => Some(Target::Rail(RailHit::Entry(n))),
        (Some("near"), _) => {
            FORCED_POINTER.with(|forced| forced.set(Some((true, false))));
            None
        }
        (Some("over"), _) => {
            FORCED_POINTER.with(|forced| forced.set(Some((true, true))));
            None
        }
        (Some("hover"), Some(n)) => {
            FORCED_HOVER.with(|forced| forced.set(Some(RailHit::Entry(n))));
            None
        }
        (Some("hover"), None) => {
            FORCED_CLOSE.with(|forced| forced.set(command.split_whitespace().nth(1) == Some("close")));
            let hit = match words.next() {
                Some("add") => Some(RailHit::Add),
                Some("fold") => Some(RailHit::Fold),
                _ => None,
            };
            FORCED_HOVER.with(|forced| forced.set(hit));
            None
        }
        (Some("tint"), _) => {
            let tint = command.split_whitespace().nth(1).and_then(|hex| u32::from_str_radix(hex, 16).ok());
            FORCED_TINT.with(|forced| forced.set(tint));
            None
        }
        (Some("fake"), _) => {
            let mut words = command.split_whitespace().skip(1);
            let name = words.next().unwrap_or("Fake").to_owned();
            let look = match words.next() {
                Some(path) => Look::Picture(PathBuf::from(path)),
                None => Look::Letter(name.chars().next().unwrap_or('?')),
            };
            FAKE.with(|fake| fake.borrow_mut().push(EntryLook { name, look }));
            None
        }
        (Some("away"), _) => {
            FORCED_CLOSE.with(|forced| forced.set(false));
            FORCED_HOVER.with(|forced| forced.set(None));
            FORCED_POINTER.with(|forced| forced.set(None));
            None
        }
        _ => None,
    };
    if let Some(target) = target {
        perform(window, target);
    }
    crate::frame::update_appbar(window);
}

/// Forgets a window that is going away.
pub fn forget(window: HWND) {
    RAILS.with(|rails| rails.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr10_rail_buttons_are_thirty_dips_two_apart_after_six() {
        assert_eq!(rail_hit(20.0, 20.0, 600.0, 3), Some(RailHit::Entry(0)));
        assert_eq!(rail_hit(20.0, 6.0 + 32.0 + 5.0, 600.0, 3), Some(RailHit::Entry(1)));
        assert_eq!(rail_hit(20.0, 6.0 + 31.0, 600.0, 3), None, "the gap between buttons is not a button");
        assert_eq!(rail_hit(2.0, 20.0, 600.0, 3), None, "the rail's padding is not a button");
    }

    #[test]
    fn fr10_rail_ends_with_a_fold_button_after_the_add_button() {
        assert_eq!(rail_hit(20.0, 600.0 - 6.0 - 15.0, 600.0, 3), Some(RailHit::Fold));
        let add = add_top(3) + 10.0;
        assert_eq!(rail_hit(20.0, add, 600.0, 3), Some(RailHit::Add));
        assert!(add_top(3) > 6.0 + 3.0 * 32.0);
        assert!(add_top(2) < add_top(3), "the add button follows however many entries there are");
    }

    #[test]
    fn fr10_open_button_sits_four_from_the_right_and_six_from_the_bottom() {
        let page = PageGeometry { x: 268, right: 1092, top: 46, bottom: 734 };
        assert_eq!(open_button(page), (1058.0, 698.0));
        assert!(in_hot_area(page, 1091.0, 733.0));
        assert!(!in_hot_area(page, 1019.0, 733.0));
        assert!(on_open_button(page, 1060.0, 700.0));
        assert!(!on_open_button(page, 1050.0, 700.0));
    }

    #[test]
    fn fr10_open_button_is_hidden_far_from_the_corner_faint_near_it_and_solid_over_it() {
        assert_eq!(open_opacity(false, false), 0.0);
        assert_eq!(open_opacity(true, false), 0.6);
        assert_eq!(open_opacity(true, true), 1.0);
    }

    #[test]
    fn fr10_rail_labels_chromes_entries_and_draws_their_glyphs() {
        let mut known = HashMap::new();
        let mut key: panel::EntryKey = [0; 0x28];
        key[..4].copy_from_slice(&1u32.to_le_bytes());
        let look = entry_look(&key, &mut known);
        assert_eq!(look, EntryLook { name: "Bookmarks".into(), look: Look::Glyph('\u{E736}') });
        assert!(known.is_empty(), "Chrome's own entries need no disk");
    }

    #[test]
    fn fr10_label_of_a_hovered_button_sits_beside_the_button_it_names() {
        assert_eq!(hit_top(RailHit::Entry(1), 600.0, 3), 6.0 + 32.0);
        assert_eq!(hit_top(RailHit::Fold, 600.0, 3), 600.0 - 6.0 - 30.0);
        assert!(hit_top(RailHit::Add, 600.0, 3) > hit_top(RailHit::Entry(2), 600.0, 3));
    }

    #[test]
    fn fr10_open_panel_header_is_forty_dips_with_a_close_button_six_from_the_rail() {
        let page = PageGeometry { x: 268, right: 1092, top: 44, bottom: 734 };
        assert_eq!(panel_left(page), 812.0, "the panel is 240 wide and ends at the 40 wide rail");
        assert_eq!(on_header(page, 900.0, 60.0), (true, false));
        assert_eq!(on_header(page, 900.0, 85.0), (false, false), "below the header is the browser's own panel");
        assert_eq!(on_header(page, 811.0, 60.0), (false, false));
        assert_eq!(close_button(page), (1018.0, 44.0 + 5.5));
        assert_eq!(on_header(page, 1030.0, 58.0), (true, true));
        assert_eq!(on_header(page, 1055.0, 58.0), (false, false), "the rail is not the header");
    }

    #[test]
    fn fr10_open_button_fades_in_steps() {
        assert_eq!(fade(0.0, 0.6, 0.2), 0.2);
        assert_eq!(fade(0.5, 0.6, 0.2), 0.6);
        assert!((fade(0.6, 0.0, 0.2) - 0.4).abs() < 1e-6);
    }
}
