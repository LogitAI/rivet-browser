//! Makes Chromium's own frame layout leave room for the Rivet appbar.
//!
//! Chromium's Windows browser frame asks its frame view where the client view goes. Rivet
//! replaces that one virtual function in the frame view's vtable. For windows that carry a Rivet
//! appbar it moves the client view right by the appbar width. Chromium then lays out, paints,
//! hit-tests and positions menus, IME windows and accessibility bounds for the narrower view
//! itself, so nothing has to be translated after the fact.
//!
//! The vtable layout is private to each Chromium build. Rivet only patches builds whose module
//! contents match an entry of the supported-build table exactly, and leaves every other build
//! untouched.

use std::ffi::c_void;
use std::sync::OnceLock;

use rivet_platform_windows::chrome_build::{self, ChromeBuild};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

/// The loaded `chrome.dll`, checked before any recorded address is read.
///
/// Every address Rivet patches or calls is an offset into one exact Chrome build. Reading such an
/// offset in another build can leave the mapped image and crash the browser, so the module's own
/// version resource and size are checked first and every offset is bounds checked against the
/// mapped size.
pub mod image {
    use std::sync::OnceLock;

    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    /// The only Chrome build whose offsets are recorded in the runtime.
    pub const RECORDED_VERSION: [u16; 4] = [156, 0, 8078, 48];

    #[derive(Clone, Copy, Debug)]
    pub struct Image {
        pub base: usize,
        pub size: usize,
    }

    fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
        ))
    }

    fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    /// `SizeOfImage` of a mapped 64-bit PE, from its first bytes.
    pub fn size_of_image(head: &[u8]) -> Option<usize> {
        if u16_at(head, 0)? != 0x5a4d {
            return None;
        }
        let nt = u32_at(head, 0x3c)? as usize;
        if head.get(nt..nt.checked_add(4)?)? != b"PE\0\0" || u16_at(head, nt + 24)? != 0x20b {
            return None;
        }
        Some(u32_at(head, nt + 24 + 56)? as usize)
    }

    /// The file version from the version resource of a mapped 64-bit PE, where every RVA is an
    /// offset into `image`.
    pub fn file_version(image: &[u8]) -> Option<[u16; 4]> {
        let nt = u32_at(image, 0x3c)? as usize;
        // Data directory 2 is the resource table.
        let root = u32_at(image, nt.checked_add(24 + 112 + 16)?)? as usize;
        if root == 0 {
            return None;
        }
        // Entry `index` of the directory at `at`: its id and the offset of what it names.
        let entry = |at: usize, index: usize| -> Option<(u32, u32)> {
            let count = u16_at(image, at.checked_add(12)?)? as usize
                + u16_at(image, at.checked_add(14)?)? as usize;
            if index >= count {
                return None;
            }
            let entry = at.checked_add(16 + index * 8)?;
            Some((u32_at(image, entry)?, u32_at(image, entry.checked_add(4)?)?))
        };
        let directory = |offset: u32| root.checked_add((offset & 0x7fff_ffff) as usize);
        // RT_VERSION is resource type 16.
        let mut kind = None;
        for index in 0..64 {
            let Some((id, offset)) = entry(root, index) else {
                break;
            };
            if id == 16 && offset & 0x8000_0000 != 0 {
                kind = directory(offset);
                break;
            }
        }
        let name = directory(entry(kind?, 0)?.1)?;
        let (_, data) = entry(name, 0)?;
        if data & 0x8000_0000 != 0 {
            return None;
        }
        let data = root.checked_add(data as usize)?;
        let start = u32_at(image, data)? as usize;
        let length = u32_at(image, data.checked_add(4)?)? as usize;
        let info = image.get(start..start.checked_add(length)?)?;
        // VS_VERSIONINFO: three words, the key `VS_VERSION_INFO`, padding to four bytes, then
        // VS_FIXEDFILEINFO.
        let key: Vec<u16> = (0..15)
            .map(|index| u16_at(info, 6 + index * 2))
            .collect::<Option<_>>()?;
        if String::from_utf16(&key).ok()? != "VS_VERSION_INFO" || u32_at(info, 40)? != 0xfeef_04bd {
            return None;
        }
        let (most, least) = (u32_at(info, 48)?, u32_at(info, 52)?);
        Some([
            (most >> 16) as u16,
            most as u16,
            (least >> 16) as u16,
            least as u16,
        ])
    }

    impl Image {
        /// The address of `length` bytes at `rva`, or `None` when they are not inside the image.
        pub fn at(&self, rva: usize, length: usize) -> Option<usize> {
            (rva.checked_add(length)? <= self.size).then(|| self.base + rva)
        }

        /// The pointer-sized value stored at `rva`.
        pub unsafe fn word(&self, rva: usize) -> Option<usize> {
            Some((self.at(rva, 8)? as *const usize).read_unaligned())
        }

        /// The bytes at `rva`.
        pub unsafe fn bytes(&self, rva: usize, length: usize) -> Option<&'static [u8]> {
            Some(std::slice::from_raw_parts(
                self.at(rva, length)? as *const u8,
                length,
            ))
        }

        /// Whether the bytes at `rva` are exactly `expected`.
        pub unsafe fn has(&self, rva: usize, expected: &[u8]) -> bool {
            self.bytes(rva, expected.len()) == Some(expected)
        }

        /// Whether the pointer-sized value at `rva` is the address of the function at `target`.
        pub unsafe fn points_to(&self, rva: usize, target: usize) -> bool {
            self.word(rva) == Some(self.base + target)
        }
    }

    /// Checks the module at `base`: a 64-bit PE whose size is plausible and whose version
    /// resource names `version`.
    pub unsafe fn verify(base: usize, version: [u16; 4]) -> Option<Image> {
        if base == 0 {
            return None;
        }
        // The first page of a loaded module is always mapped.
        let size = size_of_image(std::slice::from_raw_parts(base as *const u8, 0x400))?;
        if !(0x1_0000..=0x4000_0000).contains(&size) {
            return None;
        }
        let bytes = std::slice::from_raw_parts(base as *const u8, size);
        (file_version(bytes)? == version).then_some(Image { base, size })
    }

    static IMAGE: OnceLock<Option<Image>> = OnceLock::new();

    /// The loaded Chrome whose offsets are recorded, or `None` for any other build or process.
    pub fn recorded() -> Option<Image> {
        *IMAGE.get_or_init(|| unsafe {
            verify(
                GetModuleHandleW(windows_sys::w!("chrome.dll")) as usize,
                RECORDED_VERSION,
            )
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A mapped image with a version resource, laid out with RVA equal to offset.
        fn fake(version: [u16; 4]) -> Vec<u8> {
            let mut image = vec![0u8; 0x2000];
            let put32 = |image: &mut Vec<u8>, at: usize, value: u32| {
                image[at..at + 4].copy_from_slice(&value.to_le_bytes())
            };
            image[0..2].copy_from_slice(b"MZ");
            put32(&mut image, 0x3c, 0x80);
            image[0x80..0x84].copy_from_slice(b"PE\0\0");
            image[0x80 + 24..0x80 + 26].copy_from_slice(&0x20bu16.to_le_bytes());
            put32(&mut image, 0x80 + 24 + 56, 0x2000);
            // Resource table at 0x400: type directory, name directory, language directory, entry.
            put32(&mut image, 0x80 + 24 + 112 + 16, 0x400);
            image[0x400 + 14] = 1;
            put32(&mut image, 0x410, 16);
            put32(&mut image, 0x414, 0x8000_0000 | 0x20);
            image[0x420 + 14] = 1;
            put32(&mut image, 0x430, 1);
            put32(&mut image, 0x434, 0x8000_0000 | 0x40);
            image[0x440 + 14] = 1;
            put32(&mut image, 0x450, 0x409);
            put32(&mut image, 0x454, 0x60);
            put32(&mut image, 0x460, 0x800);
            put32(&mut image, 0x464, 0x100);
            let key: Vec<u8> = "VS_VERSION_INFO"
                .encode_utf16()
                .flat_map(|unit| unit.to_le_bytes())
                .collect();
            image[0x806..0x806 + key.len()].copy_from_slice(&key);
            put32(&mut image, 0x800 + 40, 0xfeef_04bd);
            put32(
                &mut image,
                0x800 + 48,
                (version[0] as u32) << 16 | version[1] as u32,
            );
            put32(
                &mut image,
                0x800 + 52,
                (version[2] as u32) << 16 | version[3] as u32,
            );
            image
        }

        #[test]
        fn nfr3_version_resource_names_the_build() {
            let image = fake([156, 0, 8078, 48]);
            assert_eq!(file_version(&image), Some(RECORDED_VERSION));
            assert_eq!(size_of_image(&image), Some(0x2000));
        }

        #[test]
        fn nfr3_another_version_is_not_the_recorded_one() {
            let image = fake([157, 0, 8100, 1]);
            assert_ne!(file_version(&image), Some(RECORDED_VERSION));
        }

        #[test]
        fn nfr3_damaged_images_are_rejected_without_panicking() {
            let image = fake([156, 0, 8078, 48]);
            for length in [0, 1, 0x3c, 0x80, 0x120, 0x420, 0x7ff, 0x850] {
                let _ = file_version(&image[..length]);
                let _ = size_of_image(&image[..length]);
            }
            assert_eq!(file_version(&image[..0x850]), None);
            assert_eq!(size_of_image(&[0u8; 0x400]), None);
        }

        #[test]
        fn nfr3_recorded_offsets_outside_the_image_are_not_read() {
            let bytes = vec![0xabu8; 64];
            let image = Image {
                base: bytes.as_ptr() as usize,
                size: 64,
            };
            assert!(image.at(56, 8).is_some());
            assert!(image.at(57, 8).is_none());
            assert!(image.at(usize::MAX, 8).is_none());
            assert!(image.at(0x0FDA_FE00, 8).is_none());
            unsafe {
                assert_eq!(image.word(64), None);
                assert_eq!(image.bytes(60, 5), None);
                assert!(!image.has(0x1000_0000, &[0xab]));
                assert_eq!(image.word(0), Some(0xabab_abab_abab_abab));
            }
        }
    }
}

use image::Image;

fn recorded_image() -> Result<Image, String> {
    image::recorded().ok_or_else(|| "this browser build is not supported yet".to_owned())
}

/// Points the vtable slot at `rva` to `hook` after checking that the slot lies inside the image
/// and held `expected`.
unsafe fn replace_vtable_slot(
    image: &Image,
    rva: usize,
    expected: usize,
    hook: usize,
    what: &str,
) -> Result<(), String> {
    if !image.points_to(rva, expected) {
        return Err(format!("this browser build has no recorded {what}"));
    }
    let slot = image.at(rva, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err(format!("could not unprotect the {what}"));
    }
    slot.write(hook);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(())
}

/// `gfx::Rect`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Moves the client view right by `reserved` device-independent pixels. Views coordinates are
/// already scaled for the monitor, so the reservation does not depend on DPI.
pub fn reserve_leading(bounds: Rect, reserved: i32) -> Rect {
    let reserved = reserved.clamp(0, (bounds.width - 1).max(0));
    Rect {
        x: bounds.x + reserved,
        width: bounds.width - reserved,
        ..bounds
    }
}

/// Experiment: whether the page is laid out as the shell study's panel, below a taller top row
/// and inside a gap on the right and the bottom.
pub static PANEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// How much taller the top row is than the browser's title bar, and the gap, in DIPs.
pub const PANEL_TOP: i32 = 12;
pub const PANEL_GAP: i32 = 8;

fn inset_panel(bounds: Rect) -> Rect {
    Rect {
        x: bounds.x,
        y: bounds.y + PANEL_GAP,
        width: (bounds.width - PANEL_GAP).max(1),
        height: (bounds.height - 2 * PANEL_GAP).max(1),
    }
}

type ClientViewBounds = unsafe extern "system" fn(*const c_void, *mut Rect) -> *mut Rect;
type HwndForView = unsafe extern "system" fn(*const c_void) -> HWND;

/// Decides the width to reserve for a window from the client-view bounds the browser chose, or
/// returns `None` when the window carries no Rivet appbar.
pub type Reserve = fn(HWND, Rect) -> Option<i32>;

struct Hook {
    original: ClientViewBounds,
    hwnd_for_view: HwndForView,
    reserved: Reserve,
}

static HOOK: OnceLock<Result<Hook, String>> = OnceLock::new();

/// Experiment, Chrome 156.0.8078.48 only. The frame view paints its title bar with colours from
/// a colour provider that other windows of the profile share. Rivet wraps the frame view's
/// `OnPaint` and swaps the frame colours in that provider only while a Rivet window paints.
///
/// `views::View::GetColorProvider` returns the provider. It keeps generated colours in a tree
/// whose nodes hold the colour id at `+0x20` and the colour at `+0x24`.
const ON_PAINT_SLOT: usize = 89;
const ON_PAINT: usize = 0x011B_5610;
const GET_COLOR_PROVIDER: usize = 0x04AA_27D0;
const DRAW_COLOR: usize = 0x036B_AD80;
/// The active, inactive and unthemed-inactive frame colour ids. The last one fills the band
/// behind the top row, which the panel's rounded corners now reveal. An active window with an
/// unthemed frame may read 0x13D and 0x144, which hold the same blue as the active colour.
const FRAME_IDS: [i32; 5] = [0x13C, 0x13D, 0x140, 321, 0x144];
type OnPaint = unsafe extern "system" fn(*const c_void, *const c_void);
type GetProvider = unsafe extern "system" fn(*const c_void) -> *const u8;
type DrawColor = unsafe extern "system" fn(*const c_void, u32);
static PAINT: OnceLock<(OnPaint, GetProvider, DrawColor, Paint)> = OnceLock::new();
/// `gfx::Canvas::FillRect(const Rect&, SkColor)`, which composites source-over.
const FILL_RECT: usize = 0x0142_7B70;
type FillRect = unsafe extern "system" fn(*const c_void, *const Rect, u32);
static FILL_RECT_FN: OnceLock<FillRect> = OnceLock::new();

/// Edge of one tinted block, in DIPs. A block steps less than one colour level, so no seams show.
const TINT_BLOCK: i32 = 6;

/// Source-over of `top` (r, g, b) at `alpha` (0..1) onto `under`.
fn over(under: [f32; 3], top: [f32; 3], alpha: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| under[i] + (top[i] - under[i]) * alpha)
}

