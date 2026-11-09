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
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1SolidColorBrush,
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_CAP_STYLE_FLAT, D2D1_DASH_STYLE_DASH,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_LINE_JOIN_MITER, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES,
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
    DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
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

/// Colours of the shell study that no space changes.
mod palette {
    pub const TINT: u32 = rivet_core::WORK_TINT;
    pub const INK: u32 = 0x1f2024;
    pub const SOFT_INK: u32 = 0x4a4850;
    pub const MUTED: u32 = 0x8a8690;
    /// A top row button that has nothing to do, such as back on the first page.
    pub const WHITE: u32 = 0xffffff;
}
use palette::*;

/// The top bar's caret on a visual of its own.
pub(crate) mod caret;
/// The link rules panel, drawn with the appbar's canvas.
mod link_rules;
/// The command bar and the link preview, drawn with the appbar's canvas.
mod overlays;
/// The Rivet settings card, drawn with the appbar's canvas.
mod settings_card;

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
/// The title of the split group in the tab list, as the study's `.group .title`.
const GROUP_TITLE: Type = style("DM Mono", 10.0, 500, 0.08);
const META: Type = style("DM Mono", 10.0, 400, 0.0);
const META_ROW: Type = style("Manrope", 12.0, 400, 0.0);
const HELD_TEXT: Type = style("Manrope", 12.0, 500, 0.0);
const TAB: Type = style("Manrope", 12.5, 400, 0.0);
const ACTIVE: Type = style("Manrope", 12.5, 600, 0.0);
/// The study's `.where b` and `.security`.
const ADDRESS_TITLE: Type = style("Manrope", 12.0, 600, 0.0);
const CHIP_TEXT: Type = style("Manrope", 11.5, 600, 0.0);
/// The address of a page in developer mode: a monospace font every Windows has, its size and the
/// width of one of its characters at that size, in DIPs.
const MONO_FAMILY: &str = "Consolas";
const MONO_SIZE: f32 = 12.0;
const MONO_ADVANCE: f32 = 6.6;
/// The developer ground behind the address, a cool slate.
const DEV_FIELD: u32 = 0x3d5a80;
/// The developer row of the study's `.toolbar.dev`: its dark ground, its lower edge, the icons and
/// the monospaced address on it.
const DEV_GROUND: u32 = 0x2a2c31;
const DEV_EDGE: u32 = 0x17181b;
const DEV_ICON: u32 = 0xc4c7ce;
const DEV_ADDRESS: u32 = 0xaeb2ba;

/// Whether a window's own top row is dark: in developer mode, and not a split pane's row, which is
/// drawn with the pane.
pub(crate) fn row_is_dark(dev: bool, pane: bool) -> bool {
    dev && !pane
}

/// The ground, lower edge and icon ink of the top row: dark while the page is in developer mode,
/// as the browser's own row under the caption buttons and the developer buttons then is.
pub(crate) fn row_colours(dev: bool) -> (u32, u32, u32) {
    if dev {
        (DEV_GROUND, DEV_EDGE, DEV_ICON)
    } else {
        (PAPER, HAIRLINE, SOFT_INK)
    }
}
const FAVICON: Type = style("Manrope", 9.0, 700, 0.0);
/// Interface icons come from the font Windows 11 draws its own caption buttons with.
const ICONS: &str = "Segoe Fluent Icons";
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

/// The ground of a page's row from `x` for `width`, `height` tall from `y`, as rectangles and their
/// colours. `hole` is the browser's own address as (left from `x`, width), which shows through.
///
/// The ground is one run of paper to the right end, caption buttons included. Two fills that meet
/// at a fractional pixel each cover that pixel only in part, so the browser's own frame showed
/// through between them as a thin line. The band just above the lower edge is also covered under
/// the hole, where the browser draws the line under its own toolbar.
fn pane_row_ground(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    hole: Option<(f32, f32)>,
) -> Vec<(D2D_RECT_F, u32)> {
    let edge = rect(x, y + height - 1.0, width, 1.0);
    let Some((left, room)) = hole else {
        return vec![(rect(x, y, width, height), PAPER), (edge, HAIRLINE)];
    };
    let after = left + room;
    vec![
        (rect(x, y, left, height), PAPER),
        (rect(x + after, y, width - after, height), PAPER),
        (
            rect(
                x + left,
                y + height - 1.0 - TOOLBAR_LINE,
                room,
                TOOLBAR_LINE,
            ),
            PAPER,
        ),
        (edge, HAIRLINE),
    ]
}

/// How much of the row above its lower edge is kept from the browser's own toolbar line.
const TOOLBAR_LINE: f32 = 2.0;

/// The held build warning is 28 DIPs tall and at most 660 wide, 8 from the page's edges.
const HELD_HEIGHT: f32 = 28.0;
const HELD_WIDTH: f32 = 660.0;
const HELD_MARGIN: f32 = 8.0;
/// How far above the page area's bottom the warning reaches with a margin above it, in DIPs.
/// Rivet's surfaces that float over the page bottom keep clear of it while the warning shows.
pub(crate) const HELD_RESERVE: i32 = (HELD_MARGIN + HELD_HEIGHT + HELD_MARGIN) as i32;

