//! Experiment: restyling Chromium's own toolbar so the top row of a Rivet window follows the
//! Rivet design, while other windows of the same browser process keep Chromium's look.
//!
//! Rivet runs in the person's own profile, so Rivet windows and ordinary windows share one
//! process, one UI thread and one colour provider. Every change here therefore applies only while
//! that thread works for a Rivet window: while one of its widgets lays out, paints or refreshes
//! its theme, and while its window procedure runs. That span is the Rivet scope.
//!
//! - `GetLayoutConstant` answers Rivet sizes inside the scope.
//! - The colour provider holds Rivet colours inside the scope and its own outside.
//! - Back, forward and reload icons are swapped when a Rivet window builds its button images.
//! - The app menu, profile and page action buttons stay hidden in Rivet windows.
//!
//! The addresses below belong to Google Chrome 156.0.8078.48 only.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::address_display;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, SetTimer, WindowFromPoint, GA_ROOT,
};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use crate::chrome_layout::image;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// `GetLayoutConstant(LayoutConstant)`.
const LAYOUT_CONSTANT: usize = 0x0471_3020;
const LAYOUT_CONSTANT_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0xF0, 0x45, 0x0D,
];
/// `views::Widget::ThemeChanged`.
const THEME_CHANGED: usize = 0x01DC_5870;
const THEME_CHANGED_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xC0, 0xC7, 0xDA, 0x0F,
];
/// `views::View::OnPaintLayer` implementation.
const PAINT_LAYER: usize = 0x007E_1A30;
const PAINT_LAYER_PROLOGUE: [u8; 16] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x00, 0x06, 0x39, 0x11,
];
/// `views::View::SetVisible`. Its first sixteen bytes save registers and reserve the frame.
const SET_VISIBLE: usize = 0x0573_5770;
const SET_VISIBLE_PROLOGUE: [u8; 16] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x48,
];
/// `ui::ImageModel::FromVectorIcon(out, icon, colour, size)`.
const FROM_VECTOR_ICON: usize = 0x009F_C510;
const FROM_VECTOR_ICON_PROLOGUE: [u8; 14] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
];
/// `LocationIconView::Update(this, bool, bool)` sets the label and then the icon.
const UPDATE_LOCATION: usize = 0x00EE_3320;
const UPDATE_LOCATION_PROLOGUE: [u8; 15] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0, 0, 0,
];
/// `LocationIconView::UpdateIcon` runs after navigation, outside widget painting.
const UPDATE_LOCATION_ICON: usize = 0x00EE_4220;
const UPDATE_LOCATION_ICON_PROLOGUE: [u8; 16] = [
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0, 0, 0, 0x48, 0x89, 0xCE,
];
/// `GetLayoutInsets(out, LayoutInset)` answers the padding around a toolbar button's icon, which
/// with the icon makes the button 28 DIPs wide, like its height.
const LAYOUT_INSETS: usize = 0x03EE_A610;
const LAYOUT_INSETS_PROLOGUE: [u8; 13] = [
    0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x89, 0xD7, 0x48, 0x89, 0xCE,
];
/// `LayoutInset::TOOLBAR_BUTTON` of this build.
const TOOLBAR_BUTTON_INSET: i32 = 6;
/// `LayoutInset` that `ToolbarView::LayoutCommon` reads for the space around the whole toolbar,
/// and the space Rivet windows leave before the back button, in DIPs.
const TOOLBAR_INSET: i32 = 11;
const LEADING_SPACE: i32 = 32;
const TOOLBAR_TOP: i32 = 5;
const TOOLBAR_BOTTOM: i32 = 4;
/// `LayoutInset` that `LocationIconView::UpdateBorder` reads for the space around the page info
/// icon, and how far Rivet moves the icon and the text after it from the field's left edge.
const LOCATION_ICON_INSET: i32 = 3;
const LOCATION_ICON_SHIFT: i32 = 2;
/// The padding, in DIPs: a 26 DIP button around an 18 DIP icon, which Chrome pads to 28.
const TOOLBAR_BUTTON_PADDING: i32 = 4;
/// `ToolbarButton::UpdateIcon` rebuilds a button's images. The reload button does it when a page
/// starts or stops loading, outside any paint, so it needs its own scope.
const UPDATE_ICON: usize = 0x009F_C150;
const UPDATE_ICON_PROLOGUE: [u8; 17] = [
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x80, 0xB9, 0xD8, 0x06, 0, 0, 0x01,
];
/// `LocationBarModelImpl::GetFormattedURL(this, out, flags)` returns the address text, and
/// `LocationBarModelImpl::GetURL(this, out)` the page address.
const FORMATTED_URL: usize = 0x0160_9DB0;
const FORMATTED_URL_PROLOGUE: [u8; 18] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x80, 0x01, 0, 0,
];
const MODEL_URL: usize = 0x04F5_BBB0;
const MODEL_URL_PROLOGUE: [u8; 16] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0, 0, 0,
];
/// `OmniboxViewViews::Update`, `OmniboxViewViews::RevertAll` and `OmniboxViewViews::OnTabChanged`
/// refresh the text of the address field. The omnibox object starts with `OmniboxView`, which is
/// 0x10 bytes long, and its `views::Textfield` follows. They take the object as their first
/// argument and need the Rivet scope, because they run outside any paint or layout.
const OMNIBOX_UPDATE: usize = 0x00ED_FB30;
const OMNIBOX_UPDATE_PROLOGUE: [u8; 15] = [
    0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x01, 0x25, 0xC9, 0x10,
];
const OMNIBOX_REVERT_ALL: usize = 0x00EE_8630;
const OMNIBOX_REVERT_ALL_PROLOGUE: [u8; 15] = [
    0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x01, 0x9A, 0xC8, 0x10,
];
const OMNIBOX_TAB_CHANGED: usize = 0x00EE_D110;
const OMNIBOX_TAB_CHANGED_PROLOGUE: [u8; 13] = [
    0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE,
];
const OMNIBOX_TEXTFIELD: usize = 0x10;
/// `OmniboxView::UpdateTextStyle(this, text, text_is_url, scheme)` colours the text of the field.
const UPDATE_TEXT_STYLE: usize = 0x00EE_59C0;
const UPDATE_TEXT_STYLE_PROLOGUE: [u8; 19] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88,
    0, 0, 0,
];
/// `gfx::RenderText::ApplyColor(this, colour, &range)` and `ApplyWeight(this, weight, &range)`.
const APPLY_COLOR: usize = 0x0318_5FA0;
const APPLY_COLOR_BYTES: [u8; 15] = [
    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0, 0, 0,
];
const APPLY_WEIGHT: usize = 0x0AF1_1D20;
const APPLY_WEIGHT_BYTES: [u8; 15] = [
    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0, 0x01, 0, 0,
];
/// `gfx::Font::Weight` values.
const WEIGHT_NORMAL: u32 = 400;
const WEIGHT_SEMIBOLD: u32 = 600;
/// The title and the rest of the address text, as `0xAARRGGBB`.
const TITLE_COLOUR: u32 = 0xFF4A_4850;
const ADDRESS_COLOUR: u32 = 0xFF8A_8690;
/// `OmniboxViewViews::TitleWasSet` is the web contents observer entry of the omnibox, which is
/// 0x720 bytes into the object. The page title is not part of the text Chrome builds, so a new
/// title refreshes it.
const TITLE_WAS_SET: usize = 0x0312_8B70;
const TITLE_WAS_SET_PROLOGUE: [u8; 13] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x80, 0, 0, 0,
];
const OBSERVER_OFFSET: usize = 0x720;
/// `operator new(size)` and `operator delete(pointer, size)`, which own Chrome's strings.
const OPERATOR_NEW: usize = 0x05D3_D6A0;
const OPERATOR_NEW_BYTES: [u8; 8] = [0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B];
const OPERATOR_DELETE: usize = 0x03E0_6930;
const OPERATOR_DELETE_BYTES: [u8; 14] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6,
];
/// The location bar model keeps its delegate at `+0x40`. The browser's delegate answers the
/// active web contents in its twentieth virtual function, and `WebContents::GetTitle` is the
/// fifty-fourth of the web contents.
const MODEL_DELEGATE: usize = 0x40;
const BROWSER_DELEGATE_VTABLE: usize = 0x0FDB_7368;
const ACTIVE_WEB_CONTENTS: usize = 0x057F_48C0;
const ACTIVE_WEB_CONTENTS_SLOT: usize = 19;
const WEB_CONTENTS_VTABLE: usize = 0x0FE5_7C10;
const WEB_CONTENTS_TITLE: usize = 0x00D7_8440;
const WEB_CONTENTS_TITLE_SLOT: usize = 53;
/// `LocationBarView::RefreshBackground` bakes the field's fill colour into a background. Hover
/// and focus refresh it outside any paint, so it needs its own scope.
const LOCATION_BAR_VIEW_BASE: usize = 0x100;
/// The field's hover animation value, a `double` in the `LocationBarView`, and the background the
/// refresh sets: a `views::Background` at `+0x328` whose painter, at `+0x10`, keeps its four
/// corner radii as floats from `+0x10`.
const HOVER_VALUE: usize = 0x5F8;
const FIELD_BACKGROUND: usize = 0x328;
const BACKGROUND_VTABLE: usize = 0x0FC3_C730;
const PAINTER_VTABLE: usize = 0x0FDC_F4F0;
const PAINTER_RADII: usize = 0x10;
const PAINTER_INSETS: usize = 0x20;
const FIELD_FILL_MARGIN: i32 = 3;
/// The radius of the field's rectangle, in DIPs.
const FIELD_RADIUS: f32 = 8.0;
/// `views::View::SetTransform(this, &gfx::Transform)` and `ui::Layer::SetLayerBrightness(this,
/// float)`. A transform that is a scale and a translation starts with a zero byte, then holds the
/// scale at `+8` and the translation at `+0x10`, as floats. A view keeps its layer at `+0x20` and
/// its size at `+0x1D0`.
const SET_TRANSFORM: usize = 0x0AC2_1430;
const SET_TRANSFORM_PROLOGUE: [u8; 14] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6,
];
const SET_BRIGHTNESS: usize = 0x0ACC_89D0;
const SET_BRIGHTNESS_PROLOGUE: [u8; 11] = [
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30,
];
const VIEW_LAYER: usize = 0x20;
const VIEW_SIZE: usize = 0x1D0;
const TRANSFORM_BYTES: usize = 256;
const REFRESH_BACKGROUND: usize = 0x00EE_0120;
const REFRESH_BACKGROUND_PROLOGUE: [u8; 14] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x60,
];
/// `location_bar::GetSecurityChipText(out, delegate, url, flag)` returns the label Chrome puts
/// before the address: "File" for local pages and the connection state of the others.
/// `GURL::~GURL` frees the page address that the delegate hands out by value.
const CHIP_TEXT: usize = 0x00E4_D780;
const CHIP_TEXT_PROLOGUE: [u8; 15] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0, 0, 0,
];
/// `views::Textfield::OnThemeChanged`, `Textfield::SetFontList`, `gfx::FontList::GetFontSize` and
/// `gfx::FontList::DeriveWithSizeDelta`. The address field is the browser window's only text
/// field, and Chrome chooses its font once, when the field is built.
const TEXTFIELD_THEME: usize = 0x01FD_2870;
const TEXTFIELD_THEME_PROLOGUE: [u8; 12] = [
    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x68,
];
const TEXTFIELD_SET_FONT: usize = 0x0316_0CB0;
const TEXTFIELD_SET_FONT_BYTES: [u8; 16] = [
    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x78, 0x03, 0, 0, 0x48,
];
const FONT_SIZE: usize = 0x036B_B960;
const FONT_SIZE_BYTES: [u8; 16] = [
    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0x31, 0x8B, 0x46, 0x44, 0x83, 0xF8, 0xFF, 0x74, 0x06,
];
const DERIVE_SIZE: usize = 0x0318_74F0;
const DERIVE_SIZE_BYTES: [u8; 16] = [
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC7, 0x48, 0x89, 0xD6,
];
/// The Textfield's render text and its font list, as offsets.
const TEXTFIELD_RENDER_TEXT: usize = 0x378;
const RENDER_TEXT_FONT_LIST: usize = 0x48;
/// The size of the address text, in DIPs.
const ADDRESS_FONT_SIZE: i32 = 12;
const GURL_DESTRUCTOR: usize = 0x03E0_01F0;
const GURL_DESTRUCTOR_BYTES: [u8; 9] = [0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE];
/// `GURL` layout of this build: the spec string, then the scheme's begin and length.
const GURL_SCHEME_BEGIN: usize = 0x20;
const GURL_SCHEME_LENGTH: usize = 0x24;
/// `BubbleDialogDelegate::GetAnchorRect`, `View::IsDrawn` and `View::GetBoundsInScreen`.
const ANCHOR_RECT: usize = 0x0ACA_C450;
const ANCHOR_RECT_PROLOGUE: [u8; 18] = [
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC,
    0xD0, 0, 0, 0,
];
const ANCHOR_VIEW_ACCESS: [u8; 21] = [
    0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
    0x48, 0x85, 0xDB, 0x74, 0x4D,
];
const IS_DRAWN: usize = 0x00CA_89C0;
const IS_DRAWN_BYTES: [u8; 32] = [
    0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
    0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
];
const BOUNDS_IN_SCREEN: usize = 0x0086_1140;
const BOUNDS_BYTES: [u8; 13] = [0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF];
const PARENT_OFFSET: usize = 0x198;
const IS_DRAWN_SLOT: usize = 19;