/// Opacity of a radial gradient that fades from `peak` at its centre to nothing at 70% of an
/// ellipse with radii `rx` and `ry`.
fn radial_alpha(dx: f32, dy: f32, rx: f32, ry: f32, peak: f32) -> f32 {
    let distance = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
    (peak * (1.0 - distance / 0.7)).max(0.0)
}

/// The frame colour at a point of a `width` by `height` frame: the base, a warm tint rising from
/// the lower left and a faint white glow from the top right.
fn frame_tint(base: u32, x: f32, y: f32, width: f32, height: f32) -> u32 {
    let base = [16, 8, 0].map(|shift| ((base >> shift) & 0xff) as f32);
    let glow = radial_alpha(x - width, y, 0.6 * width, 0.5 * height, 0.55);
    let tint = radial_alpha(x, y - height, 0.8 * width, 0.6 * height, 0.34);
    let colour = over(over(base, [255.0; 3], glow), [208.0, 138.0, 82.0], tint);
    let [r, g, b] = colour.map(|c| c.round().clamp(0.0, 255.0) as u32);
    0xff00_0000 | (r << 16) | (g << 8) | b
}

/// Paints the frame tint over a frame of `width` by `height` DIPs, merging equal neighbours.
unsafe fn paint_frame_tint(canvas: *const c_void, fill: FillRect, base: u32, width: i32, height: i32) {
    let (w, h) = (width as f32, height as f32);
    let mut y = 0;
    while y < height {
        let block_height = TINT_BLOCK.min(height - y);
        let centre_y = y as f32 + block_height as f32 / 2.0;
        let mut x = 0;
        while x < width {
            let at = |from: i32, to: i32| frame_tint(base, (from + to) as f32 / 2.0, centre_y, w, h);
            let mut end = (x + TINT_BLOCK).min(width);
            let colour = at(x, end);
            while end < width {
                let next = (end + TINT_BLOCK).min(width);
                if at(end, next) != colour {
                    break;
                }
                end = next;
            }
            if colour != base {
                fill(canvas, &Rect { x, y, width: end - x, height: block_height }, colour);
            }
            x = end;
        }
        y += block_height;
    }
}

/// Opacity of a box shadow's blur at `distance` DIPs outside the box edge, for a blur radius of
/// `blur` (a Gaussian of half that deviation) and a darkest opacity of `peak`.
fn shadow_opacity(distance: f32, blur: f32, peak: f32) -> f32 {
    let sigma = blur / 2.0;
    let z = distance / (sigma * std::f32::consts::SQRT_2);
    // Abramowitz and Stegun 7.1.26 for erfc on z >= 0, mirrored for z < 0.
    let t = 1.0 / (1.0 + 0.327_591_1 * z.abs());
    let poly = t * (0.254_829_6 + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152_1 + t * 1.061_405_4))));
    let erfc_abs = poly * (-z * z).exp();
    let erfc = if z >= 0.0 { erfc_abs } else { 2.0 - erfc_abs };
    peak * 0.5 * erfc
}

/// Nested rectangles `(grow, alpha)` that stack to a blurred shadow: each is grown by `grow`
/// DIPs from the shadow box and carries the alpha that lifts the stacked opacity to the blur's.
fn shadow_layers(blur: f32, peak: f32) -> Vec<(i32, u8)> {
    let mut stacked = 0.0_f32;
    let mut layers = Vec::new();
    for grow in (0..=blur.ceil() as i32).rev() {
        let wanted = shadow_opacity(grow as f32, blur, peak);
        let alpha = ((wanted - stacked) / (1.0 - stacked)).max(0.0);
        let level = (alpha * 255.0).round() as u8;
        if level > 0 {
            layers.push((grow, level));
            stacked += (1.0 - stacked) * (level as f32 / 255.0);
        }
    }
    layers
}

/// The panel's two soft shadows as `(offset_y, blur, colour, peak)`.
const PANEL_SHADOWS: [(i32, f32, u32, f32); 2] = [
    (1, 3.0, 0x0000_0000, 0.05),
    (12, 32.0, 0x0028_1c1e, 0.08),
];

/// Paints the panel's shadows into the frame around `panel`. The panel's own layer covers the
/// middle, so only the part that reaches into the gap shows.
unsafe fn paint_panel_shadow(canvas: *const c_void, fill: FillRect, panel: Rect) {
    paint_shadows(canvas, fill, panel, &PANEL_SHADOWS);
}

/// The shadows of the raised page in a split, which the shell study gives as
/// `0 0 0 1px rgba(0,0,0,.07), 0 2px 6px rgba(0,0,0,.06), 0 18px 44px rgba(40,28,30,.16)`.
/// A blur of 0 is the 1 DIP ring.
const FOCUSED_PANE_SHADOWS: [(i32, f32, u32, f32); 3] = [
    (0, 0.0, 0x0000_0000, 0.07),
    (2, 6.0, 0x0000_0000, 0.06),
    (18, 44.0, 0x0028_1c1e, 0.16),
];
/// The other pages of a split sit lower: `0 0 0 1px rgba(0,0,0,.04), 0 6px 18px rgba(40,28,30,.05)`.
const OTHER_PANE_SHADOWS: [(i32, f32, u32, f32); 2] =
    [(0, 0.0, 0x0000_0000, 0.04), (6, 18.0, 0x0028_1c1e, 0.05)];

