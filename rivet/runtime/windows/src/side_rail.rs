//! The Sidebar's rail and its open button, drawn as visuals of the browser window.
//!
//! The rail lies in the column the browser's layout leaves free at the right of the page area
//! (see `chrome_side_panel`), so it covers nothing of the browser. The open button of a folded
//! Sidebar appears over the page's lower right corner while the pointer is near it. Both take
//! their input from the window's own message queue, because Chromium's page child window would
//! otherwise swallow clicks that land on a visual above it.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::{Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1DeviceContext, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDesktopDevice, IDCompositionVisual, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory5, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
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
use crate::chrome_side_panel::{self as panel, PageGeometry, RAIL_WIDTH};

const PAPER: u32 = 0xfbfaf7;
const TINT: u32 = 0xd08a52;
const ACCENT: u32 = 0x9a5424;
const INK: u32 = 0x1f2024;
const SOFT_INK: u32 = 0x4a4850;
const ICONS: &str = "Segoe Fluent Icons";
const PANEL_RADIUS: f32 = 12.0;

/// The built-in entries of Chrome's side panel the rail lists: entry id, glyph and name.
pub const ENTRIES: [(u32, char, &str); 3] = [
    (0, '\u{E7BC}', "Reading list"),
    (1, '\u{E736}', "Bookmarks"),
    (3, '\u{E81C}', "History"),
];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailHit {
    Entry(usize),
    Add,
    Fold,
}

/// What a point of the rail, in its own DIPs, is. `height` is the rail's height.
pub fn rail_hit(x: f32, y: f32, height: f32) -> Option<RailHit> {
    let inside = |top: f32| {
        x >= (RAIL_WIDTH as f32 - BUTTON) / 2.0
            && x < (RAIL_WIDTH as f32 + BUTTON) / 2.0
            && y >= top && y < top + BUTTON
    };
    for index in 0..ENTRIES.len() {
        if inside(RAIL_PAD + index as f32 * (BUTTON + BUTTON_GAP)) {
            return Some(RailHit::Entry(index));
        }
    }
    if inside(add_top()) {
        return Some(RailHit::Add);
    }
    inside(height - RAIL_PAD - BUTTON).then_some(RailHit::Fold)
}