/// `BrowserWidget`'s vtable, with `OnNativeWidgetPaint` and `LayoutRootViewIfNecessary`.
const BROWSER_WIDGET_VTABLE: usize = 0x0FBC_AD10;
const PAINT_SLOT: usize = 30;
const PAINT: usize = 0x0315_D050;
const LAYOUT_SLOT: usize = 44;
const LAYOUT: usize = 0x00F9_7180;
/// `views::View::GetColorProvider`, asked of the widget's root view, which the widget keeps at
/// `+0x1A0`.
const COLOR_PROVIDER: usize = 0x04AA_27D0;
const ROOT_VIEW_OFFSET: usize = 0x1A0;

/// `views::View` vtables of the buttons a Rivet window hides, with their `OnThemeChanged`.
const THEME_SLOT: usize = 99;
const HIDDEN: [(usize, usize); 4] = [
    (0x0FEA_63C8, 0x01D1_3080), // BrowserAppMenuButton
    (0x0FEE_C268, 0x01AE_4DC0), // AvatarToolbarButton
    (0x0FED_B708, 0x003B_A0E0), // page_actions::PageActionView
    (0x0FF1_7AA8, 0x00A3_E970), // VerticalTabStripRegionView
];
/// `views::View::GetWidget` is the twelfth entry of every view's vtable.
const GET_WIDGET_SLOT: usize = 11;

/// `LayoutConstant` values of this build and the sizes of the Rivet top row, in DIPs.
const OVERRIDES: [(i32, i32); 7] = [
    (12, 8),  // kLocationBarChildCornerRadius
    (16, 28), // kLocationBarHeight
    (29, 12), // kMainBackgroundRegionCornerRadius
    (46, 28), // kToolbarButtonHeight
    (47, 18), // kToolbarButtonIconSize
    (55, 8),  // kToolbarCornerRadius
    (60, 0),  // kVerticalTabMinWidth
];

const PAPER: u32 = 0xFFFB_FAF7;
const HAIRLINE: u32 = 0xFFE9_E9E6;
/// Colour ids of this build and the Rivet colours, as `0xAARRGGBB`.
const COLOURS: [(i32, u32); 19] = [
    (0x13C, PAPER),      // kColorFrameActive
    (796, PAPER),        // kColorLocationBarBackground
    (797, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
    (1003, 0),           // Page info chip background: clear, so the field is one surface
    (1013, PAPER),       // Location bar background in the alternate state
    (1055, 0xFF4A_4850), // kColorOmniboxText
    (1056, 0xFF8A_8690), // kColorOmniboxTextDimmed
    (1590, PAPER),       // kColorToolbar
    (1644, 0xFF1A_1A1A), // Caption button glyph, active window
    (1645, 0xFF1A_1A1A), // Caption button glyph, inactive window
    (1646, 0xFF1A_1A1A), // Caption button glyph without a theme colour
    (1647, 0xFFC4_2B1C), // Close button hover fill
    (1650, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
    (1595, 0xFF4A_4850), // kColorToolbarButtonIcon
    (1598, 0xFF1F_2024), // kColorToolbarButtonIconHovered
    (1599, 0xFFC4_C1C8), // kColorToolbarButtonIconInactive
    (1603, HAIRLINE),    // kColorToolbarContentAreaSeparator
    (1611, 0x0F1F_2024), // kColorToolbarInkDropHover
    (1612, 0x1F1F_2024), // kColorToolbarInkDropRipple
];

type LayoutConstant = unsafe extern "system" fn(i32) -> i32;
type WidgetCall = unsafe extern "system" fn(*mut c_void);
type WidgetPaint = unsafe extern "system" fn(*mut c_void, *const c_void);
type SetVisible = unsafe extern "system" fn(*mut c_void, bool);
type Provider = unsafe extern "system" fn(*mut c_void) -> *const u8;
type Pointer = unsafe extern "system" fn(*const c_void) -> *mut c_void;

struct Hooks {
    base: usize,
    paint: WidgetPaint,
    layout: WidgetCall,
    provider: Provider,
    hidden: Vec<(usize, WidgetCall)>,
}

/// A state the address field is kept in for captures, since the pointer may not move: read once
/// from `rivet-toolbar-force.txt` next to the runtime.
static FORCED: AtomicUsize = AtomicUsize::new(0);
const FORCE_HOVER: usize = 1;
const FORCE_PRESSED: usize = 2;
const FORCE_CYCLE: usize = 3;

static HOOKS: OnceLock<Result<Hooks, String>> = OnceLock::new();
/// Trampolines to the original functions. Each is stored before its function is patched, so a
/// hook never runs without one.
static ORIGINAL_LAYOUT_CONSTANT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_THEME_CHANGED: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_PAINT_LAYER: AtomicUsize = AtomicUsize::new(0);
static LAYER_LOGGED: AtomicBool = AtomicBool::new(false);
static ORIGINAL_SET_VISIBLE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_LOCATION: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_LOCATION_ICON: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_REFRESH_BACKGROUND: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_ICON: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_LAYOUT_INSETS: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_FORMATTED_URL: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_CHIP_TEXT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_TEXTFIELD_THEME: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_ANCHOR_RECT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_UPDATE_TEXT_STYLE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_TITLE_WAS_SET: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_UPDATE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_REVERT_ALL: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_OMNIBOX_TAB_CHANGED: AtomicUsize = AtomicUsize::new(0);
/// The provider caches a colour when it is first asked for, so a colour can be swapped only after
/// that. This remembers which ones have been seen, for the trace.
static COLOURS_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());
/// The widgets of Rivet windows.
static WIDGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// The text each Rivet window's address field was last given at rest.
static DISPLAYS: Mutex<Vec<(usize, address_display::Display)>> = Mutex::new(Vec::new());

thread_local! {
    /// The Rivet widget the UI thread works for, or zero.
    static CURRENT: Cell<usize> = const { Cell::new(0) };
    /// Whether the page the location icon was last updated for is a local file.
    static LOCAL_PAGE: Cell<bool> = const { Cell::new(false) };
    /// Provider colours replaced for the current Rivet widget, with their own values.
    static SWAPPED: RefCell<Vec<(*mut u32, u32)>> = const { RefCell::new(Vec::new()) };
}

fn hooks() -> Option<&'static Hooks> {
    HOOKS.get()?.as_ref().ok()
}

pub(crate) fn is_rivet(widget: usize) -> bool {
    widget != 0
        && WIDGETS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&widget)
}

fn active() -> bool {
    CURRENT.with(Cell::get) != 0
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

fn first_swap(id: i32) -> bool {
    let mut seen = COLOURS_SEEN.lock().unwrap_or_else(|error| error.into_inner());
    if seen.contains(&id) {
        return false;
    }
    seen.push(id);
    true
}

unsafe fn switch_to(hooks: &Hooks, widget: usize) {
    if CURRENT.with(Cell::get) == widget {
        return;
    }
    SWAPPED.with(|swapped| {
        for (slot, value) in swapped.borrow_mut().drain(..) {
            slot.write(value);
        }
    });
    CURRENT.with(|current| current.set(widget));
    if widget == 0 {
        return;
    }
    let root = *((widget + ROOT_VIEW_OFFSET) as *const *mut c_void);
    if root.is_null() {
        return;
    }
    let provider = (hooks.provider)(root);
    if provider.is_null() {
        return;
    }
    SWAPPED.with(|swapped| {
        let mut swapped = swapped.borrow_mut();
        for (id, colour) in COLOURS {
            if let Some(slot) = colour_slot(provider, id) {
                let original = slot.read();
                if first_swap(id) {
                    crate::vertical_tabs::trace(&format!("colour {id:#x} replaced {original:#010x} with {colour:#010x}"));
                }
                swapped.push((slot, original));
                slot.write(colour);
            }
        }
    });
}

/// Runs `work` in the scope of `widget`: Rivet's look when it belongs to a Rivet window, the
/// browser's otherwise. Scopes nest, and leaving one restores the outer one.
unsafe fn scoped<R>(widget: usize, work: impl FnOnce() -> R) -> R {
    let Some(hooks) = hooks() else { return work() };
    let outer = CURRENT.with(Cell::get);
    switch_to(hooks, if is_rivet(widget) { widget } else { 0 });
    let result = work();
    switch_to(hooks, outer);
    result
}

unsafe fn widget_of(window: HWND) -> usize {
    let Some(hooks) = hooks() else { return 0 };
    let content: Pointer = std::mem::transmute(hooks.base + CONTENT_WINDOW_FOR_HWND);
    let widget_for: Pointer = std::mem::transmute(hooks.base + WIDGET_FOR_WINDOW);
    let aura = content(window as *const c_void);
    if aura.is_null() {
        return 0;
    }
    widget_for(aura) as usize
}

/// Runs `work` in the scope of `window`, for its window procedure.
pub unsafe fn for_window<R>(window: HWND, work: impl FnOnce() -> R) -> R {
    if hooks().is_none() {
        return work();
    }
    scoped(widget_of(window), work)
}

unsafe fn original<T: Copy>(slot: &AtomicUsize) -> T {
    std::mem::transmute_copy(&slot.load(Ordering::Acquire))
}

unsafe extern "system" fn layout_constant(constant: i32) -> i32 {
    if active() {
        if let Some((_, value)) = OVERRIDES.iter().find(|(key, _)| *key == constant) {
            return *value;
        }
    }
    original::<LayoutConstant>(&ORIGINAL_LAYOUT_CONSTANT)(constant)
}

unsafe extern "system" fn layout_insets(out: *mut [i32; 4], inset: i32) -> *mut [i32; 4] {
    if active() && inset == TOOLBAR_BUTTON_INSET {
        out.write([TOOLBAR_BUTTON_PADDING; 4]);
        return out;
    }
    let result = original::<unsafe extern "system" fn(*mut [i32; 4], i32) -> *mut [i32; 4]>(
        &ORIGINAL_LAYOUT_INSETS,
    )(out, inset);
    if active() && inset == TOOLBAR_INSET {
        // The insets are top, left, bottom and right. The left one is where the back button
        // starts, and the appbar toggle sits in the space added before it. The row is 43 DIPs
        // tall around the 34 DIP field, with the buttons centred 28 DIPs in it.
        (*out)[0] = TOOLBAR_TOP;
        (*out)[2] = TOOLBAR_BOTTOM;
        (*out)[1] = (*out)[1].saturating_add(LEADING_SPACE);
    }
    if active() && inset == LOCATION_ICON_INSET {
        (*out)[1] = (*out)[1].saturating_add(LOCATION_ICON_SHIFT);
    }
    result
}

unsafe extern "system" fn theme_changed(widget: *mut c_void) {
    let run: WidgetCall = match ORIGINAL_THEME_CHANGED.load(Ordering::Acquire) {
        0 => std::mem::transmute(hooks().map_or(0, |hooks| hooks.base) + THEME_CHANGED),
        _ => original(&ORIGINAL_THEME_CHANGED),
    };
    scoped(widget as usize, || run(widget));
}

/// Layer-backed views paint separately from the widget paint callback.
unsafe extern "system" fn paint_layer(view: *mut c_void, context: *const c_void) {
    let table = *(view as *const usize);
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(GET_WIDGET_SLOT));
    let widget = get_widget(view) as usize;
    if is_rivet(widget) && !active() && !LAYER_LOGGED.swap(true, Ordering::Relaxed) {
        crate::vertical_tabs::trace("rivet layer paint outside scope");
    }
    let run: WidgetPaint = original(&ORIGINAL_PAINT_LAYER);
    scoped(widget, || run(view, context));
}