/// The colour of the gap between the pages of a split, the shell study's paper.
const SPLIT_PAPER: u32 = 0xfffb_faf7;

/// Paints the gap of a split and the shadows of its pages into the canvas of the view that holds
/// them. The pages' own layers cover their middles.
pub unsafe fn paint_split(canvas: *const c_void, panes: &[Rect], active: usize) {
    let Some(fill) = FILL_RECT_FN.get().copied() else { return };
    // The canvas clips to the view, so one very large fill covers it.
    fill(canvas, &Rect { x: -8192, y: -8192, width: 16384, height: 16384 }, SPLIT_PAPER);
    // The raised page goes last so its deeper shadow is not dimmed by the others.
    for pane in panes.iter().enumerate().filter(|(index, _)| *index != active).map(|(_, pane)| pane) {
        paint_shadows(canvas, fill, *pane, &OTHER_PANE_SHADOWS);
    }
    if let Some(pane) = panes.get(active) {
        paint_shadows(canvas, fill, *pane, &FOCUSED_PANE_SHADOWS);
    }
}

unsafe fn paint_shadows(canvas: *const c_void, fill: FillRect, panel: Rect, shadows: &[(i32, f32, u32, f32)]) {
    for &(offset, blur, colour, peak) in shadows {
        if blur == 0.0 {
            let rect = Rect {
                x: panel.x - 1,
                y: panel.y + offset - 1,
                width: panel.width + 2,
                height: panel.height + 2,
            };
            fill(canvas, &rect, (((peak * 255.0).round() as u32) << 24) | colour);
            continue;
        }
        for (grow, alpha) in shadow_layers(blur, peak) {
            let rect = Rect {
                x: panel.x - grow,
                y: panel.y + offset - grow,
                width: panel.width + 2 * grow,
                height: panel.height + 2 * grow,
            };
            fill(canvas, &rect, ((alpha as u32) << 24) | colour);
        }
    }
}


unsafe fn colour_slot(provider: *const u8, id: i32) -> Option<*mut u32> {
    let mut node = *(provider.add(0x18) as *const *mut u8);
    while !node.is_null() {
        let key = *(node.add(0x20) as *const i32);
        if key == id {
            return Some(node.add(0x24) as *mut u32);
        }
        node = if id < key {
            *(node as *const *mut u8)
        } else {
            *(node.add(8) as *const *mut u8)
        };
    }
    None
}

unsafe extern "system" fn on_paint(view: *const c_void, canvas: *const c_void) {
    let (original, provider_of, draw_color, paint) =
        PAINT.get().expect("patched after it is stored");
    let colour = match HOOK.get() {
        Some(Ok(hook)) => paint((hook.hwnd_for_view)(view)),
        _ => None,
    };
    let provider = provider_of(view);
    let Some(colour) = colour.filter(|_| !provider.is_null()) else {
        return original(view, canvas);
    };
    draw_color(canvas, colour);
    let saved: Vec<(*mut u32, u32)> = FRAME_IDS
        .iter()
        .filter_map(|id| colour_slot(provider, *id))
        .map(|slot| (slot, slot.read()))
        .collect();
    for (slot, _) in &saved {
        slot.write(colour);
    }
    original(view, canvas);
    for (slot, value) in saved {
        slot.write(value);
    }
    if let (Some(fill), Some(Ok(hook))) = (FILL_RECT_FN.get(), HOOK.get()) {
        let bounds = *(view.cast::<u8>().add(VIEW_BOUNDS_OFFSET) as *const [i32; 4]);
        let (width, height) = (bounds[2], bounds[3]);
        if width > 0 && height > 0 {
            paint_frame_tint(canvas, *fill, colour, width, height);
            let whole = Rect { x: 0, y: 0, width, height };
            let reserved = (hook.reserved)((hook.hwnd_for_view)(view), whole).unwrap_or(0);
            let panel = inset_panel(Rect { x: reserved, ..whole });
            paint_panel_shadow(canvas, *fill, Rect { width: panel.width - reserved, ..panel });
        }
    }
}

/// Experiment, Chrome 156.0.8078.48 only. `CustomFloatingCorner::OnPaint`: the browser view's two
/// corner views at the page's left edge fill a wedge with the browser's frame colour, which
/// rounds the page's corners against the frame. In a Rivet window the frame is Rivet's, so the
/// wedge is painted with Rivet's frame colour instead of Chrome's blue.
const CORNER_VTABLE: usize = 0x0FEC_AFC8;
const CORNER_PAINT: usize = 0x0A4C_73A0;
static CORNER_ORIGINAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Stacks the layers of `shadows` that cover the point `(x, y)` onto `colour`, as the frame
/// paint does with filled rectangles.
fn shadowed(colour: u32, x: f32, y: f32, panel: Rect, shadows: &[(i32, f32, u32, f32)]) -> u32 {
    let mut rgb = [16, 8, 0].map(|shift| ((colour >> shift) & 0xff) as f32);
    for &(offset, blur, shadow, peak) in shadows {
        let layers = if blur == 0.0 {
            vec![(1, (peak * 255.0).round() as u8)]
        } else {
            shadow_layers(blur, peak)
        };
        for (grow, alpha) in layers {
            let (left, top) = ((panel.x - grow) as f32, (panel.y + offset - grow) as f32);
            let (right, bottom) = (left + (panel.width + 2 * grow) as f32, top + (panel.height + 2 * grow) as f32);
            if x >= left && x < right && y >= top && y < bottom {
                let tone = [16, 8, 0].map(|shift| ((shadow >> shift) & 0xff) as f32);
                rgb = over(rgb, tone, alpha as f32 / 255.0);
            }
        }
    }
    let [r, g, b] = rgb.map(|c| c.round().clamp(0.0, 255.0) as u32);
    0xff00_0000 | (r << 16) | (g << 8) | b
}

/// Where the frame colour is sampled for a wedge, as a fraction of its size inward from the
/// page's corner. The top one sits just outside the corner and the bottom one just inside, where
/// the panel's shadow best matches the gap beside the wedge.
const WEDGE_TOP_REACH: f32 = -0.1;
const WEDGE_BOTTOM_REACH: f32 = 0.06;