/// Where the add button begins: after the entries, a rule and its margins.
fn add_top() -> f32 {
    RAIL_PAD + ENTRIES.len() as f32 * (BUTTON + BUTTON_GAP) + 4.0 + 1.0 + 4.0 + BUTTON_GAP
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
    showing: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct OpenKey {
    dpi: u32,
    opacity: i32,
    over: bool,
}

struct Rail {
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    column: IDCompositionVisual2,
    open: IDCompositionVisual2,
    column_key: Option<ColumnKey>,
    open_key: Option<OpenKey>,
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
        let open = device.CreateVisual()?;
        root.AddVisual(&open, true, None::<&IDCompositionVisual>)?;
        Ok(Self {
            device,
            dwrite,
            column,
            open,
            column_key: None,
            open_key: None,
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
        let ground = mix(PAPER, TINT, 0.04);
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
        let button = |top: f32, hit: RailHit, glyph: char, on: bool| -> Result<()> {
            let area = rect(left, top, BUTTON, BUTTON);
            let pressed = key.pressed == Some(hit) && key.hover == Some(hit);
            if on {
                self.rounded(context, area, 8.0, rgb(mix(0xffffff, TINT, 0.14), 1.0))?;
            } else if pressed || key.hover == Some(hit) {
                self.rounded(context, area, 8.0, rgb(INK, if pressed { 0.1 } else { 0.06 }))?;
            }
            let ink = if on { ACCENT } else if key.hover == Some(hit) { INK } else { SOFT_INK };
            self.icon(context, glyph, area, 14.0, rgb(ink, 1.0))
        };
        for (index, (_, glyph, _)) in ENTRIES.iter().enumerate() {
            let top = RAIL_PAD + index as f32 * (BUTTON + BUTTON_GAP);
            button(top, RailHit::Entry(index), *glyph, key.showing & (1 << index) != 0)?;
        }
        let rule = RAIL_PAD + ENTRIES.len() as f32 * (BUTTON + BUTTON_GAP) + 4.0;
        context.FillRectangle(
            &rect((width - 18.0) / 2.0, rule, 18.0, 1.0),
            &context.CreateSolidColorBrush(&rgb(INK, 0.08), None)?,
        );
        button(add_top(), RailHit::Add, ADD_GLYPH, false)?;
        button(height - RAIL_PAD - BUTTON, RailHit::Fold, FOLD_GLYPH, false)
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

/// Which of the entries Chrome's side panel shows, as a bit mask over `ENTRIES`.
unsafe fn showing_mask(window: HWND) -> u32 {
    ENTRIES
        .iter()
        .enumerate()
        .filter(|(_, (id, _, _))| panel::entry_showing(window, *id))
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
        return refresh_open(rail, geometry.filter(|_| !unfolded), dpi);
    };
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
        showing: showing_mask(window),
    };
    if rail.column_key.as_ref() != Some(&key) && width_px > 0 && height_px > 0 {
        let shown = key.clone();
        rail.paint(&rail.column, width_px, height_px, dpi, |context| {
            rail.draw_column(context, RAIL_WIDTH as f32, height, &shown)
        })?;
        rail.column_key = Some(key);
    }
    refresh_open(rail, None, dpi)?;
    rail.device.Commit()
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
}

fn target_at(rail: &Rail, unfolded: bool, x: f32, y: f32) -> Option<Target> {
    let geometry = rail.geometry?;
    if unfolded {
        let (left, top, height) = rail_rect(geometry);
        return rail_hit(x - left, y - top, height).map(Target::Rail);
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
    let changed = RAILS.with(|rails| {
        let mut rails = rails.borrow_mut();
        let Some(rail) = rails.get_mut(&(window as isize)) else { return false };
        let unfolded = panel::is_unfolded(window);
        let (x, y) = to_dips(pointer, rail.scale);
        let before = (rail.hover, rail.near, rail.over, rail.opacity);
        rail.near = !unfolded && rail.geometry.is_some_and(|g| in_hot_area(g, x, y));
        rail.over = !unfolded && rail.geometry.is_some_and(|g| on_open_button(g, x, y));
        rail.hover = match target_at(rail, unfolded, x, y) {
            Some(Target::Rail(hit)) => Some(hit),
            _ => None,
        };
        rail.opacity = fade(rail.opacity, open_opacity(rail.near, rail.over), 0.2);
        before != (rail.hover, rail.near, rail.over, rail.opacity)
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
            None => None,
        }
    });
    let Some(action) = action else { return false };
    match action {
        Some(Target::Open) => panel::set_unfolded(window, true),
        Some(Target::Rail(RailHit::Entry(index))) => {
            panel::toggle_entry(window, ENTRIES[index].0);
        }
        Some(Target::Rail(RailHit::Add)) => {
            if crate::chrome_tabs::new_tab(window) {
                crate::chrome_navigation::open(window, "chrome://extensions");
            }
        }
        Some(Target::Rail(RailHit::Fold)) => {
            for (id, _, _) in ENTRIES {
                if panel::entry_showing(window, id) {
                    panel::toggle_entry(window, id);
                }
            }
            panel::set_unfolded(window, false);
        }
        None => {}
    }
    crate::frame::update_appbar(window);
    true
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
        assert_eq!(rail_hit(20.0, 20.0, 600.0), Some(RailHit::Entry(0)));
        assert_eq!(rail_hit(20.0, 6.0 + 32.0 + 5.0, 600.0), Some(RailHit::Entry(1)));
        assert_eq!(rail_hit(20.0, 6.0 + 31.0, 600.0), None, "the gap between buttons is not a button");
        assert_eq!(rail_hit(2.0, 20.0, 600.0), None, "the rail's padding is not a button");
    }

    #[test]
    fn fr10_rail_ends_with_a_fold_button_after_the_add_button() {
        assert_eq!(rail_hit(20.0, 600.0 - 6.0 - 15.0, 600.0), Some(RailHit::Fold));
        let add = add_top() + 10.0;
        assert_eq!(rail_hit(20.0, add, 600.0), Some(RailHit::Add));
        assert!(add_top() > 6.0 + 3.0 * 32.0);
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
    fn fr10_open_button_fades_in_steps() {
        assert_eq!(fade(0.0, 0.6, 0.2), 0.2);
        assert_eq!(fade(0.5, 0.6, 0.2), 0.6);
        assert!((fade(0.6, 0.0, 0.2) - 0.4).abs() < 1e-6);
    }
}