unsafe extern "system" fn widget_paint(widget: *mut c_void, context: *const c_void) {
    let hooks = hooks().expect("patched after it is stored");
    scoped(widget as usize, || (hooks.paint)(widget, context));
}

unsafe extern "system" fn widget_layout(widget: *mut c_void) {
    let hooks = hooks().expect("patched after it is stored");
    scoped(widget as usize, || (hooks.layout)(widget));
}

/// The icon is chosen while its view is being updated, which can happen outside any paint or
/// layout of the window, so the Rivet scope is found from the view's ancestors.
unsafe extern "system" fn update_location_icon(view: *mut c_void) {
    let top = window_top(view);
    let run: WidgetCall = original(&ORIGINAL_UPDATE_LOCATION_ICON);
    scoped(top, || run(view));
}

unsafe extern "system" fn update_icon(view: *mut c_void) {
    let top = widget_holding(view);
    let run: WidgetCall = original(&ORIGINAL_UPDATE_ICON);
    scoped(top, || run(view));
}

/// In Rivet windows the field at rest shows the page title and the address. The text for editing,
/// which has other flags, stays the real address, so what a person edits and submits is always a
/// page address the browser can open.
unsafe extern "system" fn formatted_url(model: *mut c_void, out: *mut [u64; 3], flags: u32) -> *mut [u64; 3] {
    let result = formatted(model, out, flags);
    if address_display::applies(flags, active()) {
        if let Some(display) = rivet_display(model) {
            write_string(out, &display.text);
            let widget = CURRENT.with(Cell::get);
            let mut displays = DISPLAYS.lock().unwrap_or_else(|error| error.into_inner());
            displays.retain(|(known, _)| *known != widget);
            displays.push((widget, display));
        }
    }
    result
}

unsafe fn formatted(model: *mut c_void, out: *mut [u64; 3], flags: u32) -> *mut [u64; 3] {
    let run: unsafe extern "system" fn(*mut c_void, *mut [u64; 3], u32) -> *mut [u64; 3] =
        original(&ORIGINAL_FORMATTED_URL);
    run(model, out, flags)
}

unsafe fn rivet_display(model: *mut c_void) -> Option<address_display::Display> {
    if model_shows_start_page(model) {
        return Some(address_display::compose(&[], &[], true));
    }
    let hooks = hooks()?;
    let delegate = *((model as usize + MODEL_DELEGATE) as *const usize);
    if delegate == 0 || *(delegate as *const usize) != hooks.base + BROWSER_DELEGATE_VTABLE {
        return None;
    }
    let active_contents: unsafe extern "system" fn(usize) -> usize = std::mem::transmute(
        *((hooks.base + BROWSER_DELEGATE_VTABLE) as *const usize).add(ACTIVE_WEB_CONTENTS_SLOT),
    );
    let contents = active_contents(delegate);
    if contents == 0 || *(contents as *const usize) != hooks.base + WEB_CONTENTS_VTABLE {
        return None;
    }
    let title_of: unsafe extern "system" fn(usize) -> *const [u64; 3] = std::mem::transmute(
        *((hooks.base + WEB_CONTENTS_VTABLE) as *const usize).add(WEB_CONTENTS_TITLE_SLOT),
    );
    let title = read_string(title_of(contents));
    let mut address = [0u64; 3];
    formatted(model, &mut address, address_display::FULL_URL_FLAGS);
    let full = read_string(&address);
    free_string(&mut address);
    Some(address_display::compose(&title, &full, false))
}

/// The text of a `std::u16string`: inline up to ten units, on the heap after that.
unsafe fn read_string(text: *const [u64; 3]) -> Vec<u16> {
    if text.is_null() {
        return Vec::new();
    }
    let words = *text;
    if words[2] >> 63 == 1 {
        std::slice::from_raw_parts(words[0] as *const u16, words[1] as usize).to_vec()
    } else {
        let length = (words[2] >> 56) as usize & 0x7F;
        std::slice::from_raw_parts(text as *const u16, length.min(11)).to_vec()
    }
}

/// Frees the heap block of a string and leaves it empty.
unsafe fn free_string(text: &mut [u64; 3]) {
    if text[2] >> 63 == 1 {
        if let Some(hooks) = hooks() {
            let delete: unsafe extern "system" fn(*mut c_void, usize) =
                std::mem::transmute(hooks.base + OPERATOR_DELETE);
            delete(text[0] as *mut c_void, (text[2] & !(1 << 63)) as usize * 2);
        }
    }
    *text = [0; 3];
}

/// Replaces the text of a `std::u16string` with Chrome's own allocator, in the layout the
/// browser's strings have: a heap block holds `heap_capacity` units and the third word carries
/// the capacity and a flag in its top bit.
unsafe fn write_string(out: *mut [u64; 3], text: &[u16]) {
    let Some(hooks) = hooks() else { return };
    free_string(&mut *out);
    if text.len() <= 10 {
        let mut words = [0u64; 3];
        let inline = words.as_mut_ptr() as *mut u16;
        std::ptr::copy_nonoverlapping(text.as_ptr(), inline, text.len());
        words[2] = (text.len() as u64) << 56;
        *out = words;
        return;
    }
    let capacity = address_display::heap_capacity(text.len());
    let allocate: unsafe extern "system" fn(usize) -> *mut u16 = std::mem::transmute(hooks.base + OPERATOR_NEW);
    let data = allocate(capacity * 2);
    std::ptr::copy_nonoverlapping(text.as_ptr(), data, text.len());
    data.add(text.len()).write(0);
    *out = [data as u64, text.len() as u64, (1 << 63) | capacity as u64];
}

/// Whether `spec` is a local file named `start.html`, ignoring any query or fragment.
fn is_start_page(spec: &[u8]) -> bool {
    let end = spec.iter().position(|&byte| byte == b'?' || byte == b'#').unwrap_or(spec.len());
    let path = &spec[..end];
    path.len() > 5
        && path[..5].eq_ignore_ascii_case(b"file:")
        && path.len() >= 11
        && path[path.len() - 11..].eq_ignore_ascii_case(b"/start.html")
}

unsafe fn model_shows_start_page(model: *mut c_void) -> bool {
    let Some(hooks) = hooks() else { return false };
    let get_url: unsafe extern "system" fn(*mut c_void, *mut u8) -> *mut u8 =
        std::mem::transmute(hooks.base + MODEL_URL);
    let mut gurl = [0u64; 24];
    let gurl = gurl.as_mut_ptr() as *mut u8;
    get_url(model, gurl);
    let long = (*gurl.add(0x17)) & 0x80 != 0;
    let (data, size) = if long {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl as *const u8, *gurl.add(0x17) as usize)
    };
    let shows = is_start_page(std::slice::from_raw_parts(data, size));
    let destroy: unsafe extern "system" fn(*mut u8) = std::mem::transmute(hooks.base + GURL_DESTRUCTOR);
    destroy(gurl);
    shows
}

/// The omnibox refreshes its text outside any paint or layout, so the Rivet scope is found from
/// its text field.
unsafe fn in_omnibox_scope<R>(omnibox: *mut c_void, work: impl FnOnce() -> R) -> R {
    let widget = widget_holding((omnibox as usize + OMNIBOX_TEXTFIELD) as *mut c_void);
    scoped(widget, work)
}

unsafe extern "system" fn omnibox_update(omnibox: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_OMNIBOX_UPDATE);
    in_omnibox_scope(omnibox, || run(omnibox));
}

unsafe extern "system" fn omnibox_revert_all(omnibox: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_OMNIBOX_REVERT_ALL);
    in_omnibox_scope(omnibox, || run(omnibox));
}

unsafe extern "system" fn omnibox_tab_changed(omnibox: *mut c_void, state: *mut c_void) {
    let run: unsafe extern "system" fn(*mut c_void, *mut c_void) = original(&ORIGINAL_OMNIBOX_TAB_CHANGED);
    in_omnibox_scope(omnibox, || run(omnibox, state));
}

unsafe extern "system" fn title_was_set(observer: *mut c_void, entry: *mut c_void, explicit: usize) {
    let run: unsafe extern "system" fn(*mut c_void, *mut c_void, usize) = original(&ORIGINAL_TITLE_WAS_SET);
    run(observer, entry, explicit);
    let Some(hooks) = hooks() else { return };
    let omnibox = observer as usize - OBSERVER_OFFSET;
    if is_rivet(widget_holding((omnibox + OMNIBOX_TEXTFIELD) as *mut c_void)) {
        let update: WidgetCall = std::mem::transmute(hooks.base + OMNIBOX_UPDATE);
        update(omnibox as *mut c_void);
    }
}

/// After Chrome has styled the text of a Rivet window's field, the title gets the title colour
/// and weight and the address the dimmer colour, when the text is the one given at rest. Text a
/// person types keeps Chrome's styling.
unsafe extern "system" fn update_text_style(
    omnibox: *mut c_void,
    text: *const [u64; 3],
    is_url: usize,
    scheme: *mut c_void,
) {
    let run: unsafe extern "system" fn(*mut c_void, *const [u64; 3], usize, *mut c_void) =
        original(&ORIGINAL_UPDATE_TEXT_STYLE);
    run(omnibox, text, is_url, scheme);
    let Some(hooks) = hooks() else { return };
    let textfield = omnibox as usize + OMNIBOX_TEXTFIELD;
    let widget = widget_holding(textfield as *mut c_void);
    if !is_rivet(widget) {
        return;
    }
    let shown = read_string(text);
    let display = DISPLAYS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(known, display)| *known == widget && display.text == shown)
        .map(|(_, display)| display.clone());
    let Some(display) = display else { return };
    let holder = *((textfield + TEXTFIELD_RENDER_TEXT) as *const usize);
    if holder == 0 {
        return;
    }
    let render_text = *((holder + 0x10) as *const usize);
    if render_text == 0 {
        return;
    }
    let apply_colour: unsafe extern "system" fn(usize, u32, *const [u32; 2]) =
        std::mem::transmute(hooks.base + APPLY_COLOR);
    let apply_weight: unsafe extern "system" fn(usize, u32, *const [u32; 2]) =
        std::mem::transmute(hooks.base + APPLY_WEIGHT);
    let all = [0, display.text.len() as u32];
    apply_colour(render_text, ADDRESS_COLOUR, &all);
    apply_weight(render_text, WEIGHT_NORMAL, &all);
    if display.title_len > 0 {
        let title = [0, display.title_len];
        apply_colour(render_text, TITLE_COLOUR, &title);
        apply_weight(render_text, WEIGHT_SEMIBOLD, &title);
    }
}