/// The colour the frame paint leaves at the centre of area of a corner wedge: the tint at that spot of
/// the window with the panel's shadows over it. The wedge is flat, so it matches the frame
/// within a few levels across its small size.
unsafe fn wedge_colour(view: *const c_void, base: u32) -> u32 {
    let Some(Ok(hook)) = HOOK.get() else { return base };
    let window = (hook.hwnd_for_view)(view);
    let mut client = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 {
        return base;
    }
    let scale = GetDpiForWindow(window).max(96) as f32 / 96.0;
    let (width, height) = (client.right as f32 / scale, client.bottom as f32 / scale);
    let whole = Rect { x: 0, y: 0, width: width as i32, height: height as i32 };
    let bounds = *(view.cast::<u8>().add(VIEW_BOUNDS_OFFSET) as *const [i32; 4]);
    // The wedge sits at the page's left edge, so its own x is the width Rivet reserves.
    let reserved = bounds[0];
    let panel = inset_panel(Rect { x: reserved, ..whole });
    let (wedge_width, wedge_height) = (bounds[2] as f32, bounds[3] as f32);
    let x = panel.x as f32 + wedge_width * WEDGE_BOTTOM_REACH;
    let y = if bounds[1] * 2 > panel.height {
        (panel.y + panel.height) as f32 - wedge_height * WEDGE_BOTTOM_REACH
    } else {
        panel.y as f32 + wedge_height * WEDGE_TOP_REACH
    };
    let tinted = frame_tint(base, x, y, width, height);
    shadowed(tinted, x, y, Rect { width: panel.width - reserved, ..panel }, &PANEL_SHADOWS)
}

unsafe extern "system" fn corner_paint(view: *const c_void, canvas: *const c_void) {
    let original: OnPaint = std::mem::transmute(CORNER_ORIGINAL.load(std::sync::atomic::Ordering::Relaxed));
    let colour = match (HOOK.get(), PAINT.get()) {
        (Some(Ok(hook)), Some((_, provider_of, _, paint))) => {
            paint((hook.hwnd_for_view)(view)).map(|colour| (colour, provider_of(view)))
        }
        _ => None,
    };
    let Some((colour, provider)) = colour.filter(|(_, provider)| !provider.is_null()) else {
        return original(view, canvas);
    };
    let saved: Vec<(*mut u32, u32)> = FRAME_IDS
        .iter()
        .filter_map(|id| colour_slot(provider, *id))
        .map(|slot| (slot, slot.read()))
        .collect();
    let colour = wedge_colour(view, colour);
    for (slot, _) in &saved {
        slot.write(colour);
    }
    original(view, canvas);
    for (slot, value) in saved {
        slot.write(value);
    }
}

unsafe fn install_corner_paint(image: &Image) -> Result<(), String> {
    let slot_rva = CORNER_VTABLE + ON_PAINT_SLOT * 8;
    if !image.has(CORNER_PAINT, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]) {
        return Err("this browser build has no recorded corner paint".into());
    }
    CORNER_ORIGINAL.store(image.base + CORNER_PAINT, std::sync::atomic::Ordering::Relaxed);
    replace_vtable_slot(image, slot_rva, CORNER_PAINT, corner_paint as *const () as usize, "corner view vtable")
}

/// Experiment: lets Rivet choose the title bar colour of its own windows only.
pub unsafe fn install_frame_paint(paint: Paint) -> Result<(), String> {
    if PAINT.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = 0x0FE7_3E28 + ON_PAINT_SLOT * 8;
    if !image.points_to(slot_rva, ON_PAINT) {
        return Err("this browser build has no recorded frame paint".into());
    }
    if !image.has(ON_PAINT, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54])
        || !image.has(DRAW_COLOR, &[0x48, 0x83, 0xec, 0x38, 0x48, 0x8b])
        || !image.has(GET_COLOR_PROVIDER, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x78])
    {
        return Err("this browser build has no recorded frame paint helpers".into());
    }
    if !image.has(FILL_RECT, &[0x56, 0x48, 0x83, 0xc4, 0x80]) {
        return Err("this browser build has no recorded frame fill".into());
    }
    let base = image.base;
    let _ = FILL_RECT_FN.set(std::mem::transmute::<usize, FillRect>(base + FILL_RECT));
    let _ = PAINT.set((
        std::mem::transmute::<usize, OnPaint>(base + ON_PAINT),
        std::mem::transmute::<usize, GetProvider>(base + GET_COLOR_PROVIDER),
        std::mem::transmute::<usize, DrawColor>(base + DRAW_COLOR),
        paint,
    ));
    replace_vtable_slot(&image, slot_rva, ON_PAINT, on_paint as *const () as usize, "frame view vtable")?;
    install_corner_paint(&image)
}

/// Decides the title bar colour of a window as `0xAARRGGBB`, or `None` to keep the browser's.
pub type Paint = fn(HWND) -> Option<u32>;

unsafe extern "system" fn client_view_bounds(view: *const c_void, out: *mut Rect) -> *mut Rect {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    let result = (hook.original)(view, out);
    if let Some(reserved) = (hook.reserved)((hook.hwnd_for_view)(view), *out) {
        *out = reserve_leading(*out, reserved);
        if PANEL.load(std::sync::atomic::Ordering::Relaxed) {
            *out = inset_panel(*out);
        }
    }
    result
}