/// Where the held build warning goes, as (left, top, width) in window DIPs: centred at the bottom of
/// `page` (left, top, right, bottom), and kept left of the open button zone `open_zone` by the same
/// margin, so the warning never covers the corner where the Sidebar's open button shows. `None`
/// when no room is left.
fn held_place(page: [f32; 4], open_zone: Option<[f32; 4]>) -> Option<[f32; 3]> {
    let [page_left, _, page_right, page_bottom] = page;
    let top = page_bottom - HELD_MARGIN - HELD_HEIGHT;
    let low = page_left + HELD_MARGIN;
    let mut high = page_right - HELD_MARGIN;
    if let Some([zone_left, zone_top, _, zone_bottom]) = open_zone {
        if zone_top < page_bottom - HELD_MARGIN && zone_bottom > top {
            high = high.min(zone_left - HELD_MARGIN);
        }
    }
    let width = HELD_WIDTH.min(high - low);
    if width <= 0.0 {
        return None;
    }
    let centre = (page_left + page_right) / 2.0;
    let left = (centre - width / 2.0).clamp(low, high - width);
    Some([left, top, width])
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
    root: IDCompositionVisual2,
    visual: IDCompositionVisual2,
    painted: Option<Painted>,
    /// The ground of the whole window, with a hole for each page, and what it was last painted for.
    ground: IDCompositionVisual2,
    ground_painted: Option<(i32, i32, u32, u32, Vec<ground::Hole>)>,
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
    toggle_painted: Option<(i32, u32, bool, i32, i32, bool)>,
    /// Whether the browser's top row under the toggle is dark, as in developer mode.
    dark_row: bool,
    peek: IDCompositionVisual2,
    peek_painted: Option<Painted>,
    /// The drop target and the dragged tab's ghost, which exist only while a tab is dragged.
    drag: Option<DragVisuals>,
    /// The top rows of the pages of a split, which exist while the window shows one.
    rows: Vec<(IDCompositionVisual2, Option<(PaneRow, u32)>)>,
    /// The developer bar beside the caption buttons, and what it was last painted for.
    dev: IDCompositionVisual2,
    dev_painted: Option<(crate::dev_bar::View, u32)>,
    /// The warning of a window opened from a held browser build, and what it was last painted for.
    held: IDCompositionVisual2,
    held_painted: Option<(String, i32, u32)>,
    /// The top bar of a window with a single page, and what it was last painted for.
    address: IDCompositionVisual2,
    address_painted: Option<(crate::features::address_bar::View, u32)>,
    /// The top bar's caret, inside the top bar's visual.
    address_caret: caret::AddressCaret,
    /// The top bar's surface and its size in pixels, drawn over again while the size stays.
    address_surface: Option<(IDCompositionSurface, u32, u32)>,
    /// The command bar over the page, and what it was last painted for.
    command: IDCompositionVisual2,
    command_painted: Option<(crate::features::command_bar::View, u32)>,
    /// The command bar's caret, inside the command bar's visual.
    command_caret: caret::AddressCaret,
    /// The Rivet settings card over the page, and what it was last painted for.
    settings: IDCompositionVisual2,
    settings_painted: Option<(crate::features::settings::View, u32)>,
    /// The settings card's caret, inside the card's visual.
    settings_caret: caret::AddressCaret,
    /// The header and shadow of a link preview, and what they were last painted for.
    preview: IDCompositionVisual2,
    preview_painted: Option<(crate::features::link_preview::View, u32)>,
    /// The link rules panel over the page, and what it was last painted for.
    rules: IDCompositionVisual2,
    rules_painted: Option<(crate::features::link_rules::View, u32)>,
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
    /// Whether the browser's own address bar sits in this row, which is the focused page's row when
    /// that is the topmost one.
    pub editable: bool,
    pub first: bool,
    pub last: bool,
    pub title: String,
    pub url: String,
    pub hover: Option<crate::features::split::RowButton>,
    /// Whether back and forward can go anywhere. Only a page that is no tab says otherwise: a tab's
    /// buttons are the browser's.
    pub can_back: bool,
    pub can_forward: bool,
    /// Whether the address is only shown, so clicking it does nothing.
    pub read_only: bool,
    /// The caption button under the pointer on the row that carries them, and whether the window
    /// is maximized.
    pub caption_hover: Option<crate::features::address_bar::Caption>,
    pub zoomed: bool,
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
fn floating_fill(frame: u32) -> u32 {
    let mix = |shift: u32| {
        let channel = ((frame >> shift) & 0xff) as f32;
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
        let toggle = device.CreateVisual()?;
        root.AddVisual(&toggle, true, &visual)?;
        // The floating appbar covers the toggle, as it covers the rest of the top row.
        let peek = device.CreateVisual()?;
        root.AddVisual(&peek, true, &toggle)?;
        let dev = device.CreateVisual()?;
        root.AddVisual(&dev, true, &peek)?;
        let held = device.CreateVisual()?;
        root.AddVisual(&held, true, &dev)?;
        // The top bar lies under the toggle and the floating appbar, like the browser row it replaces.
        let address = device.CreateVisual()?;
        root.AddVisual(&address, true, &ground)?;
        let address_caret = caret::AddressCaret::new(&device, &address)?;
        // The link preview and the command bar lie over everything else of the window.
        let preview = device.CreateVisual()?;
        root.AddVisual(&preview, true, None::<&IDCompositionVisual>)?;
        let command = device.CreateVisual()?;
        root.AddVisual(&command, true, None::<&IDCompositionVisual>)?;
        let settings = device.CreateVisual()?;
        root.AddVisual(&settings, true, None::<&IDCompositionVisual>)?;
        let command_caret = caret::AddressCaret::new(&device, &command)?;
        let settings_caret = caret::AddressCaret::new(&device, &settings)?;
        let rules = device.CreateVisual()?;
        root.AddVisual(&rules, true, None::<&IDCompositionVisual>)?;
        target.SetRoot(&root)?;
        let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = load_fonts(&dwrite)?;
        Ok(Self {
            device,
            dwrite,
            fonts,
            _target: target,
            root,
            visual,
            painted: None,
            ground,
            ground_painted: None,
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
            dark_row: false,
            peek,
            peek_painted: None,
            drag: None,
            rows: Vec::new(),
            dev,
            dev_painted: None,
            held,
            held_painted: None,
            address,
            address_painted: None,
            address_caret,
            address_surface: None,
            command,
            command_painted: None,
            command_caret,
            settings,
            settings_painted: None,
            settings_caret,
            preview,
            preview_painted: None,
            rules,
            rules_painted: None,
        })
    }

    /// The composition device, the root of the visual tree and the text factory, for other parts
    /// of the window's surface such as the Sidebar's rail.
    pub(crate) fn composition(
        &self,
    ) -> (
        IDCompositionDesktopDevice,
        IDCompositionVisual2,
        IDWriteFactory5,
    ) {
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
            self.dark_row,
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
            let dark = self.dark_row;
            let result = canvas.pressed(area, 8.0, 0.96, effect, || {
                if hot || effect.is_some() {
                    canvas.fill_rounded(
                        area,
                        8.0,
                        if dark {
                            rgba(WHITE, 0.09)
                        } else {
                            rgba(INK, 0.06)
                        },
                    )?;
                }
                // The design's glyph sits above where DirectWrite centres it, and darkens to the
                // ink colour on hover like the other icons of the row.
                let (w, h) = (area.right - area.left, area.bottom - area.top);
                let glyph = rect(area.left, area.top - 0.6, w, h);
                let colour = match (dark, hot || effect.is_some()) {
                    (true, true) => WHITE,
                    (true, false) => 0xc4c7ce,
                    (false, true) => INK,
                    (false, false) => SOFT_INK,
                };
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
                let frame = rivet_core::space_frame(self.scene().tint());
                canvas.floating(rect(PEEK_PAD, PEEK_PAD, side as f32, height as f32), frame)?;
                context.SetTransform(&(Matrix3x2::translation(PEEK_PAD, PEEK_PAD) * origin));
                canvas.draw(side as f32, height as f32, &self.scene(), Surface::Floating)
            })();
            surface.EndDraw()?;
            result?;
            self.peek.SetContent(&surface)?;
            self.peek_painted = Some(Painted {
                width: pw,
                height: ph,
                dpi,
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
            self.painted = Some(Painted { width, height, dpi });
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
        let result = ground::paint(
            &context,
            width as f32,
            height as f32,
            dpi as f32 / 96.0,
            tint,
            holes,
        );
        surface.EndDraw()?;
        result?;
        self.ground.SetContent(&surface)?;
        self.ground_painted = Some(key);
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
const HELD_GROUND: u32 = 0xfdeab8;
const HELD_INK: u32 = 0x4a3200;
const HAIRLINE: u32 = 0xe9e9e6;

impl Appbar {
    /// Tells the toggle whether the row under it is dark.
    pub(crate) fn set_dark_row(&mut self, dark: bool) {
        self.dark_row = dark;
    }

    /// Shows the developer bar, or removes it when `bar` is none. It is transparent: the buttons lie
    /// on the browser's own dark top row.
    pub(crate) unsafe fn update_dev(
        &mut self,
        bar: Option<&crate::dev_bar::View>,
        dpi: u32,
    ) -> Result<()> {
        let Some(bar) = bar else {
            if self.dev_painted.take().is_some() {
                self.dev.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        self.dev.SetOffsetX2(bar.x.round())?;
        self.dev.SetOffsetY2(bar.y.round())?;
        if self.dev_painted.as_ref() == Some(&(bar.clone(), dpi)) {
            return self.device.Commit();
        }
        let (width, height) = (bar.width.round() as i32, bar.height.round() as i32);
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let scale = dpi as f32 / 96.0;
        let surface = self.device.CreateSurface(
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
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let h = height as f32 / scale;
        let w = width as f32 / scale;
        let mut result = Ok(());
        // The stripes of the study's `.toolbar.dev`: 9 DIP bands of a slightly lighter ground every
        // 18 DIPs, running up to the right. They are faint white over the browser's own dark row.
        result = result.and_then(|_| {
            let brush = canvas.paint_brush(rgba(WHITE, 0.035))?;
            let step = 18.0 * std::f32::consts::SQRT_2;
            let mut x = 13.5 * std::f32::consts::SQRT_2 - step;
            while x < w + h {
                context.DrawLine(
                    windows_numerics::Vector2 { X: x, Y: h },
                    windows_numerics::Vector2 { X: x + h, Y: 0.0 },
                    &brush,
                    9.0,
                    None,
                );
                x += step;
            }
            Ok(())
        });
        let origin = bar.bar_x / scale;
        for (index, glyph) in bar.glyphs.iter().enumerate() {
            let area = rect(
                origin + crate::dev_mode::LEAD + index as f32 * crate::dev_mode::BUTTON,
                (h - 28.0) / 2.0,
                crate::dev_mode::BUTTON,
                28.0,
            );
            let hot = bar.hover == Some(index);
            result = result.and_then(|_| {
                if hot {
                    canvas.fill_rounded(area, 8.0, rgba(WHITE, 0.09))?;
                }
                canvas.icon(*glyph, area, 14.0, if hot { WHITE } else { 0xc4c7ce })
            });
        }
        surface.EndDraw()?;
        result?;
        self.dev.SetContent(&surface)?;
        self.dev_painted = Some((bar.clone(), dpi));
        self.device.Commit()
    }

    /// Shows why this window's browser is older than the installed one, at the bottom centre of the
    /// page area `page` (left, top, right, bottom in window DIPs), clear of the Sidebar's open button
    /// zone `open_zone` at the page's lower right corner. Pointer input still reaches the page below,
    /// since the visual takes none.
    pub(crate) unsafe fn update_held(
        &mut self,
        warning: Option<&str>,
        page: [f32; 4],
        open_zone: Option<[f32; 4]>,
        dpi: u32,
    ) -> Result<()> {
        let Some(warning) = warning else {
            return Ok(());
        };
        let scale = dpi as f32 / 96.0;
        let Some([left, top, room]) = held_place(page, open_zone) else {
            return Ok(());
        };
        let width = (room * scale).round() as i32;
        let height = (HELD_HEIGHT * scale).round() as i32;
        if width <= 0 {
            return Ok(());
        }
        self.held.SetOffsetX2((left * scale).round())?;
        self.held.SetOffsetY2((top * scale).round())?;
        crate::trace::trace_changed(
            "held warning",
            &format!(
                "held warning at {} {} {} {}",
                (left * scale).round(),
                (top * scale).round(),
                width,
                height
            ),
        );
        let key = (warning.to_owned(), width, dpi);
        if self.held_painted.as_ref() == Some(&key) {
            return self.device.Commit();
        }
        let surface = self.device.CreateSurface(
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
        let canvas = Canvas {
            context: &context,
            dwrite: &self.dwrite,
            fonts: &self.fonts,
        };
        let w = width as f32 / scale;
        let result = canvas
            .fill_rounded(rect(0.0, 0.0, w, HELD_HEIGHT), 8.0, rgba(HELD_GROUND, 1.0))
            .and_then(|()| {
                canvas.text(
                    HELD_TEXT,
                    rect(12.0, 0.0, w - 24.0, HELD_HEIGHT),
                    warning,
                    HELD_INK,
                    false,
                )
            });
        surface.EndDraw()?;
        result?;
        self.held.SetContent(&surface)?;
        self.held_painted = Some(key);
        self.device.Commit()
    }

    /// Shows the top bar of a window with a single page: back, forward, reload, the security chip and
    /// the address, as the study's `.toolbar`. While the address is edited, the text engine's pixels
    /// take the place of the text. `None` removes the bar.
    pub(crate) unsafe fn update_address(
        &mut self,
        window: isize,
        bar: Option<&crate::features::address_bar::View>,
        dpi: u32,
    ) -> Result<()> {
        use crate::features::address_bar::{Part, Security, CHIP, FIELD_GROUND};
        let Some(bar) = bar else {
            if self.address_painted.take().is_some() {
                self.hide_address_caret()?;
                self.address_surface = None;
                self.address.SetContent(None)?;
                return self.device.Commit();
            }
            return Ok(());
        };
        self.address.SetOffsetX2(bar.x.round())?;
        self.address.SetOffsetY2(bar.y.round())?;
        // The caret is a visual of its own, so a blink or a caret move paints nothing here. The
        // field's text and selection count in the view's generation.
        self.address_caret
            .set_origin((bar.x.round(), bar.y.round()));
        let caret = if bar.editing {
            crate::features::address_bar::caret(window as _)
        } else {
            None
        };
        self.place_address_caret(caret, dpi, false)?;
        if !caret::address_needs_paint(self.address_painted.as_ref(), bar, dpi) {
            return self.device.Commit();
        }
        crate::features::activity::count(crate::features::activity::Counter::AddressPaint);
        let (width, height) = (bar.width.round() as i32, bar.height.round() as i32);
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let scale = dpi as f32 / 96.0;
        use crate::features::address_bar::{SUGGESTION_PAD, SUGGESTION_ROW};
        // The list of suggestions hangs below the bar, over the page.
        let list_h = if bar.editing && !bar.suggestions.is_empty() {
            SUGGESTION_PAD * 2.0 + SUGGESTION_ROW * bar.suggestions.len() as f32
        } else {
            0.0
        };
        let total = height + ((list_h + 8.0) * scale).ceil() as i32 * (list_h > 0.0) as i32;
        // The surface is kept while the bar keeps its size, and drawn over again in place.
        let reused = self
            .address_surface
            .as_ref()
            .filter(|(_, w, h)| (*w, *h) == (width as u32, total as u32))
            .map(|(surface, _, _)| surface.clone());
        let fresh = reused.is_none();
        let surface = match reused {
            Some(surface) => surface,
            None => {
                let surface = self.device.CreateSurface(
                    width as u32,
                    total as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.address_surface = Some((surface.clone(), width as u32, total as u32));
                surface
            }
        };
        let mut offset = POINT::default();
        let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
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
        let (w, h) = (width as f32 / scale, height as f32 / scale);
        let draw = || -> Result<()> {
            let dark = row_is_dark(bar.dev, bar.pane);
            let (ground, edge, icon_ink) = row_colours(dark);
            if bar.pane {
                // Only the address of a pane's row: the row around it is drawn with the row.
                canvas.fill(rect(0.0, 0.0, w, h - 1.0), PAPER)?;
            } else {
                // The panel's rounded top corners, then a square lower half and the hairline.
                canvas.fill_rounded(rect(0.0, 0.0, w, h), PANEL_RADIUS, rgba(ground, 1.0))?;
                canvas.fill(rect(0.0, h / 2.0, w, h / 2.0), ground)?;
                canvas.fill(rect(0.0, h - 1.0, w, 1.0), edge)?;
            }
            for (part, x, pw) in
                crate::features::address_bar::parts_reserving(w, bar.pane, bar.reserve)
            {
                let area = rect(x, (h - 28.0) / 2.0, pw, 28.0);
                let hot = bar.hover == Some(part);
                match part {
                    Part::Back | Part::Forward | Part::Reload => {
                        if hot {
                            canvas.fill_rounded(area, 8.0, rgba(INK, 0.06))?;
                        }
                        let (glyph, ink) = match part {
                            Part::Back => {
                                ('\u{E72B}', if bar.can_back { icon_ink } else { 0xb9b6bb })
                            }
                            Part::Forward => (
                                '\u{E72A}',
                                if !bar.can_forward {
                                    0xd3d1d6
                                } else if dark {
                                    icon_ink
                                } else {
                                    MUTED
                                },
                            ),
                            _ => ('\u{E72C}', icon_ink),
                        };
                        canvas.icon(glyph, area, 16.0, ink)?;
                    }
                    Part::Chip => {}
                    Part::Address => {
                        if bar.editing {
                            canvas.fill_rounded(area, 8.0, rgba(FIELD_GROUND, 1.0))?;
                        } else if bar.dev {
                            // The developer row's own ground, so a local page is told apart at a glance.
                            canvas.fill_rounded(
                                area,
                                8.0,
                                rgba(DEV_FIELD, if hot { 0.28 } else { 0.18 }),
                            )?;
                        } else if hot || bar.hover == Some(Part::Chip) {
                            canvas.fill_rounded(area, 8.0, rgba(INK, 0.05))?;
                        }
                        let (glyph, words) = bar.security.look();
                        let insecure = bar.security == Security::NotSecure;
                        let words = if bar.editing { "" } else { words };
                        let chip_w = if words.is_empty() {
                            CHIP
                        } else {
                            CHIP + 4.0 + measure(&self.dwrite, &self.fonts, CHIP_TEXT, words)?
                        };
                        let chip = rect(x + 6.0, (h - 22.0) / 2.0, chip_w, 22.0);
                        let ink = if insecure { 0xb3261e } else { icon_ink };
                        if insecure && !bar.editing {
                            canvas.fill_rounded(chip, 6.0, rgba(0xb3261e, 0.08))?;
                        }
                        if bar.hover == Some(Part::Chip) {
                            canvas.fill_rounded(chip, 6.0, rgba(INK, 0.06))?;
                        }
                        canvas.icon(glyph, rect(chip.left, chip.top, CHIP, 22.0), 13.0, ink)?;
                        if !words.is_empty() {
                            canvas.text(
                                CHIP_TEXT,
                                rect(chip.left + CHIP - 2.0, chip.top, chip_w - CHIP, 22.0),
                                words,
                                ink,
                                false,
                            )?;
                        }
                        let left = chip.right + 4.0;
                        let room = (area.right - 10.0 - left).max(0.0);
                        if !bar.editing && bar.dev {
                            // Developer mode: the whole address in a monospace system font.
                            let shown = crate::dev_mode::fit_address(&bar.url, room, MONO_ADVANCE);
                            canvas.mono_text(
                                rect(left, area.top, room, 28.0),
                                &shown,
                                DEV_ADDRESS,
                            )?;
                        } else if !bar.editing {
                            let title_w = if bar.title.is_empty() {
                                0.0
                            } else {
                                measure(&self.dwrite, &self.fonts, ADDRESS_TITLE, &bar.title)?
                                    .min(room)
                            };
                            if title_w > 0.0 {
                                let title = elide(
                                    &self.dwrite,
                                    &self.fonts,
                                    ADDRESS_TITLE,
                                    &bar.title,
                                    room,
                                )?;
                                canvas.text(
                                    ADDRESS_TITLE,
                                    rect(left, area.top, title_w, 28.0),
                                    &title,
                                    SOFT_INK,
                                    false,
                                )?;
                            }
                            let rest_room = room - title_w;
                            if rest_room > 8.0 {
                                let rest = elide(
                                    &self.dwrite,
                                    &self.fonts,
                                    META_ROW,
                                    &bar.rest,
                                    rest_room,
                                )?;
                                canvas.text(
                                    META_ROW,
                                    rect(left + title_w, area.top, rest_room, 28.0),
                                    &rest,
                                    MUTED,
                                    false,
                                )?;
                            }
                        }
                    }
                }
            }
            if !bar.pane {
                // The glyphs follow the ground drawn under them in this same paint, so they never stay
                // white on a light row after the page leaves developer mode.
                canvas.caption_buttons(w, 0.0, h, bar.caption_hover, bar.zoomed, dark)?;
            }
            if list_h > 0.0 {
                if let Some((left, lw)) = crate::features::address_bar::dropdown_of(w, bar.pane) {
                    let list = rect(left, h + 2.0, lw, list_h);
                    canvas.fill_rounded(
                        D2D_RECT_F {
                            top: list.top + 2.0,
                            bottom: list.bottom + 3.0,
                            ..list
                        },
                        10.0,
                        rgba(0x28_1c_1e, 0.08),
                    )?;
                    canvas.fill_rounded(list, 10.0, rgba(WHITE, 1.0))?;
                    canvas.ring(list, 10.0, rgba(INK, 0.08))?;
                    for (index, (contents, description, search)) in
                        bar.suggestions.iter().enumerate()
                    {
                        let row = rect(
                            left + 4.0,
                            list.top + SUGGESTION_PAD + index as f32 * SUGGESTION_ROW,
                            lw - 8.0,
                            SUGGESTION_ROW,
                        );
                        if bar.selected == Some(index) {
                            canvas.fill_rounded(row, 7.0, rgba(INK, 0.06))?;
                        }
                        // A page the browser knows has a globe, a search a magnifier.
                        let glyph = if *search { '\u{E721}' } else { '\u{E774}' };
                        canvas.icon(
                            glyph,
                            rect(row.left + 4.0, row.top, 24.0, SUGGESTION_ROW),
                            13.0,
                            MUTED,
                        )?;
                        let room = (row.right - row.left - 40.0).max(0.0);
                        let shown =
                            elide(&self.dwrite, &self.fonts, META_ROW, contents, room * 0.6)?;
                        let used = measure(&self.dwrite, &self.fonts, META_ROW, &shown)?;
                        canvas.text(
                            META_ROW,
                            rect(row.left + 32.0, row.top, used, SUGGESTION_ROW),
                            &shown,
                            INK,
                            false,
                        )?;
                        if !description.is_empty() {
                            let rest = format!(" \u{b7} {description}");
                            let rest =
                                elide(&self.dwrite, &self.fonts, META_ROW, &rest, room - used)?;
                            canvas.text(
                                META_ROW,
                                rect(row.left + 32.0 + used, row.top, room - used, SUGGESTION_ROW),
                                &rest,
                                MUTED,
                                false,
                            )?;
                        }
                    }
                }
            }
            if bar.editing {
                let origin = (bar.x, bar.y);
                crate::features::address_bar::with_field_pixels(window as _, |place, bytes, pw, ph, _caret| -> Result<()> {
                    let properties = windows::Win32::Graphics::Direct2D::D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: windows::Win32::Graphics::Direct2D::Common::D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE,
                        },
                        dpiX: dpi as f32,
                        dpiY: dpi as f32,
                        ..Default::default()
                    };
                    let size = windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U { width: pw as u32, height: ph as u32 };
                    let bitmap = context.CreateBitmap(size, Some(bytes.as_ptr().cast()), pw as u32 * 4, &properties)?;
                    let left = (place.left as f32 - origin.0) / scale;
                    let top = (place.top as f32 - origin.1) / scale;
                    let dest = rect(left, top, pw as f32 / scale, ph as f32 / scale);
                    context.DrawBitmap(
                        &bitmap,
                        Some(&dest),
                        1.0,
                        windows::Win32::Graphics::Direct2D::D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                        None,
                        None,
                    );
                    Ok(())
                })
                .unwrap_or(Ok(()))?;
            }
            Ok(())
        };
        let result = draw();
        surface.EndDraw()?;
        result?;
        if fresh {
            self.address.SetContent(&surface)?;
        }
        self.address_painted = Some((bar.clone(), dpi));
        self.device.Commit()
    }

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
        let (device, dwrite, fonts) =
            (self.device.clone(), self.dwrite.clone(), self.fonts.clone());
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
                (row.reach_left + row.width + row.reach_right).round() as i32,
                (row.y + row.height).round() as i32,
            );
            if painted.as_ref() == Some(&(row.clone(), dpi)) || width <= 0 || height <= 0 {
                continue;
            }
            crate::trace::trace(&format!(
                "page row {index}: x {} y {} width {} height {} caption {} editable {} title {:?}",
                row.x, row.y, row.width, row.height, row.caption, row.editable, row.title
            ));
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
            let canvas = Canvas {
                context: &context,
                dwrite: &dwrite,
                fonts: &fonts,
            };
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
            self.root
                .AddVisual(&zone, true, None::<&IDCompositionVisual>)?;
            let ghost = self.device.CreateVisual()?;
            self.root
                .AddVisual(&ghost, true, None::<&IDCompositionVisual>)?;
            self.drag = Some(DragVisuals {
                zone,
                zone_key: None,
                ghost,
                ghost_key: None,
            });
        }
        let scale = dpi as f32 / 96.0;
        let tint = self.scene().tint();
        let (device, dwrite, fonts) =
            (self.device.clone(), self.dwrite.clone(), self.fonts.clone());
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
                    let canvas = Canvas {
                        context: &context,
                        dwrite: &dwrite,
                        fonts: &fonts,
                    };
                    let result = canvas.drop_target(
                        width as f32 / scale,
                        height as f32 / scale,
                        &zone.label,
                        tint,
                    );
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
                drag.ghost
                    .SetOffsetX2((ghost.x - GHOST_PAD * scale).round())?;
                drag.ghost
                    .SetOffsetY2((ghost.y - GHOST_PAD * scale).round())?;
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
                    let canvas = Canvas {
                        context: &context,
                        dwrite: &dwrite,
                        fonts: &fonts,
                    };
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

    /// Draws one line in the system's monospace font, vertically centred in a box.
    unsafe fn mono_text(&self, area: D2D_RECT_F, value: &str, rgb: u32) -> Result<()> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from(MONO_FAMILY),
            // The system's own fonts: Rivet's collection holds only the fonts it brings.
            None::<&windows::Win32::Graphics::DirectWrite::IDWriteFontCollection>,
            DWRITE_FONT_WEIGHT(400),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            MONO_SIZE,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let wide: Vec<u16> = value.encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(
            &wide,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
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
    unsafe fn draw(&self, w: f32, h: f32, scene: &Scene, surface: Surface) -> Result<()> {
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
        let badge_text = space.badge_in(crate::installed_browsers::sole_label());
        let badge_width = 26.0 + 6.0 * badge_text.chars().count() as f32;
        let badge = rect(w - 8.0 - 8.0 - badge_width, 111.0, badge_width, 20.0);
        self.fill_rounded(badge, 6.0, rgba(WHITE, 0.5))?;
        self.dot(badge.left + 7.0, 118.0, 6.0, rivet_core::space_accent(tint))?;
        self.text(
            BADGE,
            rect(badge.left + 18.0, 111.0, badge_width - 22.0, 20.0),
            &badge_text,
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
                        if group.count > 0 {
                            self.text(
                                TAB,
                                rect(left + inner - 50.0, top, 22.0, 34.0),
                                &group.count.to_string(),
                                MUTED,
                                false,
                            )?;
                        }
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
                TabRow::SplitTitle { count } => {
                    // The group's glass reaches over its title and the tabs that follow, as the
                    // study's `.group`.
                    let tall = crate::tab_rows::SPLIT_TITLE_HEIGHT + 34.0 * *count as f32;
                    let glass = rect(left - 4.0, top - 2.0, inner + 8.0, tall + 6.0);
                    self.fill_rounded(glass, 12.0, rgba(WHITE, 0.35))?;
                    self.ring(glass, 12.0, rgba(INK, 0.06))?;
                    let height = crate::tab_rows::SPLIT_TITLE_HEIGHT;
                    self.icon('\u{ECA5}', rect(left + 8.0, top, 12.0, height), 12.0, MUTED)?;
                    self.text(
                        GROUP_TITLE,
                        rect(left + 26.0, top, inner - 34.0, height),
                        "SPLIT VIEW",
                        MUTED,
                        false,
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
            top += crate::tab_rows::height(item);
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
                            &info.badge_in(crate::installed_browsers::sole_label()),
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
                    let Some(hit) = crate::frame::tab_item_hit(row, tab) else {
                        continue;
                    };
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
                let provider = crate::frame::saved_credentials();
                for (index, (title, hit)) in crate::frame::SETTINGS_ITEMS.iter().enumerate() {
                    let title = crate::frame::settings_title(title, *hit, provider);
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
                            &title,
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
        let accent = rivet_core::space_accent(tint);
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
        let outline = D2D1_ROUNDED_RECT {
            rect: area,
            radiusX: PANEL_RADIUS,
            radiusY: PANEL_RADIUS,
        };
        self.context.DrawRoundedRectangle(
            &outline,
            &self.paint_brush(rgba(accent, 0.7))?,
            1.5,
            &dashes,
        );
        let text = measure(self.dwrite, self.fonts, ACTIVE, label)?;
        let (pill_w, pill_h) = (text + 24.0, 30.0);
        let pill = rect((w - pill_w) / 2.0, (h - pill_h) / 2.0, pill_w, pill_h);
        self.fill_rounded(
            D2D_RECT_F {
                top: pill.top + 3.0,
                bottom: pill.bottom + 3.0,
                ..pill
            },
            9.0,
            rgba(0x28_1c_1e, 0.08),
        )?;
        self.fill_rounded(pill, 9.0, rgba(WHITE, 0.9))?;
        self.text_centred(ACTIVE, pill, label, accent)
    }

    /// The top row of one page of a split, as the shell study's `.toolbar`: grip, back, forward,
    /// reload, the address, then the buttons that move the page to a Mini Window or end the split.
    /// The active page's row is the browser's, so only the grip and those buttons are drawn on it.
    unsafe fn pane_row(&self, row: &PaneRow, vw: f32, vh: f32, scale: f32) -> Result<()> {
        // Every page of a split is its own rounded panel, so the row's top corners follow the
        // panel's radius and the ground shows in the gap between pages.
        let (ox, oy) = (row.reach_left / scale, row.y / scale);
        let clip = rect(
            ox,
            oy,
            vw - ox - row.reach_right / scale,
            (vh - oy) + PANEL_RADIUS,
        );
        self.with_rounded_clip(clip, PANEL_RADIUS, || {
            self.pane_row_inner(row, vw, vh, scale)
        })
    }

    /// Runs `draw` with everything outside a rounded rectangle masked away.
    unsafe fn with_rounded_clip(
        &self,
        area: D2D_RECT_F,
        radius: f32,
        draw: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        use std::mem::ManuallyDrop;
        use windows::Win32::Graphics::Direct2D::{
            ID2D1Geometry, ID2D1Layer, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_LAYER_OPTIONS1_NONE,
            D2D1_LAYER_PARAMETERS1,
        };
        let factory = self.context.GetFactory()?;
        let shape = D2D1_ROUNDED_RECT {
            rect: area,
            radiusX: radius,
            radiusY: radius,
        };
        let geometry: ID2D1Geometry = factory.CreateRoundedRectangleGeometry(&shape)?.cast()?;
        let mut layer = D2D1_LAYER_PARAMETERS1 {
            // The layer only needs the rounded area; `rect` takes a width and height, not a far corner.
            contentBounds: area,
            geometricMask: ManuallyDrop::new(Some(geometry)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: 1.0,
            opacityBrush: ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        self.context.PushLayer(&layer, None::<&ID2D1Layer>);
        ManuallyDrop::drop(&mut layer.geometricMask);
        let result = draw();
        self.context.PopLayer();
        result
    }

    unsafe fn pane_row_inner(&self, row: &PaneRow, vw: f32, vh: f32, scale: f32) -> Result<()> {
        use crate::features::split::{row_buttons, RowButton};
        // The visual holds the row at (ox, oy), with the frame above it and the gap beside it.
        // The native caption buttons keep the right end. The browser's own header shows above and
        // below them, so those bands are covered, and the middle stays bare.
        let caption = row.caption / scale;
        let full = vw;
        let vw = vw - caption;
        let (ox, oy) = (row.reach_left / scale, row.y / scale);
        let w = row.width / scale;
        let h = vh - oy;
        self.fill(rect(0.0, oy, ox, h), PAPER)?;
        self.fill(
            rect(vw - row.reach_right / scale, oy, row.reach_right / scale, h),
            PAPER,
        )?;
        let buttons = row_buttons(w, row.first, row.caption / scale);
        // The focused page's address is the browser's own address bar, which shows through a hole in
        // the row. Everything else on every row is drawn here.
        let hole = buttons
            .iter()
            .find(|b| b.0 == RowButton::Address && row.editable)
            .map(|b| (b.1, b.2));
        for (area, colour) in pane_row_ground(ox, oy, full - ox, h, hole) {
            self.fill(area, colour)?;
        }
        if caption > 0.0 {
            // Rivet draws the caption buttons on the row of the page at the window's top right.
            // The window buttons fade with the rest of a row whose page is not focused.
            self.with_opacity(crate::features::split::row_ink_opacity(row.active), || {
                self.caption_buttons(full, oy, h, row.caption_hover, row.zoomed, false)
            })?;
        }
        self.with_opacity(crate::features::split::row_ink_opacity(row.active), || {
            self.pane_row_buttons(row, &buttons, ox, oy, h)
        })
    }

    /// Runs `draw` with everything it draws faded to `opacity`.
    unsafe fn with_opacity(&self, opacity: f32, draw: impl FnOnce() -> Result<()>) -> Result<()> {
        use std::mem::ManuallyDrop;
        use windows::Win32::Graphics::Direct2D::{
            ID2D1Layer, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_LAYER_OPTIONS1_NONE,
            D2D1_LAYER_PARAMETERS1,
        };
        if opacity >= 1.0 {
            return draw();
        }
        let layer = D2D1_LAYER_PARAMETERS1 {
            contentBounds: D2D_RECT_F {
                left: -1.0e6,
                top: -1.0e6,
                right: 1.0e6,
                bottom: 1.0e6,
            },
            geometricMask: ManuallyDrop::new(None),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity,
            opacityBrush: ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        self.context.PushLayer(&layer, None::<&ID2D1Layer>);
        let result = draw();
        self.context.PopLayer();
        result
    }

    /// The buttons and the address of a page's row.
    unsafe fn pane_row_buttons(
        &self,
        row: &PaneRow,
        buttons: &[(crate::features::split::RowButton, f32, f32)],
        ox: f32,
        oy: f32,
        h: f32,
    ) -> Result<()> {
        use crate::features::split::RowButton;
        for &(button, x, width) in buttons {
            let area = rect(ox + x, oy + (h - 28.0) / 2.0, width, 28.0);
            if row.editable && button == RowButton::Address {
                continue;
            }
            let able = match button {
                RowButton::Back => row.can_back,
                RowButton::Forward => row.can_forward,
                RowButton::Address => !row.read_only,
                _ => true,
            };
            let hot = row.hover == Some(button) && able;
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
                    // A long address ends in an ellipsis inside its field.
                    let room = (width - 20.0).max(0.0);
                    let shown = elide(self.dwrite, self.fonts, META_ROW, &shown, room)?;
                    self.text(
                        META_ROW,
                        rect(area.left + 10.0, area.top, room, 28.0),
                        &shown,
                        MUTED,
                        false,
                    )?;
                }
                _ => {
                    if hot {
                        self.fill_rounded(area, 8.0, rgba(INK, 0.06))?;
                    }
                    let (glyph, size, ink) = match button {
                        RowButton::Grip => ('\u{E7C2}', 13.0, SOFT_INK),
                        // A disabled icon is faded, like `.icon:disabled`.
                        RowButton::Back => {
                            ('\u{E72B}', 16.0, if able { SOFT_INK } else { 0xb9b6bb })
                        }
                        RowButton::Forward => {
                            ('\u{E72A}', 16.0, if able { MUTED } else { 0xd3d1d6 })
                        }
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

    /// Rivet's minimize, maximize and close buttons at the right end of a row that ends at `right`,
    /// from `top` down `h` DIPs, as the study's `.caption`: 46 DIP buttons over the full row
    /// height, 10 DIP glyphs, a faint hover and a red close whose hover reaches the panel's corner.
    unsafe fn caption_buttons(
        &self,
        right: f32,
        top: f32,
        h: f32,
        hover: Option<crate::features::address_bar::Caption>,
        zoomed: bool,
        dark: bool,
    ) -> Result<()> {
        use crate::features::address_bar::{caption_buttons, caption_glyph, Caption};
        for (button, x, bw) in caption_buttons(right) {
            let area = rect(x, top, bw, h);
            let hot = hover == Some(button);
            let close = button == Caption::Close;
            if hot && close {
                // Only the top right corner is the panel's; the rest of the button is square.
                self.fill_rounded(area, PANEL_RADIUS, color(0xc42b1c))?;
                self.fill(rect(x, top, bw / 2.0, h), 0xc42b1c)?;
                self.fill(rect(x, top + h / 2.0, bw, h / 2.0), 0xc42b1c)?;
            } else if hot {
                self.fill(area, if dark { 0x3a3c42 } else { 0xf1f0ed })?;
            }
            let ink = match (dark, hot && close) {
                (_, true) | (true, _) => WHITE,
                _ => 0x1a1a1a,
            };
            self.icon(caption_glyph(button, zoomed), area, 10.0, ink)?;
        }
        // The row's hairline stays under a hovered button.
        if hover.is_some() {
            self.fill(rect(right - 138.0, top + h - 1.0, 138.0, 1.0), HAIRLINE)?;
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
        self.favicon(
            body.left + 9.0,
            body.top + 7.0,
            18.0,
            ghost.colour,
            &ghost.letter,
        )?;
        let label = rect(body.left + 35.0, body.top, width - 47.0, GHOST_HEIGHT);
        self.text(ACTIVE, label, &ghost.title, INK, false)
    }

    unsafe fn floating(&self, area: D2D_RECT_F, frame: u32) -> Result<()> {
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
        self.fill_rounded(area, 14.0, color(floating_fill(frame)))?;
        self.ring(area, 14.0, rgba(0x000000, 0.07))
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

    #[allow(
        clippy::too_many_arguments,
        reason = "one row of the tab list is drawn from all of these at once"
    )]
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
                Lead::Favicon(rgb, letter) => self.favicon(lead_x, top + 9.0, 16.0, rgb, letter)?,
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

/// `value` cut to fit `room` DIPs, ending in an ellipsis when it was cut, as `text-overflow: ellipsis`.
unsafe fn elide(
    dwrite: &IDWriteFactory5,
    fonts: &IDWriteFontCollection1,
    style: Type,
    value: &str,
    room: f32,
) -> Result<String> {
    if measure(dwrite, fonts, style, value)? <= room {
        return Ok(value.to_owned());
    }
    let chars: Vec<char> = value.chars().collect();
    let (mut low, mut high) = (0usize, chars.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate: String = chars[..middle].iter().collect::<String>() + "\u{2026}";
        if measure(dwrite, fonts, style, &candidate)? <= room {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Ok(chars[..low].iter().collect::<String>() + "\u{2026}")
}

/// Makes the shell study's fonts known to GDI in this process, for the Windows text engine, which
/// draws with GDI. Only this process sees them.
pub(crate) fn make_fonts_known_to_gdi() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| unsafe {
        for font in FONTS {
            let count = 0u32;
            windows_sys::Win32::Graphics::Gdi::AddFontMemResourceEx(
                font.as_ptr().cast(),
                font.len() as u32,
                std::ptr::null(),
                &count,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_a_page_in_developer_mode_has_the_dark_row_of_the_study() {
        // `.toolbar.dev` of rivet-shell-demo.html: ground #2a2c31, lower edge #17181b, icons #c4c7ce.
        assert_eq!(row_colours(true), (0x2a2c31, 0x17181b, 0xc4c7ce));
        // Any other page keeps the paper row, where the dark caption glyphs show.
        assert_eq!(row_colours(false), (PAPER, HAIRLINE, SOFT_INK));
    }

    #[test]
    fn fr11_the_caption_glyphs_follow_the_row_they_are_drawn_on() {
        // The row and its caption glyphs take one answer, so leaving developer mode turns both light.
        assert!(row_is_dark(true, false));
        assert!(!row_is_dark(false, false));
        assert!(!row_is_dark(true, true));
    }

    #[test]
    fn fr9_a_page_row_ground_has_no_seam_where_the_browser_shows_through() {
        // A right page at 1.125 scale: the caption buttons start at a fractional pixel.
        let (x, y, width, height) = (0.0, 0.0, 636.0, 57.0);
        let covered = |ground: &[(D2D_RECT_F, u32)], px: f32, py: f32| {
            ground
                .iter()
                .any(|(r, _)| r.left <= px && px < r.right && r.top <= py && py < r.bottom)
        };
        for hole in [None, Some((140.0, 276.0))] {
            let ground = pane_row_ground(x, y, width, height, hole);
            // One run of paper reaches the right end, so nothing meets where the caption begins.
            let runs: Vec<_> = ground
                .iter()
                .filter(|(r, c)| *c == PAPER && (r.right - (x + width)).abs() < 0.01)
                .collect();
            assert_eq!(runs.len(), 1, "{hole:?}");
            assert!(runs[0].0.left <= 498.0 - 1.0);
            // The lower band is covered across the whole row, hole included.
            for px in [10.0, 200.0, 400.0, 600.0] {
                assert!(covered(&ground, px, height - 2.5), "{hole:?} at {px}");
                assert!(covered(&ground, px, height - 0.5), "{hole:?} at {px}");
            }
            // The browser's own address still shows through the middle of the hole.
            if let Some((left, room)) = hole {
                assert!(!covered(&ground, left + room / 2.0, height / 2.0));
            }
        }
    }

    #[test]
    fn fr10_the_held_build_warning_sits_at_the_bottom_centre_clear_of_the_open_button() {
        let zone = |page: [f32; 4]| crate::side_rail::open_zone_at(page[2], page[3]);
        let overlaps =
            |a: [f32; 4], b: [f32; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        for page in [
            [336.0, 56.0, 1600.0, 1000.0],
            [336.0, 56.0, 1000.0, 800.0],
            [0.0, 0.0, 700.0, 400.0],
        ] {
            let [left, top, width] = held_place(page, Some(zone(page))).unwrap();
            let warning = [left, top, left + width, top + HELD_HEIGHT];
            assert!(!overlaps(warning, zone(page)), "{page:?} {warning:?}");
            assert!(left >= page[0] + HELD_MARGIN && top + HELD_HEIGHT == page[3] - HELD_MARGIN);
        }
        // A wide page has the warning centred.
        let page = [336.0, 56.0, 1600.0, 1000.0];
        let [left, _, width] = held_place(page, Some(zone(page))).unwrap();
        assert!(((left + width / 2.0) - (page[0] + page[2]) / 2.0).abs() < 0.5);
        // No room, no warning.
        assert_eq!(
            held_place([0.0, 0.0, 80.0, 100.0], Some(zone([0.0, 0.0, 80.0, 100.0]))),
            None
        );
    }
}