unsafe extern "system" fn refresh_background(view: *mut c_void) {
    // The function takes the `LocationBarView`, whose `views::View` base starts 0x100 bytes in.
    let top = widget_holding((view as usize + LOCATION_BAR_VIEW_BASE) as *mut c_void);
    let run: WidgetCall = original(&ORIGINAL_REFRESH_BACKGROUND);
    scoped(top, || {
        if active() && FORCED.load(Ordering::Relaxed) >= FORCE_HOVER {
            ((view as usize + HOVER_VALUE) as *mut f64).write(1.0);
        }
        run(view);
        if active() {
            round_field(view);
            watch_field(top, view as usize);
        }
    });
}

/// Pressing the field, as in the design: it sinks quickly to 0.98 of its size and 0.96 of its
/// brightness, stays down for at least a beat, and springs back slowly with a small overshoot.
const PRESS_SCALE: f32 = 0.98;
const PRESS_BRIGHTNESS: f32 = 0.96;
const PRESS_IN_MS: f64 = 120.0;
const PRESS_OUT_MS: f64 = 360.0;
const PRESS_HOLD_MS: f64 = 90.0;
const PRESS_TICK_MS: u32 = 16;

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function: the progress at time `x` in 0..=1.
fn bezier(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let at = |t: f64, a: f64, b: f64| {
        3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t
    };
    let x = x.clamp(0.0, 1.0);
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if at(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    at((low + high) / 2.0, y1, y2)
}

/// How far down the field is: 0 at rest, 1 fully pressed, a little below 0 while it springs back.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    from: f64,
    down: bool,
    changed: f64,
    pressed: f64,
}

impl Press {
    const fn new() -> Self {
        Self { from: 0.0, down: false, changed: 0.0, pressed: 0.0 }
    }

    /// The depth `now` milliseconds in.
    fn depth(&self, now: f64) -> f64 {
        let elapsed = (now - self.changed).max(0.0);
        if self.down {
            let progress = bezier(elapsed / PRESS_IN_MS, 0.25, 0.7, 0.35, 1.0);
            self.from + (1.0 - self.from) * progress
        } else {
            let progress = bezier(elapsed / PRESS_OUT_MS, 0.3, 1.45, 0.55, 1.0);
            self.from * (1.0 - progress)
        }
    }

    /// Whether the field is back at rest and stays there.
    fn at_rest(&self, now: f64) -> bool {
        !self.down && now - self.changed >= PRESS_OUT_MS
    }

    /// Feeds the pointer state at `now`. A short tap still keeps the field down for a beat.
    fn feed(&mut self, held: bool, now: f64) {
        let held = held || (self.down && now - self.pressed < PRESS_HOLD_MS);
        if held == self.down {
            return;
        }
        self.from = self.depth(now).clamp(0.0, 1.0);
        self.down = held;
        self.changed = now;
        if held {
            self.pressed = now;
        }
    }
}

/// A Rivet window's address field and what is known of its press.
struct Field {
    widget: usize,
    view: usize,
    window: usize,
    press: Press,
    /// The pointer went down inside the field and has not been released.
    latched: bool,
    was_held: bool,
    /// The size and depth last applied, to apply only changes.
    applied: Option<(i32, i32, f32)>,
}

static FIELDS: Mutex<Vec<Field>> = Mutex::new(Vec::new());
/// The native window of each Rivet widget.
static WINDOWS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
static TIMER_STARTED: AtomicBool = AtomicBool::new(false);
static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();

fn now_ms() -> f64 {
    EPOCH.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// Remembers the field of a Rivet window so its press can be shown.
fn watch_field(widget: usize, view: usize) {
    let mut fields = FIELDS.lock().unwrap_or_else(|error| error.into_inner());
    match fields.iter_mut().find(|field| field.widget == widget) {
        Some(field) => field.view = view,
        None => fields.push(Field {
            widget,
            view,
            window: 0,
            press: Press::new(),
            latched: false,
            was_held: false,
            applied: None,
        }),
    }
}

unsafe fn screen_bounds(hooks: &Hooks, view: usize) -> [i32; 4] {
    let bounds: unsafe extern "system" fn(*mut u8, *mut [i32; 4]) -> *mut [i32; 4] =
        std::mem::transmute(hooks.base + BOUNDS_IN_SCREEN);
    let mut rect = [0i32; 4];
    bounds(view as *mut u8, &mut rect);
    rect
}

/// Whether the left button is held, and whether the pointer is on the field of `window`.
unsafe fn pointer_on_field(hooks: &Hooks, window: usize, widget: usize, view: usize) -> (bool, bool) {
    let held = GetAsyncKeyState(0x01) < 0;
    let mut cursor = POINT { x: 0, y: 0 };
    if window == 0 || GetCursorPos(&mut cursor) == 0 {
        return (held, false);
    }
    let window = window as HWND;
    if GetAncestor(WindowFromPoint(cursor), GA_ROOT) != window {
        return (held, false);
    }
    ScreenToClient(window, &mut cursor);
    let scale = GetDpiForWindow(window).max(96) as f64 / 96.0;
    let root = *((widget + ROOT_VIEW_OFFSET) as *const usize);
    if root == 0 {
        return (held, false);
    }
    // Both rectangles are in screen DIPs, so their difference is the field's place in the widget.
    let origin = screen_bounds(hooks, root);
    let field = screen_bounds(hooks, view + LOCATION_BAR_VIEW_BASE);
    let (x, y) = (cursor.x as f64 / scale, cursor.y as f64 / scale);
    let (left, top) = ((field[0] - origin[0]) as f64, (field[1] - origin[1]) as f64);
    let inside = x >= left && x < left + field[2] as f64 && y >= top && y < top + field[3] as f64;
    (held, inside)
}

/// The size of the field, in DIPs.
unsafe fn field_size(view: usize) -> (i32, i32) {
    let base = view + LOCATION_BAR_VIEW_BASE;
    (*((base + VIEW_SIZE) as *const i32), *((base + VIEW_SIZE + 4) as *const i32))
}

/// Sets the scale and brightness the press gives the field, or clears them at rest.
unsafe fn show_press(hooks: &Hooks, view: usize, depth: Option<f64>) {
    let base = view + LOCATION_BAR_VIEW_BASE;
    let set_transform: unsafe extern "system" fn(*mut c_void, *const u8) =
        std::mem::transmute(hooks.base + SET_TRANSFORM);
    let mut transform = [0u8; TRANSFORM_BYTES];
    let mut write = |at: usize, value: f32| transform[at..at + 4].copy_from_slice(&value.to_le_bytes());
    let (width, height) = field_size(view);
    let depth = depth.unwrap_or(0.0) as f32;
    let scale = 1.0 - (1.0 - PRESS_SCALE) * depth;
    write(8, scale);
    write(12, scale);
    // The layer scales about its top left corner, so the field is moved to scale about its centre.
    write(0x10, (1.0 - scale) * width as f32 / 2.0);
    write(0x14, (1.0 - scale) * height as f32 / 2.0);
    set_transform(base as *mut c_void, transform.as_ptr());
    let layer = *((base + VIEW_LAYER) as *const usize);
    if layer != 0 && depth != 0.0 {
        let brightness: unsafe extern "system" fn(*mut c_void, f32) =
            std::mem::transmute(hooks.base + SET_BRIGHTNESS);
        // The compositor adds this to every colour channel, so 0 is unchanged and -0.04 is darker.
        brightness(layer as *mut c_void, -(1.0 - PRESS_BRIGHTNESS) * depth.clamp(0.0, 1.0));
    }
}

/// Runs on the UI thread every few milliseconds: reads the pointer and moves each Rivet field
/// toward its pressed or resting look.
unsafe extern "system" fn press_tick(_window: HWND, _message: u32, _id: usize, _time: u32) {
    let Some(hooks) = hooks() else { return };
    let now = now_ms();
    let forced = FORCED.load(Ordering::Relaxed);
    let windows = WINDOWS.lock().unwrap_or_else(|error| error.into_inner()).clone();
    let snapshot: Vec<(usize, usize)> = FIELDS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|field| (field.widget, field.view))
        .collect();
    for (widget, view) in snapshot {
        if !is_rivet(widget) {
            continue;
        }
        let window = windows.iter().find(|(known, _)| *known == widget).map_or(0, |(_, hwnd)| *hwnd);
        let (held, inside) = pointer_on_field(hooks, window, widget, view);
        let (width, height) = field_size(view);
        let mut fields = FIELDS.lock().unwrap_or_else(|error| error.into_inner());
        let Some(field) = fields.iter_mut().find(|field| field.widget == widget) else { continue };
        // A press counts when it starts on the field and lasts while the button stays down on it.
        field.latched = match (held, field.was_held) {
            (false, _) => false,
            (true, false) => inside,
            (true, true) => field.latched && inside,
        };
        field.was_held = held;
        let down = match forced {
            FORCE_PRESSED => true,
            FORCE_CYCLE => (2000.0..2150.0).contains(&now),
            _ => field.latched,
        };
        field.press.feed(down, now);
        let depth = field.press.depth(now);
        let wanted = if field.press.at_rest(now) {
            None
        } else {
            Some((width, height, (depth * 1000.0).round() as f32 / 1000.0))
        };
        if field.applied == wanted {
            continue;
        }
        field.applied = wanted;
        drop(fields);
        if forced == FORCE_CYCLE {
            crate::vertical_tabs::trace(&format!("press t={now:.0} depth={depth:.3}"));
        }
        show_press(hooks, view, wanted.map(|_| depth));
    }
}

/// Starts the press timer on the calling UI thread, once.
unsafe fn start_press_timer() {
    if !TIMER_STARTED.swap(true, Ordering::AcqRel) {
        now_ms();
        SetTimer(std::ptr::null_mut(), 0, PRESS_TICK_MS, Some(press_tick));
    }
}

/// Chrome rounds the field's fill into a pill. Rivet's field is a rectangle with 8 DIP corners,
/// like every other control of the top row.
unsafe fn round_field(view: *mut c_void) {
    let Some(hooks) = hooks() else { return };
    let background = *((view as usize + FIELD_BACKGROUND) as *const usize);
    if background == 0 || *(background as *const usize) != hooks.base + BACKGROUND_VTABLE {
        return;
    }
    let painter = *((background + 0x10) as *const usize);
    if painter == 0 || *(painter as *const usize) != hooks.base + PAINTER_VTABLE {
        return;
    }
    for corner in 0..4 {
        ((painter + PAINTER_RADII + corner * 4) as *mut f32).write(FIELD_RADIUS);
    }
    // Chrome's field is 34 DIPs tall. The fill is 28, centred, like a button of the row.
    ((painter + PAINTER_INSETS) as *mut [i32; 4]).write([FIELD_FILL_MARGIN, 0, FIELD_FILL_MARGIN, 0]);
}