/// Installs the layout hook once per browser process. Returns why it cannot when the build is
/// not recognised, so Rivet refuses to attach rather than guess.
pub unsafe fn install(reserved: Reserve) -> Result<(), String> {
    HOOK.get_or_init(|| patch(reserved))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

unsafe fn patch(reserved: Reserve) -> Result<Hook, String> {
    let builds = chrome_build::builds(chrome_build::TABLE);
    let (build, image) = builds
        .iter()
        .find_map(|build| {
            let module = wide(&build.module);
            let base = GetModuleHandleW(module.as_ptr()) as usize;
            let version = parse_version(&build.version)?;
            let image = image::verify(base, version)?;
            matches(&image, build).then_some((build, image))
        })
        .ok_or("this browser build is not supported yet")?;
    let base = image.base;
    let slot_rva = build.frame_view_vtable as usize + build.client_view_slot as usize * 8;
    let slot = image.at(slot_rva, 8).ok_or("recorded slot outside the image")? as *mut usize;
    let hook = Hook {
        original: std::mem::transmute::<usize, ClientViewBounds>(slot.read()),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(
            base + build.hwnd_for_view as usize,
        ),
        reserved,
    };
    // Layout runs on this UI thread only, so no call can reach the new slot before HOOK holds
    // this value.
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(client_view_bounds as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    log(&format!(
        "rivet: laying out {} {}",
        build.browser, build.version
    ));
    Ok(hook)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(text.as_ptr()) };
}

/// Checks that the loaded module is the recorded build: the vtable slot must still point at the
/// recorded function, and the helper must start with the recorded instructions.
unsafe fn matches(image: &Image, build: &ChromeBuild) -> bool {
    let Some(expected) = chrome_build::hex(&build.hwnd_for_view_prologue) else {
        return false;
    };
    let slot = build.frame_view_vtable as usize + build.client_view_slot as usize * 8;
    image.points_to(slot, build.client_view_bounds as usize)
        && image.has(build.hwnd_for_view as usize, &expected)
}

/// `156.0.8078.48` as the four numbers of a file version.
fn parse_version(text: &str) -> Option<[u16; 4]> {
    let parts: Vec<u16> = text
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_caption_fills_the_panel_top_row() {
        assert_eq!(
            panel_caption_bounds([900, 0, 138, 32], 44),
            [892, 8, 138, 44]
        );
    }

    #[test]
    fn fr12_caption_ink_is_independent_of_window_activation() {
        assert_eq!(caption_ink(), 0xff1a_1a1a);
    }

    #[test]
    fn fr12_wedge_colour_darkens_inside_the_panel_shadow_and_not_outside_it() {
        let panel = Rect { x: 0, y: 8, width: 400, height: 300 };
        let base = 0xffec_e8e2;
        assert_eq!(shadowed(base, 200.0, -100.0, panel, &PANEL_SHADOWS), base);
        let inside = shadowed(base, 200.0, 300.0, panel, &PANEL_SHADOWS);
        assert!((inside >> 16 & 0xff) < 0xec);
    }

    #[test]
    fn fr12_frame_tint_is_warm_at_the_lower_left_and_bright_at_the_top_right() {
        let base = 0xffec_e8e2;
        let low_left = frame_tint(base, 0.0, 800.0, 1000.0, 800.0);
        let top_right = frame_tint(base, 1000.0, 0.0, 1000.0, 800.0);
        assert!((low_left >> 16 & 0xff) < 0xec && (low_left & 0xff) < 0xe2);
        assert!((top_right & 0xff) > 0xe2);
        assert_eq!(frame_tint(base, 500.0, 400.0, 1000.0, 800.0), base);
    }

    #[test]
    fn fr12_panel_shadow_fades_with_distance() {
        let layers = shadow_layers(32.0, 0.08);
        assert!(layers.len() > 4);
        assert!(layers.windows(2).all(|pair| pair[0].0 > pair[1].0));
        assert!(shadow_opacity(32.0, 32.0, 0.08) < shadow_opacity(0.0, 32.0, 0.08));
    }

    #[test]
    fn fr2_panel_clips_each_corner_at_twelve_dips() {
        assert_eq!(panel_radii(), [12.0; 4]);
    }

    #[test]
    fn fr12_panel_reserves_the_frame_gap() {
        assert_eq!(
            inset_panel(Rect {
                x: 220,
                y: 0,
                width: 980,
                height: 800
            }),
            Rect {
                x: 220,
                y: 8,
                width: 972,
                height: 784
            }
        );
    }

    #[test]
    fn fr2_client_view_moves_right_by_the_appbar_width() {
        let bounds = Rect {
            x: 0,
            y: 32,
            width: 1200,
            height: 800,
        };
        assert_eq!(
            reserve_leading(bounds, 292),
            Rect {
                x: 292,
                y: 32,
                width: 908,
                height: 800
            }
        );
    }

    #[test]
    fn fr2_narrow_client_view_keeps_one_pixel() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        assert_eq!(reserve_leading(bounds, 292).width, 1);
    }
}

/// Experiment, Chrome 156.0.8078.48 only: `WindowsCaptionButton::CalculatePreferredSize` in the
/// caption button's vtable. Rivet windows get caption buttons as tall as the shell study's top
/// row, so the native buttons sit centred in it.
const CAPTION_VTABLE: usize = 0x0FF0_25C8;
const CAPTION_SIZE_SLOT: usize = 74;
const CAPTION_SIZE: usize = 0x0313_2D90;
type PreferredSize =
    unsafe extern "system" fn(*const c_void, *mut [i32; 2], *const c_void) -> *mut [i32; 2];
/// Decides the caption button height of a window in DIPs, or `None` to keep the browser's.
pub type CaptionHeight = fn(HWND) -> Option<i32>;
static CAPTION: OnceLock<(PreferredSize, CaptionHeight)> = OnceLock::new();

const CAPTION_PAINT_SLOT: usize = 114;
const CAPTION_PAINT: usize = 0x0313_32A0;
static CAPTION_PAINT_HOOK: OnceLock<(OnPaint, GetProvider)> = OnceLock::new();

fn caption_ink() -> u32 {
    0xff1a_1a1a
}

unsafe extern "system" fn caption_paint(button: *const c_void, canvas: *const c_void) {
    let (original, provider_of) = CAPTION_PAINT_HOOK
        .get()
        .expect("patched after it is stored");
    let rivet = matches!((HOOK.get(), PAINT.get()), (Some(Ok(hook)), Some((_, _, _, paint))) if paint((hook.hwnd_for_view)(button)).is_some());
    if !rivet {
        return original(button, canvas);
    }
    let provider = provider_of(button);
    if provider.is_null() {
        return original(button, canvas);
    }
    let saved: Vec<_> = [0x66c, 0x66d, 0x66e]
        .into_iter()
        .filter_map(|id| colour_slot(provider, id))
        .map(|slot| (slot, slot.read()))
        .collect();
    for (slot, _) in &saved {
        slot.write(caption_ink());
    }
    original(button, canvas);
    for (slot, colour) in saved {
        slot.write(colour);
    }
}

pub unsafe fn install_caption_paint() -> Result<(), String> {
    if CAPTION_PAINT_HOOK.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = CAPTION_VTABLE + CAPTION_PAINT_SLOT * 8;
    if !image.points_to(slot_rva, CAPTION_PAINT)
        || !image.has(CAPTION_PAINT, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54])
        || !image.has(GET_COLOR_PROVIDER, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x78])
    {
        return Err("this browser build has no recorded caption paint".into());
    }
    let _ = CAPTION_PAINT_HOOK.set((
        std::mem::transmute::<usize, OnPaint>(image.base + CAPTION_PAINT),
        std::mem::transmute::<usize, GetProvider>(image.base + GET_COLOR_PROVIDER),
    ));
    replace_vtable_slot(&image, slot_rva, CAPTION_PAINT, caption_paint as *const () as usize, "caption paint slot")
}

unsafe extern "system" fn caption_size(
    button: *const c_void,
    out: *mut [i32; 2],
    bounds: *const c_void,
) -> *mut [i32; 2] {
    let (original, height) = CAPTION.get().expect("patched after it is stored");
    let result = original(button, out, bounds);
    if let Some(Ok(hook)) = HOOK.get() {
        if let Some(height) = height((hook.hwnd_for_view)(button)) {
            (*out)[1] = height;
        }
    }
    result
}