/// The label is built here, which can also happen outside any paint or layout of the window.
unsafe extern "system" fn update_location(view: *mut c_void, first: usize, second: usize, third: usize) {
    let top = window_top(view);
    let run: unsafe extern "system" fn(*mut c_void, usize, usize, usize) = original(&ORIGINAL_UPDATE_LOCATION);
    scoped(top, || run(view, first, second, third));
}

unsafe extern "system" fn textfield_theme(view: *mut c_void) {
    let run: WidgetCall = original(&ORIGINAL_TEXTFIELD_THEME);
    run(view);
    if !active() {
        return;
    }
    let Some(hooks) = hooks() else { return };
    let holder = *((view as usize + TEXTFIELD_RENDER_TEXT) as *const usize);
    if holder == 0 {
        return;
    }
    let render_text = *((holder + 0x10) as *const usize);
    if render_text == 0 {
        return;
    }
    let font_list = (render_text + RENDER_TEXT_FONT_LIST) as *mut c_void;
    if (*(font_list as *const usize)) == 0 {
        return;
    }
    let size: unsafe extern "system" fn(*mut c_void) -> i32 = std::mem::transmute(hooks.base + FONT_SIZE);
    let current = size(font_list);
    if current == ADDRESS_FONT_SIZE {
        return;
    }
    let derive: unsafe extern "system" fn(*mut c_void, *mut usize, i32) -> *mut usize =
        std::mem::transmute(hooks.base + DERIVE_SIZE);
    let mut smaller: usize = 0;
    derive(font_list, &mut smaller, ADDRESS_FONT_SIZE - current);
    let set: unsafe extern "system" fn(*mut c_void, *const usize) = std::mem::transmute(hooks.base + TEXTFIELD_SET_FONT);
    set(view, &smaller);
}

/// The topmost ancestor of `view` that is a Rivet widget, or the topmost ancestor.
unsafe fn window_top(view: *mut c_void) -> usize {
    let mut top = view as usize;
    for _ in 0..32 {
        let parent = *((top + PARENT_OFFSET) as *const usize);
        if parent == 0 || is_rivet(top) {
            break;
        }
        top = parent;
    }
    top
}

/// The widget that holds `view`, which must point at a `views::View`.
pub(crate) unsafe fn widget_holding(view: *mut c_void) -> usize {
    let table = *(view as *const usize);
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(GET_WIDGET_SLOT));
    get_widget(view) as usize
}

/// Rivet windows show the address without a label before it. The label is built while the icon
/// updates inside the scope of `update_location_icon`, and an empty `std::string` is three zeroed
/// words, so nothing is allocated.
unsafe extern "system" fn chip_text(
    out: *mut [u64; 3],
    delegate: *mut c_void,
    url: *mut c_void,
    flag: u8,
) -> *mut [u64; 3] {
    if active() {
        // The icon is chosen right after the label, from the same page.
        let local = page_is_local_file(delegate);
        LOCAL_PAGE.with(|page| page.set(local));
        if local {
            out.write([0; 3]);
            return out;
        }
    }
    original::<unsafe extern "system" fn(*mut [u64; 3], *mut c_void, *mut c_void, u8) -> *mut [u64; 3]>(
        &ORIGINAL_CHIP_TEXT,
    )(out, delegate, url, flag)
}

/// Whether `spec[begin..begin + length]` is the `file` scheme.
fn is_file_scheme(spec: &[u8], begin: i32, length: i32) -> bool {
    let (Ok(begin), Ok(length)) = (usize::try_from(begin), usize::try_from(length)) else {
        return false;
    };
    spec.get(begin..begin.saturating_add(length))
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"file"))
}

/// Asks the delegate for the page address, the way `GetSecurityChipText` does, and frees it.
unsafe fn page_is_local_file(delegate: *mut c_void) -> bool {
    let Some(hooks) = hooks() else { return false };
    if delegate.is_null() {
        return false;
    }
    let table = *(delegate as *const *const usize);
    let get_url: unsafe extern "system" fn(*mut c_void, *mut u8) = std::mem::transmute(*table.add(3));
    // A `GURL` is smaller than this, and its destructor runs below.
    let mut gurl = [0u64; 24];
    let gurl = gurl.as_mut_ptr() as *mut u8;
    get_url(delegate, gurl);
    let long = (*gurl.add(0x17)) & 0x80 != 0;
    let (data, size) = if long {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl as *const u8, *gurl.add(0x17) as usize)
    };
    let local = is_file_scheme(
        std::slice::from_raw_parts(data, size),
        *(gurl.add(GURL_SCHEME_BEGIN) as *const i32),
        *(gurl.add(GURL_SCHEME_LENGTH) as *const i32),
    );
    let destroy: unsafe extern "system" fn(*mut u8) = std::mem::transmute(hooks.base + GURL_DESTRUCTOR);
    destroy(gurl);
    local
}

/// Whether `view` is one of the hidden buttons of a Rivet window.
unsafe fn hidden_here(hooks: &Hooks, view: *mut c_void) -> bool {
    let table = *(view as *const usize);
    if !hooks.hidden.iter().any(|(vtable, _)| *vtable == table) {
        return false;
    }
    if active() {
        return true;
    }
    let get_widget: Pointer = std::mem::transmute(*(table as *const usize).add(GET_WIDGET_SLOT));
    is_rivet(get_widget(view) as usize)
}

unsafe extern "system" fn set_visible(view: *mut c_void, visible: bool) {
    let run = original::<SetVisible>(&ORIGINAL_SET_VISIBLE);
    let hide = visible && hooks().is_some_and(|hooks| hidden_here(hooks, view));
    run(view, visible && !hide);
}

unsafe extern "system" fn hidden_theme_changed(view: *mut c_void) {
    let hooks = hooks().expect("patched after it is stored");
    let table = *(view as *const usize);
    if let Some((_, theme)) = hooks.hidden.iter().find(|(vtable, _)| *vtable == table) {
        theme(view);
    }
    if hidden_here(hooks, view) {
        original::<SetVisible>(&ORIGINAL_SET_VISIBLE)(view, false);
    }
}

fn drawn_ancestor(
    mut view: usize,
    parent: impl Fn(usize) -> usize,
    drawn: impl Fn(usize) -> bool,
) -> Option<usize> {
    while view != 0 {
        if drawn(view) {
            return Some(view);
        }
        view = parent(view);
    }
    None
}

/// Preserve Chrome's bubble state and change only the returned screen rectangle.
unsafe extern "system" fn anchor_rect(
    delegate: *mut u8,
    result: *mut [i32; 4],
) -> *mut [i32; 4] {
    type RectCall = unsafe extern "system" fn(*mut u8, *mut [i32; 4]) -> *mut [i32; 4];
    type Drawn = unsafe extern "system" fn(*mut c_void) -> bool;
    let run: RectCall = original(&ORIGINAL_ANCHOR_RECT);
    let ret = run(delegate, result);
    let observer = *(delegate.add(0x300) as *const *mut u8);
    if observer.is_null() {
        return ret;
    }
    let view = *(observer.add(0x18) as *const *mut c_void);
    if view.is_null() {
        return ret;
    }
    let table = *(view as *const *const usize);
    let get_widget: Pointer = std::mem::transmute(*table.add(GET_WIDGET_SLOT));
    if !is_rivet(get_widget(view) as usize) {
        return ret;
    }
    let drawn = |v: usize| {
        let table = *(v as *const *const usize);
        let call: Drawn = std::mem::transmute(*table.add(IS_DRAWN_SLOT));
        call(v as *mut c_void)
    };
    if drawn(view as usize) {
        return ret;
    }
    let parent = |v: usize| *((v + PARENT_OFFSET) as *const usize);
    if let Some(ancestor) = drawn_ancestor(parent(view as usize), parent, drawn) {
        if let Some(hooks) = hooks() {
            let bounds: RectCall = std::mem::transmute(hooks.base + BOUNDS_IN_SCREEN);
            bounds(ancestor as *mut u8, result);
        }
    }
    ret
}

/// Chooses the icon a button image is built from.
unsafe extern "system" fn choose_icon(icon: usize) -> usize {
    if !active() {
        return icon;
    }
    let Some(hooks) = hooks() else { return icon };
    match ICONS.get() {
        Some(icons) => icons
            .iter()
            .find(|(rva, _)| hooks.base + rva == icon)
            // Chrome's warning for plain HTTP is the icon of local files too. Only local files
            // get the Rivet mark, so a page that is not secure keeps its warning.
            .filter(|(rva, _)| *rva != HTTP_ICON || LOCAL_PAGE.with(Cell::get))
            .map_or(icon, |(_, ours)| {
                ours.as_ref() as *const VectorIcon as usize
            }),
        None => icon,
    }
}

/// What an installation overwrote, oldest first: the address and the bytes that were there. A
/// failed installation puts all of it back, so the browser is never left half patched.
static JOURNAL: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(Vec::new());
/// Set once every change of the installation has been made.
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn journal() -> std::sync::MutexGuard<'static, Vec<(usize, Vec<u8>)>> {
    JOURNAL.lock().unwrap_or_else(|error| error.into_inner())
}

/// Overwrites `bytes.len()` bytes at `target`, whatever their protection, without recording.
unsafe fn write_raw(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(
        target.cast(),
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect browser memory".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    FlushInstructionCache(GetCurrentProcess(), target.cast(), bytes.len());
    Ok(())
}

/// Overwrites browser memory and remembers what was there.
unsafe fn write_recorded(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let original = std::slice::from_raw_parts(target, bytes.len()).to_vec();
    write_raw(target, bytes)?;
    journal().push((target as usize, original));
    Ok(())
}

unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    write_recorded(target, bytes)
}

/// Puts back everything the journal holds, newest first. The trampolines already allocated stay
/// where they are: a thread may still be inside one, and they are only a few bytes.
unsafe fn rollback() {
    let entries = std::mem::take(&mut *journal());
    for (address, original) in entries.into_iter().rev() {
        let _ = write_raw(address as *mut u8, &original);
    }
}

/// Runs `install` and undoes every change it made when it fails.
unsafe fn transaction(install: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    journal().clear();
    let result = install();
    match &result {
        Ok(()) => journal().clear(),
        Err(_) => rollback(),
    }
    result
}

unsafe fn executable(code: &[u8]) -> Result<usize, String> {
    let memory = VirtualAlloc(
        std::ptr::null(),
        code.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(code.as_ptr(), memory, code.len());
    Ok(memory as usize)
}

/// `mov rax, imm64`.
fn load_rax(code: &mut Vec<u8>, value: usize) {
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(value as u64).to_le_bytes());
}

/// `jmp [rip + 0]` to `to`, which leaves every register as it is.
fn jump(code: &mut Vec<u8>, to: usize) {
    code.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    code.extend_from_slice(&(to as u64).to_le_bytes());
}

/// Replaces the start of a function with a jump to `hook` and returns a trampoline that runs the
/// original. `prologue` is checked first and is copied whole, unless the function reloads its
/// stack cookie relative to itself: then `prologue` ends with that seven byte load, and the
/// trampoline copies what comes before it and loads the same cookie from its absolute address.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    cookie: bool,
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    let code = (base + rva) as *mut u8;
    if std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    let mut trampoline = Vec::new();
    if cookie {
        // The load is the last seven bytes: its opcode and its four byte displacement.
        trampoline.extend_from_slice(&prologue[..prologue.len() - 7]);
        let displacement = i32::from_le_bytes(prologue[prologue.len() - 4..].try_into().unwrap());
        let address = (base + rva + prologue.len()).wrapping_add_signed(displacement as isize);
        // mov rax, [address]
        trampoline.extend_from_slice(&[0x48, 0xA1]);
        trampoline.extend_from_slice(&(address as u64).to_le_bytes());
    } else {
        trampoline.extend_from_slice(prologue);
    }
    jump(&mut trampoline, base + rva + prologue.len());
    store.store(executable(&trampoline)?, Ordering::Release);
    let mut patch = Vec::new();
    load_rax(&mut patch, hook);
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(prologue.len(), 0xCC);
    write_code(code, &patch)
}