pub unsafe fn install_caption_height(height: CaptionHeight) -> Result<(), String> {
    if CAPTION.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = CAPTION_VTABLE + CAPTION_SIZE_SLOT * 8;
    if !image.points_to(slot_rva, CAPTION_SIZE) {
        return Err("this browser build has no recorded caption button size".into());
    }
    let _ = CAPTION.set((
        std::mem::transmute::<usize, PreferredSize>(image.base + CAPTION_SIZE),
        height,
    ));
    replace_vtable_slot(&image, slot_rva, CAPTION_SIZE, caption_size as *const () as usize, "caption button vtable")
}

/// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::GetTopInset` in the frame view's
/// vtable. It is the height of the browser's own title bar, which also sizes the caption buttons.
const TOP_INSET_SLOT: usize = 127;
const TOP_INSET: usize = 0x0A40_33E0;
type TopInset = unsafe extern "system" fn(*const c_void, bool) -> i32;
/// Extra title bar height in DIPs for a window, or `None` to keep the browser's.
pub type ExtraTop = fn(HWND) -> Option<i32>;
static TOP: OnceLock<(TopInset, ExtraTop)> = OnceLock::new();

unsafe extern "system" fn top_inset(view: *const c_void, restored: bool) -> i32 {
    let (original, extra) = TOP.get().expect("patched after it is stored");
    let inset = original(view, restored);
    match HOOK.get() {
        Some(Ok(hook)) => inset + extra((hook.hwnd_for_view)(view)).unwrap_or(0),
        _ => inset,
    }
}

pub unsafe fn install_top_inset(extra: ExtraTop) -> Result<(), String> {
    if TOP.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = 0x0FE7_3E28 + TOP_INSET_SLOT * 8;
    if !image.points_to(slot_rva, TOP_INSET) {
        return Err("this browser build has no recorded top inset".into());
    }
    let _ = TOP.set((std::mem::transmute::<usize, TopInset>(image.base + TOP_INSET), extra));
    replace_vtable_slot(&image, slot_rva, TOP_INSET, top_inset as *const () as usize, "frame view vtable")
}

/// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::Layout` in the frame view's
/// vtable. It sizes the caption button container from the tab strip, not from the title bar
/// height, so the container is resized once the browser's own layout is done.
const LAYOUT_SLOT: usize = 23;
const LAYOUT: usize = 0x017A_E5E0;
/// `views::View::SetBoundsRect`, and where a frame view keeps its caption button container and a
/// view keeps its bounds.
const SET_BOUNDS_RECT: usize = 0x04F4_2540;
const CAPTION_CONTAINER_OFFSET: usize = 0x3C8;
const VIEW_BOUNDS_OFFSET: usize = 0x1C8;
type Layout = unsafe extern "system" fn(*mut c_void);
type SetBoundsRect = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);
static LAYOUT_HOOK: OnceLock<(Layout, SetBoundsRect, CaptionHeight)> = OnceLock::new();
type SetPaintToLayer = unsafe extern "system" fn(*mut c_void, i32);
type SetRadius = unsafe extern "system" fn(*mut c_void, *const [f32; 4]);
type SetMasks = unsafe extern "system" fn(*mut c_void, bool);
type StackAtTop = unsafe extern "system" fn(*mut c_void, *mut c_void);
type SetOpaque = unsafe extern "system" fn(*mut c_void, bool);
static SET_OPAQUE: OnceLock<SetOpaque> = OnceLock::new();
static STACK_AT_TOP: OnceLock<StackAtTop> = OnceLock::new();
static PANEL_LAYER: OnceLock<(SetPaintToLayer, SetRadius, SetMasks)> = OnceLock::new();

fn panel_radii() -> [f32; 4] {
    [12.0; 4]
}

/// The vtable of the `views::View` part of `BrowserView`, which sits behind other base classes.
const BROWSER_VIEW_VIEW_VTABLE: usize = 0x0FE7_BF68;

unsafe fn clip_panel(frame: *mut c_void) {
    let Some((set_layer, set_radius, set_masks)) = PANEL_LAYER.get() else { return };
    // BrowserFrameViewWin holds BrowserView's BrowserWindow subobject at +0x340.
    let browser_window = *(frame.cast::<u8>().add(0x340) as *const *mut u8);
    if browser_window.is_null() {
        return;
    }
    let browser = browser_window.sub(0x1f0).cast::<usize>();
    let Some(image) = image::recorded() else { return };
    let base = image.base;
    let Some(index) = (0..0x100).find(|index| *browser.add(*index) == base + BROWSER_VIEW_VIEW_VTABLE)
    else {
        return;
    };
    let view = browser.add(index).cast::<c_void>();
    // `views::View` keeps its `ui::Layer` at +0x20.
    let layer_of = || *(view.cast::<u8>().add(0x20) as *const *mut c_void);
    if layer_of().is_null() {
        set_layer(view, 1);
    }
    let layer = layer_of();
    if !layer.is_null() {
        set_masks(layer, true);
        set_radius(layer, &panel_radii());
    }
}

/// Keeps the caption buttons above the rounded panel. Both are layers under the widget's root
/// layer and views order layers by tree position, which puts the client view on top, so the
/// caption container gets its own layer that is stacked on top again after every layout.
unsafe fn raise_caption(container: *mut c_void) {
    let Some((set_layer, set_radius, set_masks)) = PANEL_LAYER.get() else { return };
    let Some(stack_at_top) = STACK_AT_TOP.get() else { return };
    let layer_of = || *(container.cast::<u8>().add(0x20) as *const *mut c_void);
    if layer_of().is_null() {
        set_layer(container, 1);
    }
    let layer = layer_of();
    if layer.is_null() {
        return;
    }
    if let Some(set_opaque) = SET_OPAQUE.get() {
        set_opaque(layer, false);
    }
    set_masks(layer, true);
    set_radius(layer, &[0.0, 12.0, 0.0, 0.0]);
    let parent = *(layer.cast::<u8>().add(0x20) as *const *mut c_void);
    if !parent.is_null() {
        stack_at_top(parent, layer);
    }
}

/// Gives `view` its own layer and stacks it above its siblings' layers, so it paints over a
/// sibling it overlaps, such as the page. Views order layers by tree position, so a later layout
/// can undo it and the caller repeats it after each layout.
pub unsafe fn raise_view(view: *mut c_void) {
    let Some((set_layer, _, _)) = PANEL_LAYER.get() else { return };
    let Some(stack_at_top) = STACK_AT_TOP.get() else { return };
    let layer_of = || *(view.cast::<u8>().add(0x20) as *const *mut c_void);
    if layer_of().is_null() {
        set_layer(view, 1);
    }
    let layer = layer_of();
    if layer.is_null() {
        return;
    }
    let parent = *(layer.cast::<u8>().add(0x20) as *const *mut c_void);
    if !parent.is_null() {
        stack_at_top(parent, layer);
    }
}