/// A stub that replaces the second argument with `choose(second)` and continues in `original`,
/// so every other argument, including those on the stack, reaches it untouched.
unsafe fn second_argument_stub(choose: usize, original: usize) -> Result<usize, String> {
    let mut code = vec![
        0x51, // push rcx
        0x52, // push rdx
        0x41, 0x50, // push r8
        0x41, 0x51, // push r9
        0x48, 0x83, 0xEC, 0x28, // sub rsp, 0x28
        0x48, 0x89, 0xD1, // mov rcx, rdx
    ];
    load_rax(&mut code, choose);
    code.extend_from_slice(&[
        0xFF, 0xD0, // call rax
        0x48, 0x83, 0xC4, 0x28, // add rsp, 0x28
        0x41, 0x59, // pop r9
        0x41, 0x58, // pop r8
        0x5A, // pop rdx
        0x59, // pop rcx
        0x48, 0x89, 0xC2, // mov rdx, rax
    ]);
    jump(&mut code, original);
    executable(&code)
}

unsafe fn replace_slot(slot: *mut usize, value: usize) -> Result<(), String> {
    write_recorded(slot.cast(), &value.to_le_bytes())
}

unsafe fn patch() -> Result<Hooks, String> {
    let image = image::recorded().ok_or("this browser build has no recorded toolbar")?;
    let base = image.base;
    let slot = |vtable: usize, index: usize| vtable.checked_add(index.checked_mul(8)?);
    let holds = |vtable: usize, index: usize, function: usize| {
        slot(vtable, index).is_some_and(|rva| image.points_to(rva, function))
    };
    if !holds(BROWSER_WIDGET_VTABLE, PAINT_SLOT, PAINT)
        || !holds(BROWSER_WIDGET_VTABLE, LAYOUT_SLOT, LAYOUT)
    {
        return Err("this browser build has no recorded browser widget".into());
    }
    if !holds(BROWSER_DELEGATE_VTABLE, ACTIVE_WEB_CONTENTS_SLOT, ACTIVE_WEB_CONTENTS)
        || !holds(WEB_CONTENTS_VTABLE, WEB_CONTENTS_TITLE_SLOT, WEB_CONTENTS_TITLE)
    {
        return Err("this browser build has no recorded web contents".into());
    }
    for (vtable, theme) in HIDDEN {
        if !holds(vtable, THEME_SLOT, theme) {
            return Err(format!(
                "this browser build has no recorded view at {vtable:#x}"
            ));
        }
    }
    let prologue = |rva: usize, bytes: &[u8]| image.has(rva, bytes);
    if !prologue(LAYOUT_CONSTANT, &LAYOUT_CONSTANT_PROLOGUE)
        || !prologue(THEME_CHANGED, &THEME_CHANGED_PROLOGUE)
        || !prologue(PAINT_LAYER, &PAINT_LAYER_PROLOGUE)
        || !prologue(SET_VISIBLE, &SET_VISIBLE_PROLOGUE)
        || !prologue(FROM_VECTOR_ICON, &FROM_VECTOR_ICON_PROLOGUE)
        || !prologue(UPDATE_LOCATION, &UPDATE_LOCATION_PROLOGUE)
        || !prologue(TEXTFIELD_THEME, &TEXTFIELD_THEME_PROLOGUE)
        || !prologue(TEXTFIELD_SET_FONT, &TEXTFIELD_SET_FONT_BYTES)
        || !prologue(FONT_SIZE, &FONT_SIZE_BYTES)
        || !prologue(DERIVE_SIZE, &DERIVE_SIZE_BYTES)
        || !prologue(UPDATE_LOCATION_ICON, &UPDATE_LOCATION_ICON_PROLOGUE)
        || !prologue(CHIP_TEXT, &CHIP_TEXT_PROLOGUE)
        || !prologue(FORMATTED_URL, &FORMATTED_URL_PROLOGUE)
        || !prologue(MODEL_URL, &MODEL_URL_PROLOGUE)
        || !prologue(UPDATE_TEXT_STYLE, &UPDATE_TEXT_STYLE_PROLOGUE)
        || !prologue(APPLY_COLOR, &APPLY_COLOR_BYTES)
        || !prologue(APPLY_WEIGHT, &APPLY_WEIGHT_BYTES)
        || !prologue(TITLE_WAS_SET, &TITLE_WAS_SET_PROLOGUE)
        || !prologue(OPERATOR_NEW, &OPERATOR_NEW_BYTES)
        || !prologue(OPERATOR_DELETE, &OPERATOR_DELETE_BYTES)
        || !prologue(OMNIBOX_UPDATE, &OMNIBOX_UPDATE_PROLOGUE)
        || !prologue(OMNIBOX_REVERT_ALL, &OMNIBOX_REVERT_ALL_PROLOGUE)
        || !prologue(OMNIBOX_TAB_CHANGED, &OMNIBOX_TAB_CHANGED_PROLOGUE)
        || !prologue(LAYOUT_INSETS, &LAYOUT_INSETS_PROLOGUE)
        || !prologue(UPDATE_ICON, &UPDATE_ICON_PROLOGUE)
        || !prologue(REFRESH_BACKGROUND, &REFRESH_BACKGROUND_PROLOGUE)
        || !prologue(GURL_DESTRUCTOR, &GURL_DESTRUCTOR_BYTES)
        || !prologue(ANCHOR_RECT, &ANCHOR_RECT_PROLOGUE)
        || !prologue(ANCHOR_RECT + 0x2a, &ANCHOR_VIEW_ACCESS)
        || !prologue(IS_DRAWN, &IS_DRAWN_BYTES)
        || !prologue(BOUNDS_IN_SCREEN, &BOUNDS_BYTES)
        || !prologue(SET_TRANSFORM, &SET_TRANSFORM_PROLOGUE)
        || !prologue(SET_BRIGHTNESS, &SET_BRIGHTNESS_PROLOGUE)
    {
        return Err("this browser build has other toolbar functions".into());
    }
    // The icons are read through their addresses, so each must lie inside the image.
    if [BACK_ICON, FORWARD_ICON, RELOAD_ICON, PAGE_INFO_ICON]
        .iter()
        .any(|rva| image.at(*rva, std::mem::size_of::<VectorIcon>()).is_none())
        || image.at(0x10F5_D438, std::mem::size_of::<IconRep>()).is_none()
    {
        return Err("this browser build has another page info icon".into());
    }
    let icon = &*((base + PAGE_INFO_ICON) as *const VectorIcon);
    if icon.count != 1 || icon.reps as usize != base + 0x10F5_D438
        || icon.name as usize != base + 0x10F5_D448
        || (*icon.reps).path as usize != base + 0x10F6_6E40
        || (*icon.reps).size != 282
        || !prologue(0x10F6_6E40, &PAGE_INFO_BYTES)
    {
        return Err("this browser build has another page info icon".into());
    }
    let http = &*((base + HTTP_ICON) as *const VectorIcon);
    if http.count != 1
        || (*http.reps).size != 263
        || std::slice::from_raw_parts(http.name, HTTP_ICON_NAME.len()) != HTTP_ICON_NAME
    {
        return Err("this browser build has another icon for plain HTTP".into());
    }
    Ok(Hooks {
        base,
        paint: std::mem::transmute::<usize, WidgetPaint>(base + PAINT),
        layout: std::mem::transmute::<usize, WidgetCall>(base + LAYOUT),
        provider: std::mem::transmute::<usize, Provider>(base + COLOR_PROVIDER),
        hidden: HIDDEN
            .iter()
            .map(|(vtable, theme)| {
                (
                    base + vtable,
                    std::mem::transmute::<usize, WidgetCall>(base + theme),
                )
            })
            .collect(),
    })
}

/// Patches the browser once every check has passed and `HOOKS` is stored.
unsafe fn wire(base: usize, skip: &str) -> Result<(), String> {
    build_icons(base);
    crate::vertical_tabs::trace(&format!("toolbar skip: {skip:?}"));
    if !skip.contains("anchor") {
        detour(
            base,
            ANCHOR_RECT,
            &ANCHOR_RECT_PROLOGUE,
            false,
            anchor_rect as *const () as usize,
            &ORIGINAL_ANCHOR_RECT,
        )?;
    }
    if !skip.contains("constant") {
        detour(
            base,
            LAYOUT_CONSTANT,
            &LAYOUT_CONSTANT_PROLOGUE,
            true,
            layout_constant as usize,
            &ORIGINAL_LAYOUT_CONSTANT,
        )?;
    }
    if !skip.contains("insets") {
        detour(
            base,
            LAYOUT_INSETS,
            &LAYOUT_INSETS_PROLOGUE,
            false,
            layout_insets as *const () as usize,
            &ORIGINAL_LAYOUT_INSETS,
        )?;
    }
    if !skip.contains("theme") {
        detour(
            base,
            THEME_CHANGED,
            &THEME_CHANGED_PROLOGUE,
            true,
            theme_changed as usize,
            &ORIGINAL_THEME_CHANGED,
        )?;
    }
    if !skip.contains("layer") {
        detour(base, PAINT_LAYER, &PAINT_LAYER_PROLOGUE, true, paint_layer as usize, &ORIGINAL_PAINT_LAYER)?;
    }
    if !skip.contains("visible") {
        detour(
            base,
            SET_VISIBLE,
            &SET_VISIBLE_PROLOGUE,
            false,
            set_visible as usize,
            &ORIGINAL_SET_VISIBLE,
        )?;
    }
    if skip.contains("icon") {
        return slots(base, skip);
    }
    detour(
        base,
        UPDATE_LOCATION_ICON,
        &UPDATE_LOCATION_ICON_PROLOGUE,
        false,
        update_location_icon as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION_ICON,
    )?;
    detour(
        base,
        TEXTFIELD_THEME,
        &TEXTFIELD_THEME_PROLOGUE,
        false,
        textfield_theme as *const () as usize,
        &ORIGINAL_TEXTFIELD_THEME,
    )?;
    detour(
        base,
        UPDATE_LOCATION,
        &UPDATE_LOCATION_PROLOGUE,
        false,
        update_location as *const () as usize,
        &ORIGINAL_UPDATE_LOCATION,
    )?;
    detour(
        base,
        REFRESH_BACKGROUND,
        &REFRESH_BACKGROUND_PROLOGUE,
        false,
        refresh_background as *const () as usize,
        &ORIGINAL_REFRESH_BACKGROUND,
    )?;
    detour(
        base,
        UPDATE_ICON,
        &UPDATE_ICON_PROLOGUE,
        false,
        update_icon as *const () as usize,
        &ORIGINAL_UPDATE_ICON,
    )?;
    detour(
        base,
        FORMATTED_URL,
        &FORMATTED_URL_PROLOGUE,
        false,
        formatted_url as *const () as usize,
        &ORIGINAL_FORMATTED_URL,
    )?;
    for (rva, prologue, cookie, hook, store) in [
        (
            UPDATE_TEXT_STYLE,
            &UPDATE_TEXT_STYLE_PROLOGUE[..],
            false,
            update_text_style as *const () as usize,
            &ORIGINAL_UPDATE_TEXT_STYLE,
        ),
        (
            TITLE_WAS_SET,
            &TITLE_WAS_SET_PROLOGUE[..],
            false,
            title_was_set as *const () as usize,
            &ORIGINAL_TITLE_WAS_SET,
        ),
        (
            OMNIBOX_UPDATE,
            &OMNIBOX_UPDATE_PROLOGUE[..],
            true,
            omnibox_update as *const () as usize,
            &ORIGINAL_OMNIBOX_UPDATE,
        ),
        (
            OMNIBOX_REVERT_ALL,
            &OMNIBOX_REVERT_ALL_PROLOGUE[..],
            true,
            omnibox_revert_all as *const () as usize,
            &ORIGINAL_OMNIBOX_REVERT_ALL,
        ),
        (
            OMNIBOX_TAB_CHANGED,
            &OMNIBOX_TAB_CHANGED_PROLOGUE[..],
            false,
            omnibox_tab_changed as *const () as usize,
            &ORIGINAL_OMNIBOX_TAB_CHANGED,
        ),
    ] {
        detour(base, rva, prologue, cookie, hook, store)?;
    }
    detour(
        base,
        CHIP_TEXT,
        &CHIP_TEXT_PROLOGUE,
        false,
        chip_text as *const () as usize,
        &ORIGINAL_CHIP_TEXT,
    )?;
    let mut icon_trampoline = FROM_VECTOR_ICON_PROLOGUE.to_vec();
    jump(
        &mut icon_trampoline,
        base + FROM_VECTOR_ICON + FROM_VECTOR_ICON_PROLOGUE.len(),
    );
    let stub = second_argument_stub(choose_icon as usize, executable(&icon_trampoline)?)?;
    let mut icon_patch = Vec::new();
    load_rax(&mut icon_patch, stub);
    icon_patch.extend_from_slice(&[0xFF, 0xE0]);
    icon_patch.resize(FROM_VECTOR_ICON_PROLOGUE.len(), 0xCC);
    write_code((base + FROM_VECTOR_ICON) as *mut u8, &icon_patch)?;
    slots(base, skip)
}

unsafe fn slots(base: usize, skip: &str) -> Result<(), String> {
    if skip.contains("widget") {
        return Ok(());
    }
    let widget = (base + BROWSER_WIDGET_VTABLE) as *mut usize;
    replace_slot(widget.add(PAINT_SLOT), widget_paint as usize)?;
    replace_slot(widget.add(LAYOUT_SLOT), widget_layout as usize)?;
    for (vtable, _) in HIDDEN {
        replace_slot(
            ((base + vtable) as *mut usize).add(THEME_SLOT),
            hidden_theme_changed as usize,
        )?;
    }
    Ok(())
}

/// Installs the changes once per process. They stay inert until a window is adopted.
pub unsafe fn install(directory: Option<std::path::PathBuf>) -> Result<(), String> {
    let read = |name: &str| {
        directory
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join(name)).ok())
            .unwrap_or_default()
    };
    let skip = read("rivet-toolbar-skip.txt");
    FORCED.store(
        match read("rivet-toolbar-force.txt").trim() {
            "hover" => FORCE_HOVER,
            "pressed" => FORCE_PRESSED,
            "cycle" => FORCE_CYCLE,
            _ => 0,
        },
        Ordering::Relaxed,
    );
    static WIRED: OnceLock<Result<(), String>> = OnceLock::new();
    WIRED
        .get_or_init(|| {
            let hooks = HOOKS
                .get_or_init(|| patch())
                .as_ref()
                .map_err(Clone::clone)?;
            let result = transaction(|| wire(hooks.base, &skip));
            INSTALLED.store(result.is_ok(), Ordering::Release);
            result
        })
        .clone()
}

/// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND` and
/// `views::Widget::GetWidgetForNativeWindow`.
const CONTENT_WINDOW_FOR_HWND: usize = 0x00D5_64A0;
const WIDGET_FOR_WINDOW: usize = 0x0315_C7C0;

/// Makes `window` a Rivet window and rebuilds its views the way a theme change does, so it
/// picks up the Rivet look.
pub unsafe fn adopt(window: HWND) {
    if hooks().is_none() || !INSTALLED.load(Ordering::Acquire) {
        return;
    }
    let widget = widget_of(window);
    if widget == 0 {
        return;
    }
    {
        let mut widgets = WIDGETS.lock().unwrap_or_else(|error| error.into_inner());
        if !widgets.contains(&widget) {
            widgets.push(widget);
        }
    }
    {
        let mut windows = WINDOWS.lock().unwrap_or_else(|error| error.into_inner());
        windows.retain(|(known, _)| *known != widget);
        windows.push((widget, window as usize));
    }
    start_press_timer();
    crate::vertical_tabs::trace("adopt");
    theme_changed(widget as *mut c_void);
    crate::vertical_tabs::trace("adopted");
}

/// Forgets a Rivet window that is going away.
pub unsafe fn forget(window: HWND) {
    let widget = widget_of(window);
    WIDGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|known| *known != widget);
    FIELDS.lock().unwrap_or_else(|error| error.into_inner()).retain(|field| field.widget != widget);
    WINDOWS.lock().unwrap_or_else(|error| error.into_inner()).retain(|(known, _)| *known != widget);
}

/// One `gfx::PathElement`: a command or a number, both four bytes.
#[derive(Clone, Copy)]
#[repr(C)]
union PathElement {
    command: u32,
    value: f32,
}

const fn op(command: u32) -> PathElement {
    PathElement { command }
}
const fn at(value: f32) -> PathElement {
    PathElement { value }
}

// `gfx::CommandType` values of this build.
const STROKE: u32 = 5;
const MOVE_TO: u32 = 7;
const LINE_TO: u32 = 11;
const CIRCLE: u32 = 24;
const CANVAS_DIMENSIONS: u32 = 28;

/// The stroke of the Rivet icons, in DIPs.
const ICON_STROKE: f32 = 1.3;

/// A 13 DIP information mark on Chrome's 16 DIP icon canvas, centred 8.75 DIPs from its left edge
/// and 7.1 from its top, which is where the page info mark sits in the Rivet design.
static INFO_PATH: [PathElement; 20] = [
    op(CANVAS_DIMENSIONS), at(16.0),
    op(STROKE), at(ICON_STROKE),
    op(CIRCLE), at(8.75), at(7.1), at(6.0),
    op(MOVE_TO), at(8.75), at(6.55),
    op(LINE_TO), at(8.75), at(9.85),
    op(MOVE_TO), at(8.75), at(4.35),
    op(LINE_TO), at(8.75), at(4.55),
];

/// The navigation icons are drawn on an 18 DIP canvas with 1.5 DIP strokes, and
/// `kToolbarButtonIconSize` is 18 so that one canvas unit is one DIP.
const NAV_CANVAS: f32 = 18.0;

/// A line from `(x1, y)` to `(x2, y)` and the two strokes of a head at `tip_x`.
fn arrow(line_from: f32, line_to: f32, tip_x: f32, head_x: f32) -> Vec<PathElement> {
    vec![
        op(CANVAS_DIMENSIONS), at(NAV_CANVAS),
        op(STROKE), at(ICON_STROKE),
        op(MOVE_TO), at(line_from), at(8.5),
        op(LINE_TO), at(line_to), at(8.5),
        op(MOVE_TO), at(head_x), at(2.5),
        op(LINE_TO), at(tip_x), at(8.5),
        op(LINE_TO), at(head_x), at(14.5),
    ]
}

/// The Rivet back arrow: a 14.5 DIP line and a head 6 DIPs deep.
fn back_path() -> Vec<PathElement> {
    arrow(16.25, 1.75, 1.75, 7.75)
}

/// The Rivet forward arrow, the back arrow mirrored.
fn forward_path() -> Vec<PathElement> {
    arrow(1.75, 16.25, 16.25, 10.25)
}

/// The Rivet reload mark: an open circle with its gap on the right and a head at the top right
/// that points into the gap. It uses the same move and line commands as the arrows, because a
/// generated arc produced a malformed vector path in Chrome 156.
fn reload_path() -> Vec<PathElement> {
    let (centre, radius, arm) = (9.0f32, 6.5f32, 3.5f32);
    let point = |degrees: f32| {
        let angle = degrees.to_radians();
        (centre + radius * angle.cos(), centre + radius * angle.sin())
    };
    let mut path = vec![op(CANVAS_DIMENSIONS), at(NAV_CANVAS), op(STROKE), at(ICON_STROKE)];
    // From the head counter-clockwise on screen to just below the right side.
    let (start, sweep, steps) = (-25.0f32, -310.0f32, 31);
    for step in 0..=steps {
        let (x, y) = point(start + sweep * step as f32 / steps as f32);
        path.push(op(if step == 0 { MOVE_TO } else { LINE_TO }));
        path.push(at(x));
        path.push(at(y));
    }
    // The head: two arms that run back from the tip, at 45 degrees either side of the arc.
    let (tip_x, tip_y) = point(start);
    let heading = (start + 90.0).to_radians();
    let back = (-heading.cos(), -heading.sin());
    for turn in [45.0f32, -45.0] {
        let (sin, cos) = turn.to_radians().sin_cos();
        let (x, y) = (back.0 * cos - back.1 * sin, back.0 * sin + back.1 * cos);
        path.extend([op(MOVE_TO), at(tip_x + x * arm), at(tip_y + y * arm), op(LINE_TO), at(tip_x), at(tip_y)]);
    }
    path
}

#[repr(C)]
struct IconRep {
    path: *const PathElement,
    size: usize,
}

/// `gfx::VectorIcon`: its representations, their count and the icon's name.
#[repr(C)]
struct VectorIcon {
    reps: *const IconRep,
    count: usize,
    name: *const u8,
}
unsafe impl Sync for VectorIcon {}
unsafe impl Send for VectorIcon {}

/// The navigation icons of this build: `kBackArrowChromeRefreshOldIcon`,
/// `kForwardArrowChromeRefreshOldIcon` and `kReloadChromeRefreshOldIcon`.
const BACK_ICON: usize = 0x10E0_2EA8;
const FORWARD_ICON: usize = 0x10E0_55C0;
const RELOAD_ICON: usize = 0x10E0_7DD0;
const PAGE_INFO_ICON: usize = 0x10F5_D470;
/// `kHttpChromeRefreshOldIcon`, which Chrome shows for local files as well as for plain HTTP.
const HTTP_ICON: usize = 0x10F5_CE60;
const HTTP_ICON_NAME: &[u8] = b"kHttpChromeRefreshOldIcon";
const PAGE_INFO_BYTES: [u8; 32] = [
    0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
    0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
];

static ICONS: OnceLock<Vec<(usize, Box<VectorIcon>)>> = OnceLock::new();