fn panel_caption_bounds(bounds: [i32; 4], height: i32) -> [i32; 4] {
    // The container ends at the window edge, but the panel ends one gap before it.
    [bounds[0] - PANEL_GAP, PANEL_GAP, bounds[2], height.max(0)]
}

/// The frame view each window last laid out, so Rivet can ask for a layout when the space it
/// reserves changes. Chromium only lays out again on its own when the window's size changes, and
/// a maximized window cannot be resized to make it.
static FRAME_VIEWS: std::sync::Mutex<Vec<(isize, usize)>> = std::sync::Mutex::new(Vec::new());

fn remember_view(window: HWND, view: *mut c_void) {
    let mut views = FRAME_VIEWS.lock().unwrap_or_else(|error| error.into_inner());
    match views.iter_mut().find(|entry| entry.0 == window as isize) {
        Some(entry) => entry.1 = view as usize,
        None => views.push((window as isize, view as usize)),
    }
}

/// The window a view belongs to, once the layout hook is installed.
pub unsafe fn hwnd_of_view(view: *const c_void) -> Option<HWND> {
    match HOOK.get() {
        Some(Ok(hook)) => Some((hook.hwnd_for_view)(view)),
        _ => None,
    }
}

/// The frame view a window last laid out, if any.
pub fn frame_view(window: HWND) -> Option<usize> {
    FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1)
}

/// Forgets a window that is going away, so its view is never used again.
pub fn forget_view(window: HWND) {
    FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.0 != window as isize);
}

/// Lays the window's frame out again. Returns false when the window has not been laid out yet.
pub unsafe fn relayout(window: HWND) -> bool {
    let view = FRAME_VIEWS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window as isize)
        .map(|entry| entry.1);
    match view {
        Some(view) => {
            let Some(image) = image::recorded() else { return false };
            let base = image.base;
            // `views::View::DeprecatedLayoutImmediately` does the bookkeeping around a layout
            // and then runs the view's own `Layout`, which is this hook. The wanted bytes are
            // its first instructions in this build.
            const IMMEDIATELY: usize = 0x04BE_C780;
            const PROLOGUE: [u8; 12] =
                [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];
            if !image.has(IMMEDIATELY, &PROLOGUE) {
                return false;
            }
            let layout_immediately: unsafe extern "system" fn(*mut c_void) =
                std::mem::transmute(base + IMMEDIATELY);
            layout_immediately(view as *mut c_void);
            true
        }
        None => false,
    }
}

unsafe extern "system" fn frame_layout(view: *mut c_void) {
    let (original, set_bounds, height) = LAYOUT_HOOK.get().expect("patched after it is stored");
    original(view);
    let Some(Ok(hook)) = HOOK.get() else { return };
    remember_view((hook.hwnd_for_view)(view), view);
    let Some(bottom) = height((hook.hwnd_for_view)(view)) else {
        return;
    };
    clip_panel(view);
    let container = *(view.cast::<u8>().add(CAPTION_CONTAINER_OFFSET) as *const *mut c_void);
    if container.is_null() {
        return;
    }
    let bounds = *(container.cast::<u8>().add(VIEW_BOUNDS_OFFSET) as *const [i32; 4]);
    let wanted = panel_caption_bounds(bounds, bottom);
    if wanted != bounds {
        set_bounds(container, &wanted);
    }
    raise_caption(container);
}

/// Makes the caption button container of a window reach `height` DIPs from the top of the frame.
pub unsafe fn install_caption_layout(height: CaptionHeight) -> Result<(), String> {
    if LAYOUT_HOOK.get().is_some() {
        return Ok(());
    }
    let image = recorded_image()?;
    let slot_rva = 0x0FE7_3E28 + LAYOUT_SLOT * 8;
    if !image.points_to(slot_rva, LAYOUT) {
        return Err("this browser build has no recorded frame layout".into());
    }
    if !image.has(LAYOUT, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53])
        || !image.has(SET_BOUNDS_RECT, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54])
    {
        return Err("this browser build has no recorded caption layout helpers".into());
    }
    const SET_LAYER: usize = 0x00F0_5F10;
    const SET_RADIUS: usize = 0x01D3_AB80;
    const SET_MASKS: usize = 0x0316_7C40;
    const STACK_AT_TOP_RVA: usize = 0x0ACC_8FD0;
    const SET_OPAQUE_RVA: usize = 0x020A_FEF0;
    if !image.has(SET_LAYER, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28])
        || !image.has(SET_RADIUS, &[0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x30])
        || !image.has(SET_MASKS, &[0x48, 0x8b, 0x89, 0x70, 0x01, 0x00, 0x00])
        || !image.has(STACK_AT_TOP_RVA, &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57])
        || !image.has(SET_OPAQUE_RVA, &[0x56, 0x48, 0x83, 0xec, 0x20])
    {
        return Err("this browser build has no recorded panel layer helpers".into());
    }
    let base = image.base;
    let _ = SET_OPAQUE.set(std::mem::transmute::<usize, SetOpaque>(base + SET_OPAQUE_RVA));
    let _ = STACK_AT_TOP.set(std::mem::transmute::<usize, StackAtTop>(base + STACK_AT_TOP_RVA));
    let _ = PANEL_LAYER.set((
        std::mem::transmute::<usize, SetPaintToLayer>(base + SET_LAYER),
        std::mem::transmute::<usize, SetRadius>(base + SET_RADIUS),
        std::mem::transmute::<usize, SetMasks>(base + SET_MASKS),
    ));
    let _ = LAYOUT_HOOK.set((
        std::mem::transmute::<usize, Layout>(base + LAYOUT),
        std::mem::transmute::<usize, SetBoundsRect>(base + SET_BOUNDS_RECT),
        height,
    ));
    replace_vtable_slot(&image, slot_rva, LAYOUT, frame_layout as *const () as usize, "frame view vtable")
}


#[cfg(test)]
mod split_shadow_tests {
    use super::{shadow_layers, FOCUSED_PANE_SHADOWS, OTHER_PANE_SHADOWS};

    #[test]
    fn fr9_focused_page_is_raised_above_the_others() {
        let reach = |shadows: &[(i32, f32, u32, f32)]| {
            shadows
                .iter()
                .filter(|(_, blur, _, _)| *blur > 0.0)
                .map(|(offset, blur, _, _)| offset + blur.ceil() as i32)
                .max()
        };
        assert!(reach(&FOCUSED_PANE_SHADOWS) > reach(&OTHER_PANE_SHADOWS));
        assert!(!shadow_layers(44.0, 0.16).is_empty());
    }
}