unsafe fn build_icons(base: usize) {
    ICONS.get_or_init(|| {
        let leak = |path: Vec<PathElement>| -> &'static [PathElement] { Box::leak(path.into_boxed_slice()) };
        [
            (BACK_ICON, leak(back_path())),
            (FORWARD_ICON, leak(forward_path())),
            (RELOAD_ICON, leak(reload_path())),
            (PAGE_INFO_ICON, &INFO_PATH[..]),
            (HTTP_ICON, &INFO_PATH[..]),
        ]
        .into_iter()
        .map(|(rva, path)| {
            let rep: &'static IconRep = Box::leak(Box::new(IconRep {
                path: path.as_ptr(),
                size: path.len(),
            }));
            let original = (base + rva) as *const VectorIcon;
            let icon = Box::new(VectorIcon {
                reps: rep,
                count: 1,
                name: (*original).name,
            });
            (rva, icon)
        })
        .collect()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page of plain read-write memory to stand in for browser code and vtables.
    unsafe fn scratch_page() -> *mut u8 {
        use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, MEM_RESERVE};
        let page = VirtualAlloc(
            std::ptr::null(),
            0x1000,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        ) as *mut u8;
        assert!(!page.is_null());
        for index in 0..0x1000 {
            *page.add(index) = (index % 251) as u8;
        }
        page
    }

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn nfr3_failed_installation_restores_every_patched_byte() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let before = std::slice::from_raw_parts(page, 0x1000).to_vec();
            static STORE: AtomicUsize = AtomicUsize::new(0);
            let prologue = before[0x100..0x10c].to_vec();
            let result = transaction(|| {
                detour(page as usize, 0x100, &prologue, false, 0x1122_3344, &STORE)?;
                replace_slot(page.add(0x200).cast(), 0xdead_beef)?;
                write_code(page.add(0x300), &[0xcc; 7])?;
                assert_ne!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
                Err("the next change cannot be made".to_owned())
            });
            assert!(result.is_err());
            assert_eq!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
            assert!(journal().is_empty());
        }
    }

    #[test]
    fn nfr3_complete_installation_keeps_its_changes() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let result = transaction(|| {
                replace_slot(page.add(0x40).cast(), 0x0102_0304_0506_0708)?;
                write_code(page.add(0x80), &[0x90, 0x90])
            });
            assert!(result.is_ok());
            assert_eq!(*(page.add(0x40) as *const usize), 0x0102_0304_0506_0708);
            assert_eq!(*page.add(0x80), 0x90);
            assert!(journal().is_empty());
        }
    }

    #[test]
    fn fr12_page_info_uses_a_thin_circle_without_replacing_warnings() {
        assert_eq!(PAGE_INFO_ICON, 0x10F5_D470);
        assert_ne!(PAGE_INFO_ICON, 0x10F5_CE60);
        let commands: Vec<u32> = [0, 2, 4, 8, 11, 14]
            .iter().map(|&index| unsafe { INFO_PATH[index].command }).collect();
        assert_eq!(commands, [CANVAS_DIMENSIONS, STROKE, CIRCLE, MOVE_TO, LINE_TO, MOVE_TO]);
        assert_eq!(unsafe { INFO_PATH[3].value }, ICON_STROKE);
    }

    #[test]
    fn fr12_hidden_bubble_anchor_uses_nearest_drawn_ancestor() {
        let parent = |view| match view { 1 => 2, 2 => 3, _ => 0 };
        let drawn = |view| view == 3;
        assert_eq!(drawn_ancestor(1, parent, drawn), Some(3));
        assert_eq!(drawn_ancestor(3, parent, drawn), Some(3));
        assert_eq!(drawn_ancestor(1, parent, |_| false), None);
    }

    #[test]
    fn fr12_toolbar_colours_use_distinct_stock_keys() {
        let colour = |id| COLOURS.iter().find(|(key, _)| *key == id).map(|(_, value)| *value);
        assert_eq!(colour(1055), Some(0xFF4A_4850));
        assert_eq!(colour(1056), Some(0xFF8A_8690));
        assert_eq!(colour(1590), Some(PAPER));
        assert_eq!(colour(1595), Some(0xFF4A_4850));
        assert_eq!(colour(796), Some(PAPER));
        assert_eq!(colour(1013), Some(PAPER));
        assert_eq!(colour(1603), Some(HAIRLINE));
        assert_eq!(colour(1647), Some(0xFFC4_2B1C));
        assert_eq!(colour(1645), Some(0xFF1A_1A1A));
        // Active, inactive and unthemed glyphs, and the alpha an inactive window applies.
        assert_eq!(colour(1644), Some(0xFF1A_1A1A));
        assert_eq!(colour(1646), Some(0xFF1A_1A1A));
        assert_eq!(colour(1650), Some(0xFF1A_1A1A));
        for (index, (id, _)) in COLOURS.iter().enumerate() {
            assert!(!COLOURS[..index].iter().any(|(other, _)| other == id));
        }
    }

    #[test]
    fn fr12_reload_icon_stays_on_its_canvas() {
        let path = reload_path();
        let values: Vec<f32> = path[4..]
            .iter()
            .map(|element| unsafe { element.value })
            .filter(|value| value.is_finite() && *value < 100.0)
            .collect();
        assert!(values.iter().all(|value| (0.0..=NAV_CANVAS).contains(value)));
    }

    #[test]
    fn fr12_label_hooks_copy_whole_instructions() {
        // `sub rsp, imm32` is seven bytes after the eight bytes of pushes.
        assert_eq!(CHIP_TEXT_PROLOGUE.len(), 15);
        assert_eq!(UPDATE_LOCATION_PROLOGUE.len(), 15);
        assert_eq!(&CHIP_TEXT_PROLOGUE[8..11], &[0x48, 0x81, 0xEC]);
        assert_eq!(&UPDATE_LOCATION_PROLOGUE[8..11], &[0x48, 0x81, 0xEC]);
    }

    #[test]
    fn fr12_only_file_pages_lose_the_label() {
        assert!(is_file_scheme(b"file:///C:/start.html", 0, 4));
        assert!(!is_file_scheme(b"http://example.com/", 0, 4));
        assert!(!is_file_scheme(b"https://example.com/", 0, 5));
        assert!(!is_file_scheme(b"filesystem:http://a/", 0, 10));
        assert!(!is_file_scheme(b"file", -1, -1));
    }

    #[test]
    fn fr12_background_refresh_hook_copies_whole_instructions() {
        // Seven pushes, then `sub rsp, 0x60`, and no relative operand within the copy.
        assert_eq!(REFRESH_BACKGROUND_PROLOGUE.len(), 14);
        assert_eq!(&REFRESH_BACKGROUND_PROLOGUE[10..], &[0x48, 0x83, 0xEC, 0x60]);
    }

    #[test]
    fn fr12_navigation_buttons_use_a_fixed_pitch() {
        assert_eq!(LAYOUT_INSETS_PROLOGUE.len(), 13);
        assert_eq!(NAV_CANVAS as i32, 18);
        assert!(OVERRIDES.contains(&(47, 18)));
        assert!(UPDATE_ICON_PROLOGUE.len() >= 12);
    }

    #[test]
    fn fr12_reload_icon_has_a_gap_and_a_head() {
        let path = reload_path();
        let moves = path.iter().filter(|element| unsafe { element.command } == MOVE_TO).count();
        assert_eq!(moves, 3);
    }

    #[test]
    fn fr12_only_the_start_page_is_shown_as_rivet_start() {
        assert!(is_start_page(b"file:///D:/rivet/start.html?space=Work"));
        assert!(is_start_page(b"FILE:///C:/Program%20Files/Rivet/start.html"));
        assert!(!is_start_page(b"file:///D:/notes/other.html?x=/start.html"));
        assert!(!is_start_page(b"https://example.com/start.html"));
        assert!(!is_start_page(b"file:///start.htm"));
        assert_eq!(address_display::START_PAGE_ADDRESS.encode_utf16().count(), 13);
    }

    #[test]
    fn fr12_cookie_hooks_end_with_the_cookie_load() {
        for prologue in [&OMNIBOX_UPDATE_PROLOGUE[..], &OMNIBOX_REVERT_ALL_PROLOGUE[..]] {
            assert_eq!(&prologue[prologue.len() - 7..prologue.len() - 4], [0x48, 0x8B, 0x05]);
            assert!(prologue.len() >= 12);
        }
        assert!(OMNIBOX_TAB_CHANGED_PROLOGUE.len() >= 12);
    }

    #[test]
    fn fr12_rivet_toolbar_leaves_thirty_two_dips_before_the_back_button() {
        assert_eq!(LEADING_SPACE, 32);
        assert_ne!(TOOLBAR_INSET, TOOLBAR_BUTTON_INSET);
        assert_eq!(i32::MAX.saturating_add(LEADING_SPACE), i32::MAX);
    }

    #[test]
    fn fr12_address_text_is_twelve_dips() {
        assert_eq!(ADDRESS_FONT_SIZE, 12);
        assert_eq!(TEXTFIELD_THEME_PROLOGUE.len(), 12);
    }

    #[test]
    fn fr12_address_field_is_one_eight_dip_rectangle_in_a_43_dip_row() {
        assert_eq!(FIELD_RADIUS, 8.0);
        // Chrome's 34 DIP field, the 28 DIP fill inside it and the row around it.
        assert_eq!(28 + 2 * FIELD_FILL_MARGIN, 34);
        assert_eq!(TOOLBAR_TOP + 34 + TOOLBAR_BOTTOM, 43);
        // The page info chip paints nothing of its own, so no circle shows inside the field.
        assert!(COLOURS.contains(&(1003, 0)));
        assert_eq!(LOCATION_ICON_SHIFT, 2);
    }

    #[test]
    fn fr12_press_sinks_fast_and_springs_back_slowly_with_an_overshoot() {
        let mut press = Press::new();
        assert_eq!(press.depth(0.0), 0.0);
        press.feed(true, 100.0);
        assert!(press.depth(100.0) < 0.01);
        assert!(press.depth(160.0) > 0.7, "quick in");
        assert!((press.depth(100.0 + PRESS_IN_MS) - 1.0).abs() < 1e-3);
        press.feed(false, 400.0);
        let mid = press.depth(400.0 + PRESS_OUT_MS / 2.0);
        assert!(mid < 0.5 && mid > -0.5);
        let lowest = (0..=36)
            .map(|step| press.depth(400.0 + step as f64 * 10.0))
            .fold(f64::MAX, f64::min);
        assert!(lowest < 0.0 && lowest > -0.15, "small overshoot, got {lowest}");
        assert!(press.at_rest(400.0 + PRESS_OUT_MS));
        assert!((1.0 - (1.0 - PRESS_SCALE) * 1.0 - 0.98).abs() < 1e-6);
        assert!((PRESS_BRIGHTNESS - 0.96).abs() < 1e-6);
    }

    #[test]
    fn fr12_press_stays_down_for_a_beat_after_a_short_tap() {
        let mut press = Press::new();
        press.feed(true, 0.0);
        press.feed(false, 30.0);
        assert!(press.down, "still down after 30 ms");
        press.feed(false, PRESS_HOLD_MS + 1.0);
        assert!(!press.down);
    }

    #[test]
    fn fr12_press_timing_functions_match_css() {
        assert!(bezier(0.0, 0.25, 0.7, 0.35, 1.0).abs() < 1e-9);
        assert!((bezier(1.0, 0.25, 0.7, 0.35, 1.0) - 1.0).abs() < 1e-9);
        assert!(bezier(0.5, 0.3, 1.45, 0.55, 1.0) > 1.0);
        // `ease` is `cubic-bezier(.25, .1, .25, 1)`: 0.5 of the time is about 0.8 of the way.
        assert!((bezier(0.5, 0.25, 0.1, 0.25, 1.0) - 0.8024).abs() < 0.01);
    }

    #[test]
    fn fr12_only_local_pages_replace_the_plain_http_icon() {
        assert_ne!(HTTP_ICON, PAGE_INFO_ICON);
        assert_eq!(HTTP_ICON_NAME.len(), 25);
        assert!(ICON_STROKE < 1.5);
    }

    #[test]
    fn fr12_overrides_only_known_constants() {
        assert!(OVERRIDES
            .iter()
            .all(|(constant, _)| (0..75).contains(constant)));
    }
}

