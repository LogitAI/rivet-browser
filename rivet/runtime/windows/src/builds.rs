//! The supported-build table: every address, vtable, slot, member offset, instruction prologue and
//! enum value the runtime reads from one exact browser module.
//!
//! One row describes one browser build. The runtime picks the row whose module is loaded and
//! whose file version resource matches, and touches nothing when no row matches. Chrome and Edge
//! share Chromium's source, but each build is compiled on its own, so every row is filled from
//! that build's own public symbols and checked against its own instruction bytes.

#![allow(clippy::unreadable_literal)]

use std::sync::OnceLock;

use crate::row_data::row_struct;

#[allow(unused_imports)]
use crate::chrome_toolbar::{ACCENT_TINT, FOCUS_RING, HAIRLINE, INK, PAPER, SELECTION, SOFT_INK};

/// An address the build does not have. Every check against it fails, so the feature that needs it
/// stays off.
pub(crate) const ABSENT: usize = 0x7FFF_0000;

row_struct! {
/// One browser build.
pub struct Build {
    pub browser: &'static str,
    /// The module that holds the browser UI.
    pub module: &'static str,
    pub version: [u16; 4],
    pub toolbar: Toolbar,
    pub vertical: VerticalTabs,
    pub navigation: Navigation,
    pub tabs: Tabs,
    pub split: Split,
    pub side_panel: SidePanel,
    pub layout: Layout,
    /// Edge's own vertical-tab architecture. Chrome has none.
    pub edge_vertical: Option<EdgeVertical>,
    /// What Edge draws around its page on its own, which a Rivet window turns off.
    pub edge_page: Option<EdgePage>,
    /// Views a Rivet window leaves out of Whale's top row and its right sidebar.
    pub hidden_views: Option<HiddenViews>,
    /// Where Whale's top row puts its buttons and address field, which a Rivet window moves to Chrome's.
    pub whale_row: Option<WhaleRow>,
    /// The location bar change of this build, for the builds that need one.
    pub location_bar: Option<LocationBarMinimum>,
}
}

row_struct! {
/// Values for `chrome_toolbar.rs`.
pub struct Toolbar {
    /// `GetLayoutConstant(LayoutConstant)`.
    pub layout_constant: usize,
    pub layout_constant_prologue: &'static [u8],
    /// `views::Widget::ThemeChanged`.
    pub theme_changed: usize,
    pub theme_changed_prologue: &'static [u8],
    /// `views::View::OnPaintLayer` implementation.
    pub paint_layer: usize,
    pub paint_layer_prologue: &'static [u8],
    /// `views::View::SetVisible`. Its first sixteen bytes save registers and reserve the frame.
    pub set_visible: usize,
    pub set_visible_prologue: &'static [u8],
    /// `ui::ImageModel::FromVectorIcon(out, icon, colour, size)`.
    pub from_vector_icon: usize,
    pub from_vector_icon_prologue: &'static [u8],
    /// `LocationIconView::Update(this, bool, bool)` sets the label and then the icon.
    pub update_location: usize,
    pub update_location_prologue: &'static [u8],
    /// `LocationIconView::UpdateIcon` runs after navigation, outside widget painting.
    pub update_location_icon: usize,
    pub update_location_icon_prologue: &'static [u8],
    /// `GetLayoutInsets(out, LayoutInset)` answers the padding around a toolbar button's icon, which
    /// with the icon makes the button 28 DIPs wide, like its height.
    pub layout_insets: usize,
    pub layout_insets_prologue: &'static [u8],
    /// `LayoutInset::TOOLBAR_BUTTON` of this build.
    pub toolbar_button_inset: i32,
    /// `LayoutInset` that `ToolbarView::LayoutCommon` reads for the space around the whole toolbar,
    /// and the space Rivet windows leave before the back button, in DIPs.
    pub toolbar_inset: i32,
    /// `LayoutInset` that `LocationIconView::UpdateBorder` reads for the space around the page info
    /// icon, and how far Rivet moves the icon and the text after it from the field's left edge.
    pub location_icon_inset: i32,
    /// `ToolbarButton::UpdateIcon` rebuilds a button's images. The reload button does it when a page
    /// starts or stops loading, outside any paint, so it needs its own scope.
    pub update_icon: usize,
    pub update_icon_prologue: &'static [u8],
    /// `LocationBarModelImpl::GetFormattedURL(this, out, flags)` returns the address text, and
    /// `LocationBarModelImpl::GetURL(this, out)` the page address.
    pub formatted_url: usize,
    pub formatted_url_prologue: &'static [u8],
    pub model_url: usize,
    pub model_url_prologue: &'static [u8],
    /// `OmniboxViewViews::Update`, `OmniboxViewViews::RevertAll` and `OmniboxViewViews::OnTabChanged`
    /// refresh the text of the address field. The omnibox object starts with `OmniboxView`, which is
    /// 0x10 bytes long, and its `views::Textfield` follows. They take the object as their first
    /// argument and need the Rivet scope, because they run outside any paint or layout.
    pub omnibox_update: usize,
    pub omnibox_update_prologue: &'static [u8],
    pub omnibox_revert_all: usize,
    pub omnibox_revert_all_prologue: &'static [u8],
    pub omnibox_tab_changed: usize,
    pub omnibox_tab_changed_prologue: &'static [u8],
    pub omnibox_textfield: usize,
    /// `OmniboxView::UpdateTextStyle(this, text, text_is_url, scheme)` colours the text of the field.
    pub update_text_style: usize,
    pub update_text_style_prologue: &'static [u8],
    /// `gfx::RenderText::ApplyColor(this, colour, &range)` and `ApplyWeight(this, weight, &range)`.
    pub apply_color: usize,
    pub apply_color_bytes: &'static [u8],
    /// The weight of the page title in the address field. Edge's font has a true semibold, which
    /// looks lighter than the semibold face Chrome's draws, so the weight that matches differs.
    pub title_weight: u32,
    pub apply_weight: usize,
    pub apply_weight_bytes: &'static [u8],
    /// `OmniboxViewViews::TitleWasSet` is the web contents observer entry of the omnibox, which is
    /// 0x720 bytes into the object. The page title is not part of the text Chrome builds, so a new
    /// title refreshes it.
    pub title_was_set: usize,
    pub title_was_set_prologue: &'static [u8],
    pub observer_offset: usize,
    /// `operator new(size)` and `operator delete(pointer, size)`, which own Chrome's strings.
    pub operator_new: usize,
    pub operator_new_bytes: &'static [u8],
    pub operator_delete: usize,
    pub operator_delete_bytes: &'static [u8],
    /// The location bar model keeps its delegate at `+0x40`. The browser's delegate answers the
    /// active web contents in its twentieth virtual function, and `WebContents::GetTitle` is the
    /// fifty-fourth of the web contents.
    pub model_delegate: usize,
    pub browser_delegate_vtable: usize,
    pub active_web_contents: usize,
    pub active_web_contents_slot: usize,
    pub web_contents_vtable: usize,
    pub web_contents_title: usize,
    pub web_contents_title_slot: usize,
    /// `LocationBarView::RefreshBackground` bakes the field's fill colour into a background. Hover
    /// and focus refresh it outside any paint, so it needs its own scope.
    pub location_bar_view_base: usize,
    /// The field's hover animation value, a `double` in the `LocationBarView`, and the background the
    /// refresh sets: a `views::Background` at `+0x328` whose painter, at `+0x10`, keeps its four
    /// corner radii as floats from `+0x10`.
    pub hover_value: usize,
    pub field_background: usize,
    pub background_vtable: usize,
    pub painter_vtable: usize,
    pub painter_radii: usize,
    pub painter_insets: usize,
    /// The omnibox state a `LocationBarView` reads while it refreshes its background: a pointer at
    /// `+0x4e8` to an object that keeps the state object at `+0x28`, whose `+0x18` is 1 while the
    /// field has focus.
    pub field_state: usize,
    pub field_state_focused: usize,
    /// The fill Chrome uses for a focused field is the colour of the suggestion list. The design
    /// keeps the hover fill instead, so the field stays the same rectangle while it is edited.
    pub focused_fill_id: i32,
    /// `views::FocusRing::Remove(View*)`, which does nothing when the view has no ring.
    pub focus_ring_remove: usize,
    /// `views::View::SetTransform(this, &gfx::Transform)` and `ui::Layer::SetLayerBrightness(this,
    /// float)`. A transform that is a scale and a translation starts with a zero byte, then holds the
    /// scale at `+8` and the translation at `+0x10`, as floats. A view keeps its layer at `+0x20` and
    /// its size at `+0x1D0`.
    pub set_transform: usize,
    pub set_transform_prologue: &'static [u8],
    pub set_brightness: usize,
    pub set_brightness_prologue: &'static [u8],
    pub view_layer: usize,
    pub view_size: usize,
    pub refresh_background: usize,
    pub refresh_background_prologue: &'static [u8],
    /// `location_bar::GetSecurityChipText(out, delegate, url, flag)` returns the label Chrome puts
    /// before the address: "File" for local pages and the connection state of the others.
    /// `GURL::~GURL` frees the page address that the delegate hands out by value.
    pub chip_text: usize,
    pub chip_text_prologue: &'static [u8],
    /// `views::Textfield::OnThemeChanged`, `Textfield::SetFontList`, `gfx::FontList::GetFontSize` and
    /// `gfx::FontList::DeriveWithSizeDelta`. The address field is the browser window's only text
    /// field, and Chrome chooses its font once, when the field is built.
    pub textfield_theme: usize,
    pub textfield_theme_prologue: &'static [u8],
    pub textfield_set_font: usize,
    pub textfield_set_font_bytes: &'static [u8],
    pub font_size: usize,
    pub font_size_bytes: &'static [u8],
    pub derive_size: usize,
    pub derive_size_bytes: &'static [u8],
    /// The Textfield's render text and its font list, as offsets.
    pub textfield_render_text: usize,
    pub render_text_font_list: usize,
    pub gurl_destructor: usize,
    pub gurl_destructor_bytes: &'static [u8],
    /// `GURL` layout of this build: the spec string, then the scheme's begin and length.
    pub gurl_scheme_begin: usize,
    pub gurl_scheme_length: usize,
    /// `BubbleDialogDelegate::GetAnchorRect`, `View::IsDrawn` and `View::GetBoundsInScreen`.
    pub anchor_rect: usize,
    pub anchor_rect_prologue: &'static [u8],
    pub anchor_view_access: &'static [u8],
    pub is_drawn: usize,
    pub is_drawn_bytes: &'static [u8],
    pub bounds_in_screen: usize,
    pub bounds_bytes: &'static [u8],
    pub parent_offset: usize,
    pub is_drawn_slot: usize,
    /// `BrowserWidget`'s vtable, with `OnNativeWidgetPaint` and `LayoutRootViewIfNecessary`.
    pub browser_widget_vtable: usize,
    /// `views::Widget`'s own vtable, which popups such as the omnibox's suggestion list use. It holds
    /// the same paint and layout functions as the browser widget's.
    pub plain_widget_vtable: usize,
    pub paint_slot: usize,
    pub paint: usize,
    pub layout_slot: usize,
    pub layout: usize,
    /// `views::View::GetColorProvider`, asked of the widget's root view, which the widget keeps at
    /// `+0x1A0`.
    pub color_provider: usize,
    pub root_view_offset: usize,
    /// `views::View` vtables of the buttons a Rivet window hides, with their `OnThemeChanged`.
    pub theme_slot: usize,
    pub hidden: &'static [(usize, usize)],
    /// The vtable slots of `CalculatePreferredSize` and `GetMinimumSize`, or zero when hidden views are
    /// hidden with `SetVisible`. Edge's toolbar lays itself out again whenever a button's visibility
    /// differs from what it asked for, so a button kept hidden behind its back loops forever. Its views
    /// stay visible and ask for no room instead, and nothing of them is drawn.
    pub size_slots: (usize, usize),
    /// The vtable slot of `PaintChildren`, which a hidden view skips: a view's children are not clipped
    /// to it, so the children of a view with no room would still be drawn. Zero when unused.
    pub paint_children_slot: usize,
    /// `views::View::GetWidget` is the twelfth entry of every view's vtable.
    pub get_widget_slot: usize,
    /// `LayoutConstant` values of this build and the sizes of the Rivet top row, in DIPs.
    pub overrides: &'static [(i32, i32)],
    /// Colour ids of this build and the Rivet colours, as `0xAARRGGBB`.
    pub colours: &'static [(i32, u32)],
    /// What this build needs to show a page in developer mode.
    pub dev: DevMode,
    /// `ui::ColorProvider::GetColor(this, id)`. A provider makes a colour the first time it is asked
    /// for it, so one that was never asked for is asked for before its value is replaced.
    pub get_color: usize,
    pub get_color_bytes: &'static [u8],
    /// `views::HWNDForWidget`, which is a call and a jump through two thunks.
    pub hwnd_for_widget: usize,
    pub hwnd_for_widget_head: &'static [u8],
    pub hwnd_for_widget_tail: &'static [u8],
    /// Where the colour provider source lies in a widget.
    pub source_in_widget: usize,
    /// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND` and
    /// `views::Widget::GetWidgetForNativeWindow`.
    pub content_window_for_hwnd: usize,
    pub widget_for_window: usize,
    /// The navigation icons of this build: `kBackArrowChromeRefreshOldIcon`,
    /// `kForwardArrowChromeRefreshOldIcon` and `kReloadChromeRefreshOldIcon`.
    pub back_icon: usize,
    pub forward_icon: usize,
    pub reload_icon: usize,
    /// The names the back, forward and reload icons carry in this build, checked before the icons are replaced.
    pub nav_icon_names: &'static [&'static [u8]],
    pub page_info_icon: usize,
    /// The Google G logos Chrome shows in the field while a person edits it, in colour and in one
    /// colour. The design shows a magnifier there.
    pub google_g_icon: usize,
    pub google_g_monochrome_icon: usize,
    /// The globe Chrome shows for an address being edited, and its search icons.
    pub edit_icons: &'static [usize],
    /// `kHttpChromeRefreshOldIcon`, which Chrome shows for local files as well as for plain HTTP.
    pub http_icon: usize,
    pub http_icon_name: &'static [u8],
    pub page_info_bytes: &'static [u8],
    /// The page info icon's rep list, name and path data, which Rivet checks before it reuses the icon.
    pub page_info_reps: usize,
    pub page_info_id: usize,
    pub page_info_path: usize,
    /// How many path elements the page info and plain HTTP icons have, and the other icons of this build
    /// that show the same two states: more padlocks and more warnings, each with one path.
    pub page_info_size: usize,
    pub http_size: usize,
    pub extra_lock_icons: &'static [usize],
    pub extra_warning_icons: &'static [usize],
    /// Where the anchor view access starts inside `GetAnchorRect`.
    pub anchor_view_access_at: usize,
    /// Features this build leaves alone: "toolbar" (the whole top row), "address", "icon" and "updateicon".
    pub skip: &'static str,
    /// Where a `BubbleDialogDelegate` keeps the tracker of its anchor view.
    pub anchor_tracker_offset: usize,
    /// The toolbar view's own `Layout`, when the browser lays the toolbar out outside the window's layout pass
    /// (Brave does, from its browser view), so the layout would miss the Rivet scope. `ABSENT` when not needed.
    pub toolbar_layout: usize,
    pub toolbar_layout_prologue: &'static [u8],
    /// The vtable of the infobar container, its `Layout` and that function's slot. A browser whose layout hands the
    /// container the width of the whole browser view while it moves it right by the appbar's width makes the
    /// container overflow its parent by that much, so a Rivet window's container is cut to fit before it lays out.
    pub infobar_vtable: usize,
    pub infobar_layout: usize,
    pub infobar_layout_slot: usize,
    /// The padding of a toolbar button around its icon, and the margin the field's fill keeps from the top and
    /// bottom of the field view, both in DIPs. A build whose buttons or field are laid out larger or smaller than
    /// Chrome's needs other numbers to reach the 28 DIP button and the 28 DIP fill of the design.
    /// The space before the first toolbar button, in DIPs, where the appbar's toggle sits.
    pub leading_space: i32,
    /// The space above and below the row's content, in DIPs: with Chrome's 34 DIP field they make the 43 DIP row.
    pub toolbar_top: i32,
    pub toolbar_bottom: i32,
    pub button_padding: i32,
    pub field_fill_margin: i32,
}
}

row_struct! {
/// Values for `whale_toolbar.rs`. Whale's toolbar holds buttons that a Rivet window does not want, and
/// a sidebar that takes a column of the page. A view the layout cannot be told to skip is made to ask
/// for no room and to draw nothing, which keeps `SetVisible` out of it: the layout shows a view again
/// whenever its own idea of the view's visibility differs.
pub struct HiddenViews {
    /// The vtable slots of `GetMinimumSize`, `CalculatePreferredSize`, `PaintChildren` and `OnPaint`.
    pub slots: [usize; 4],
    /// Where a view keeps its parent, and the start and end of its vector of children.
    pub parent_offset: usize,
    pub children_offset: usize,
    /// `views::View::InvalidateLayout(this, flag)`, which asks the layouts above a view to run again.
    pub invalidate_layout: usize,
    pub invalidate_layout_prologue: &'static [u8],
    /// The sidebar's own `GetMinimumSize`, which the layout of the page calls directly instead of
    /// through the vtable to learn how much the sidebar must keep. `ABSENT` for a build whose sidebar
    /// is not a view of the browser window.
    pub sidebar_minimum: usize,
    pub sidebar_minimum_prologue: &'static [u8],
    /// Each class: its vtable and the functions its four slots hold, which are checked before a slot
    /// is replaced and run for every view that is not in a Rivet window.
    pub classes: &'static [(usize, [usize; 4])],
    /// Classes of views that only draw nothing while they lie below a view of one of `classes`:
    /// their vtable and the functions of the `PaintChildren` and `OnPaint` slots. A button's image
    /// can paint on a layer of its own, which its parent's silence does not stop.
    pub inside: &'static [(usize, [usize; 2])],
}
}

row_struct! {
/// Values for the row geometry in `whale_toolbar.rs`. Whale's toolbar view lays out one container view
/// whose children are the buttons and the address field. With the tab strip gone the container starts
/// 100 DIPs in and stops 219 DIPs short of the right edge, and its buttons are 26 DIPs wide. The row
/// is checked by classes, so a build with other ones is left as it is.
pub struct WhaleRow {
    /// `views::View::SetBoundsRect(this, rect)`, which every layout ends in.
    pub set_bounds_rect: usize,
    pub set_bounds_rect_prologue: &'static [u8],
    /// A view keeps its parent here, and its bounds as x, y, width and height.
    pub parent_offset: usize,
    pub bounds_offset: usize,
    /// The vtables of the container, of the back, forward and reload buttons and of the address field.
    pub container: usize,
    pub buttons: [usize; 3],
    pub field: usize,
    /// Whale keeps its caption buttons as children of the frame view, which has no caption container.
    /// The frame's `Layout` sits in `frame_layout_slot` of `layout.frame_vtable`, and the start of its
    /// children vector in `children_offset`. `caption` is the vtable of the minimize, maximize and close
    /// buttons and `tab_search` that of the tab search button, a chevron the frame draws before them.
    pub frame_layout_slot: usize,
    pub frame_layout: usize,
    /// The toolbar view's vtable and the height the browser's layout gives it, in DIPs. The layout does not
    /// ask the toolbar for its size through its vtable, so a Rivet window grows the row where its rectangle is set.
    pub toolbar: usize,
    pub toolbar_height: i32,
    /// The toolbar's separator, a 1 DIP tall view at its bottom edge that draws the line between the row and
    /// the page. Whale lays it out 42 DIPs short of the right edge.
    pub separator: usize,
    pub children_offset: usize,
    pub caption: usize,
    pub tab_search: usize,
    /// `gfx::PaintVectorIcon(canvas, icon, dip, colour)`, which draws Rivet's icons here. Whale's own
    /// come from resource images and image views, so there is no icon to swap as on Chrome.
    pub paint_vector_icon: usize,
    pub paint_vector_icon_prologue: &'static [u8],
    /// `CANVAS_DIMENSIONS` in this build's `gfx::CommandType`. Whale adds commands of its own to the
    /// enum after `R_QUADRATIC_TO_SHORTHAND`, which moves it; the commands before them are Chromium's.
    pub canvas_command: u32,
    /// The slot of `views::Button::PaintButtonContents`, and what it holds for the caption buttons and
    /// for the back, forward and reload buttons.
    pub button_paint_slot: usize,
    pub caption_button_paint: usize,
    pub nav_button_paints: [usize; 3],
    /// The vtable of the image view a navigation button holds its icon in, `views::Button::state_`, and
    /// where the reload button keeps whether it shows stop.
    pub button_image: usize,
    pub button_state_offset: usize,
    pub reload_mode_offset: usize,
    /// The vtable of the views Whale lays over the page's top corners, each a filled curve beside its
    /// sidebar, and what `views::View::OnPaint`'s slot holds for them.
    pub page_corner: usize,
    pub page_corner_paint: usize,
    /// The address field, its lock chip, its text view and the view holding the address and title Whale
    /// shows at rest paint the pill, the white circle and boxes of the pill's colour, and the row's container is white, through
    /// `views::View::OnPaintBackground`, which this slot holds.
    pub bare: [usize; 5],
    pub paint_background_slot: usize,
    pub paint_background: usize,
    /// `views::View::SchedulePaint`. A view keeps its recorded paint until it is asked again.
    pub schedule_paint: usize,
    pub schedule_paint_prologue: &'static [u8],
    /// The view fading the end of the address into the pill, its paint and where it keeps the colour.
    pub address_fade: usize,
    pub address_fade_paint: usize,
    pub address_fade_colour: usize,
    /// `ui::Layer::SetColor(this, colour)`. The separator paints nothing: the line is its solid colour
    /// layer, which a view keeps here.
    pub layer_set_color: usize,
    pub layer_set_color_prologue: &'static [u8],
    pub layer_offset: usize,
    /// The button showing the address at rest, inside `bare[3]`, which knows the hover and would paint
    /// its own, and what its `OnPaintBackground` slot holds.
    pub address_button: usize,
    pub address_button_background: usize,
    /// `gfx::Canvas::DrawRoundRect(this, rect, radius, flags)` and the constructor and destructor of
    /// `cc::PaintFlags`, which draw the address field's hover.
    pub draw_round_rect: usize,
    pub draw_round_rect_prologue: &'static [u8],
    pub paint_flags_new: usize,
    pub paint_flags_new_prologue: &'static [u8],
    pub paint_flags_drop: usize,
    pub paint_flags_drop_prologue: &'static [u8],
}
}

row_struct! {
/// Values for `edge_page.rs`: Edge insets its page and paints a border around it, which would
/// fight Rivet's own panel.
pub struct EdgePage {
    /// `EdgeContentsContainerBorder::OnPaintBackground`, which paints the page's hairline.
    pub paint_border: usize,
    pub paint_border_prologue: &'static [u8],
    /// `AppLayerStyle::GetContentsContainerMargin`, which insets the page from its container and
    /// returns `gfx::Insets` through a hidden pointer.
    pub container_margin: usize,
    pub container_margin_prologue: &'static [u8],
    /// `ContentsWebView::EdgeUpdateRoundedCornerVisibility(this, visible)` shows or hides the overlay
    /// that rounds the page's corners, which the page view keeps at `corner_offset`.
    pub update_corners: usize,
    pub update_corners_prologue: &'static [u8],
    pub corner_offset: usize,
    /// `EdgeRoundedCornerView::SetVisible(this, visible)`.
    pub corner_set_visible: usize,
}
}

row_struct! {
/// Values for `edge_vertical.rs`: Edge keeps its vertical tabs in its own container view, which
/// the browser view creates for every window and which a state number puts into vertical mode.
pub struct EdgeVertical {
    pub content_window_for_hwnd: usize,
    pub content_window_for_hwnd_prologue: &'static [u8],
    pub browser_view_for_window: usize,
    pub browser_view_for_window_prologue: &'static [u8],
    /// `BrowserView::EdgeSetTabStripState(this, state, animate)` puts the container view into a state.
    pub set_state: usize,
    pub set_state_prologue: &'static [u8],
    /// The state that shows the strip expanded.
    pub vertical_state: u32,
    /// `BrowserView::EdgeShouldHideTitlebarForVerticalTabs(this)` reads the profile's setting. Its
    /// prologue ends before the instruction after the stack cookie store, and the cookie load sits
    /// `cookie_at` bytes into it.
    pub hide_titlebar: usize,
    pub hide_titlebar_prologue: &'static [u8],
    pub cookie_at: usize,
    /// `EdgeVerticalTabContainerView::CalculatePreferredSize(this, out)` and `GetMinimumSize(this,
    /// out)`, which the layout asks for the strip's width. The second also starts with a cookie load.
    pub container_preferred_size: usize,
    pub container_preferred_size_prologue: &'static [u8],
    pub container_minimum_size: usize,
    pub container_minimum_size_prologue: &'static [u8],
    pub container_minimum_cookie_at: usize,
    /// `views::View::SetBoundsRect(this, const Rect*)`, the toolbar's vtable (`EdgeToolbarView`), the
    /// gap Rivet keeps between the toolbar and the caption buttons, and how far the top inset Rivet adds
    /// differs from Chrome's, in DIPs. Edge lays its toolbar out across the whole window width when the
    /// title bar is hidden, from the window's left edge, so Rivet moves it to the page's edge.
    pub set_bounds_rect: usize,
    pub set_bounds_rect_prologue: &'static [u8],
    pub toolbar_vtable: usize,
    /// The vtables of the back and forward buttons and of the reload button, whose preferred size
    /// Rivet windows make 28 by 28, and the toolbar's preferred height there. `CalculatePreferredSize`
    /// sits in the slot of `size_slot`.
    pub button_vtables: &'static [usize],
    pub toolbar_height: i32,
    pub toolbar_overlap: i32,
    pub button_size: i32,
    /// Where the toolbar's children sit inside it in a Rivet window, in DIPs: the first button's x
    /// and y, the step to the next button, and the address field's vtable, x, y, height and the
    /// room left after it.
    pub button_x: i32,
    pub button_y: i32,
    pub button_pitch: i32,
    pub field_vtable: usize,
    pub field_x: i32,
    pub field_y: i32,
    pub field_height: i32,
    pub field_margin: i32,
    /// The page info icon's view and the text view inside the address field: the icon's bounds, and
    /// the text's x, y, height and the room left after it.
    pub icon_vtable: usize,
    pub icon_bounds: [i32; 4],
    pub text_vtable: usize,
    pub text_x: i32,
    pub text_y: i32,
    pub text_height: i32,
    pub text_margin: i32,
    pub size_slot: usize,
    pub caption_gap: i32,
    /// The hairline below the row: `ContentsSeparator` inside `TopContainerView`, which Edge keeps
    /// hidden, and its height in DIPs. The toolbar and this line make the row.
    pub separator_vtable: usize,
    pub top_container_vtable: usize,
    pub separator_height: i32,
    /// The room Rivet's toggle button takes before the toolbar's first button, in DIPs, so the back
    /// button sits where Chrome's does.
    pub toolbar_leading: i32,
    pub top_inset_adjust: i32,
    /// The least top inset the panel is measured from. Edge answers 6 DIPs with one tab and 0 later.
    pub top_inset_floor: i32,
    /// Where the browser view keeps its container view, and where that keeps its state.
    pub container_offset: usize,
    pub state_offset: usize,
}
}

row_struct! {
/// The views of a top row that is placed one by one, told apart by their vtables.
#[derive(Debug)]
pub struct TopRow {
    pub parent_offset: usize,
    pub children_offset: usize,
    pub bounds_offset: usize,
    /// The caption button container and its buttons, in the frame view.
    pub caption: usize,
    pub caption_button: usize,
    /// The parent of the row and of the page, the row, its line, and the row's content.
    pub content: usize,
    pub toolbar: usize,
    pub separator: usize,
    pub container: usize,
    pub corner: usize,
    pub inner: usize,
    /// Back and forward, then reload.
    pub buttons: &'static [usize],
    pub field: usize,
    pub page: usize,
    /// The height the browser gives the row.
    pub browser_row: i32,
    /// How far the browser's client view starts below the frame's top, which the panel's gap replaces.
    pub panel_top_trim: i32,
    /// `PaintChildren` and `OnPaint`.
    pub paint_slots: [usize; 2],
    /// The classes that paint with Rivet's colours, each with the functions those slots hold.
    pub painted: &'static [(usize, [usize; 2])],
    /// Views whose fill or border was baked when their theme changed, each with the slot of the paint
    /// to leave out in Rivet windows and the function it holds: `OnPaint`, or `OnPaintBackground` for
    /// a view that draws more than that. The design's field has no fill at rest and its page no border.
    pub bare: &'static [(usize, usize, usize)],
    /// The view that holds the page inside the page's border, which Rivet windows have no room for.
    pub holder: usize,
    pub holder_parent: usize,
    /// The sidebar's root view and the functions those slots hold.
    pub sidebar_root: (usize, [usize; 2]),
    /// Where a root view keeps its widget and a widget its native widget.
    pub root_widget: usize,
    pub widget_native: usize,
    /// The native widget's vtable and `Hide`.
    pub native_vtable: usize,
    pub native_hide_slot: usize,
    pub native_hide: usize,
    pub native_hide_prologue: &'static [u8],
    /// Colour ids of this build and the Rivet colours, as `0xAARRGGBB`.
    pub colours: &'static [(i32, u32)],
    /// `gfx::PaintVectorIcon(canvas, icon, dip, colour)` and `CANVAS_DIMENSIONS` in this build's
    /// `gfx::CommandType`. The row's own icons are images baked by theme changes, so Rivet draws its own.
    pub paint_vector_icon: usize,
    pub paint_vector_icon_prologue: &'static [u8],
    pub canvas_command: u32,
    /// The slot of `views::Button::PaintButtonContents`, with what it holds for back and forward, for
    /// reload, for the location icon and for the caption buttons.
    pub button_paint_slot: usize,
    pub button_paints: &'static [(usize, usize)],
    /// The image view a button keeps its icon in, `views::Button::state_`, and where reload keeps
    /// whether it shows stop.
    pub button_image: usize,
    /// The views the location icon holds that Rivet's chip replaces: its image, label and separator.
    pub chip_children: &'static [usize],
    /// `gfx::Canvas::DrawStringRect` and `gfx::Canvas::GetStringWidth`, for the chip's words.
    pub draw_string: usize,
    pub draw_string_prologue: &'static [u8],
    pub string_width: usize,
    pub string_width_prologue: &'static [u8],
    pub button_state_offset: usize,
    pub reload_mode_offset: usize,
    /// The address field's location bar, its location icon and its text field.
    pub location_bar: usize,
    /// Where the location bar's view keeps the hover animation's value, from 0 to 1.
    pub location_hover: usize,
    pub location_icon: usize,
    pub omnibox: usize,
    /// The text field's `OnPaint`, `OnFocus` and `OnBlur`, each with its slot.
    pub omnibox_paint: (usize, usize),
    pub omnibox_focus: (usize, usize),
    pub omnibox_blur: (usize, usize),
    /// `gfx::RenderText::ApplyColor(this, colour, range)`, and where a text field keeps the holder of
    /// its render text, which the holder keeps at +0x10.
    pub apply_color: usize,
    pub apply_color_prologue: &'static [u8],
    pub textfield_render_text: usize,
    /// `views::Textfield::SetFontList`, `gfx::Font::Font(name, size)` and `gfx::FontList::FontList(font)`.
    pub textfield_set_font: usize,
    pub textfield_set_font_prologue: &'static [u8],
    pub font_new: usize,
    pub font_new_prologue: &'static [u8],
    pub font_list_new: usize,
    pub font_list_new_prologue: &'static [u8],
}
}

row_struct! {
/// Values for `vertical_tabs.rs`.
pub struct VerticalTabs {
    /// The classes of a top row that is placed view by view, for a build whose toolbar is not mapped.
    pub top_row: Option<TopRow>,
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    /// The vtable slot of `VerticalTabStripStateControllerImpl::ShouldDisplayVerticalTabs`.
    pub slot: usize,
    pub should_display: usize,
    /// `BrowserView::GetBrowserViewForBrowser`.
    pub browser_view_for_browser: usize,
    /// `views::HWNDForView`.
    pub hwnd_for_view: usize,
    /// `VerticalTabStripStateController::NotifyModeChanged`.
    pub notify_mode_changed: usize,
    /// `VerticalTabStripStateControllerImpl::UpdateCollapseActionItem`.
    pub update_collapse_action: usize,
    pub destructor: usize,
    pub destructor_prologue: &'static [u8],
    /// Where the controller keeps its browser, its mode and its strip width.
    pub browser_offset: usize,
    pub mode_offset: usize,
    pub width_offset: usize,
    /// `BrowserView`'s vtable for its `views::View` base. The browser view is reached through another
    /// base, so the view base is found by this vtable before asking for the window.
    pub browser_view_view_vtable: usize,
    /// `VerticalTabStripRegionView::CalculatePreferredSize`, and the vtable slot that holds it.
    pub region_preferred_size: usize,
    pub region_preferred_size_slot: usize,
    /// `views::View::InvalidateLayout`, with the first bytes that identify it in this build.
    pub invalidate_layout: usize,
    pub invalidate_layout_prologue: &'static [u8],
    /// `VerticalTabStripRegionView::GetMinimumSize`. The browser lays the page out after at least
    /// this width, 126 DIPs while the strip is expanded, however little the strip asks for, so a
    /// hidden appbar would leave that much of the frame bare. The class is final, so callers reach
    /// the function directly and its vtable slot is never used. Rivet detours the function instead:
    /// its first instructions are copied to a trampoline and replaced by a jump to Rivet's version.
    pub region_minimum_size: usize,
    /// `push rsi; push rdi; sub rsp, 0x28; mov rsi, rdx; mov rdi, rcx; mov qword ptr [rdx], 0`.
    pub region_minimum_prologue: &'static [u8],
    /// `BrowserView::ShouldDrawTabStrip` and its first bytes, for the tab strip experiment.
    pub should_draw_tab_strip: usize,
    pub should_draw_tab_strip_prologue: &'static [u8],
    /// `Browser::SupportsWindowFeature(browser, feature, check)`, for builds whose layout has the tab
    /// strip decision inlined so no function of its own answers it. Rivet detours it to answer no
    /// for the tab strip feature of the browsers of Rivet windows. The prologue must be 12 or more
    /// bytes, end on an instruction boundary and hold no relative operand.
    pub window_feature: usize,
    pub window_feature_prologue: &'static [u8],
    /// The value of the tab strip feature.
    pub window_feature_tab_strip: u32,
    /// The code range whose questions Rivet answers, as start and end RVAs, or `(0, 0)` for every
    /// caller. Other callers ask the same question for other reasons, and the frame view takes the
    /// caption buttons away with the strip when it hears the answer.
    pub window_feature_callers: (usize, usize),
    /// `View::SetBoundsRect` and the offset of the toolbar view in the browser view. With the strip
    /// gone the toolbar runs under the caption buttons, so Rivet detours the bounds function to stop
    /// the toolbar of a Rivet window short of them. The prologue must be 12 or more bytes with no
    /// relative operand.
    pub set_bounds_rect: usize,
    pub set_bounds_rect_prologue: &'static [u8],
    pub toolbar_in_view: usize,
    /// How far, in DIPs, the toolbar of a Rivet window starts to the right of the browser view's left
    /// edge, so Rivet's toggle button has the room before the back button that Chrome's toolbar gives it.
    pub toolbar_leading: i32,
    /// Whether a Rivet window leaves that function out of its layout by a detour, with this build's
    /// prologue copied to a trampoline. Only builds where this has been checked set it.
    pub detour_strip: bool,
}
}

row_struct! {
/// Values for `chrome_navigation.rs`.
pub struct Navigation {
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    /// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND`.
    pub content_window_for_hwnd: usize,
    /// `BrowserView::GetBrowserViewForNativeWindow`.
    pub browser_view_for_window: usize,
    /// `BrowserView::GetActiveWebContents`.
    pub active_web_contents: usize,
    /// `GURL::GURL(std::string_view)`, `GURL::GURL()` and `GURL::~GURL`.
    pub gurl_from_text: usize,
    /// Whether that constructor reads UTF-16 text, which is what the one Samsung's build has takes.
    pub gurl_wide: bool,
    pub gurl_empty: usize,
    pub gurl_drop: usize,
    /// `content::OpenURLParams::OpenURLParams(url, referrer, disposition, transition, renderer)`.
    pub open_url_params: usize,
    pub open_url_params_drop: usize,
    /// `content::WebContentsImpl::OpenURL`.
    pub open_url: usize,
    /// The first instructions of the functions the navigation calls need.
    pub signatures: &'static [(usize, &'static [u8])],
}
}

row_struct! {
/// Values for `chrome_tabs.rs`.
pub struct Tabs {
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    pub content_window_for_hwnd: usize,
    pub browser_view_for_window: usize,
    pub active_web_contents: usize,
    /// Where the browser view keeps its `Browser`, and whether the tab groups of this build are read.
    pub browser_in_view: usize,
    pub groups: bool,
    /// Tab strip and browser actions for Chrome 156.0.8078.48.
    pub get_tab_model: usize,
    pub tab_count: usize,
    pub tab_at: usize,
    /// For a build that has no `GetWebContentsAt` of its own: `tab_object(collection, index)` answers
    /// the tab object, the collection sits `tab_collection` bytes into the model and the tab keeps
    /// its web contents `tab_contents` bytes in. A build that records `tab_at` leaves these unused.
    pub tab_object: usize,
    pub tab_collection: usize,
    pub tab_contents: usize,
    pub activate_tab: usize,
    pub close_tab: usize,
    pub title: usize,
    pub new_tab: usize,
    /// The first instructions of `chrome::NewTab`, which every new-tab command of the browser
    /// ends in. When this is not empty, a new tab in a Rivet window opens the Rivet start page.
    pub new_tab_prologue: &'static [u8],
    /// Set when `chrome::NewTab` of this build takes the address to open as a third argument, which is
    /// empty when the command asks for the browser's own new tab page. The value is how far past the
    /// browser object the first argument points, since that build passes its window interface.
    pub new_tab_address: Option<usize>,
    /// Whether `chrome::NewTab` takes an address as its third argument, which only an empty one sends
    /// to the new tab page. A build whose command has two arguments leaves the third unread.
    pub new_tab_takes_address: bool,
    /// How long after `chrome::NewTab` returns the start page is opened, for a build that loads its
    /// own new tab page after the command and would replace an address opened at once. Zero opens it
    /// in the command itself.
    pub new_tab_delay_ms: u32,
    /// `WebContentsImpl::GetLastCommittedURL` and `GetVisibleURL`, which return a `const GURL&`.
    pub last_committed_url: usize,
    pub visible_url: usize,
    pub get_group: usize,
    pub has_group: usize,
    pub lookup_group: usize,
    pub change_group: usize,
    /// `operator new` of the browser and `TabStripModel::AddToNewGroup`, which the group probe uses.
    pub allocate: usize,
    pub add_to_new_group: usize,
    pub allocate_prologue: &'static [u8],
    pub add_to_new_group_prologue: &'static [u8],
    /// The first instructions of the functions the tab calls need.
    pub signatures: &'static [(usize, &'static [u8])],
}
}

row_struct! {
/// Values for `chrome_split.rs`.
pub struct Split {
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    pub add_to_new_split: usize,
    pub remove_split: usize,
    pub get_split_for_tab: usize,
    /// PartitionAlloc's operator new, which frees the index vector the callee takes over.
    pub allocate: usize,
    /// `ContentsContainerView::SetRoundedCorners(this, const RoundedCornersF*)`. It starts with
    /// `movss xmm0, [rdx]; ucomiss xmm0, [rcx + 0x484]; jne changed`, 13 bytes.
    pub set_rounded_corners: usize,
    pub set_rounded_corners_prologue: &'static [u8],
    /// Where the `jne` goes when the corners differ, and the `jp` after it.
    pub set_rounded_corners_changed: usize,
    pub set_rounded_corners_parity: usize,
    /// `ContentsContainerView::UpdateBorderAndOverlay(this, show, a, b)`. Chrome rings the page it
    /// works in with a 1 DIP outline and keeps 4 DIPs of space inside the ring. A Rivet panel has no
    /// ring, so the page takes that space and the panel's depth marks it instead.
    pub update_border: usize,
    pub update_border_prologue: &'static [u8],
    /// `views::View::SetBorder` and `SetVisible` in the view's vtable.
    pub set_border_slot: usize,
    pub set_visible_slot: usize,
    /// The member of the container that holds Chrome's outline view.
    pub outline_member: usize,
    /// `MultiContentsView::CalculateProposedLayout`, a method of the layout delegate that sits
    /// `LAYOUT_DELEGATE_OFFSET` bytes into the view.
    pub calculate_layout: usize,
    pub calculate_layout_prologue: &'static [u8],
    pub layout_delegate_offset: usize,
    /// The insets Chrome keeps around the pages of a split, as top, left, bottom and right.
    pub split_insets_member: usize,
    /// The container vector, its length and the active index of a `MultiContentsView`.
    pub containers_member: usize,
    pub container_count_member: usize,
    pub active_member: usize,
    /// `views::View::SchedulePaint(this)`. The background only paints again when it is told to, so a
    /// change of the pages or of the focused one must tell it.
    pub schedule_paint: usize,
    /// `MultiContentsView::OnWebContentsFocused(this, web view)`, which changes the focused page.
    pub web_contents_focused: usize,
    pub web_contents_focused_prologue: &'static [u8],
    pub container_layout: usize,
    pub container_delegate_offset: usize,
    /// The member of a contents container that holds its mini toolbar.
    pub mini_toolbar_member: usize,
    /// `views::FlexLayout::CalculateProposedLayout`, which lays out the children of the browser's
    /// top row. It is a method of a layout manager that keeps its host view at `flex_layout_host`.
    pub flex_layout: usize,
    pub flex_layout_host: usize,
    /// The primary vtable of `LocationBarView`, which tells the top row's container among all hosts.
    pub location_bar_vtable: usize,
    /// Where a view keeps its size, two ints.
    pub view_size: usize,
    /// The first instructions of the functions the split and group calls need.
    pub signatures: &'static [(usize, &'static [u8])],
}
}

row_struct! {
/// Values for `chrome_side_panel.rs`.
pub struct SidePanel {
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    /// `SidePanel`'s own vtable, which its `views::View` base shares as the object's first word.
    pub side_panel_vtable: usize,
    pub browser_offset: usize,
    pub features_offset: usize,
    /// Where `BrowserWindowFeatures` keeps its `SidePanelCoordinator`.
    pub features_coordinator: usize,
    /// The vtable of the `views::View` part of `BrowserView`.
    pub browser_view_view_vtable: usize,
    /// `SidePanelCoordinator`'s vtable.
    pub coordinator_vtable: usize,
    /// `SidePanelUIBase::Show(this, SidePanelEntryId, optional<SidePanelOpenTrigger>, bool)`.
    pub show: usize,
    pub show_prologue: &'static [u8],
    /// `SidePanelEntryIdToString(std::string*, SidePanelEntryId)`.
    pub entry_id_to_string: usize,
    pub entry_id_to_string_prologue: &'static [u8],
    /// The id of the entries that extensions register.
    pub extension_entry: u32,
    /// How many entry ids Chrome 156 defines.
    pub entry_count: u32,
    /// `SidePanelCoordinator::Toggle(this, key*, trigger)` opens an entry or closes it when it is
    /// showing, and `SidePanelUIBase::IsSidePanelEntryShowing(this, key*)` tells which.
    pub toggle: usize,
    pub toggle_prologue: &'static [u8],
    pub entry_showing: usize,
    pub entry_showing_prologue: &'static [u8],
    /// `SidePanelRegistry::From(BrowserWindowInterface*)` returns the window's global registry. It
    /// keeps its entries as a pointer at +0x38 and a count at +0x40, and each entry keeps its
    /// `SidePanelEntryKey` at +0x30.
    pub registry_from: usize,
    pub registry_from_prologue: &'static [u8],
    /// `WebContentsImpl::GetColorProvider`, which hands back the provider of the window's widget.
    pub get_color_provider: usize,
    pub get_color_provider_prologue: &'static [u8],
    /// Where the colour provider source lies in the widget.
    pub source_in_widget: usize,
    /// Where `WebContentsImpl` keeps the colour provider source it asks.
    pub color_source: usize,
    /// `ThemeSource::GenerateColorsCss(out, provider, url, flag, flag)`, which writes the style sheet.
    pub generate_colors_css: usize,
    pub generate_colors_css_prologue: &'static [u8],
    /// `BrowserViewTabbedLayoutImpl::CalculateProposedLayout`, slot 16 of its vtable. It fills a
    /// `std::map` of view to bounds that the layout then applies.
    pub layout_vtable: usize,
    pub proposed_slot: usize,
    pub proposed: usize,
    /// The layout object keeps the host view at +0x38, the contents container at +0xb8, the side
    /// panel at +0xc0 and the content shown while the side panel animates at +0xc8.
    pub host_view: usize,
    /// The view behind the whole area below the top row, which the rail shares the height of.
    pub page_area: usize,
    pub contents_container: usize,
    /// The browser's vertical tab list, which lies left of the page.
    pub vertical_tabs: usize,
    /// The browser's top row: toolbar, address bar and the buttons around it.
    pub toolbar: usize,
    /// The view that draws the outline around the contents, which has the contents' bounds.
    pub contents_outline: usize,
    pub side_panel: usize,
    pub side_panel_animation: usize,
    /// `views::View::DeprecatedLayoutImmediately`.
    pub layout_immediately: usize,
    pub layout_immediately_prologue: &'static [u8],
    /// `views::View::GetEventHandlerForPoint`, which the click diagnostic calls.
    pub event_handler_for_point: usize,
}
}

row_struct! {
/// Values for `chrome_layout.rs`.
pub struct Layout {
    /// Experiment, Chrome 156.0.8078.48 only. The frame view paints its title bar with colours from
    /// a colour provider that other windows of the profile share. Rivet wraps the frame view's
    /// `OnPaint` and swaps the frame colours in that provider only while a Rivet window paints.
    ///
    /// `views::View::GetColorProvider` returns the provider. It keeps generated colours in a tree
    /// whose nodes hold the colour id at `+0x20` and the colour at `+0x24`.
    /// Whether the colour provider's tree layout (head at +0x18, id at +0x20, colour at +0x24) and
    /// `toolbar.get_color` were checked on this build. Only then does Rivet read, create or swap
    /// provider colours. A build that is not verified paints the browser's own frame.
    pub provider_verified: bool,
    pub on_paint_slot: usize,
    pub on_paint: usize,
    pub get_color_provider: usize,
    pub draw_color: usize,
    /// The active, inactive and unthemed-inactive frame colour ids. The last one fills the band
    /// behind the top row, which the panel's rounded corners now reveal. An active window with an
    /// unthemed frame may read 0x13D and 0x144, which hold the same blue as the active colour.
    pub frame_ids: &'static [i32],
    /// Experiment, Chrome 156.0.8078.48 only. `CustomFloatingCorner::OnPaint`: the browser view's two
    /// corner views at the page's left edge fill a wedge with the browser's frame colour, which
    /// rounds the page's corners against the frame. In a Rivet window the frame is Rivet's, so the
    /// wedge is painted with Rivet's frame colour instead of Chrome's blue.
    pub corner_vtable: usize,
    pub corner_paint: usize,
    /// Experiment, Chrome 156.0.8078.48 only: `WindowsCaptionButton::CalculatePreferredSize` in the
    /// caption button's vtable. Rivet windows get caption buttons as tall as the shell study's top
    /// row, so the native buttons sit centred in it.
    pub caption_vtable: usize,
    pub caption_size_slot: usize,
    pub caption_size: usize,
    pub caption_paint_slot: usize,
    pub caption_paint: usize,
    /// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::GetTopInset` in the frame view's
    /// vtable. It is the height of the browser's own title bar, which also sizes the caption buttons.
    pub top_inset_slot: usize,
    pub top_inset: usize,
    /// Experiment, Chrome 156.0.8078.48 only: `BrowserFrameViewWin::Layout` in the frame view's
    /// vtable. It sizes the caption button container from the tab strip, not from the title bar
    /// height, so the container is resized once the browser's own layout is done.
    pub layout_slot: usize,
    pub layout: usize,
    /// `views::View::SetBoundsRect`, and where a frame view keeps its caption button container and a
    /// view keeps its bounds.
    pub set_bounds_rect: usize,
    pub caption_container_offset: usize,
    pub view_bounds_offset: usize,
    /// The vtable of the `views::View` part of `BrowserView`, which sits behind other base classes.
    pub browser_view_view_vtable: usize,
    /// The `views::View` vtable of the frame view the browser window uses.
    pub frame_vtable: usize,
    /// `views::View::DeprecatedLayoutImmediately` and its first instructions.
    pub layout_immediately: usize,
    pub layout_immediately_prologue: &'static [u8],
    /// Layer helpers of the panel clip: `View::SetPaintToLayer`, `Layer::SetRoundedCornerRadius`,
/// `Layer::SetMasksToBounds`, `Layer::StackAtTop` and `Layer::SetFillsBoundsOpaquely`.
    pub set_layer: usize,
    pub set_layer_prologue: &'static [u8],
    pub set_radius: usize,
    pub set_radius_prologue: &'static [u8],
    pub set_masks: usize,
    pub set_masks_prologue: &'static [u8],
    pub stack_at_top: usize,
    pub stack_at_top_prologue: &'static [u8],
    pub set_opaque: usize,
    pub set_opaque_prologue: &'static [u8],
    pub on_paint_prologue: &'static [u8],
    pub draw_color_prologue: &'static [u8],
    pub get_color_provider_prologue: &'static [u8],
    pub corner_paint_prologue: &'static [u8],
    pub layout_prologue: &'static [u8],
    pub set_bounds_rect_prologue: &'static [u8],
    /// `BrowserFrameViewWin` holds the `BrowserView`'s `BrowserWindow` subobject here, and the
/// subobject lies this many bytes after the start of the `BrowserView`.
    pub frame_browser_window_offset: usize,
    pub browser_window_in_view: usize,
    pub caption_paint_prologue: &'static [u8],
    /// Colour ids `WindowsCaptionButton::PaintButtonContents` reads for its glyphs: active, inactive
    /// and on the toolbar. Ids shift between milestones.
    pub caption_ink_ids: &'static [i32],
    /// Added to the top inset a Rivet window reports, in DIPs. Chrome's client area begins at the top
    /// of the frame in its one-row layout, whatever the inset says, so Rivet's extra title height only
    /// reaches the caption buttons. Brave's client area begins at the inset, which would leave the
    /// page lower than the row, so its extra cancels out.
    pub top_inset_adjust: i32,
}
}

row_struct! {
/// What a build needs to show a page in developer mode.
pub struct DevMode {
    /// The browser's own commands the developer bar runs, as `WM_COMMAND` ids, or zero when the build
    /// has none. They are the commands of the Ctrl+Shift+C and Ctrl+Shift+J keys, read from the
    /// browser's accelerator table.
    pub inspect: u32,
    pub console: u32,
    /// Colour ids of this build and the colours the row takes in developer mode, as `0xAARRGGBB`.
    /// An id that `colours` does not hold is asked of the provider first, like the ones that it does.
    pub colours: &'static [(i32, u32)],
    /// `gfx::Font::Font(this, &name, size)` and `gfx::FontList::FontList(this, &font)`, which build
    /// the monospace font of the address. A build without them keeps the address in its own font.
    pub font_new: usize,
    pub font_new_prologue: &'static [u8],
    pub font_list_new: usize,
    pub font_list_new_prologue: &'static [u8],
}
}

const UNCHECKED_DEV: DevMode = DevMode {
    inspect: 0,
    console: 0,
    colours: &[],
    font_new: ABSENT,
    font_new_prologue: &[0],
    font_list_new: ABSENT,
    font_list_new_prologue: &[0],
};

const DEV_GROUND: u32 = 0xFF2A_2C31;
const DEV_RULE: u32 = 0xFF17_181B;
const DEV_ICON: u32 = 0xFFC4_C7CE;
const DEV_ICON_HOT: u32 = 0xFFFF_FFFF;
const DEV_ICON_OFF: u32 = 0xFF70_737B;
const DEV_HOVER: u32 = 0x17FF_FFFF;
const DEV_RIPPLE: u32 = 0x24FF_FFFF;

const DEV_CHROME_156: DevMode = DevMode {
    inspect: 40023,
    console: 40005,
    colours: &[
        (0x13C, DEV_GROUND),
        (1590, DEV_GROUND),
        (796, 0),
        (797, DEV_HOVER),
        (1595, DEV_ICON),
        (1596, DEV_ICON),
        (1597, DEV_ICON_OFF),
        (1600, DEV_ICON_HOT),
        (1598, DEV_ICON_HOT),
        (1599, DEV_ICON_OFF),
        (1603, DEV_RULE),
        (1611, DEV_HOVER),
        (1612, DEV_RIPPLE),
        (1644, DEV_ICON_HOT),
        (1645, DEV_ICON_HOT),
        (1646, DEV_ICON_HOT),
        (1005, DEV_ICON),
        (1006, DEV_ICON),
        (1007, DEV_ICON_HOT),
        (1048, DEV_ICON),
        (1049, DEV_ICON),
        (1052, DEV_ICON),
        (1055, DEV_ICON_HOT),
        (1056, DEV_ICON),
    ],
    ..UNCHECKED_DEV
};

/// Chrome 156.0.8078.74, whose font functions are read from its public symbols and checked against
/// the module file.
const DEV_CHROME_156_74: DevMode = DevMode {
    font_new: 0x02F7_2AE0, // ??0Font@gfx@@QEAA@AEBV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@std@@H@Z
    font_new_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2],
    font_list_new: 0x02F7_27D0, // ??0FontList@gfx@@QEAA@AEBVFont@1@@Z
    font_list_new_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE],
    ..DEV_CHROME_156
};

/// Brave's colour ids sit 17 above Chrome's from 574 on.
const DEV_BRAVE_156: DevMode = DevMode {
    colours: &[
        (0x13C, DEV_GROUND),
        (1607, DEV_GROUND),
        (813, 0),
        (814, DEV_HOVER),
        (1612, DEV_ICON),
        (1613, DEV_ICON),
        (1614, DEV_ICON_OFF),
        (1617, DEV_ICON_HOT),
        (1615, DEV_ICON_HOT),
        (1616, DEV_ICON_OFF),
        (1620, DEV_RULE),
        (1628, DEV_HOVER),
        (1629, DEV_RIPPLE),
        (1661, DEV_ICON_HOT),
        (1662, DEV_ICON_HOT),
        (1663, DEV_ICON_HOT),
        (1022, DEV_ICON),
        (1023, DEV_ICON),
        (1024, DEV_ICON_HOT),
        (1065, DEV_ICON),
        (1066, DEV_ICON),
        (1069, DEV_ICON),
        (1072, DEV_ICON_HOT),
        (1073, DEV_ICON),
    ],
    ..DEV_CHROME_156
};

/// Edge 156.0.4314.50. Its colour ids sit 88 to 101 above Chrome's. The font functions are read from
/// its public symbols and checked against the module file.
const DEV_EDGE_156: DevMode = DevMode {
    colours: &[
        (316, DEV_GROUND),
        (2893, DEV_GROUND),
        (2082, 0),
        (2083, DEV_HOVER),
        (2898, DEV_ICON),
        (2899, DEV_ICON),
        (2900, DEV_ICON_OFF),
        (2903, DEV_ICON_HOT),
        (2901, DEV_ICON_HOT),
        (2902, DEV_ICON_OFF),
        (2906, DEV_RULE),
        (2914, DEV_HOVER),
        (2915, DEV_RIPPLE),
        (2947, DEV_ICON_HOT),
        (2948, DEV_ICON_HOT),
        (2949, DEV_ICON_HOT),
        (2951, DEV_ICON_HOT),
        (3409, DEV_ICON_HOT),
        (3410, DEV_ICON_HOT),
        (2300, DEV_ICON),
        (2301, DEV_ICON),
        (2302, DEV_ICON_HOT),
        (2343, DEV_ICON),
        (2344, DEV_ICON),
        (2347, DEV_ICON),
        (2350, DEV_ICON_HOT),
        (2351, DEV_ICON),
        (3009, 0),
    ],
    font_new: 0x0267_058C, // ??0Font@gfx@@QEAA@AEBV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@std@@H@Z
    font_new_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2],
    font_list_new: 0x02B0_FDCE, // ??0FontList@gfx@@QEAA@AEBVFont@1@@Z
    font_list_new_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE],
    ..DEV_CHROME_156
};

const UNCHECKED_TOOLBAR: Toolbar = Toolbar {
    layout_constant: ABSENT,
    layout_constant_prologue: &[0],
    theme_changed: ABSENT,
    theme_changed_prologue: &[0],
    paint_layer: ABSENT,
    paint_layer_prologue: &[0],
    set_visible: ABSENT,
    set_visible_prologue: &[0],
    from_vector_icon: ABSENT,
    from_vector_icon_prologue: &[0],
    update_location: ABSENT,
    update_location_prologue: &[0],
    update_location_icon: ABSENT,
    update_location_icon_prologue: &[0],
    layout_insets: ABSENT,
    layout_insets_prologue: &[0],
    toolbar_button_inset: 0,
    toolbar_inset: 0,
    location_icon_inset: 0,
    update_icon: ABSENT,
    update_icon_prologue: &[0],
    formatted_url: ABSENT,
    formatted_url_prologue: &[0],
    model_url: ABSENT,
    model_url_prologue: &[0],
    omnibox_update: ABSENT,
    omnibox_update_prologue: &[0],
    omnibox_revert_all: ABSENT,
    omnibox_revert_all_prologue: &[0],
    omnibox_tab_changed: ABSENT,
    omnibox_tab_changed_prologue: &[0],
    omnibox_textfield: 0,
    update_text_style: ABSENT,
    update_text_style_prologue: &[0],
    apply_color: ABSENT,
    apply_color_bytes: &[0],
    title_weight: 600,
    apply_weight: ABSENT,
    apply_weight_bytes: &[0],
    title_was_set: ABSENT,
    title_was_set_prologue: &[0],
    observer_offset: 0,
    operator_new: ABSENT,
    operator_new_bytes: &[0],
    operator_delete: ABSENT,
    operator_delete_bytes: &[0],
    model_delegate: 0,
    browser_delegate_vtable: ABSENT,
    active_web_contents: ABSENT,
    active_web_contents_slot: 0,
    web_contents_vtable: ABSENT,
    web_contents_title: ABSENT,
    web_contents_title_slot: 0,
    location_bar_view_base: 0,
    hover_value: 0,
    field_background: 0,
    background_vtable: ABSENT,
    painter_vtable: ABSENT,
    painter_radii: 0,
    painter_insets: 0,
    field_state: 0,
    field_state_focused: 0,
    focused_fill_id: 0,
    focus_ring_remove: ABSENT,
    set_transform: ABSENT,
    set_transform_prologue: &[0],
    set_brightness: ABSENT,
    set_brightness_prologue: &[0],
    view_layer: 0,
    view_size: 0,
    refresh_background: ABSENT,
    refresh_background_prologue: &[0],
    chip_text: ABSENT,
    chip_text_prologue: &[0],
    textfield_theme: ABSENT,
    textfield_theme_prologue: &[0],
    textfield_set_font: ABSENT,
    textfield_set_font_bytes: &[0],
    font_size: ABSENT,
    font_size_bytes: &[0],
    derive_size: ABSENT,
    derive_size_bytes: &[0],
    textfield_render_text: 0,
    render_text_font_list: 0,
    gurl_destructor: ABSENT,
    gurl_destructor_bytes: &[0],
    gurl_scheme_begin: 0,
    gurl_scheme_length: 0,
    anchor_rect: ABSENT,
    anchor_rect_prologue: &[0],
    anchor_view_access: &[0],
    is_drawn: ABSENT,
    is_drawn_bytes: &[0],
    bounds_in_screen: ABSENT,
    bounds_bytes: &[0],
    parent_offset: 0,
    is_drawn_slot: 0,
    browser_widget_vtable: ABSENT,
    plain_widget_vtable: ABSENT,
    paint_slot: 0,
    paint: ABSENT,
    layout_slot: 0,
    layout: ABSENT,
    color_provider: ABSENT,
    root_view_offset: 0,
    theme_slot: 0,
    hidden: &[],
    get_widget_slot: 0,
    overrides: &[],
    colours: &[],
    dev: UNCHECKED_DEV,
    hwnd_for_widget: ABSENT,
    hwnd_for_widget_head: &[0],
    hwnd_for_widget_tail: &[0],
    source_in_widget: 0,
    content_window_for_hwnd: ABSENT,
    widget_for_window: ABSENT,
    back_icon: ABSENT,
    forward_icon: ABSENT,
    reload_icon: ABSENT,
    nav_icon_names: &[],
    page_info_icon: ABSENT,
    google_g_icon: ABSENT,
    google_g_monochrome_icon: ABSENT,
    edit_icons: &[],
    http_icon: ABSENT,
    http_icon_name: &[0],
    page_info_bytes: &[0],
    page_info_reps: ABSENT,
    page_info_id: ABSENT,
    page_info_path: ABSENT,
    page_info_size: 0,
    http_size: 0,
    extra_lock_icons: &[],
    extra_warning_icons: &[],
    anchor_view_access_at: 0,
    skip: "",
    anchor_tracker_offset: 0,
    toolbar_layout: ABSENT,
    toolbar_layout_prologue: &[0],
    infobar_vtable: ABSENT,
    infobar_layout: ABSENT,
    infobar_layout_slot: 0,
    leading_space: 32,
    toolbar_top: 5,
    toolbar_bottom: 4,
    button_padding: 4,
    field_fill_margin: 3,
    size_slots: (0, 0),
    paint_children_slot: 0,
    get_color: ABSENT,
    get_color_bytes: &[0],
};

const UNCHECKED_VERTICALTABS: VerticalTabs = VerticalTabs {
    top_row: None,
    supported: false,
    slot: 0,
    should_display: ABSENT,
    browser_view_for_browser: ABSENT,
    hwnd_for_view: ABSENT,
    notify_mode_changed: ABSENT,
    update_collapse_action: ABSENT,
    destructor: ABSENT,
    destructor_prologue: &[0],
    browser_offset: 0,
    mode_offset: 0,
    width_offset: 0,
    browser_view_view_vtable: ABSENT,
    region_preferred_size: ABSENT,
    region_preferred_size_slot: 0,
    invalidate_layout: ABSENT,
    invalidate_layout_prologue: &[0],
    region_minimum_size: ABSENT,
    region_minimum_prologue: &[0],
    should_draw_tab_strip: ABSENT,
    should_draw_tab_strip_prologue: &[0],
    window_feature: ABSENT,
    window_feature_prologue: &[0],
    window_feature_tab_strip: 0,
    window_feature_callers: (0, 0),
    set_bounds_rect: ABSENT,
    set_bounds_rect_prologue: &[0],
    toolbar_in_view: 0,
    toolbar_leading: 0,
    detour_strip: false,
};

const UNCHECKED_NAVIGATION: Navigation = Navigation {
    supported: false,
    content_window_for_hwnd: ABSENT,
    browser_view_for_window: ABSENT,
    active_web_contents: ABSENT,
    gurl_from_text: ABSENT,
    gurl_wide: false,
    gurl_empty: ABSENT,
    gurl_drop: ABSENT,
    open_url_params: ABSENT,
    open_url_params_drop: ABSENT,
    open_url: ABSENT,
    signatures: &[],
};

const UNCHECKED_TABS: Tabs = Tabs {
    supported: false,
    content_window_for_hwnd: ABSENT,
    browser_view_for_window: ABSENT,
    active_web_contents: ABSENT,
    get_tab_model: ABSENT,
    tab_count: ABSENT,
    tab_at: ABSENT,
    tab_object: ABSENT,
    tab_collection: 0,
    tab_contents: 0,
    activate_tab: ABSENT,
    close_tab: ABSENT,
    title: ABSENT,
    new_tab: ABSENT,
    new_tab_prologue: &[],
    new_tab_address: None,
    new_tab_takes_address: false,
    new_tab_delay_ms: 0,
    last_committed_url: ABSENT,
    visible_url: ABSENT,
    get_group: ABSENT,
    has_group: ABSENT,
    lookup_group: ABSENT,
    change_group: ABSENT,
    allocate: ABSENT,
    add_to_new_group: ABSENT,
    allocate_prologue: &[0],
    add_to_new_group_prologue: &[0],
    signatures: &[],
    browser_in_view: 0,
    groups: false,
};

const UNCHECKED_SPLIT: Split = Split {
    supported: false,
    add_to_new_split: ABSENT,
    remove_split: ABSENT,
    get_split_for_tab: ABSENT,
    allocate: ABSENT,
    set_rounded_corners: ABSENT,
    set_rounded_corners_prologue: &[0],
    set_rounded_corners_changed: ABSENT,
    set_rounded_corners_parity: ABSENT,
    update_border: ABSENT,
    update_border_prologue: &[0],
    set_border_slot: 0,
    set_visible_slot: 0,
    outline_member: 0,
    calculate_layout: ABSENT,
    calculate_layout_prologue: &[0],
    layout_delegate_offset: 0,
    split_insets_member: 0,
    containers_member: 0,
    container_count_member: 0,
    active_member: 0,
    schedule_paint: ABSENT,
    web_contents_focused: ABSENT,
    web_contents_focused_prologue: &[0],
    container_layout: ABSENT,
    container_delegate_offset: 0,
    mini_toolbar_member: 0,
    flex_layout: ABSENT,
    flex_layout_host: 0,
    location_bar_vtable: ABSENT,
    view_size: 0,
    signatures: &[],
};

const UNCHECKED_SIDEPANEL: SidePanel = SidePanel {
    supported: false,
    side_panel_vtable: ABSENT,
    browser_offset: 0,
    features_offset: 0,
    features_coordinator: 0,
    browser_view_view_vtable: ABSENT,
    coordinator_vtable: ABSENT,
    show: ABSENT,
    show_prologue: &[0],
    entry_id_to_string: ABSENT,
    entry_id_to_string_prologue: &[0],
    extension_entry: 0,
    entry_count: 0,
    toggle: ABSENT,
    toggle_prologue: &[0],
    entry_showing: ABSENT,
    entry_showing_prologue: &[0],
    registry_from: ABSENT,
    registry_from_prologue: &[0],
    get_color_provider: ABSENT,
    get_color_provider_prologue: &[0],
    source_in_widget: 0,
    color_source: 0,
    generate_colors_css: ABSENT,
    generate_colors_css_prologue: &[0],
    layout_vtable: ABSENT,
    proposed_slot: 0,
    proposed: ABSENT,
    host_view: 0,
    page_area: 0,
    contents_container: 0,
    vertical_tabs: 0,
    toolbar: 0,
    contents_outline: 0,
    side_panel: 0,
    side_panel_animation: 0,
    layout_immediately: ABSENT,
    layout_immediately_prologue: &[0],
    event_handler_for_point: ABSENT,
};

/// A layout section that records no address, so every frame hook stays off.
const UNCHECKED_LAYOUT: Layout = Layout {
    on_paint_slot: 0,
    provider_verified: false,
    on_paint: ABSENT,
    get_color_provider: ABSENT,
    draw_color: ABSENT,
    frame_ids: &[],
    corner_vtable: ABSENT,
    corner_paint: ABSENT,
    caption_vtable: ABSENT,
    caption_size_slot: 0,
    caption_size: ABSENT,
    caption_paint_slot: 0,
    caption_paint: ABSENT,
    top_inset_slot: 0,
    top_inset: ABSENT,
    layout_slot: 0,
    layout: ABSENT,
    set_bounds_rect: ABSENT,
    caption_container_offset: 0,
    view_bounds_offset: 0,
    browser_view_view_vtable: ABSENT,
    frame_vtable: ABSENT,
    layout_immediately: ABSENT,
    layout_immediately_prologue: &[0],
    set_layer: ABSENT,
    set_layer_prologue: &[0],
    set_radius: ABSENT,
    set_radius_prologue: &[0],
    set_masks: ABSENT,
    set_masks_prologue: &[0],
    stack_at_top: ABSENT,
    stack_at_top_prologue: &[0],
    set_opaque: ABSENT,
    set_opaque_prologue: &[0],
    on_paint_prologue: &[0],
    draw_color_prologue: &[0],
    get_color_provider_prologue: &[0],
    corner_paint_prologue: &[0],
    layout_prologue: &[0],
    set_bounds_rect_prologue: &[0],
    frame_browser_window_offset: 0,
    browser_window_in_view: 0,
    caption_paint_prologue: &[0],
    caption_ink_ids: &[],
    top_inset_adjust: 0,
};

pub static CHROME: Build = Build {
    location_bar: None,
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [156, 0, 8078, 48],
    toolbar: Toolbar {
        layout_constant: 0x0471_3020, // ?GetLayoutConstant@@YAHW4LayoutConstant@@@Z
        layout_constant_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0xF0, 0x45, 0x0D,
    ],
        theme_changed: 0x01DC_5870, // ?ThemeChanged@Widget@views@@QEAAXXZ
        theme_changed_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xC0, 0xC7, 0xDA, 0x0F,
    ],
        paint_layer: 0x007E_1A30, // ?PaintFromPaintRoot@View@views@@AEAAXAEBVPaintContext@ui@@@Z
        paint_layer_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x00, 0x06, 0x39, 0x11,
    ],
        set_visible: 0x0573_5770, // ?SetVisible@View@views@@UEAAX_N@Z
        set_visible_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x48,
    ],
        from_vector_icon: 0x009F_C510, // ?FromVectorIcon@ImageModel@ui@@SA?AV12@AEBUVectorIcon@gfx@@VColorVariant@2@HPEBU34@@Z
        from_vector_icon_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
    ],
        update_location: 0x00EE_3320, // ?Update@LocationIconView@@QEAAX_N0@Z
        update_location_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0, 0, 0,
    ],
        update_location_icon: 0x00EE_4220, // ?GetSecurityChipAccessibilityState@location_bar@@YA?AUSecurityChipAccessibilityState@1@PEBVLocationB
        update_location_icon_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0, 0, 0, 0x48, 0x89, 0xCE,
    ],
        layout_insets: 0x03EE_A610, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[
        0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x89, 0xD7, 0x48, 0x89, 0xCE,
    ],
        toolbar_button_inset: 6,
        toolbar_inset: 11,
        location_icon_inset: 3,
        update_icon: 0x009F_C150, // ?UpdateIcon@ToolbarButton@@UEAAXXZ
        update_icon_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x80, 0xB9, 0xD8, 0x06, 0, 0, 0x01,
    ],
        formatted_url: 0x0160_9DB0, // ?GetFormattedURL@LocationBarModelImpl@@AEBA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$alloca
        formatted_url_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x80, 0x01, 0, 0,
    ],
        model_url: 0x04F5_BBB0, // ?GetURL@LocationBarModelImpl@@UEBA?AVGURL@@XZ
        model_url_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0, 0, 0,
    ],
        omnibox_update: 0x00ED_FB30, // ?Update@OmniboxViewViews@@UEAAXXZ
        omnibox_update_prologue: &[
        0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x01, 0x25, 0xC9, 0x10,
    ],
        omnibox_revert_all: 0x00EE_8630, // ?RevertAll@OmniboxViewViews@@UEAAXXZ
        omnibox_revert_all_prologue: &[
        0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x01, 0x9A, 0xC8, 0x10,
    ],
        omnibox_tab_changed: 0x00EE_D110, // ?OnTabChanged@OmniboxViewViews@@QEAAXPEAVWebContents@content@@@Z
        omnibox_tab_changed_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE,
    ],
        omnibox_textfield: 0x10,
        update_text_style: 0x00EE_59C0, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@
        update_text_style_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88,
        0, 0, 0,
    ],
        apply_color: 0x0318_5FA0, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0, 0, 0,
    ],
        title_weight: 600,
        apply_weight: 0x0AF1_1D20, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0, 0x01, 0, 0,
    ],
        title_was_set: 0x0312_8B70, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x80, 0, 0, 0,
    ],
        observer_offset: 0x720,
        operator_new: 0x05D3_D6A0, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x03E0_6930, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6,
    ],
        model_delegate: 0x40,
        browser_delegate_vtable: 0x0FDB_7368, // .weak.??_7BrowserLocationBarModelDelegate@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTempl
        active_web_contents: 0x057F_48C0, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 19,
        web_contents_vtable: 0x0FE5_7C10, // .weak.??_7WebContentsImpl@content@@6BPageNavigator@1@@.default.?RejectPromiseOnDestruction@?$CdmProm
        web_contents_title: 0x00D7_8440, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        web_contents_title_slot: 53,
        location_bar_view_base: 0x100,
        hover_value: 0x5F8,
        field_background: 0x328,
        background_vtable: 0x0FC3_C730, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: 0x0FDC_F4F0, // ??_7SolidRoundRectPainter@?A0xF7BC1CD1@views@@6B@.b545234ba40531a795a52b0b8121bc9c
        painter_radii: 0x10,
        painter_insets: 0x20,
        field_state: 0x4E8,
        field_state_focused: 0x18,
        focused_fill_id: 1013,
        focus_ring_remove: 0x024C_A860, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0AC2_1430, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6,
    ],
        set_brightness: 0x0ACC_89D0, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30,
    ],
        view_layer: 0x20,
        view_size: 0x1D0,
        refresh_background: 0x00EE_0120, // ?RefreshBackground@LocationBarView@@AEAAXXZ
        refresh_background_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x60,
    ],
        chip_text: 0x00E4_D780, // ?GetSecurityChipText@location_bar@@YA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S
        chip_text_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0, 0, 0,
    ],
        textfield_theme: 0x01FD_2870, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x68,
    ],
        textfield_set_font: 0x0316_0CB0, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x78, 0x03, 0, 0, 0x48,
    ],
        font_size: 0x036B_B960, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0x31, 0x8B, 0x46, 0x44, 0x83, 0xF8, 0xFF, 0x74, 0x06,
    ],
        derive_size: 0x0318_74F0, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC7, 0x48, 0x89, 0xD6,
    ],
        textfield_render_text: 0x378,
        render_text_font_list: 0x48,
        gurl_destructor: 0x03E0_01F0, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE],
        gurl_scheme_begin: 0x20,
        gurl_scheme_length: 0x24,
        anchor_rect: 0x0ACA_C450, // ?GetAnchorRect@BubbleDialogDelegate@views@@UEBA?AVRect@gfx@@XZ
        anchor_rect_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC,
        0xD0, 0, 0, 0,
    ],
        anchor_view_access: &[
        0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
        0x48, 0x85, 0xDB, 0x74, 0x4D,
    ],
        is_drawn: 0x00CA_89C0, // ?IsDrawn@AppMenuButton@@UEBA_NXZ
        is_drawn_bytes: &[
        0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
        0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
    ],
        bounds_in_screen: 0x0086_1140, // ?GetBoundsInScreen@View@views@@QEBA?AVRect@gfx@@XZ
        bounds_bytes: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF],
        parent_offset: 0x198,
        is_drawn_slot: 19,
        browser_widget_vtable: 0x0FBC_AD10, // ??_7BrowserWidget@@6BNativeWidgetDelegate@internal@views@@@
        plain_widget_vtable: 0x0FC3_15C0, // ??_7Widget@views@@6BNativeWidgetDelegate@internal@1@@
        paint_slot: 30,
        paint: 0x0315_D050, // ?OnNativeWidgetPaint@Widget@views@@UEAAXAEBVPaintContext@ui@@@Z
        layout_slot: 44,
        layout: 0x00F9_7180, // ?LayoutRootViewIfNecessary@Widget@views@@UEAAXXZ
        color_provider: 0x04AA_27D0, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        root_view_offset: 0x1A0,
        theme_slot: 99,
        hidden: &[
        (0x0FEA_63C8, 0x01D1_3080), // BrowserAppMenuButton
        (0x0FEE_C268, 0x01AE_4DC0), // AvatarToolbarButton
        (0x0FED_B708, 0x003B_A0E0), // page_actions::PageActionView
        (0x0FF1_7AA8, 0x00A3_E970), // VerticalTabStripRegionView
        // The side panel's own header: Rivet draws that row in the strip the layout leaves above the panel.
        (0x0FE6_8688, 0x00A3_E970), // SidePanelHeader
        // The toolbar button that shows the open side panel's entry: the Sidebar's rail shows it.
        (0x0FEA_4988, 0x01D6_E1F0), // PinnedActionToolbarButton
            (0x0FEA_8908, 0x01D6_E1F0), // SplitTabsToolbarButton
    ],
        get_widget_slot: 11,
        size_slots: (0, 0),
        paint_children_slot: 0,
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (16, 28), // kLocationBarHeight
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (47, 18), // kToolbarButtonIconSize
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ],
        dev: DEV_CHROME_156,
        colours: &[
        (0x13C, PAPER),      // kColorFrameActive
        (796, PAPER),        // kColorLocationBarBackground
        (797, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
        (1003, 0),           // Page info chip background: clear, so the field is one surface
        (159, FOCUS_RING),   // kColorSysStateFocusRing
        (314, FOCUS_RING),   // kColorFocusableBorderFocused
        (465, SELECTION),    // kColorTextfieldSelectionBackground
        (466, INK),          // kColorTextfieldSelectionForeground
        (1004, 0),           // kColorOmniboxIconBackgroundTonal
        (1005, SOFT_INK),    // kColorOmniboxIconForeground: the icon of the field while editing
        (1006, SOFT_INK),    // kColorOmniboxIconForegroundTonal
        (1007, INK),         // kColorOmniboxIconHover
        (1013, PAPER),       // kColorOmniboxResultsBackground, the suggestion list and the field with it
        (1014, 0xFFF0_EFEC), // kColorOmniboxResultsBackgroundHovered
        (1015, 0x0F1F_2024), // kColorOmniboxResultsBackgroundHoverOverlay
        (1016, ACCENT_TINT), // kColorOmniboxResultsBackgroundSelected: the accent at 9 percent over paper
        (1019, SOFT_INK),    // kColorOmniboxResultsButtonIcon
        (1020, INK),         // kColorOmniboxResultsButtonIconSelected
        (1028, 0),           // kColorOmniboxResultsFocusIndicator: the design marks a row by its tint alone
        (1029, SOFT_INK),    // kColorOmniboxResultsIcon
        (1030, 0x0F1F_2024), // kColorOmniboxResultsIconGM3Background
        (1031, INK),         // kColorOmniboxResultsIconSelected
        (1032, INK),         // kColorOmniboxResultsIconHovered
        (1035, 0xFF8A_8690), // kColorOmniboxResultsTextDimmed
        (1036, 0xFF4A_4850), // kColorOmniboxResultsTextDimmedSelected
        (1041, 0xFF8A_8690), // kColorOmniboxResultsTextSecondary
        (1042, 0xFF4A_4850), // kColorOmniboxResultsTextSecondarySelected
        (1043, INK),         // kColorOmniboxResultsTextSelected
        (1044, 0xFF4A_4850), // kColorOmniboxResultsUrl
        (1045, INK),         // kColorOmniboxResultsUrlSelected
        (1052, SOFT_INK),    // kColorOmniboxSecurityChipText: the icon of the field while editing
        (1053, SELECTION),   // kColorOmniboxSelectionBackground: the accent at 25 percent over paper
        (1054, INK),         // kColorOmniboxSelectionForeground
        (1046, 0xFFB3_261E), // kColorOmniboxSecurityChipDangerous: the "Not secure" label and its warning
        (1047, 0x14B3_261E), // kColorOmniboxSecurityChipDangerousBackground: the label's tint, 8 percent
        (1055, INK),         // kColorOmniboxText: the address while it is edited
        (1056, 0xFF8A_8690), // kColorOmniboxTextDimmed
        (1590, PAPER),       // kColorToolbar
        (1644, 0xFF1A_1A1A), // Caption button glyph, active window
        (1645, 0xFF1A_1A1A), // Caption button glyph, inactive window
        (1646, 0xFF1A_1A1A), // Caption button glyph without a theme colour
        (1647, 0xFFC4_2B1C), // Close button hover fill
        (1650, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
        (1595, 0xFF4A_4850), // kColorToolbarButtonIcon
        (1598, 0xFF1F_2024), // kColorToolbarButtonIconHovered
        (1599, 0xFFBD_BCBC), // kColorToolbarButtonIconInactive: the icon colour at 35 percent over paper
        (1603, HAIRLINE),    // kColorToolbarContentAreaSeparator
        (1611, 0x0F1F_2024), // kColorToolbarInkDropHover
        (1612, 0x1F1F_2024), // kColorToolbarInkDropRipple
    ],
        get_color: 0x04AA24F0,
        get_color_bytes: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x89, 0xD7],
        hwnd_for_widget: 0x0AC0_B080, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28],
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9],
        source_in_widget: 0x48,
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x0315_C7C0, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x10E0_2EA8, // ?kBackArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x10E0_55C0, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x10E0_7DD0, // ?kReloadChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        nav_icon_names: &[b"kBackArrowChromeRefreshOldIcon", b"kForwardArrowChromeRefreshOldIcon", b"kReloadChromeRefreshOldIcon"],
        page_info_icon: 0x10F5_D470, // ?kSecurePageInfoChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: 0x10E0_A3F0, // ?kGoogleGLogoIcon@vector_icons@@3UVectorIcon@gfx@@B
        google_g_monochrome_icon: 0x10E0_A438, // ?kGoogleGLogoMonochromeIcon@vector_icons@@3UVectorIcon@gfx@@B
        edit_icons: &[0x10E0_5680, 0x10E0_56B8, 0x10E0_8278, 0x10E0_82C0, 0x10E0_82F8],
        http_icon: 0x10F5_CE60, // ?kHttpChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kHttpChromeRefreshOldIcon",
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
    ],
        page_info_reps: 0x10F5_D438, // ?kSecurePageInfoChromeRefreshOldRepList@omnibox@@3QBUVectorIconRep@gfx@@B.llvm.2910200201067176236
        page_info_id: 0x10F5_D448, // ?kSecurePageInfoChromeRefreshOldIconId@omnibox@@3QBDB.llvm.2910200201067176236
        page_info_path: 0x10F6_6E40, // ?kSecurePageInfoChromeRefreshOldPath@omnibox@@3QBUPathElement@gfx@@B.llvm.2910200201067176236
        page_info_size: 282,
        http_size: 263,
        extra_lock_icons: &[],
        extra_warning_icons: &[],
        anchor_view_access_at: 0x2a,
        skip: "",
        anchor_tracker_offset: 0x300,
        toolbar_layout: ABSENT,
        toolbar_layout_prologue: &[0],
        infobar_vtable: ABSENT,
        infobar_layout: ABSENT,
        infobar_layout_slot: 0,
        leading_space: 32,
        toolbar_top: 5,
        toolbar_bottom: 4,
        button_padding: 4,
        field_fill_margin: 3,
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x0FDA_FE00, // .weak.??_7VerticalTabStripStateControllerImpl@tabs@@6BVerticalTabStripStateController@1@@.default.?R
        should_display: 0x0369_C6E0, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x0034_D3E0, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x036A_A950, // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0A1D_B330, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x020F_3D90, // ??0VerticalTabStripStateController@tabs@@IEAA@AEAVBrowserWindowInterface@@@Z
        destructor: 0x0296_C3F0, // ??_EVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x89, 0xd7, 0x48, 0x89, 0xce, 0xe8],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x0FE7_BF68, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        region_preferred_size: 0x0BC7_FA80, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x0FF1_7CF8, // .weak.??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$Cd
        invalidate_layout: 0x0411_6480, // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        region_minimum_size: 0x0BC7_F9F0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7, 0x02, 0, 0,
        0, 0,
    ],
        should_draw_tab_strip: 0x0528_6C00, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[0x56, 0x57, 0x53],
        window_feature: ABSENT,
        window_feature_prologue: &[0],
        window_feature_tab_strip: 0,
        window_feature_callers: (0, 0),
        set_bounds_rect: ABSENT,
        set_bounds_rect_prologue: &[0],
        toolbar_in_view: 0,
        detour_strip: false,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true,
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BC8_1E10, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BC8_2340, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x042B_C7A0, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x0588_DF10, // ??0GURL@@QEAA@XZ
        gurl_drop: 0x03E0_01F0, // ??1GURL@@QEAA@XZ
        open_url_params: 0x072E_8660, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui
        open_url_params_drop: 0x0204_54C0, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x0366_BC50, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@$$A6AXAE
        signatures: &[
        (
            0x00D5_64A0,
            &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
        ),
        (0x0BC8_1E10, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
        (
            0x042B_C7A0,
            &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        ),
        (0x0588_DF10, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
        (0x03E0_01F0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
        (0x072E_8660, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
        (0x0204_54C0, &[0x41, 0x56, 0x56, 0x57, 0x53]),
        (0x0366_BC50, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
        (
            0x0BC8_2340,
            &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00],
        ),
    ],
    },
    tabs: Tabs {
        supported: true,
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BC8_1E10, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BC8_2340, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        browser_in_view: 0x560,
        groups: true,
        get_tab_model: 0x04FB_01F0, // ?Client@WebLocalFrameImpl@blink@@UEBAPEAVWebLocalFrameClient@2@XZ
        tab_count: 0x0369_CBE0, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0311_22A0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x0311_2220, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A20_11B0, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00D7_8440, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        new_tab_prologue: &[],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab: 0x0A0B_8F50, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x058A_48A0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x05CF_8BA0, // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x0311_2320, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x0a1f_10d0, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x0a1f_1130, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: 0x0a20_8550, // ?ChangeTabGroupVisuals@TabStripModel@@QEAAXAEBVTabGroupId@tab_groups@@VTabGroupVisualData@3@_N@Z
        allocate: 0x05d3_d6a0, // ??2@YAPEAX_K@Z
        add_to_new_group: 0x0a20_60b0, // ?AddToNewGroup@TabStripModel@@QEAA?AVTabGroupId@tab_groups@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        add_to_new_group_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81],
        signatures: &[
                (
                    0x0311_2320,
                    &[
                        0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x45, 0x85, 0xc0, 0x78,
                    ],
                ),
                (
                    0x0a1f_10d0,
                    &[
                        0x56, 0x57, 0x48, 0x83, 0xec, 0x38, 0x49, 0x89, 0xd0, 0x48, 0x8b, 0x05,
                    ],
                ),
                (
                    0x0a1f_1130,
                    &[
                        0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x89, 0xd6,
                    ],
                ),
                (
                    0x0a20_8550,
                    &[
                        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                    ],
                ),
                (
                    0x00D5_64A0,
                    &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
                ),
                (0x0BC8_1E10, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
                (0x0BC8_2340, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
                (0x04FB_01F0, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
                (0x0369_CBE0, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
                (0x0311_22A0, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
                (0x0311_2220, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
                (0x0A20_11B0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
                (0x00D7_8440, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
                (0x0A0B_8F50, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
                (0x058A_48A0, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
                (0x05CF_8BA0, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
            ],
    },
    split: Split {
        supported: true,
        add_to_new_split: 0x0A20_5410, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        remove_split: 0x0A20_7580, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x0311_2370, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x05d3_d6a0, // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x0369_EB40, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_prologue: &[
        0xf3, 0x0f, 0x10, 0x02, 0x0f, 0x2e, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
    ],
        set_rounded_corners_changed: 0x0369_EB80, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_parity: 0x0369_EB4D, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        update_border: 0x01FD_C720, // ?UpdateBorderAndOverlay@ContentsContainerView@@QEAAX_N00@Z
        update_border_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x48, 0x48, 0x89, 0xce,
    ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        outline_member: 0x448,
        calculate_layout: 0x0232_E330, // ?CalculateProposedLayout@MultiContentsView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        calculate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x054A_B3B0, // ?SchedulePaint@View@views@@QEAAXXZ
        web_contents_focused: 0x0A4C_B140, // ??_EMultiContentsViewDropTargetController@@UEAAPEAXI@Z
        web_contents_focused_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00],
        container_layout: 0x01FF_5270, // ?CalculateProposedLayout@ContentsContainerView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        container_delegate_offset: 0x320,
        mini_toolbar_member: 0x440,
        flex_layout: 0x054FCC60, // views::FlexLayout::CalculateProposedLayout
        flex_layout_host: 0x38,
        location_bar_vtable: 0x0FE86BE8, // LocationBarView primary vtable
        view_size: 0x1D0,
        signatures: &[
            (
                0x0A20_5410,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
            ),
            (0x0A20_7580, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58]),
            (
                0x0311_2370,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
            ),
            (0x05d3_d6a0, &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9]),
        ],
    },
    side_panel: SidePanel {
        supported: true,
        side_panel_vtable: 0x0FE6_7CA8, // .weak.??_7SidePanel@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        browser_offset: 0x560,
        features_offset: 0xd0,
        features_coordinator: 0x330,
        browser_view_view_vtable: 0x0FE7_BF68, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        coordinator_vtable: 0x0FE5_9440, // .weak.??_7SidePanelCoordinator@@6BSidePanelUIBase@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        show: 0x0997_6C50, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@std@@_N@Z
        show_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
        entry_id_to_string: 0x01E7_18C0, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@s
        entry_id_to_string_prologue: &[0x56, 0x48, 0x83, 0xec, 0x30, 0x48, 0x89, 0xce],
        extension_entry: 0x17,
        entry_count: 0x18,
        toggle: 0x0997_F450, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60],
        entry_showing: 0x0369_1A10, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x030B_E3C0, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00],
        get_color_provider: 0x0366_DB00, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x058C_6090, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@V?$alloc
        generate_colors_css_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_vtable: 0x0FDB_0278, // .weak.??_7BrowserViewTabbedLayoutImpl@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        proposed_slot: 16,
        proposed: 0x0104_8AB0, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLayoutImpl@@A
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x04BE_C780, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        event_handler_for_point: 0x007C_6530, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x011B_5610, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x04AA_27D0, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x036B_AD80, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x0FEC_AFC8, // .weak.??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        corner_paint: 0x0A4C_73A0, // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        caption_vtable: 0x0FF0_25C8, // .weak.??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        caption_size_slot: 74,
        caption_size: 0x0313_2D90, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x0313_32A0, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        top_inset_slot: 127,
        top_inset: 0x0A40_33E0, // ?GetTopInset@BrowserFrameViewWin@@UEBAH_N@Z
        layout_slot: 23,
        layout: 0x017A_E5E0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        set_bounds_rect: 0x04F4_2540, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x0FE7_BF68, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        frame_vtable: 0x0FE7_3E28, // .weak.??_7BrowserFrameViewWin@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        layout_immediately: 0x04BE_C780, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        set_layer: 0x00F0_5F10, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28],
        set_radius: 0x01D3_AB80, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x30],
        set_masks: 0x0316_7C40, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8b, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0ACC_8FD0, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_opaque: 0x020A_FEF0, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xec, 0x20],
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        draw_color_prologue: &[0x48, 0x83, 0xec, 0x38, 0x48, 0x8b],
        get_color_provider_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x78],
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        layout_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53],
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        frame_browser_window_offset: 0x340,
        browser_window_in_view: 0x1f0,
        caption_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_ink_ids: &[0x66C, 0x66D, 0x66E],
        top_inset_adjust: 0,
    },
    edge_vertical: None,
    edge_page: None,
    hidden_views: None,
    whale_row: None,
};

/// Google Chrome 156.0.8078.74, filled from that build's own public symbols and instruction bytes.
pub static CHROME_156_74: Build = Build {
    location_bar: Some(LocationBarMinimum { vtable: 0x0FCF_8EF0, slot: 15, function: 0x046A_A890, preferred_slot: 74, preferred: 0x046A_A630, width: 120, toolbar_past_caption: 1 }),
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [156, 0, 8078, 74],
    toolbar: Toolbar {
        layout_constant: 0x046A_AAA0, // ?GetLayoutConstant@@YAHW4LayoutConstant@@@Z
        layout_constant_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x90, 0xA5, 0x33, 0x0D],
        theme_changed: 0x0195_EC20, // ?ThemeChanged@Widget@views@@QEAAXXZ
        theme_changed_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0x64, 0x08, 0x10],
        paint_layer: 0x0089_8CB0, // ?PaintFromPaintRoot@View@views@@AEAAXAEBVPaintContext@ui@@@Z
        paint_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x80, 0xC3, 0x14, 0x11],
        set_visible: 0x054C_1240, // ?SetVisible@View@views@@UEAAX_N@Z
        set_visible_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD3, 0x48, 0x89, 0xCE],
        from_vector_icon: 0x00C9_1B00, // ?FromVectorIcon@ImageModel@ui@@SA?AV12@AEBUVectorIcon@gfx@@VColorVariant@2@HPEBU34@@Z
        from_vector_icon_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
    ],
        update_location: 0x00F0_9BD0, // ?Update@LocationIconView@@QEAAX_N0@Z
        update_location_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0, 0, 0,
    ],
        update_location_icon: 0x00F0_AB70, // ?GetSecurityChipAccessibilityState@location_bar@@YA?AUSecurityChipAccessibilityState@1@PEBVLocationB
        update_location_icon_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0, 0, 0, 0x48, 0x89, 0xCE,
    ],
        layout_insets: 0x04BE_FF60, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[
        0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x89, 0xD7, 0x48, 0x89, 0xCE,
    ],
        toolbar_button_inset: 6,
        toolbar_inset: 11,
        location_icon_inset: 3,
        update_icon: 0x00C9_16F0, // ?UpdateIcon@ToolbarButton@@UEAAXXZ
        update_icon_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x80, 0xB9, 0xD8, 0x06, 0, 0, 0x01,
    ],
        formatted_url: 0x0196_81A0, // ?GetFormattedURL@LocationBarModelImpl@@AEBA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$alloca
        formatted_url_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x70, 0x01, 0x00, 0x00],
        model_url: 0x056D_6520, // ?GetURL@LocationBarModelImpl@@UEBA?AVGURL@@XZ
        model_url_prologue: &[0x56, 0x57, 0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48],
        omnibox_update: 0x00F0_9460, // ?Update@OmniboxViewViews@@UEAAXXZ
        omnibox_update_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xD1, 0xBB, 0xAD, 0x10],
        omnibox_revert_all: 0x00F0_EA20, // ?RevertAll@OmniboxViewViews@@UEAAXXZ
        omnibox_revert_all_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x11, 0x66, 0xAD, 0x10],
        omnibox_tab_changed: 0x00F1_1770, // ?OnTabChanged@OmniboxViewViews@@QEAAXPEAVWebContents@content@@@Z
        omnibox_tab_changed_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE,
    ],
        omnibox_textfield: 0x10,
        update_text_style: 0x00F0_E120, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@
        update_text_style_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE],
        apply_color: 0x02F7_1730, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0, 0, 0,
    ],
        title_weight: 600,
        apply_weight: 0x0ACF_A820, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0, 0x01, 0, 0,
    ],
        title_was_set: 0x02F1_5200, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x80, 0, 0, 0,
    ],
        observer_offset: 0x720,
        operator_new: 0x05A1_E310, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x03BB_3150, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6,
    ],
        model_delegate: 0x40,
        browser_delegate_vtable: 0x0FC2_9790, // .weak.??_7BrowserLocationBarModelDelegate@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTempl
        active_web_contents: 0x0558_3E90, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 19,
        web_contents_vtable: 0x0FCC_9F18, // .weak.??_7WebContentsImpl@content@@6BPageNavigator@1@@.default.?RejectPromiseOnDestruction@?$CdmProm
        web_contents_title: 0x0106_B960, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        web_contents_title_slot: 53,
        location_bar_view_base: 0x100,
        hover_value: 0x5F8,
        field_background: 0x328,
        background_vtable: 0x0FAA_E920, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: 0x0FC4_1E08, // ??_7SolidRoundRectPainter@?A0xF7BC1CD1@views@@6B@.b545234ba40531a795a52b0b8121bc9c
        painter_radii: 0x10,
        painter_insets: 0x20,
        field_state: 0x4E8,
        field_state_focused: 0x18,
        focused_fill_id: 1013,
        focus_ring_remove: 0x0245_2770, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0A9F_2220, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6,
    ],
        set_brightness: 0x0AA9_AAA0, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30,
    ],
        view_layer: 0x20,
        view_size: 0x1D0,
        refresh_background: 0x0178_CDB0, // ?RefreshBackground@LocationBarView@@AEAAXXZ
        refresh_background_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x60,
    ],
        chip_text: 0x00C8_8D30, // ?GetSecurityChipText@location_bar@@YA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S
        chip_text_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x00, 0x00, 0x00,
    ],
        textfield_theme: 0x021D_BC90, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x68,
    ],
        textfield_set_font: 0x02F4_D2C0, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x78, 0x03, 0, 0, 0x48,
    ],
        font_size: 0x0347_2A50, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0x31, 0x8B, 0x46, 0x44, 0x83, 0xF8, 0xFF, 0x74, 0x06,
    ],
        derive_size: 0x02F7_2900, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[
        0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC7, 0x48, 0x89, 0xD6,
    ],
        textfield_render_text: 0x378,
        render_text_font_list: 0x48,
        gurl_destructor: 0x03BA_A460, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE],
        gurl_scheme_begin: 0x20,
        gurl_scheme_length: 0x24,
        anchor_rect: 0x0AA7_D550, // ?GetAnchorRect@BubbleDialogDelegate@views@@UEBA?AVRect@gfx@@XZ
        anchor_rect_prologue: &[
        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC,
        0xD0, 0, 0, 0,
    ],
        anchor_view_access: &[
        0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
        0x48, 0x85, 0xDB, 0x74, 0x4D,
    ],
        is_drawn: 0x001F_4E40, // ?IsDrawn@AppMenuButton@@UEBA_NXZ
        is_drawn_bytes: &[
        0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
        0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
    ],
        bounds_in_screen: 0x009D_6640, // ?GetBoundsInScreen@View@views@@QEBA?AVRect@gfx@@XZ
        bounds_bytes: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF],
        parent_offset: 0x198,
        is_drawn_slot: 19,
        browser_widget_vtable: 0x0FA3_CE70, // ??_7BrowserWidget@@6BNativeWidgetDelegate@internal@views@@@
        plain_widget_vtable: 0x0FAA_3790, // ??_7Widget@views@@6BNativeWidgetDelegate@internal@1@@
        paint_slot: 30,
        paint: 0x02F4_94A0, // ?OnNativeWidgetPaint@Widget@views@@UEAAXAEBVPaintContext@ui@@@Z
        layout_slot: 44,
        layout: 0x0166_5300, // ?LayoutRootViewIfNecessary@Widget@views@@UEAAXXZ
        color_provider: 0x042A_1D90, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        root_view_offset: 0x1A0,
        theme_slot: 99,
        hidden: &[
        (0x0FD1_86D0, 0x01D2_4BC0), // BrowserAppMenuButton
        (0x0FD5_E570, 0x0198_E7C0), // AvatarToolbarButton
        (0x0FD4_DA10, 0x0031_AFA0), // page_actions::PageActionView
        (0x0FD8_9DB0, 0x001F_6590), // VerticalTabStripRegionView
        // The side panel's own header: Rivet draws that row in the strip the layout leaves above the panel.
        (0x0FCD_A990, 0x001F_6590), // SidePanelHeader
        // The toolbar button that shows the open side panel's entry: the Sidebar's rail shows it.
        (0x0FD1_6C90, 0x00C9_16B0), // PinnedActionToolbarButton
            (0x0FD1_AC10, 0x00C9_16B0), // SplitTabsToolbarButton
            // The chevron Chrome adds when the row looks too narrow. The design has no such button.
            (0x0FD1_9CB0, 0x00C9_16B0), // OverflowButton
    ],
        get_widget_slot: 11,
        size_slots: (0, 0),
        paint_children_slot: 0,
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (47, 18), // kToolbarButtonIconSize
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ],
        dev: DEV_CHROME_156_74,
        colours: &[
        (0x13C, PAPER),      // kColorFrameActive
        (796, PAPER),        // kColorLocationBarBackground
        (797, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
        (1003, 0),           // Page info chip background: clear, so the field is one surface
        (159, FOCUS_RING),   // kColorSysStateFocusRing
        (314, FOCUS_RING),   // kColorFocusableBorderFocused
        (465, SELECTION),    // kColorTextfieldSelectionBackground
        (466, INK),          // kColorTextfieldSelectionForeground
        (1004, 0),           // kColorOmniboxIconBackgroundTonal
        (1005, SOFT_INK),    // kColorOmniboxIconForeground: the icon of the field while editing
        (1006, SOFT_INK),    // kColorOmniboxIconForegroundTonal
        (1007, INK),         // kColorOmniboxIconHover
        (1013, PAPER),       // kColorOmniboxResultsBackground, the suggestion list and the field with it
        (1014, 0xFFF0_EFEC), // kColorOmniboxResultsBackgroundHovered
        (1015, 0x0F1F_2024), // kColorOmniboxResultsBackgroundHoverOverlay
        (1016, ACCENT_TINT), // kColorOmniboxResultsBackgroundSelected: the accent at 9 percent over paper
        (1019, SOFT_INK),    // kColorOmniboxResultsButtonIcon
        (1020, INK),         // kColorOmniboxResultsButtonIconSelected
        (1028, 0),           // kColorOmniboxResultsFocusIndicator: the design marks a row by its tint alone
        (1029, SOFT_INK),    // kColorOmniboxResultsIcon
        (1030, 0x0F1F_2024), // kColorOmniboxResultsIconGM3Background
        (1031, INK),         // kColorOmniboxResultsIconSelected
        (1032, INK),         // kColorOmniboxResultsIconHovered
        (1035, 0xFF8A_8690), // kColorOmniboxResultsTextDimmed
        (1036, 0xFF4A_4850), // kColorOmniboxResultsTextDimmedSelected
        (1041, 0xFF8A_8690), // kColorOmniboxResultsTextSecondary
        (1042, 0xFF4A_4850), // kColorOmniboxResultsTextSecondarySelected
        (1043, INK),         // kColorOmniboxResultsTextSelected
        (1044, 0xFF4A_4850), // kColorOmniboxResultsUrl
        (1045, INK),         // kColorOmniboxResultsUrlSelected
        (1052, SOFT_INK),    // kColorOmniboxSecurityChipText: the icon of the field while editing
        (1053, SELECTION),   // kColorOmniboxSelectionBackground: the accent at 25 percent over paper
        (1054, INK),         // kColorOmniboxSelectionForeground
        (1046, 0xFFB3_261E), // kColorOmniboxSecurityChipDangerous: the "Not secure" label and its warning
        (1047, 0x14B3_261E), // kColorOmniboxSecurityChipDangerousBackground: the label's tint, 8 percent
        (1055, INK),         // kColorOmniboxText: the address while it is edited
        (1056, 0xFF8A_8690), // kColorOmniboxTextDimmed
        (1590, PAPER),       // kColorToolbar
        (1644, 0xFF9F_9F9D), // Base of the caption hover fill: at Chrome's 10 percent it matches the native 3.7 percent black
        (1645, 0xFF9F_9F9D), // The same, inactive window
        (1646, 0xFF9F_9F9D), // The same, without a theme colour
        (1647, 0xFFC4_2B1C), // Close button hover fill
        (1650, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
        (1595, 0xFF4A_4850), // kColorToolbarButtonIcon
        (1598, 0xFF1F_2024), // kColorToolbarButtonIconHovered
        (1599, 0xFFBD_BCBC), // kColorToolbarButtonIconInactive: the icon colour at 35 percent over paper
        (1603, HAIRLINE),    // kColorToolbarContentAreaSeparator
        (1611, 0x0F1F_2024), // kColorToolbarInkDropHover
        (1612, 0x1F1F_2024), // kColorToolbarInkDropRipple
    ],
        get_color: 0x0535_9A80,
        get_color_bytes: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x89, 0xD7],
        hwnd_for_widget: 0x0A9D_BEA0, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28],
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9],
        source_in_widget: 0x48,
        content_window_for_hwnd: 0x02F4_6B00, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02F4_8CE0, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x10C7_7378, // ?kBackArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x10C7_9A90, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x10C7_C2A0, // ?kReloadChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        nav_icon_names: &[b"kBackArrowChromeRefreshOldIcon", b"kForwardArrowChromeRefreshOldIcon", b"kReloadChromeRefreshOldIcon"],
        page_info_icon: 0x10DD_0F58, // ?kSecurePageInfoChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: 0x10C7_E8C0, // ?kGoogleGLogoIcon@vector_icons@@3UVectorIcon@gfx@@B
        google_g_monochrome_icon: 0x10C7_E908, // ?kGoogleGLogoMonochromeIcon@vector_icons@@3UVectorIcon@gfx@@B
        edit_icons: &[0x10C7_9B50, 0x10C7_9B88, 0x10C7_C748, 0x10C7_C790, 0x10C7_C7C8],
        http_icon: 0x10DD_0948, // ?kHttpChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kHttpChromeRefreshOldIcon",
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
    ],
        page_info_reps: 0x10DD_0F20, // ?kSecurePageInfoChromeRefreshOldRepList@omnibox@@3QBUVectorIconRep@gfx@@B
        page_info_id: 0x10DD_0F30, // ?kSecurePageInfoChromeRefreshOldIconId@omnibox@@3QBDB
        page_info_path: 0x10DD_A930, // ?kSecurePageInfoChromeRefreshOldPath@omnibox@@3QBUPathElement@gfx@@B
        page_info_size: 282,
        http_size: 263,
        extra_lock_icons: &[],
        extra_warning_icons: &[],
        anchor_view_access_at: 0x2a,
        skip: "",
        anchor_tracker_offset: 0x300,
        toolbar_layout: 0x011C_98D0, // ?Layout@ToolbarView@@UEAAXVPassKey@@@Z
        toolbar_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x8B, 0x05, 0x5E, 0xB7, 0x81, 0x10],
        infobar_vtable: ABSENT,
        infobar_layout: ABSENT,
        infobar_layout_slot: 0,
        leading_space: 32,
        toolbar_top: 5,
        toolbar_bottom: 4,
        button_padding: 5,
        field_fill_margin: 3,
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x0FC2_23A8, // .weak.??_7VerticalTabStripStateControllerImpl@tabs@@6BVerticalTabStripStateController@1@@.default.?R
        should_display: 0x0345_2850, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x002E_75B0, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x0346_0F20, // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x09F9_9710, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x01DB_9180, // ??0VerticalTabStripStateController@tabs@@IEAA@AEAVBrowserWindowInterface@@@Z
        destructor: 0x02AD_3050, // ??_EVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x0FCE_E270, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        region_preferred_size: 0x0BAA_5650, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x0FD8_A000, // .weak.??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$Cd
        invalidate_layout: 0x046F_2F30, // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        region_minimum_size: 0x0BAA_55C0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7, 0x02, 0, 0,
        0, 0,
    ],
        should_draw_tab_strip: 0x0504_F960, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[0x56, 0x57, 0x53],
        window_feature: ABSENT,
        window_feature_prologue: &[0],
        window_feature_tab_strip: 0,
        window_feature_callers: (0, 0),
        set_bounds_rect: ABSENT,
        set_bounds_rect_prologue: &[0],
        toolbar_in_view: 0,
        detour_strip: false,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true,
        content_window_for_hwnd: 0x02F4_6B00, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BAA_79F0, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BAA_7F20, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x04AA_5180, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x054D_1240, // ??0GURL@@QEAA@XZ
        gurl_drop: 0x03BA_A460, // ??1GURL@@QEAA@XZ
        open_url_params: 0x0700_9CC0, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui
        open_url_params_drop: 0x020C_7F60, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x081D_4FC0, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@$$A6AXAE
        signatures: &[
        (
            0x02F4_6B00,
            &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
        ),
        (0x0BAA_79F0, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
        (
            0x04AA_5180,
            &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        ),
        (0x054D_1240, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
        (0x03BA_A460, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
        (0x0700_9CC0, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
        (0x020C_7F60, &[0x41, 0x56, 0x56, 0x57, 0x53]),
        (0x081D_4FC0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
        (
            0x0BAA_7F20,
            &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00],
        ),
    ],
    },
    tabs: Tabs {
        supported: true,
        content_window_for_hwnd: 0x02F4_6B00, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BAA_79F0, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BAA_7F20, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        browser_in_view: 0x560,
        groups: true,
        get_tab_model: 0x04E2_60C0, // ?Client@WebLocalFrameImpl@blink@@UEBAPEAVWebLocalFrameClient@2@XZ
        tab_count: 0x0345_2D40, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x009C_51D0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x09FB_C5F0, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x09FB_F9D0, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x0106_B960, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        new_tab_prologue: &[],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab: 0x09E7_61A0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x0559_38B0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00B6_3BC0, // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02EF_9D70, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x09FA_F550, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x09FA_F5B0, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: 0x09FC_6D90, // ?ChangeTabGroupVisuals@TabStripModel@@QEAAXAEBVTabGroupId@tab_groups@@VTabGroupVisualData@3@_N@Z
        allocate: 0x05A1_E310, // ??2@YAPEAX_K@Z
        add_to_new_group: 0x09FC_48F0, // ?AddToNewGroup@TabStripModel@@QEAA?AVTabGroupId@tab_groups@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        add_to_new_group_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81],
        signatures: &[
                (
                    0x02EF_9D70,
                    &[
                        0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x45, 0x85, 0xc0, 0x78,
                    ],
                ),
                (
                    0x09FA_F550,
                    &[
                        0x56, 0x57, 0x48, 0x83, 0xec, 0x38, 0x49, 0x89, 0xd0, 0x48, 0x8b, 0x05,
                    ],
                ),
                (
                    0x09FA_F5B0,
                    &[
                        0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x89, 0xd6,
                    ],
                ),
                (
                    0x09FC_6D90,
                    &[
                        0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                    ],
                ),
                (
                    0x02F4_6B00,
                    &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8],
                ),
                (0x0BAA_79F0, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
                (0x0BAA_7F20, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
                (0x04E2_60C0, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
                (0x0345_2D40, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
                (0x009C_51D0, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
                (0x09FB_C5F0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
                (0x09FB_F9D0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
                (0x0106_B960, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
                (0x09E7_61A0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
                (0x0559_38B0, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
                (0x00B6_3BC0, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
            ],
    },
    split: Split {
        supported: true,
        add_to_new_split: 0x09FC_3C40, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        remove_split: 0x09FC_5DC0, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x02EF_9DC0, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x05A1_E310, // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x0345_5310, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_prologue: &[
        0xf3, 0x0f, 0x10, 0x02, 0x0f, 0x2e, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
    ],
        set_rounded_corners_changed: 0x0345_5350, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_parity: 0x0345_531D, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        update_border: 0x0207_2060, // ?UpdateBorderAndOverlay@ContentsContainerView@@QEAAX_N00@Z
        update_border_prologue: &[
        0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x48, 0x48, 0x89, 0xce,
    ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        outline_member: 0x448,
        calculate_layout: 0x022F_8700, // ?CalculateProposedLayout@MultiContentsView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        calculate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x0480_FBC0, // ?SchedulePaint@View@views@@QEAAXXZ
        web_contents_focused: 0x0A28_A190, // ??_EMultiContentsViewDropTargetController@@UEAAPEAXI@Z
        web_contents_focused_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00],
        container_layout: 0x0209_3800, // ?CalculateProposedLayout@ContentsContainerView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        container_delegate_offset: 0x320,
        mini_toolbar_member: 0x440,
        flex_layout: 0x0522_E1B0, // views::FlexLayout::CalculateProposedLayout
        flex_layout_host: 0x38,
        location_bar_vtable: 0x0FCF_8EF0, // LocationBarView primary vtable
        view_size: 0x1D0,
        signatures: &[
            (
                0x09FC_3C40,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
            ),
            (0x09FC_5DC0, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58]),
            (
                0x02EF_9DC0,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
            ),
            (0x05A1_E310, &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9]),
        ],
    },
    side_panel: SidePanel {
        supported: true,
        side_panel_vtable: 0x0FCD_9FB0, // .weak.??_7SidePanel@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        browser_offset: 0x560,
        features_offset: 0xd0,
        features_coordinator: 0x330,
        browser_view_view_vtable: 0x0FCE_E270, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        coordinator_vtable: 0x0FCC_B748, // .weak.??_7SidePanelCoordinator@@6BSidePanelUIBase@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        show: 0x0971_4C30, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@std@@_N@Z
        show_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
        entry_id_to_string: 0x01F0_F6F0, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@s
        entry_id_to_string_prologue: &[0x56, 0x48, 0x83, 0xec, 0x30, 0x48, 0x89, 0xce],
        extension_entry: 0x17,
        entry_count: 0x18,
        toggle: 0x0971_D2C0, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60],
        entry_showing: 0x0344_74A0, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02E9_FDA0, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00],
        get_color_provider: 0x0342_7870, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x055D_5D40, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@V?$alloc
        generate_colors_css_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_vtable: 0x0FC2_2820, // .weak.??_7BrowserViewTabbedLayoutImpl@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        proposed_slot: 16,
        proposed: 0x012B_2170, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLayoutImpl@@A
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x003E_BA00, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48],
        event_handler_for_point: 0x00BA_AC10, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x0139_5BD0, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x042A_1D90, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x0347_1E40, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x0FD3_D2D0, // .weak.??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        corner_paint: 0x0A28_63E0, // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        caption_vtable: 0x0FD7_48D0, // .weak.??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        caption_size_slot: 74,
        caption_size: 0x02F1_FA20, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x02F2_0040, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        top_inset_slot: 127,
        top_inset: 0x0184_E3A0, // ?GetTopInset@BrowserFrameViewWin@@UEBAH_N@Z
        layout_slot: 23,
        layout: 0x0184_DD70, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        set_bounds_rect: 0x04A5_2AF0, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x0FCE_E270, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        frame_vtable: 0x0FCE_6130, // .weak.??_7BrowserFrameViewWin@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        layout_immediately: 0x003E_BA00, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48],
        set_layer: 0x00F2_86B0, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28],
        set_radius: 0x01C0_25E0, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x30],
        set_masks: 0x02F5_4C40, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8b, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0AA9_B0A0, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_opaque: 0x01D7_6D40, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xec, 0x20],
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        draw_color_prologue: &[0x48, 0x83, 0xec, 0x38, 0x48, 0x8b],
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B],
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        layout_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53],
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        frame_browser_window_offset: 0x340,
        browser_window_in_view: 0x1f0,
        caption_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_ink_ids: &[0x66C, 0x66D, 0x66E],
        top_inset_adjust: 0,
    },
    edge_vertical: None,
    edge_page: None,
    hidden_views: None,
    whale_row: None,
};

pub static EDGE: Build = Build {
    location_bar: None,
    browser: "Microsoft Edge",
    module: "msedge.dll",
    version: [156, 0, 4314, 50],
    toolbar: Toolbar {
        layout_constant: 0x00FF34C6, // ?GetLayoutConstant@@YAHW4LayoutConstant@@@Z
        layout_constant_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xCB, 0x48, 0x8B, 0x05, 0x6A, 0x0B, 0x20, 0x13],
        theme_changed: 0x030E36C6, // ?ThemeChanged@Widget@views@@QEAAXXZ
        theme_changed_prologue: &[0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x00, 0x00, 0x00, 0x48, 0x89, 0xCE],
        paint_layer: 0x0267EFF6, // ?PaintFromPaintRoot@View@views@@AEAAXAEBVPaintContext@ui@@@Z
        paint_layer_prologue: &[0x56, 0x57, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00, 0x48, 0x89, 0xCE],
        set_visible: 0x01056220, // ?SetVisible@View@views@@UEAAX_N@Z
        set_visible_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE],
        from_vector_icon: 0x0100567A, // ?FromVectorIcon@ImageModel@ui@@SA?AV12@AEBUVectorIcon@gfx@@VColorVariant@2@HPEBU34@@Z
        from_vector_icon_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x4C, 0x89, 0xC7],
        update_location: 0x02822D7A, // ?Update@LocationIconView@@QEAAX_N0@Z
        update_location_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00],
        update_location_icon: 0x02823520, // ?UpdateIcon@LocationIconView@@QEAAXXZ
        update_location_icon_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00],
        layout_insets: 0x00FF3C3D, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00],
        toolbar_button_inset: 12, // LayoutInset::TOOLBAR_BUTTON, read by ToolbarButton::GetTargetInsets
        toolbar_inset: 23, // LayoutInset::TOOLBAR, read by ToolbarView::LayoutCommon
        location_icon_inset: 55, // LayoutInset read by LocationIconView::UpdateBorder
        update_icon: 0x033C8D50, // ?UpdateIcon@ToolbarButton@@UEAAXXZ
        update_icon_prologue: &[0x80, 0xB9, 0x98, 0x07, 0x00, 0x00, 0x01, 0x75, 0x0C, 0x48, 0x8B, 0x91, 0x88, 0x07, 0x00, 0x00, 0xE9],
        formatted_url: 0x027A8DC8, // ?GetFormattedURL@LocationBarModelImpl@@AEBA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@
        formatted_url_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x10, 0x01, 0x00, 0x00],
        model_url: 0x010F0BB0, // ?GetURL@LocationBarModelImpl@@UEBA?AVGURL@@XZ
        model_url_prologue: &[0x56, 0x57, 0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, 0x48, 0x89, 0xD6],
        omnibox_update: 0x02821910, // ?Update@OmniboxViewViews@@UEAAXXZ
        omnibox_update_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x21, 0x27, 0x9D, 0x11],
        omnibox_revert_all: 0x02827E80, // ?RevertAll@OmniboxViewViews@@UEAAXXZ
        omnibox_revert_all_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xB1, 0xC1, 0x9C, 0x11],
        omnibox_tab_changed: 0x02827D9C, // ?OnTabChanged@OmniboxViewViews@@QEAAXPEAVWebContents@content@@@Z
        omnibox_tab_changed_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE],
        omnibox_textfield: 0x10, // where OmniboxViewViews' constructor stores the views::View vtable
        update_text_style: 0x02824F00, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$all
        update_text_style_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xF0, 0x01, 0x00, 0x00],
        apply_color: 0x028D2F82, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0x00, 0x00, 0x00],
        title_weight: 700,
        apply_weight: 0x0B74E45C, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0x00, 0x01, 0x00, 0x00],
        title_was_set: 0x033CB530, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0x28, 0x01, 0x00, 0x00],
        observer_offset: 0x7A0, // where OmniboxViewViews' constructor stores the web contents observer's vtable
        operator_new: 0x036CDF90, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x0023EE60, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6],
        model_delegate: 0x40, // read by LocationBarModelImpl::GetURL
        browser_delegate_vtable: 0x11742B58, // ??_7BrowserLocationBarModelDelegate@@6B@
        active_web_contents: 0x00FE1AC0, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 22,
        web_contents_vtable: 0x116B5690, // ??_7WebContentsImpl@content@@6B@
        web_contents_title: 0x01119470, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?
        web_contents_title_slot: 56,
        location_bar_view_base: 0x100, // RefreshBackground's views::View base
        hover_value: 0x5F8, // Chrome value, not checked on Edge
        field_background: 0x328, // Chrome value, not checked on Edge
        background_vtable: 0x114D6E80, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: ABSENT, // not found in Edge
        painter_radii: 0x10, // Chrome value, not checked on Edge
        painter_insets: 0x20, // Chrome value, not checked on Edge
        field_state: 0x540, // read by LocationBarView::RefreshBackground
        field_state_focused: 0x18, // read by LocationBarView::RefreshBackground
        focused_fill_id: -1, // none: Edge reports its field as focused at rest, so 3009 stays paper and only the hover id shows the tint
        focus_ring_remove: 0x027876CC, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0B38D582, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6],
        set_brightness: 0x02DA797C, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30, 0x0F, 0x28, 0xF1],
        view_layer: 0x20, // views::View::layer_, the same in the views that Edge shares with Chrome
        view_size: 0x1D0, // views::View::bounds_.size
        refresh_background: 0x02824122, // ?RefreshBackground@LocationBarView@@AEAAXXZ
        refresh_background_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00],
        chip_text: 0x0282389E, // ?GetSecurityChipText@location_bar@@YA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@23@@std@@@Z
        chip_text_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xD0, 0x00, 0x00, 0x00],
        textfield_theme: 0x02D30920, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0x37, 0x4C, 0x11],
        textfield_set_font: 0x0278A88A, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0xC0, 0x03, 0x00, 0x00],
        font_size: 0x0101C850, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[0x48, 0x8B, 0x09, 0xE9, 0x00, 0x00, 0x00, 0x00, 0x56, 0x48, 0x83, 0xEC, 0x20, 0x8B, 0x41, 0x44],
        derive_size: 0x0101C9EA, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x44, 0x89, 0xC7, 0x48, 0x89, 0xD6],
        textfield_render_text: 0x3C0, // read by Textfield::SetFontList
        render_text_font_list: 0x48, // RenderText::font_list_, read by RenderText::SetFontList
        gurl_destructor: 0x00FDECAA, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70],
        gurl_scheme_begin: 0x20, // read by GURL::SchemeIsHTTPOrHTTPS
        gurl_scheme_length: 0x24,
        anchor_rect: 0x032D7810, // ?GetAnchorRect@BubbleDialogDelegate@views@@UEBA?AVRect@gfx@@XZ
        anchor_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xC8, 0x00, 0x00, 0x00],
        anchor_view_access: &[0x48, 0x8B, 0x81, 0x78, 0x03, 0x00, 0x00, 0x48, 0x85, 0xC0],
        is_drawn: 0x0112D610, // ?IsDrawn@View@views@@UEBA_NXZ
        is_drawn_bytes: &[0x80, 0xB9, 0xD8, 0x01, 0x00, 0x00, 0x01, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 0x01, 0x00, 0x00],
        bounds_in_screen: 0x028C8E20, // ?GetAnchorBoundsInScreen@View@views@@UEBA?AVRect@gfx@@XZ
        bounds_bytes: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF],
        parent_offset: 0x198, // Chrome value, not checked on Edge
        is_drawn_slot: 20,
        browser_widget_vtable: 0x11445140, // BrowserWidget NativeWidgetDelegate vtable
        plain_widget_vtable: 0x114CC290, // views::Widget NativeWidgetDelegate vtable
        paint_slot: 31,
        paint: 0x033D8DC0, // ?OnNativeWidgetPaint@Widget@views@@UEAAXAEBVPaintContext@ui@@@Z
        layout_slot: 45,
        layout: 0x032C2200, // ?LayoutRootViewIfNecessary@Widget@views@@UEAAXXZ
        color_provider: 0x01003060, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        root_view_offset: 0x1A0, // views::Widget::root_view_, read by GetRootView
        theme_slot: 111,
        hidden: &[
                (0x11892678, 0x02C7C5E0), // BrowserAppMenuButton
                (0x1181E578, 0x033F7530), // EdgeVerticalTabContainerView
                (0x11834A18, 0x00FDE2C0), // SidePanelHeader
                (0x11890838, 0x02B70B40), // PinnedActionToolbarButton
                (0x1191FEF8, 0x029E1650), // EdgeAvatarToolbarButton
                (0x1191FB58, 0x00FDE2C0), // EdgeCopilotToolbarSplitButtonContainer
                (0x11811CB8, 0x02E8BD60), // ToolbarHubIconContainerView
                (0x11894A78, 0x02B70B40), // HomeButton
                (0x118CB818, 0x00FDE2C0), // page_actions::PageActionContainerView
                (0x11918C58, 0x00FDE2C0), // EdgePageActionIconContainerView: translate, reader mode, favorite
                (0x11936E78, 0x02B70B40), // HubToggleToolbarButton
                (0x11873498, 0x02A1F9F0), // ContentSettingImageView: cookies, pop-ups and the like
                (0x1191F5B8, 0x02E3FF50), // EdgeCopilotToolbarLabelButton, the chat button inside the address field
                (0x119498F8, 0x02B70B40), // EdgeCopilotDropdownMenuButton
                (0x11817938, 0x033CA380), // BookmarkBarView, which the design has no row for
                (0x1181FC38, 0x02D1CAF0), // EdgeTabStripRegionView, which a window restored with several tabs would show
                (0x118118B8, 0x02E8BD80), // EdgeExtensionsToolbarContainer
                (0x11810CB8, 0x02E8BD80), // PinnedToolbarActionsContainer
                (0x1187DE38, 0x02B70B40), // split_screen::edge::ToolbarSplitWindowButton
                (0x1184E298, 0x02B70B40), // EdgeRemixToolbarButton
                (0x118BBF58, 0x02B70B40), // MediaToolbarButtonView
                (0x1191EAD8, 0x02B70B40), // ToggleToolbarButton
                (0x11931A18, 0x033F8950), // edge::workspaces::EdgeWorkspacesViewsContainer
            ],
        get_widget_slot: 10,
        size_slots: (86, 15),
        paint_children_slot: 100,
        overrides: &[
        (100, 8),  // kLocationBarChildCornerRadius
        (104, 28), // kLocationBarHeight
        (147, 28), // kToolbarButtonHeight
        (153, 2),  // kToolbarElementPadding
        (146, 0),  // kTabstripToolbarOverlap
    ], // Edge's ids sit 88 to 101 above Chrome's: matched by the constants the same functions ask for in both builds. Only ids that a Rivet window asks for are listed; kMainBackgroundRegionCornerRadius, kToolbarCornerRadius and kVerticalTabMinWidth were never asked for, so their Edge ids are not recorded
        dev: DEV_EDGE_156,
        colours: &[
        (316, PAPER),      // kColorFrameActive
        (1936, 0),         // kColorCaptionButtonBackground: clear, so the row shows through like Chrome's
        (2082, PAPER),        // kColorLocationBarBackground
        (2083, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
        (2298, 0),           // Page info chip background: clear, so the field is one surface
        (159, FOCUS_RING),   // kColorSysStateFocusRing
        (314, FOCUS_RING),   // kColorFocusableBorderFocused
        (465, SELECTION),    // kColorTextfieldSelectionBackground
        (466, INK),          // kColorTextfieldSelectionForeground
        (2299, 0),           // kColorOmniboxIconBackgroundTonal
        (2300, SOFT_INK),    // kColorOmniboxIconForeground: the icon of the field while editing
        (2301, SOFT_INK),    // kColorOmniboxIconForegroundTonal
        (2302, INK),         // kColorOmniboxIconHover
        (2308, PAPER),       // kColorOmniboxResultsBackground, the suggestion list and the field with it
        (2309, 0xFFF0_EFEC), // kColorOmniboxResultsBackgroundHovered
        (2310, 0x0F1F_2024), // kColorOmniboxResultsBackgroundHoverOverlay
        (2311, ACCENT_TINT), // kColorOmniboxResultsBackgroundSelected: the accent at 9 percent over paper
        (2314, SOFT_INK),    // kColorOmniboxResultsButtonIcon
        (2315, INK),         // kColorOmniboxResultsButtonIconSelected
        (2323, 0),           // kColorOmniboxResultsFocusIndicator: the design marks a row by its tint alone
        (2324, SOFT_INK),    // kColorOmniboxResultsIcon
        (2325, 0x0F1F_2024), // kColorOmniboxResultsIconGM3Background
        (2326, INK),         // kColorOmniboxResultsIconSelected
        (2327, INK),         // kColorOmniboxResultsIconHovered
        (2330, 0xFF8A_8690), // kColorOmniboxResultsTextDimmed
        (2331, 0xFF4A_4850), // kColorOmniboxResultsTextDimmedSelected
        (2336, 0xFF8A_8690), // kColorOmniboxResultsTextSecondary
        (2337, 0xFF4A_4850), // kColorOmniboxResultsTextSecondarySelected
        (2338, INK),         // kColorOmniboxResultsTextSelected
        (2339, 0xFF4A_4850), // kColorOmniboxResultsUrl
        (2340, INK),         // kColorOmniboxResultsUrlSelected
        (2343, INK),         // kColorOmniboxSecurityChipDefault: the padlock at rest
        (2346, INK),         // the padlock of a secure page
        (2347, SOFT_INK),    // kColorOmniboxSecurityChipText: the icon of the field while editing
        (2348, SELECTION),   // kColorOmniboxSelectionBackground: the accent at 25 percent over paper
        (2349, INK),         // kColorOmniboxSelectionForeground
        (2341, 0xFFB3_261E), // kColorOmniboxSecurityChipDangerous: the "Not secure" label and its warning
        (2342, 0x14B3_261E), // kColorOmniboxSecurityChipDangerousBackground: the label's tint, 8 percent
        (2350, INK),         // kColorOmniboxText: the address while it is edited
        (2351, 0xFF8A_8690), // kColorOmniboxTextDimmed
        (2893, PAPER),       // kColorToolbar
        (2947, 0xFF1A_1A1A), // Caption button glyph, active window
        (2948, 0xFF1A_1A1A), // Caption button glyph, inactive window
        (2949, 0xFF1A_1A1A), // Caption button glyph without a theme colour
        (2950, 0xFFC4_2B1C), // Close button hover fill
        (2951, 0xFF1A_1A1A), // Caption button glyph that PaintSymbol reads
        (3409, 0xFF1A_1A1A), // Caption button glyph, found by replacing ids until the glyph changed
        (3410, 0xFF1A_1A1A), // the same for an inactive window
        (2953, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
        (2898, 0xFF4A_4850), // kColorToolbarButtonIcon
        (2901, 0xFF1F_2024), // kColorToolbarButtonIconHovered
        (2902, 0xFFBD_BCBC), // kColorToolbarButtonIconInactive: the icon colour at 35 percent over paper
        (2906, HAIRLINE),    // kColorToolbarContentAreaSeparator
        (2914, 0x0F1F_2024), // kColorToolbarInkDropHover
        (2915, 0x1F1F_2024), // kColorToolbarInkDropRipple
        (3009, PAPER),       // the field's fill while it has focus, one of the two ids RefreshBackground picks
        (3011, 0),           // the field's outline, which the design does not draw, in each of its states
        (3013, 0),
        (3014, 0),
    ], // Chrome's ids moved to Edge's: +1286 for the field's background, +1295 for the omnibox block, +1303 for the toolbar block, found by matching the functions that ask for them
        get_color: 0x0100309E,
        get_color_bytes: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48],
        hwnd_for_widget: 0x0B379B4D, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28], // Chrome value, not checked on Edge
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9], // Chrome value, not checked on Edge
        source_in_widget: 0x48, // Chrome value, not checked on Edge
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x028C8D78, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x12C694F8, // ?kEdgeSmtcBackArrowIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x12C69F48, // ?kEdgeSmtcForwardArrowIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x12C6AB48, // ?kEdgeSmtcReloadIcon@vector_icons@@3UVectorIcon@gfx@@B
        nav_icon_names: &[b"kEdgeSmtcBackArrowIcon", b"kEdgeSmtcForwardArrowIcon", b"kEdgeSmtcReloadIcon"],
        page_info_icon: 0x12ED5778, // ?kEdgeVisualRefreshHttpsValidIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: ABSENT, // not found in Edge
        google_g_monochrome_icon: ABSENT, // not found in Edge
        edit_icons: &[], // the search and globe icons of an edited address, not recorded for Edge yet
        http_icon: 0x12ED5728, // ?kEdgeVisualRefreshHttpIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kEdgeVisualRefreshHttpIcon",
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0, 0, 0x10, 0x41, 0, 0, 0x10, 0x41, 0x0A, 0, 0, 0, 0, 0, 0x80, 0x3F,
    ], // the same path data as Chrome's, checked against msedge.dll
        page_info_reps: 0x12ED5740, // ?kEdgeVisualRefreshHttpsValidRepList@omnibox@@3QBUVectorIconRep@gfx@@B
        page_info_id: 0x12ED5750, // ?kEdgeVisualRefreshHttpsValidIconId@omnibox@@3QBDB
        page_info_path: 0x12EE1130, // ?kEdgeVisualRefreshHttpsValidPath@omnibox@@3QBUPathElement@gfx@@B
        page_info_size: 143,
        http_size: 87,
        extra_lock_icons: &[0x12ED57D0], // kEdgeVisualRefreshHttpsValidPageInfoIcon
        extra_warning_icons: &[0x12ED5820, 0x12ED56E0], // kEdgeVisualRefreshNotSecureWarningIcon, kEdgeVisualRefreshDangerousIcon
        anchor_view_access_at: 0x28,
        skip: "updateicon anchor visible", // the address field, icons and bubble anchors are not recorded for Edge yet
        anchor_tracker_offset: 0x378, // BubbleDialogDelegate anchor view tracker, read by GetAnchorRect
        toolbar_layout: ABSENT,
        toolbar_layout_prologue: &[0],
        infobar_vtable: ABSENT,
        infobar_layout: ABSENT,
        infobar_layout_slot: 0,
        leading_space: 32,
        toolbar_top: 5,
        toolbar_bottom: 4,
        button_padding: 4,
        field_fill_margin: 3,
    },
    vertical: VerticalTabs {
        supported: false, // not checked on Edge yet
        slot: 0x1172CF10, // VerticalTabStripStateControllerImpl vtable, ShouldDisplayVerticalTabs slot
        should_display: 0x0A3D40D0, // ?loaded@TopSitesImpl@history@@UEBA_NXZ
        browser_view_for_browser: 0x010CDA9A, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x01078A59, // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0A3D3176, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x0, // none in Edge
        destructor: 0x0A3D4830, // ??_GVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8],
        browser_offset: 0x1B8, // Chrome value, not checked on Edge
        mode_offset: 0x1E8, // Chrome value, not checked on Edge
        width_offset: 0x1CC, // Chrome value, not checked on Edge
        browser_view_view_vtable: 0x11937B18, // BrowserView View vtable
        region_preferred_size: 0x0CAE1ED0, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@
        region_preferred_size_slot: 0x1181EC28, // VerticalTabStripRegionView vtable, CalculatePreferredSize slot 86
        invalidate_layout: 0x00FFA880, // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38],
        region_minimum_size: 0x0CAE1E50, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
        0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7, 0x02, 0, 0,
        0, 0,
    ], // Chrome value, not checked on Edge
        should_draw_tab_strip: 0x0100A550, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0x18, 0x06, 0x00, 0x00],
        window_feature: ABSENT,
        window_feature_prologue: &[0],
        window_feature_tab_strip: 0,
        window_feature_callers: (0, 0),
        set_bounds_rect: ABSENT,
        set_bounds_rect_prologue: &[0],
        toolbar_in_view: 0,
        detour_strip: false,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true, // opening an address in a tab checked on Edge 156.0.4314.50
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x010CDAE8, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CAE5550, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x01016852, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x010FF97C, // ??0GURL@@QEAA@XZ
        gurl_drop: 0x00FDECAA, // ??1GURL@@QEAA@XZ
        open_url_params: 0x066E261C, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTra
        open_url_params_drop: 0x066E2C84, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x07A598E0, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallbac
        signatures: &[
            (0x026A4DC6, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x010CDAE8, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x01016852, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
            (0x010FF97C, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x00FDECAA, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x066E261C, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x066E2C84, &[0x56, 0x57, 0x48, 0x83, 0xEC]),
            (0x07A598E0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
            (0x0CAE5550, &[0x48, 0x8B, 0x81, 0x18, 0x06, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true, // tab list, switching, closing and new tabs checked on Edge 156.0.4314.50; groups are not
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x010CDAE8, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CAE5550, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        browser_in_view: 0x618, // BrowserView::EdgeIsVerticalTabStripVisible reads the browser here
        groups: false, // the group model's layout is not checked on Edge
        get_tab_model: 0x055E0CE0, // ?PdfSize@DocumentMetadata@datamodel@edge_pdf@@QEBA_KXZ
        tab_count: 0x010CBBCE, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0105E38E, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x0A404C92, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A4080EA, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x01119470, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?
        new_tab_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x18, 0x01, 0x00, 0x00, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x71, 0xD7, 0xF3, 0x09],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab: 0x0A2B68B0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x0115B9E0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x01056C50, // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02698560, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x0A3F1DFE, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x0A3F1E94, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: ABSENT, // not found in Edge
        allocate: 0x036CDF90, // ??2@YAPEAX_K@Z
        add_to_new_group: ABSENT, // not found in Edge
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9],
        add_to_new_group_prologue: &[0x00], // function not found in Edge
        signatures: &[
            (0x02698560, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x0F]),
            (0x0A3F1DFE, &[0x48, 0x8B, 0x41, 0x08, 0x48, 0x85, 0xC0, 0x74, 0x3F, 0x48, 0x83, 0xC1]),
            (0x0A3F1E94, &[0x48, 0x8B, 0x41, 0x08, 0x48, 0x85, 0xC0, 0x0F, 0x84, 0x80, 0x00, 0x00]),
            (0x026A4DC6, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x010CDAE8, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0CAE5550, &[0x48, 0x8B, 0x81, 0x18, 0x06, 0x00, 0x00]),
            (0x055E0CE0, &[0x48, 0x8B, 0x41, 0x30, 0xC3]),
            (0x010CBBCE, &[0x48, 0x8B, 0x41, 0x10, 0x8B, 0x40, 0x20]),
            (0x0105E38E, &[0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2]),
            (0x0A404C92, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38]),
            (0x0A4080EA, &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC]),
            (0x01119470, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48]),
            // `chrome::NewTab` is checked by its own prologue when the hook is installed, which
            // changes its first bytes, so it is not listed here.
            (0x0115B9E0, &[0x48, 0x63, 0x81, 0xD4, 0x02, 0x00, 0x00]),
            (0x01056C50, &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x81, 0xC1]),
        ],
    },
    split: Split {
        supported: false, // not checked on Edge yet
        add_to_new_split: 0x0A40C036, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr
        remove_split: 0x0A40E340, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x02BFCE34, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x036CDF90, // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x02E268A2, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0xF3, 0x0F, 0x10, 0x02],
        set_rounded_corners_changed: ABSENT, // not found in Edge
        set_rounded_corners_parity: ABSENT, // not found in Edge
        update_border: 0x02E26912, // ?UpdateBorderAndOverlay@ContentsContainerView@@QEAAX_N00@Z
        update_border_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xD5],
        set_border_slot: 0xf0 / 8, // Chrome value, not checked on Edge
        set_visible_slot: 0x90 / 8, // Chrome value, not checked on Edge
        outline_member: 0x448, // Chrome value, not checked on Edge
        calculate_layout: 0x02C01AD0, // ?CalculateProposedLayout@MultiContentsView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@
        calculate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_delegate_offset: 0x328, // Chrome value, not checked on Edge
        split_insets_member: 0x404, // Chrome value, not checked on Edge
        containers_member: 0x350, // Chrome value, not checked on Edge
        container_count_member: 0x358, // Chrome value, not checked on Edge
        active_member: 0x3c8, // Chrome value, not checked on Edge
        schedule_paint: 0x010038F0, // ?SchedulePaint@View@views@@QEAAXXZ
        web_contents_focused: ABSENT, // not found in Edge
        web_contents_focused_prologue: &[0x00], // function not found in Edge
        container_layout: ABSENT, // not found in Edge
        container_delegate_offset: 0x320, // Chrome value, not checked on Edge
        mini_toolbar_member: 0x440, // Chrome value, not checked on Edge
        flex_layout: ABSENT, // not found in Edge
        flex_layout_host: 0x38, // Chrome value, not checked on Edge
        location_bar_vtable: ABSENT, // not found in Edge
        view_size: 0x1D0, // Chrome value, not checked on Edge
        signatures: &[
            (0x0A40C036, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53]),
            (0x0A40E340, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x48]),
            (0x02BFCE34, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x45, 0x85, 0xC0, 0x0F]),
            (0x036CDF90, &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9]),
        ],
    },
    side_panel: SidePanel {
        supported: false, // not checked on Edge yet
        side_panel_vtable: ABSENT, // not found in Edge
        browser_offset: 0x560, // Chrome value, not checked on Edge
        features_offset: 0xd0, // Chrome value, not checked on Edge
        features_coordinator: 0x330, // Chrome value, not checked on Edge
        browser_view_view_vtable: ABSENT, // not found in Edge
        coordinator_vtable: ABSENT, // not found in Edge
        show: 0x09402420, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@
        show_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x44, 0x89, 0xCB],
        entry_id_to_string: 0x02971720, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@
        entry_id_to_string_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x8B, 0x05, 0x14, 0x29, 0x88, 0x11],
        extension_entry: 0x17, // Chrome value, not checked on Edge
        entry_count: 0x18, // Chrome value, not checked on Edge
        toggle: 0x09351EF0, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x60, 0x44, 0x89, 0xC3],
        entry_showing: 0x094027E0, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8B, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02ABEF42, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xC1, 0xB0, 0x00, 0x00, 0x00],
        get_color_provider: 0x0115D4C0, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x83, 0xE9, 0x80, 0xE8, 0x53, 0xCA, 0xE8, 0xFF, 0x48],
        source_in_widget: 0x48, // Chrome value, not checked on Edge
        color_source: 0x98, // Chrome value, not checked on Edge
        generate_colors_css: 0x08966BAC, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std
        generate_colors_css_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        layout_vtable: ABSENT, // not found in Edge
        proposed_slot: 16, // Chrome value, not checked on Edge
        proposed: 0x0A65CF30, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLay
        host_view: 0x38, // Chrome value, not checked on Edge
        page_area: 0x50, // Chrome value, not checked on Edge
        contents_container: 0xb8, // Chrome value, not checked on Edge
        vertical_tabs: 0x80, // Chrome value, not checked on Edge
        toolbar: 0x60, // Chrome value, not checked on Edge
        contents_outline: 0x58, // Chrome value, not checked on Edge
        side_panel: 0xc0, // Chrome value, not checked on Edge
        side_panel_animation: 0xc8, // Chrome value, not checked on Edge
        layout_immediately: 0x01044270, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xBE, 0xFD, 0x1A, 0x13],
        event_handler_for_point: 0x0109FB78, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 101,
        provider_verified: true,
        on_paint: 0x02B44110, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x01003060, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x02A3F86A, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144, 3406], // 3406 is what BrowserFrameViewWin::PaintTitlebar fills the caption row with
        corner_vtable: 0x118B7ED8, // CustomFloatingCorner vtable
        corner_paint: 0x0A66B240, // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        caption_vtable: 0x118F2478, // WindowsCaptionButton vtable
        caption_size_slot: 86,
        caption_size: 0x0A7EDBF0, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 127,
        caption_paint: 0x02D1ECF0, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        top_inset_slot: 164,
        top_inset: 0x033CA100, // ?GetTopInset@BrowserFrameViewWin@@UEBAH_N@Z
        layout_slot: 24,
        layout: 0x02930FA0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        set_bounds_rect: 0x01021E78, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        caption_container_offset: 0x440, // EdgeBrowserFrameViewWin::caption_button_container_, read by CaptionButtonsRegionWidth
        view_bounds_offset: 0x1C8, // views::View::bounds_, unchanged
        browser_view_view_vtable: 0x11937B18, // BrowserView View vtable
        frame_vtable: 0x11857718, // EdgeBrowserFrameViewWin primary vtable (the View vtable)
        layout_immediately: 0x01044270, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48],
        set_layer: 0x02782F98, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28],
        set_radius: 0x02DA71BE, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30],
        set_masks: 0x0306E54C, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8B, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0B415B2E, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48],
        set_opaque: 0x0103A07E, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20],
        on_paint_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89],
        draw_color_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38],
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B],
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        layout_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48],
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        frame_browser_window_offset: 0x3B0, // frame view member that holds the BrowserWindow subobject, read by GetBrowserView
        browser_window_in_view: 0x1F8, // BrowserWindow subobject distance from the BrowserView, read by GetBrowserView
        caption_paint_prologue: &[0xE9, 0x01, 0x00, 0x00, 0x00, 0xCC, 0x41, 0x57],
        caption_ink_ids: &[0x66C, 0x66D, 0x66E], // Chrome value, not checked on Edge
        top_inset_adjust: 0,
    },
    edge_page: Some(EdgePage {
        paint_border: 0x033F7B70, // ?OnPaintBackground@EdgeContentsContainerBorder@@
        paint_border_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0x00, 0x00, 0x00],
        container_margin: 0x02A1AA04, // ?GetContentsContainerMargin@AppLayerStyle@edge_app_layer@@
        container_margin_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38],
        update_corners: 0x0273862E, // ?EdgeUpdateRoundedCornerVisibility@ContentsWebView@@
        update_corners_prologue: &[0x48, 0x8B, 0x89, 0x88, 0x05, 0x00, 0x00, 0x48, 0x85, 0xC9, 0x0F, 0x85, 0x02, 0x00, 0x00, 0x00],
        corner_offset: 0x588,
        corner_set_visible: 0x02738640, // ?SetVisible@EdgeRoundedCornerView@@
    }),
    hidden_views: None,
    whale_row: None,
    edge_vertical: Some(EdgeVertical {
        content_window_for_hwnd: 0x026A4DC6,
        content_window_for_hwnd_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8],
        browser_view_for_window: 0x010CDAE8,
        browser_view_for_window_prologue: &[0x48, 0x83, 0xEC, 0x28, 0xE8],
        set_state: 0x02AFA950,
        set_state_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x8B, 0x05, 0xDD],
        vertical_state: 2,
        hide_titlebar: 0x028B5C7A,
        hide_titlebar_prologue: &[0x48, 0x83, 0xEC, 0x38, 0x48, 0x8B, 0x05, 0xBB, 0xE3, 0x93, 0x11, 0x48, 0x31, 0xE0, 0x48, 0x89, 0x44, 0x24, 0x30],
        cookie_at: 4,
        container_preferred_size: 0x03433540,
        container_preferred_size_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x8B, 0x81, 0x10, 0x06, 0x00, 0x00],
        container_minimum_size: 0x028C98C0,
        container_minimum_size_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xD6, 0x48, 0x8B, 0x05, 0x6E, 0xA7, 0x92, 0x11],
        container_minimum_cookie_at: 11,
        set_bounds_rect: 0x01021E78,
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        toolbar_vtable: 0x1180FCB8,
        button_vtables: &[0x11892138, 0x11895898], // BackForwardButton, ReloadButton
        toolbar_height: 43,
        toolbar_overlap: 8,
        button_size: 28,
        button_x: 38,
        button_y: 8,
        button_pitch: 30,
        field_vtable: 0x11874078, // LocationBarView
        field_x: 135,
        field_y: 5,
        field_height: 34,
        field_margin: 9,
        icon_vtable: 0x11874418, // LocationIconView
        icon_bounds: [5, 5, 26, 24],
        text_vtable: 0x118C4218, // OmniboxViewViews
        text_x: 35, // Edge insets its text 4 DIPs more than Chrome does
        text_y: 5,
        text_height: 24,
        text_margin: 10,
        size_slot: 86,
        caption_gap: 10,
        separator_vtable: 0x118B7B38, // ContentsSeparator
        top_container_vtable: 0x118B9BD8, // TopContainerView
        separator_height: 1,
        toolbar_leading: 0,
        top_inset_adjust: -18,
        top_inset_floor: 6,
        container_offset: 0x958,
        state_offset: 0x610,
    }),
};

/// Google Chrome 150.0.7871.187. Only the frame, caption and panel hooks are checked against this
/// build's symbols; every other feature stays off until its addresses are verified.
pub static CHROME_150: Build = Build {
    location_bar: None,
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [150, 0, 7871, 187],
    toolbar: Toolbar {
        skip: "toolbar",
        ..UNCHECKED_TOOLBAR
    },
    vertical: UNCHECKED_VERTICALTABS,
    navigation: UNCHECKED_NAVIGATION,
    tabs: UNCHECKED_TABS,
    split: UNCHECKED_SPLIT,
    edge_vertical: None,
    edge_page: None,
    hidden_views: None,
    whale_row: None,
    side_panel: UNCHECKED_SIDEPANEL,
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x13a4880,
        get_color_provider: 0x421ccb0,
        draw_color: 0x3192520,
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0xef34798,
        corner_paint: 0xe451e40,
        caption_vtable: 0xef54f58,
        caption_size_slot: 74,
        caption_size: 0x322fdb0,
        caption_paint_slot: 114,
        caption_paint: 0x1d823e0,
        top_inset_slot: 127,
        top_inset: 0xe5435a0,
        layout_slot: 23,
        layout: 0x17e4cd0,
        set_bounds_rect: 0x44cff90,
        caption_container_offset: 0x3c0,
        view_bounds_offset: 0x1c0,
        browser_view_view_vtable: 0xeeaecd8,
        frame_vtable: 0xef32f18,
        layout_immediately: 0x44d1380,
        layout_immediately_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        set_layer: 0xd28650,
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28],
        set_radius: 0x2c89450,
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30],
        set_masks: 0x2c893c0,
        set_masks_prologue: &[0x48, 0x8B, 0x89, 0xC8, 0x01, 0x00, 0x00],
        stack_at_top: 0x9baf850,
        stack_at_top_prologue: &[0x4C, 0x8B, 0x41, 0x30, 0x48, 0x8B],
        set_opaque: 0x126da80,
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20],
        on_paint_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89],
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38, 0x48, 0x8B],
        get_color_provider_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x68],
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        frame_browser_window_offset: 0x338,
        browser_window_in_view: 0x1f0,
        caption_paint_prologue: &[0xE9, 0x0B, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC],
        caption_ink_ids: &[1593, 1594, 1595],
        top_inset_adjust: 0,
    },
};

/// Naver Whale 5.39.412.57 (Chromium 150.0.7871.252). Whale builds its frame view as a subclass whose
/// virtual functions override Chromium's, so the table names the overrides that sit in its vtable.
/// Only the client-view bounds, frame paint and top inset are mapped; everything else stays off.
pub static WHALE: Build = Build {
    location_bar: None,
    browser: "Naver Whale",
    module: "whale.dll",
    version: [5, 39, 412, 57],
    toolbar: Toolbar {
        skip: "toolbar",
        ..UNCHECKED_TOOLBAR
    },
    vertical: VerticalTabs {
        // Whale's `BrowserView::ShouldDrawTabStrip`, the target of slot 1 of its layout delegate. The
        // prologue ends on an instruction boundary and holds no relative operand.
        should_draw_tab_strip: 0x0E6CBE60,
        should_draw_tab_strip_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0xC0, 0x05, 0x00, 0x00,
        ],
        detour_strip: true,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true, // opening an address in a tab checked on Whale 5.39.412.57
        content_window_for_hwnd: 0x08F36620,
        browser_view_for_window: 0x0E6CB8A0,
        active_web_contents: 0x0E6CC090,
        gurl_from_text: 0x03677F60,
        gurl_wide: false,
        gurl_empty: 0x037537F0,
        gurl_drop: 0x02C212F0,
        open_url_params: 0x05357FB0,
        open_url_params_drop: 0x053585F0,
        open_url: 0x06775BE0,
        signatures: &[
            (0x08F36620, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0E6CB8A0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0E6CC090, &[0x48, 0x8B, 0x81, 0xC0, 0x05, 0x00, 0x00]),
            (0x03677F60, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53]),
            (0x037537F0, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0, 0x0F, 0x11, 0x41, 0x0B, 0x0F, 0x11, 0x01]),
            (0x02C212F0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70]),
            (0x05357FB0, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xCF]),
            (0x053585F0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE]),
            (0x06775BE0, &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x04, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true, // list, switch and close checked on Whale 5.39.412.57; groups are not
        content_window_for_hwnd: 0x08F36620,
        browser_view_for_window: 0x0E6CB8A0,
        active_web_contents: 0x0E6CC090,
        browser_in_view: 0x5C0, // the field `GetActiveWebContents` reads first
        groups: false,
        get_tab_model: 0x03E24F10, // `mov rax, [rcx + 0x228]; ret`, the holder's tab strip model
        tab_count: 0x08181300,
        tab_at: 0x0818A300,
        activate_tab: 0x08187360,
        close_tab: 0x0818BE60,
        title: 0x06764D90,
        // Whale's `chrome::NewTab(Browser*, NewTabTypes, const GURL&)`, mapped from Chrome 150's by the
        // calls it makes. The prologue ends with the stack cookie load the detour relocates.
        new_tab_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x18, 0x01, 0x00, 0x00, 0x4C, 0x89, 0xC7, 0x89,
            0xD3, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x75, 0x03, 0x75, 0x11,
        ],
        new_tab_address: Some(0x68),
        new_tab_takes_address: true,
        new_tab_delay_ms: 0,
        new_tab: 0x02569CB0,
        last_committed_url: 0x06762170,
        visible_url: 0x06762120,
        get_group: 0x0818C770,
        signatures: &[
            (0x08F36620, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0E6CB8A0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0E6CC090, &[0x48, 0x8B, 0x81, 0xC0, 0x05, 0x00, 0x00]),
            (0x03E24F10, &[0x48, 0x8B, 0x81, 0x28, 0x02, 0x00, 0x00, 0xC3]),
            (0x08181300, &[0x48, 0x8B, 0x01, 0x8B, 0x40, 0x18, 0xC3]),
            (0x0818A300, &[0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2, 0x78, 0x08, 0x48, 0x8B, 0x09, 0x3B]),
            (0x08187360, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81]),
            (0x0818BE60, &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x44, 0x89, 0xC6, 0x89, 0xD3]),
            (0x06764D90, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05]),
            (0x0818C770, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x78]),
            (0x06762170, &[0x48, 0x63, 0x81, 0xAC, 0x02, 0x00, 0x00, 0x48, 0x83, 0xF8, 0xFF, 0x74]),
            (0x06762120, &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x81, 0xC1, 0x58, 0x02, 0x00, 0x00, 0xE8]),
        ],
        ..UNCHECKED_TABS
    },
    split: UNCHECKED_SPLIT,
    edge_vertical: None,
    edge_page: None,
    hidden_views: Some(HiddenViews {
        slots: [15, 74, 88, 89],
        parent_offset: 0x190,
        children_offset: 0x198,
        invalidate_layout: 0x08F7BEF0,
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38],
        sidebar_minimum: 0x024C3C10,
        sidebar_minimum_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0x8B, 0x05, 0x1C, 0x64, 0x7F, 0x11,
        ],
        classes: &[
            // The toolbar's N button, translate flag, screenshot, split view, profile, AI Chat and menu
            // buttons, then the root view of the right sidebar. Their classes have no symbols; each is
            // told apart by the vtable the live view carries.
            (0xfacb030, [0x9016f30, 0x83d45b0, 0x8f845c0, 0x901fc40]),
            (0xfb070f0, [0x9016f30, 0x83d45b0, 0x8f845c0, 0x901fc40]),
            (0xfb02710, [0x9016f30, 0x83d45b0, 0x8f845c0, 0x901fc40]),
            (0xfb018d0, [0x9016f30, 0x83d45b0, 0x8f845c0, 0x901fc40]),
            (0xfb63b90, [0xe5bd2f0, 0xe5bd200, 0x8f845c0, 0x901fc40]),
            (0xfb010b0, [0x9016f30, 0x230c660, 0x8f845c0, 0x901fc40]),
            (0xfac9e70, [0x83bb9b0, 0x227f080, 0x8f845c0, 0x901fc40]),
            (0xfb0e8e0, [0x24c3c10, 0xe2bcc70, 0x8f845c0, 0x8f84800]),
        ],
        inside: &[],
    }),
    whale_row: Some(WhaleRow {
        set_bounds_rect: 0x08F7_C700,
        set_bounds_rect_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, 0x0F, 0x29, 0xB4, 0x24,
            0xA0, 0x00, 0x00, 0x00, 0x49, 0x89, 0xD6, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x18, 0xD9, 0xD3, 0x0A,
        ],
        parent_offset: 0x190,
        bounds_offset: 0x1C0,
        container: 0xfaac4e0,
        buttons: [0xfad0b10, 0xfad0670, 0xfacbff0],
        field: 0xfabf960,
        frame_layout_slot: 23,
        frame_layout: 0x0E6D_E310,
        toolbar: 0xfb84760,
        toolbar_height: 43,
        separator: 0xfb7f960,
        children_offset: 0x198,
        caption: 0xfb15d60,
        tab_search: 0xfac5608,
        // Found by Chrome 150's body: the rep loop over `[rdx]` and `[rdx + 8]`, then dip times the
        // canvas scale at +8 and `ceilf`.
        paint_vector_icon: 0x09BB_B210,
        paint_vector_icon_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x40],
        canvas_command: 30,
        button_paint_slot: 114,
        caption_button_paint: 0x0901_A370,
        nav_button_paints: [0x03EE_6B20, 0x03EE_6B20, 0x083C_D8D0],
        button_image: 0xfaf7480,
        button_state_offset: 0x3C4,
        reload_mode_offset: 0x8DC,
        page_corner: 0xfb81360,
        page_corner_paint: 0x0E68_CCE0,
        bare: [0xfabf960, 0xfabfca0, 0xfb78080, 0xfb23de0, 0xfaac4e0],
        paint_background_slot: 90,
        paint_background: 0x08F8_4930,
        schedule_paint: 0x08F7_E530,
        schedule_paint_prologue: &[0x48, 0x83, 0xEC, 0x38],
        address_fade: 0xfb244e0,
        address_fade_paint: 0x0E3C_0420,
        address_fade_colour: 0x318,
        layer_set_color: 0x0907_5850,
        layer_set_color_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD6],
        layer_offset: 0x20,
        address_button: 0xfb23660,
        address_button_background: 0x0E3B_C6C0,
        draw_round_rect: 0x09BC_4FD0,
        draw_round_rect_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x0F, 0x29, 0x74, 0x24, 0x40],
        paint_flags_new: 0x03AC_5D60,
        paint_flags_new_prologue: &[0x48, 0x89, 0xC8, 0x0F, 0x10, 0x05],
        paint_flags_drop: 0x031B_E260,
        paint_flags_drop_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE],
    }),
    side_panel: UNCHECKED_SIDEPANEL,
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true, // the tree walk is checked on 5.39.412.57; `toolbar.get_color` is not mapped, so no colour is created
        on_paint: 0xe2f3b70,
        on_paint_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89],
        get_color_provider: 0x8f819d0,
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B],
        draw_color: 0x9bc45b0,
        draw_color_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        top_inset_slot: 127,
        top_inset: 0xe2f32b0,
        frame_vtable: 0xfb16120,
        // The caption buttons' `CalculatePreferredSize`, which lies among Whale's own overrides in their
        // vtable. Their paint is the image button's, and the glyphs are resource images, so Chrome's ink
        // colour swap has nothing to change here.
        caption_vtable: 0xfb15d60,
        caption_size_slot: 74,
        caption_size: 0x0E2E_F360,
        frame_browser_window_offset: 0x338,
        browser_window_in_view: 0x210,
        ..UNCHECKED_LAYOUT
    },
};

/// Brave 156.1.98.62 (Chromium 156.0.8078.74). Brave publishes its symbols, so this row is filled from
/// them. Its windows are `BraveBrowserFrameViewWin` and `BraveBrowserView`, subclasses whose vtables are
/// the ones the objects carry, and the two overrides that sit in the frame vtable are the targets of
/// the slots Rivet replaces. Only the frame, caption and panel hooks are checked; every other feature
/// stays off until its addresses are verified. Chromium's colour ids after the frame ones are shifted by
/// Brave's own, so `caption_ink_ids` are read from this build's `PaintButtonContents`.
pub static BRAVE: Build = Build {
    location_bar: Some(LocationBarMinimum { vtable: 0x111625C0, slot: 15, function: 0x038C54F0, preferred_slot: 74, preferred: 0x03973090, width: 120, toolbar_past_caption: 1 }),
    browser: "Brave",
    module: "chrome.dll",
    version: [156, 1, 98, 62],
    toolbar: Toolbar {
        layout_constant: 0x00365DD0, // ?GetLayoutConstant@@YAHW4LayoutConstant@@@Z
        layout_constant_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xCF, 0x48, 0x8B, 0x05, 0x60, 0x82, 0xE3, 0x12],
        theme_changed: 0x018AA490, // ?ThemeChanged@Widget@views@@QEAAXXZ
        theme_changed_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xA0, 0x3B, 0x8F, 0x11],
        paint_layer: 0x00853790, // ?PaintFromPaintRoot@View@views@@AEAAXAEBVPaintContext@ui@@@Z
        paint_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xA0, 0xA8, 0x94, 0x12],
        set_visible: 0x066CD280, // ?SetVisible@View@views@@UEAAX_N@Z
        set_visible_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD3],
        from_vector_icon: 0x00E63040, // ?FromVectorIcon@ImageModel@ui@@SA?AV12@AEBUVectorIcon@gfx@@VColorVariant@2@HPEBU34@@Z
        from_vector_icon_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20],
        update_location: 0x01665520, // ?Update@LocationIconView@@QEAAX_N0@Z
        update_location_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0x00, 0x00, 0x00],
        update_location_icon: 0x01665BE0, // ?UpdateIcon@LocationIconView@@QEAAXXZ
        update_location_icon_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x98, 0x00, 0x00, 0x00, 0x48, 0x89, 0xCE],
        layout_insets: 0x007F42E0, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD7, 0x48, 0x89, 0xCE],
        toolbar_button_inset: 6,
        toolbar_inset: 11,
        location_icon_inset: 3,
        update_icon: 0x012AE3F0, // ?UpdateIcon@ToolbarButton@@UEAAXXZ
        update_icon_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x80, 0xB9, 0xD8, 0x06, 0x00, 0x00, 0x01],
        formatted_url: 0x01176D00, // ?GetFormattedURL@LocationBarModelImpl@@AEBA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$alloca
        formatted_url_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x60, 0x01, 0x00, 0x00],
        model_url: 0x06C63190, // ?GetURL@LocationBarModelImpl@@UEBA?AVGURL@@XZ
        model_url_prologue: &[0x56, 0x57, 0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, 0x48, 0x89, 0xD6],
        omnibox_update: 0x01663580, // ?Update@OmniboxViewViews@@UEAAXXZ
        omnibox_update_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xB1, 0xAA, 0xB3, 0x11],
        omnibox_revert_all: 0x016646C0, // ?RevertAll@OmniboxViewViews@@UEAAXXZ
        omnibox_revert_all_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x71, 0x99, 0xB3, 0x11],
        omnibox_tab_changed: 0x01666E90, // ?OnTabChanged@OmniboxViewViews@@QEAAXPEAVWebContents@content@@@Z
        omnibox_tab_changed_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE],
        omnibox_textfield: 0x10,
        update_text_style: 0x01663DC0, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@
        update_text_style_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        apply_color: 0x02F14DB0, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0x00, 0x00, 0x00],
        title_weight: 600,
        apply_weight: 0x0C1A2270, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0x00, 0x01, 0x00, 0x00],
        title_was_set: 0x02EB6110, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x80, 0x00, 0x00, 0x00],
        observer_offset: 0x720,
        operator_new: 0x06FA7DE0, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x05165DC0, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6],
        model_delegate: 0x40,
        browser_delegate_vtable: 0x1106D030, // .weak.??_7BrowserLocationBarModelDelegate@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTempl
        active_web_contents: 0x06AD9680, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 19,
        web_contents_vtable: 0x1111AEB0, // .weak.??_7WebContentsImpl@content@@6BPageNavigator@1@@.default.?RejectPromiseOnDestruction@?$CdmProm
        web_contents_title: 0x01029360, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        web_contents_title_slot: 53,
        location_bar_view_base: 0x100,
        hover_value: 0x5F8,
        field_background: 0x328,
        background_vtable: 0x10EDC810, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: 0x11085E78, // ??_7SolidRoundRectPainter@?A0xF7BC1CD1@views@@6B@.b545234ba40531a795a52b0b8121bc9c
        painter_radii: 0x10,
        painter_insets: 0x20,
        field_state: 0x4E8,
        field_state_focused: 0x18,
        focused_fill_id: 1030,
        focus_ring_remove: 0x03BFF210, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0BE97F40, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6],
        set_brightness: 0x0BF3DF20, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30, 0x0F, 0x28, 0xF1],
        view_layer: 0x20,
        view_size: 0x1D0,
        refresh_background: 0x039747C0, // ?RefreshBackground@LocationBarView@@AEAAXXZ
        refresh_background_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xC4, 0x80],
        chip_text: 0x00D30500, // ?GetSecurityChipText@location_bar@@YA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S
        chip_text_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x00, 0x00, 0x00],
        textfield_theme: 0x020E4BE0, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x68],
        textfield_set_font: 0x02EF0590, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x78, 0x03, 0x00, 0x00],
        font_size: 0x03C2AC50, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0x31, 0x8B, 0x46, 0x44, 0x83, 0xF8, 0xFF],
        derive_size: 0x02F162E0, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC7],
        textfield_render_text: 0x378,
        render_text_font_list: 0x48,
        gurl_destructor: 0x0515D0D0, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE],
        gurl_scheme_begin: 0x20,
        gurl_scheme_length: 0x24,
        anchor_rect: 0x0BF20AD0, // ?GetAnchorRect@BubbleDialogDelegate@views@@UEBA?AVRect@gfx@@XZ
        anchor_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xD0, 0x00, 0x00, 0x00],
        anchor_view_access: &[
        0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
        0x48, 0x85, 0xDB, 0x74, 0x4D,
    ],
        is_drawn: 0x001E62F0, // ?IsDrawn@AppMenuButton@@UEBA_NXZ
        is_drawn_bytes: &[
        0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
        0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
    ],
        bounds_in_screen: 0x00993730, // ?GetBoundsInScreen@View@views@@QEBA?AVRect@gfx@@XZ
        bounds_bytes: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF],
        parent_offset: 0x198,
        is_drawn_slot: 19,
        browser_widget_vtable: 0x11098720, // ??_7BrowserWidget@@6BNativeWidgetDelegate@internal@views@@@
        plain_widget_vtable: 0x10ED1670, // ??_7Widget@views@@6BNativeWidgetDelegate@internal@1@@
        paint_slot: 30,
        paint: 0x02EEC870, // ?OnNativeWidgetPaint@Widget@views@@UEAAXAEBVPaintContext@ui@@@Z
        layout_slot: 44,
        layout: 0x015D6CD0, // ?LayoutRootViewIfNecessary@Widget@views@@UEAAXXZ
        color_provider: 0x05836160, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        root_view_offset: 0x1A0,
        theme_slot: 99,
        hidden: &[
        (0x1113B0A0, 0x071A9120), // BraveBrowserAppMenuButton
        (0x111450C0, 0x036F84F0), // BraveAvatarToolbarButton
        (0x111BFD40, 0x00A9A540), // page_actions::PageActionView
        (0x111BFA00, 0x001F3B20), // page_actions::PageActionContainerView
        (0x11144700, 0x001F3B20), // BraveActionsContainer, which holds the Shields and Rewards buttons
        (0x11130EE0, 0x001F3B20), // VerticalTabStripRegionView
        (0x1113A4E0, 0x012AE1A0), // BraveBookmarkButton
        (0x1113B9E0, 0x0AB1BEC0), // SidePanelButton, the Sidebar toggle
        (0x1112A380, 0x0AB0BC50), // WalletButton
        (0x1113E920, 0x0AB1F800), // AIChatButton, Leo
        (0x11146540, 0x0AB240B0), // BraveVPNButton
        (0x1118D020, 0x012AE1A0), // PinnedActionToolbarButton
        (0x1118FC20, 0x012AE1A0), // OverflowButton, the toolbar's chevron for buttons it has no room for
        (0x11191040, 0x012AE1A0), // SplitTabsToolbarButton
    ],
        get_widget_slot: 11,
        size_slots: (0, 0),
        paint_children_slot: 0,
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (16, 28), // kLocationBarHeight
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ],
        dev: DEV_BRAVE_156,
        colours: &[
        (0x13C, PAPER),      // kColorFrameActive
        (813, PAPER),        // kColorLocationBarBackground
        (814, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
        (1020, 0),           // Page info chip background: clear, so the field is one surface
        (159, FOCUS_RING),   // kColorSysStateFocusRing
        (314, FOCUS_RING),   // kColorFocusableBorderFocused
        (465, SELECTION),    // kColorTextfieldSelectionBackground
        (466, INK),          // kColorTextfieldSelectionForeground
        (1021, 0),           // kColorOmniboxIconBackgroundTonal
        (1022, SOFT_INK),    // kColorOmniboxIconForeground: the icon of the field while editing
        (1023, SOFT_INK),    // kColorOmniboxIconForegroundTonal
        (1024, INK),         // kColorOmniboxIconHover
        (1030, PAPER),       // kColorOmniboxResultsBackground, the suggestion list and the field with it
        (1031, 0xFFF0_EFEC), // kColorOmniboxResultsBackgroundHovered
        (1032, 0x0F1F_2024), // kColorOmniboxResultsBackgroundHoverOverlay
        (1033, ACCENT_TINT), // kColorOmniboxResultsBackgroundSelected: the accent at 9 percent over paper
        (1036, SOFT_INK),    // kColorOmniboxResultsButtonIcon
        (1037, INK),         // kColorOmniboxResultsButtonIconSelected
        (1045, 0),           // kColorOmniboxResultsFocusIndicator: the design marks a row by its tint alone
        (1046, SOFT_INK),    // kColorOmniboxResultsIcon
        (1047, 0x0F1F_2024), // kColorOmniboxResultsIconGM3Background
        (1048, INK),         // kColorOmniboxResultsIconSelected
        (1049, INK),         // kColorOmniboxResultsIconHovered
        (1052, 0xFF8A_8690), // kColorOmniboxResultsTextDimmed
        (1053, 0xFF4A_4850), // kColorOmniboxResultsTextDimmedSelected
        (1058, 0xFF8A_8690), // kColorOmniboxResultsTextSecondary
        (1059, 0xFF4A_4850), // kColorOmniboxResultsTextSecondarySelected
        (1060, INK),         // kColorOmniboxResultsTextSelected
        (1061, 0xFF4A_4850), // kColorOmniboxResultsUrl
        (1062, INK),         // kColorOmniboxResultsUrlSelected
        (1069, SOFT_INK),    // kColorOmniboxSecurityChipText: the icon of the field while editing
        (1070, SELECTION),   // kColorOmniboxSelectionBackground: the accent at 25 percent over paper
        (1071, INK),         // kColorOmniboxSelectionForeground
        (1063, 0xFFB3_261E), // kColorOmniboxSecurityChipDangerous: the "Not secure" label and its warning
        (1064, 0x14B3_261E), // kColorOmniboxSecurityChipDangerousBackground: the label's tint, 8 percent
        (1072, INK),         // kColorOmniboxText: the address while it is edited
        (1073, 0xFF8A_8690), // kColorOmniboxTextDimmed
        (1607, PAPER),       // kColorToolbar
        (1661, 0xFF9F_9F9D), // Base of the caption hover fill: at Chrome's 10 percent it matches the native 3.7 percent black
        (1662, 0xFF9F_9F9D), // The same, inactive window
        (1663, 0xFF9F_9F9D), // The same, without a theme colour
        (1664, 0xFFC4_2B1C), // Close button hover fill
        (1667, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
        (1612, 0xFF4A_4850), // kColorToolbarButtonIcon
        (1615, 0xFF1F_2024), // kColorToolbarButtonIconHovered
        (1616, 0xFFBD_BCBC), // kColorToolbarButtonIconInactive: the icon colour at 35 percent over paper
        (1620, HAIRLINE),    // kColorToolbarContentAreaSeparator
        (1628, 0x0F1F_2024), // kColorToolbarInkDropHover
        (1629, 0x1F1F_2024), // kColorToolbarInkDropRipple
    ],
        get_color: 0x06A9A890, // ?GetColor@ColorProvider@ui@@QEBAIH@Z
        get_color_bytes: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x89, 0xD7],
        hwnd_for_widget: 0x0BE82A60, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28],
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9],
        source_in_widget: 0x48,
        content_window_for_hwnd: 0x02EE93A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02EEBD70, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x1218CC50, // ?kBackArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x1218F2A8, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x12191A58, // ?kReloadChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        nav_icon_names: &[b"kBackArrowChromeRefreshOldIcon", b"kForwardArrowChromeRefreshOldIcon", b"kReloadChromeRefreshOldIcon"],
        page_info_icon: 0x122DB7B8, // ?kSecurePageInfoChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: ABSENT, // Brave builds without the Google logos // ?kGoogleGLogoIcon@vector_icons@@3UVectorIcon@gfx@@B
        google_g_monochrome_icon: ABSENT, // ?kGoogleGLogoMonochromeIcon@vector_icons@@3UVectorIcon@gfx@@B
        edit_icons: &[0x1218F358, 0x1218F390, 0x12191EC8, 0x12191F10, 0x12191F48], // kGlobeIcon, kGlobeOldIcon, kSearchIcon, kSearchChromeRefreshOldIcon, kSearchOldIcon
        http_icon: 0x122DB1D8, // ?kHttpChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kHttpChromeRefreshOldIcon",
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
    ],
        page_info_reps: 0x122DB780, // ?kSecurePageInfoChromeRefreshOldRepList@omnibox@@3QBUVectorIconRep@gfx@@B.llvm.2910200201067176236
        page_info_id: 0x122DB790, // ?kSecurePageInfoChromeRefreshOldIconId@omnibox@@3QBDB.llvm.2910200201067176236
        page_info_path: 0x122E53F0, // ?kSecurePageInfoChromeRefreshOldPath@omnibox@@3QBUPathElement@gfx@@B.llvm.2910200201067176236
        page_info_size: 282,
        http_size: 81,
        extra_lock_icons: &[0x122F0540, 0x12193220], // kLeoTuneSmallIcon, kTuneIcon: Brave's mark for a secure page
        extra_warning_icons: &[],
        anchor_view_access_at: 0x2a,
        skip: "",
        anchor_tracker_offset: 0x300,
        toolbar_layout: 0x036F_FFD0, // ?Layout@BraveToolbarView@@UEAAXVPassKey@@@Z
        toolbar_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE],
        infobar_vtable: 0x1113_FDE0, // ??_7BraveInfoBarContainerView@@6B@
        infobar_layout: 0x02EE_1D00, // ?Layout@InfoBarContainerView@@UEAAXVPassKey@@@Z
        infobar_layout_slot: 23,
        leading_space: 27,
        toolbar_top: 6, // the content is 28 DIPs tall, not Chrome's 34, and Brave adds 4 of its own: 6 and 5 reach the 43 DIP row
        toolbar_bottom: 5,
        button_padding: 4,
        field_fill_margin: 0, // the field view is 28 DIPs tall already
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x11066E60, // the vtable of tabs::VerticalTabStripStateControllerImpl, at ShouldDisplayVerticalTabs
        should_display: 0x039AC440, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x00365C40, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x03BF81A0, // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0B4B7890, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x02916550, // ?UpdateCollapseActionItem@VerticalTabStripStateControllerImpl@tabs@@AEAAXXZ
        destructor: 0x02A3E050, // the scalar deleting destructor, two words before ShouldDisplayVerticalTabs
        destructor_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x1112C600, // ??_7BraveBrowserView@@6BLayerDelegate@ui@@@
        region_preferred_size: 0x0CF05490, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x11131130, // slot 74 of ??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@
        invalidate_layout: 0x05CB4A60, // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        region_minimum_size: 0x0CF05400, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0xC7, 0x02, 0x00, 0x00, 0x00, 0x00,
        ],
        should_draw_tab_strip: 0x0661FA10, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[0x56, 0x57, 0x53],
        detour_strip: false,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true,
        content_window_for_hwnd: 0x02EE93A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0CF09280, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CF09730, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x0609D7A0, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x06A5E800, // ??0GURL@@QEAA@XZ
        gurl_drop: 0x0515D0D0, // ??1GURL@@QEAA@XZ
        open_url_params: 0x0857FB20, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui@@_N@Z
        open_url_params_drop: 0x01FC71B0, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x0972ED60, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@...@Z
        signatures: &[
            (0x02EE93A0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CF09280, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0609D7A0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
            (0x06A5E800, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x0515D0D0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x0857FB20, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x01FC71B0, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (0x0972ED60, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
            (0x0CF09730, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true, // list, switch, close and new tab; groups are not read
        content_window_for_hwnd: 0x02EE93A0,
        browser_view_for_window: 0x0CF09280,
        active_web_contents: 0x0CF09730,
        browser_in_view: 0x560, // the field `GetActiveWebContents` reads first
        groups: false,
        get_tab_model: 0x063A9C70, // ?GetTabStripModel@Browser@@UEAAPEAVTabStripModel@@XZ, `mov rax, [rcx + 0x30]; ret`
        tab_count: 0x039AD7B0, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x00B0DDC0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0B4DAF50, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0B4DDC20, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x01029360, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@23@@__Cr@std@@XZ
        new_tab: 0x0B3DA400, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x06B1CB70, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00B27C70, // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02E99DE0, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        signatures: &[
            (0x02EE93A0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CF09280, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0CF09730, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
            (0x063A9C70, &[0x48, 0x8B, 0x41, 0x30, 0xC3]),
            (0x039AD7B0, &[0x48, 0x8B, 0x41, 0x08, 0x8B, 0x40, 0x20]),
            (0x00B0DDC0, &[0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2]),
            (0x0B4DAF50, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38]),
            (0x0B4DDC20, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x01029360, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48]),
            (0x0B3DA400, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
            (0x06B1CB70, &[0x48, 0x63, 0x81, 0xD4, 0x02, 0x00, 0x00]),
            (0x00B27C70, &[0x48, 0x8B, 0x81, 0xB0, 0x02, 0x00, 0x00]),
        ],
        ..UNCHECKED_TABS
    },
    split: UNCHECKED_SPLIT,
    edge_vertical: None,
    edge_page: None,
    hidden_views: None,
    whale_row: None,
    side_panel: UNCHECKED_SIDEPANEL,
    layout: Layout {
        on_paint_slot: 89,
        on_paint: 0x036CEF30, // ?OnPaint@BraveBrowserFrameViewWin@@EEAAXPEAVCanvas@gfx@@@Z
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00],
        get_color_provider: 0x05836160, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x01],
        draw_color: 0x03C29C10, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x111B08C0, // ??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@
        corner_paint: 0x0B762730, // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_vtable: 0x111E7580, // ??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@
        caption_size_slot: 74,
        caption_size: 0x03A4ABD0, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x02EBEF10, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        caption_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        top_inset_slot: 127,
        top_inset: 0x036CF100, // ?GetTopInset@BraveBrowserFrameViewWin@@EEBAH_N@Z
        layout_slot: 23,
        layout: 0x039F55C0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        layout_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40],
        set_bounds_rect: 0x06008490, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x1112C600, // ??_7BraveBrowserView@@6BLayerDelegate@ui@@@
        frame_vtable: 0x1112F460, // ??_7BraveBrowserFrameViewWin@@6BLayerDelegate@ui@@@
        layout_immediately: 0x003E5B50, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE],
        set_layer: 0x0087C150, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28],
        set_radius: 0x01B50760, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30],
        set_masks: 0x02EF7E10, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8B, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0BF3E520, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_opaque: 0x02322E10, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20],
        frame_browser_window_offset: 0x340,
        browser_window_in_view: 0x1F0,
        caption_ink_ids: &[0x67D, 0x67E, 0x67F],
        top_inset_adjust: -12,
        provider_verified: true,
    },
};

/// Brave 156.1.98.63 (Chromium 156.0.8078.80), the same layout of objects and colour ids as Brave
/// 156.1.98.62 at new addresses from its own symbols. `WindowsCaptionButton::PaintButtonContents` is a
/// jump to the next function here, and the vtable slot holds that jump.
pub static BRAVE_63: Build = Build {
    location_bar: Some(LocationBarMinimum { vtable: 0x11120500, slot: 15, function: 0x038E1210, preferred_slot: 74, preferred: 0x0398C100, width: 120, toolbar_past_caption: 1 }),
    browser: "Brave",
    module: "chrome.dll",
    version: [156, 1, 98, 63],
    toolbar: Toolbar {
        layout_constant: 0x0039C9D0, // ?GetLayoutConstant@@YAHW4LayoutConstant@@@Z
        layout_constant_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xCF, 0x48, 0x8B, 0x05, 0x60, 0x26, 0xDC, 0x12],
        theme_changed: 0x0103E8B0, // ?ThemeChanged@Widget@views@@QEAAXXZ
        theme_changed_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x80, 0x07, 0x12, 0x12],
        paint_layer: 0x00883550, // ?PaintFromPaintRoot@View@views@@AEAAXAEBVPaintContext@ui@@@Z
        paint_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xE0, 0xBA, 0x8D, 0x12],
        set_visible: 0x066514D0, // ?SetVisible@View@views@@UEAAX_N@Z
        set_visible_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD3],
        from_vector_icon: 0x00E7C880, // ?FromVectorIcon@ImageModel@ui@@SA?AV12@AEBUVectorIcon@gfx@@VColorVariant@2@HPEBU34@@Z
        from_vector_icon_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20],
        update_location: 0x01628320, // ?Update@LocationIconView@@QEAAX_N0@Z
        update_location_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA8, 0x00, 0x00, 0x00],
        update_location_icon: 0x01628BE0, // ?UpdateIcon@LocationIconView@@QEAAXXZ
        update_location_icon_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x98, 0x00, 0x00, 0x00, 0x48, 0x89, 0xCE],
        layout_insets: 0x00740870, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x89, 0xD7, 0x48, 0x89, 0xCE],
        toolbar_button_inset: 6,
        toolbar_inset: 11,
        location_icon_inset: 3,
        update_icon: 0x012524A0, // ?UpdateIcon@ToolbarButton@@UEAAXXZ
        update_icon_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x80, 0xB9, 0xD8, 0x06, 0x00, 0x00, 0x01],
        formatted_url: 0x011B2530, // ?GetFormattedURL@LocationBarModelImpl@@AEBA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$alloca
        formatted_url_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x60, 0x01, 0x00, 0x00],
        model_url: 0x0637E8D0, // ?GetURL@LocationBarModelImpl@@UEBA?AVGURL@@XZ
        model_url_prologue: &[0x56, 0x57, 0x48, 0x81, 0xEC, 0xB8, 0x00, 0x00, 0x00, 0x48, 0x89, 0xD6],
        omnibox_update: 0x01626A50, // ?Update@OmniboxViewViews@@UEAAXXZ
        omnibox_update_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xE1, 0x85, 0xB3, 0x11],
        omnibox_revert_all: 0x01627070, // ?RevertAll@OmniboxViewViews@@UEAAXXZ
        omnibox_revert_all_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x70, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xC1, 0x7F, 0xB3, 0x11],
        omnibox_tab_changed: 0x01629BD0, // ?OnTabChanged@OmniboxViewViews@@QEAAXPEAVWebContents@content@@@Z
        omnibox_tab_changed_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE],
        omnibox_textfield: 0x10,
        update_text_style: 0x01C87A50, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@
        update_text_style_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        apply_color: 0x02F5E750, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0x00, 0x00, 0x00],
        title_weight: 600,
        apply_weight: 0x0C163F30, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0x00, 0x01, 0x00, 0x00],
        title_was_set: 0x02F05A30, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x80, 0x00, 0x00, 0x00],
        observer_offset: 0x720,
        operator_new: 0x06F4DE00, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x05147E60, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6],
        model_delegate: 0x40,
        browser_delegate_vtable: 0x1102ADA8, // .weak.??_7BrowserLocationBarModelDelegate@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTempl
        active_web_contents: 0x06A5D6A0, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 19,
        web_contents_vtable: 0x110D8DF0, // .weak.??_7WebContentsImpl@content@@6BPageNavigator@1@@.default.?RejectPromiseOnDestruction@?$CdmProm
        web_contents_title: 0x00AE2AE0, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        web_contents_title_slot: 53,
        location_bar_view_base: 0x100,
        hover_value: 0x5F8,
        field_background: 0x328,
        background_vtable: 0x10E9A7D0, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: 0x11043C90, // ??_7SolidRoundRectPainter@?A0xF7BC1CD1@views@@6B@.b545234ba40531a795a52b0b8121bc9c
        painter_radii: 0x10,
        painter_insets: 0x20,
        field_state: 0x4E8,
        field_state_focused: 0x18,
        focused_fill_id: 1030,
        focus_ring_remove: 0x03C0DAE0, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0BE53A80, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6],
        set_brightness: 0x0BEF9E60, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30, 0x0F, 0x28, 0xF1],
        view_layer: 0x20,
        view_size: 0x1D0,
        refresh_background: 0x0398D790, // ?RefreshBackground@LocationBarView@@AEAAXXZ
        refresh_background_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xC4, 0x80],
        chip_text: 0x005AD770, // ?GetSecurityChipText@location_bar@@YA?AV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S
        chip_text_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x00, 0x00, 0x00],
        textfield_theme: 0x020671C0, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x68],
        textfield_set_font: 0x02F3BC60, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x78, 0x03, 0x00, 0x00],
        font_size: 0x03C36860, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0x31, 0x8B, 0x46, 0x44, 0x83, 0xF8, 0xFF],
        derive_size: 0x02F5FBE0, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC7],
        textfield_render_text: 0x378,
        render_text_font_list: 0x48,
        gurl_destructor: 0x0513AB00, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE],
        gurl_scheme_begin: 0x20,
        gurl_scheme_length: 0x24,
        anchor_rect: 0x0BEDC9D0, // ?GetAnchorRect@BubbleDialogDelegate@views@@UEBA?AVRect@gfx@@XZ
        anchor_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xD0, 0x00, 0x00, 0x00],
        anchor_view_access: &[
        0x48, 0x8B, 0x81, 0, 3, 0, 0, 0x48, 0x85, 0xC0, 0x74, 0x56, 0x48, 0x8B, 0x58, 0x18,
        0x48, 0x85, 0xDB, 0x74, 0x4D,
    ],
        is_drawn: 0x000E3710, // ?IsDrawn@AppMenuButton@@UEBA_NXZ
        is_drawn_bytes: &[
        0x80, 0xB9, 0xD8, 1, 0, 0, 1, 0x75, 0x1D, 0x48, 0x8B, 0x89, 0x98, 1, 0, 0,
        0x48, 0x85, 0xC9, 0x74, 0x11, 0x48, 0x8B, 1, 0x48, 0x8B, 0x80, 0x98, 0, 0, 0, 0x48,
    ],
        bounds_in_screen: 0x009A5990, // ?GetBoundsInScreen@View@views@@QEBA?AVRect@gfx@@XZ
        bounds_bytes: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF],
        parent_offset: 0x198,
        is_drawn_slot: 19,
        browser_widget_vtable: 0x11056318, // ??_7BrowserWidget@@6BNativeWidgetDelegate@internal@views@@@
        plain_widget_vtable: 0x10E8F630, // ??_7Widget@views@@6BNativeWidgetDelegate@internal@1@@
        paint_slot: 30,
        paint: 0x02F386F0, // ?OnNativeWidgetPaint@Widget@views@@UEAAXAEBVPaintContext@ui@@@Z
        layout_slot: 44,
        layout: 0x016B29E0, // ?LayoutRootViewIfNecessary@Widget@views@@UEAAXXZ
        color_provider: 0x06735E30, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        root_view_offset: 0x1A0,
        theme_slot: 99,
        hidden: &[
        (0x110F8FE0, 0x070E0200), // BraveBrowserAppMenuButton
        (0x11103000, 0x037187E0), // BraveAvatarToolbarButton
        (0x1117DC80, 0x00915270), // page_actions::PageActionView
        (0x1117D940, 0x00038540), // page_actions::PageActionContainerView
        (0x11102640, 0x00038540), // BraveActionsContainer, which holds the Shields and Rewards buttons
        (0x110EEE20, 0x00038540), // VerticalTabStripRegionView
        (0x110F8420, 0x01252260), // BraveBookmarkButton
        (0x110F9920, 0x0AAD4CA0), // SidePanelButton, the Sidebar toggle
        (0x110E82C0, 0x0AAC5720), // WalletButton
        (0x110FC860, 0x0AAD85E0), // AIChatButton, Leo
        (0x11104480, 0x0AADCE30), // BraveVPNButton
        (0x1114AF60, 0x01252260), // PinnedActionToolbarButton
        (0x1114DB60, 0x01252260), // OverflowButton, the toolbar's chevron for buttons it has no room for
        (0x1114EF80, 0x01252260), // SplitTabsToolbarButton
    ],
        get_widget_slot: 11,
        size_slots: (0, 0),
        paint_children_slot: 0,
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (16, 28), // kLocationBarHeight
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ],
        dev: DEV_BRAVE_156,
        colours: &[
        (0x13C, PAPER),      // kColorFrameActive
        (813, PAPER),        // kColorLocationBarBackground
        (814, 0xFFF0_EFEC),  // kColorLocationBarBackgroundHovered
        (1020, 0),           // Page info chip background: clear, so the field is one surface
        (159, FOCUS_RING),   // kColorSysStateFocusRing
        (314, FOCUS_RING),   // kColorFocusableBorderFocused
        (465, SELECTION),    // kColorTextfieldSelectionBackground
        (466, INK),          // kColorTextfieldSelectionForeground
        (1021, 0),           // kColorOmniboxIconBackgroundTonal
        (1022, SOFT_INK),    // kColorOmniboxIconForeground: the icon of the field while editing
        (1023, SOFT_INK),    // kColorOmniboxIconForegroundTonal
        (1024, INK),         // kColorOmniboxIconHover
        (1030, PAPER),       // kColorOmniboxResultsBackground, the suggestion list and the field with it
        (1031, 0xFFF0_EFEC), // kColorOmniboxResultsBackgroundHovered
        (1032, 0x0F1F_2024), // kColorOmniboxResultsBackgroundHoverOverlay
        (1033, ACCENT_TINT), // kColorOmniboxResultsBackgroundSelected: the accent at 9 percent over paper
        (1036, SOFT_INK),    // kColorOmniboxResultsButtonIcon
        (1037, INK),         // kColorOmniboxResultsButtonIconSelected
        (1045, 0),           // kColorOmniboxResultsFocusIndicator: the design marks a row by its tint alone
        (1046, SOFT_INK),    // kColorOmniboxResultsIcon
        (1047, 0x0F1F_2024), // kColorOmniboxResultsIconGM3Background
        (1048, INK),         // kColorOmniboxResultsIconSelected
        (1049, INK),         // kColorOmniboxResultsIconHovered
        (1052, 0xFF8A_8690), // kColorOmniboxResultsTextDimmed
        (1053, 0xFF4A_4850), // kColorOmniboxResultsTextDimmedSelected
        (1058, 0xFF8A_8690), // kColorOmniboxResultsTextSecondary
        (1059, 0xFF4A_4850), // kColorOmniboxResultsTextSecondarySelected
        (1060, INK),         // kColorOmniboxResultsTextSelected
        (1061, 0xFF4A_4850), // kColorOmniboxResultsUrl
        (1062, INK),         // kColorOmniboxResultsUrlSelected
        (1069, SOFT_INK),    // kColorOmniboxSecurityChipText: the icon of the field while editing
        (1070, SELECTION),   // kColorOmniboxSelectionBackground: the accent at 25 percent over paper
        (1071, INK),         // kColorOmniboxSelectionForeground
        (1063, 0xFFB3_261E), // kColorOmniboxSecurityChipDangerous: the "Not secure" label and its warning
        (1064, 0x14B3_261E), // kColorOmniboxSecurityChipDangerousBackground: the label's tint, 8 percent
        (1072, INK),         // kColorOmniboxText: the address while it is edited
        (1073, 0xFF8A_8690), // kColorOmniboxTextDimmed
        (1607, PAPER),       // kColorToolbar
        (1661, 0xFF9F_9F9D), // Base of the caption hover fill: at Chrome's 10 percent it matches the native 3.7 percent black
        (1662, 0xFF9F_9F9D), // The same, inactive window
        (1663, 0xFF9F_9F9D), // The same, without a theme colour
        (1664, 0xFFC4_2B1C), // Close button hover fill
        (1667, 0xFF1A_1A1A), // Caption button glyph alpha when the window is inactive
        (1612, 0xFF4A_4850), // kColorToolbarButtonIcon
        (1615, 0xFF1F_2024), // kColorToolbarButtonIconHovered
        (1616, 0xFFBD_BCBC), // kColorToolbarButtonIconInactive: the icon colour at 35 percent over paper
        (1620, HAIRLINE),    // kColorToolbarContentAreaSeparator
        (1628, 0x0F1F_2024), // kColorToolbarInkDropHover
        (1629, 0x1F1F_2024), // kColorToolbarInkDropRipple
    ],
        get_color: 0x06A5CB00, // ?GetColor@ColorProvider@ui@@QEBAIH@Z
        get_color_bytes: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x89, 0xD7],
        hwnd_for_widget: 0x0BE3E480, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28],
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9],
        source_in_widget: 0x48,
        content_window_for_hwnd: 0x02F353F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02F37C00, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x1214D920, // ?kBackArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x1214FF78, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x12152728, // ?kReloadChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        nav_icon_names: &[b"kBackArrowChromeRefreshOldIcon", b"kForwardArrowChromeRefreshOldIcon", b"kReloadChromeRefreshOldIcon"],
        page_info_icon: 0x1229C4D8, // ?kSecurePageInfoChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: ABSENT, // Brave builds without the Google logos // ?kGoogleGLogoIcon@vector_icons@@3UVectorIcon@gfx@@B
        google_g_monochrome_icon: ABSENT, // ?kGoogleGLogoMonochromeIcon@vector_icons@@3UVectorIcon@gfx@@B
        edit_icons: &[0x12150028, 0x12150060, 0x12152B98, 0x12152BE0, 0x12152C18], // kGlobeIcon, kGlobeOldIcon, kSearchIcon, kSearchChromeRefreshOldIcon, kSearchOldIcon
        http_icon: 0x1229BEF8, // ?kHttpChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kHttpChromeRefreshOldIcon",
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
    ],
        page_info_reps: 0x1229C4A0, // ?kSecurePageInfoChromeRefreshOldRepList@omnibox@@3QBUVectorIconRep@gfx@@B.llvm.2910200201067176236
        page_info_id: 0x1229C4B0, // ?kSecurePageInfoChromeRefreshOldIconId@omnibox@@3QBDB.llvm.2910200201067176236
        page_info_path: 0x122A6110, // ?kSecurePageInfoChromeRefreshOldPath@omnibox@@3QBUPathElement@gfx@@B.llvm.2910200201067176236
        page_info_size: 282,
        http_size: 81,
        extra_lock_icons: &[0x122B1260, 0x12153EF0], // kLeoTuneSmallIcon, kTuneIcon: Brave's mark for a secure page
        extra_warning_icons: &[],
        anchor_view_access_at: 0x2a,
        skip: "",
        anchor_tracker_offset: 0x300,
        toolbar_layout: 0x03720270, // ?Layout@BraveToolbarView@@UEAAXVPassKey@@@Z
        toolbar_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE],
        infobar_vtable: 0x110FDD20, // ??_7BraveInfoBarContainerView@@6B@
        infobar_layout: 0x02F2D5F0, // ?Layout@InfoBarContainerView@@UEAAXVPassKey@@@Z
        infobar_layout_slot: 23,
        leading_space: 27,
        toolbar_top: 6, // the content is 28 DIPs tall, not Chrome's 34, and Brave adds 4 of its own: 6 and 5 reach the 43 DIP row
        toolbar_bottom: 5,
        button_padding: 4,
        field_fill_margin: 0, // the field view is 28 DIPs tall already
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x11024BD8, // the vtable of tabs::VerticalTabStripStateControllerImpl, at ShouldDisplayVerticalTabs
        should_display: 0x039C3900, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x00227A40, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x03C06C50, // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0B4738F0, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x029F0BE0, // ?UpdateCollapseActionItem@VerticalTabStripStateControllerImpl@tabs@@AEAAXXZ
        destructor: 0x02B00B60, // the scalar deleting destructor, two words before ShouldDisplayVerticalTabs
        destructor_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x110EA540, // ??_7BraveBrowserView@@6BLayerDelegate@ui@@@
        region_preferred_size: 0x0CEC7C70, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x110EF070, // slot 74 of ??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@
        invalidate_layout: 0x05A276E0, // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        region_minimum_size: 0x0CEC7BE0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0xC7, 0x02, 0x00, 0x00, 0x00, 0x00,
        ],
        should_draw_tab_strip: 0x065EAD20, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[0x56, 0x57, 0x53],
        detour_strip: false,
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true,
        content_window_for_hwnd: 0x02F353F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0CECB9F0, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CECBEA0, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x05C641F0, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x06A10280, // ??0GURL@@QEAA@XZ
        gurl_drop: 0x0513AB00, // ??1GURL@@QEAA@XZ
        open_url_params: 0x08537500, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui@@_N@Z
        open_url_params_drop: 0x020CAE20, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x096EA220, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@...@Z
        signatures: &[
            (0x02F353F0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CECB9F0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x05C641F0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
            (0x06A10280, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x0513AB00, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x08537500, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x020CAE20, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (0x096EA220, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
            (0x0CECBEA0, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true, // list, switch, close and new tab; groups are not read
        content_window_for_hwnd: 0x02F353F0,
        browser_view_for_window: 0x0CECB9F0,
        active_web_contents: 0x0CECBEA0,
        browser_in_view: 0x560, // the field `GetActiveWebContents` reads first
        groups: false,
        get_tab_model: 0x0616A870, // ?GetTabStripModel@Browser@@UEAAPEAVTabStripModel@@XZ, `mov rax, [rcx + 0x30]; ret`
        tab_count: 0x039C4B40, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x00526C00, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0B496FB0, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0B499C40, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00AE2AE0, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@23@@__Cr@std@@XZ
        new_tab: 0x0B3962E0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x06AC7B50, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00A77600, // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02EED4A0, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        signatures: &[
            (0x02F353F0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CECB9F0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0CECBEA0, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
            (0x0616A870, &[0x48, 0x8B, 0x41, 0x30, 0xC3]),
            (0x039C4B40, &[0x48, 0x8B, 0x41, 0x08, 0x8B, 0x40, 0x20]),
            (0x00526C00, &[0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2]),
            (0x0B496FB0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38]),
            (0x0B499C40, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x00AE2AE0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48]),
            (0x0B3962E0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57]),
            (0x06AC7B50, &[0x48, 0x63, 0x81, 0xD4, 0x02, 0x00, 0x00]),
            (0x00A77600, &[0x48, 0x8B, 0x81, 0xB0, 0x02, 0x00, 0x00]),
        ],
        ..UNCHECKED_TABS
    },
    split: UNCHECKED_SPLIT,
    edge_vertical: None,
    edge_page: None,
    hidden_views: None,
    whale_row: None,
    side_panel: UNCHECKED_SIDEPANEL,
    layout: Layout {
        on_paint_slot: 89,
        on_paint: 0x036F06C0, // ?OnPaint@BraveBrowserFrameViewWin@@EEAAXPEAVCanvas@gfx@@@Z
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00],
        get_color_provider: 0x06735E30, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48],
        draw_color: 0x03C35B00, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x1116E800, // ??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@
        corner_paint: 0x0B71E3A0, // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_vtable: 0x111A54C0, // ??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@
        caption_size_slot: 74,
        caption_size: 0x03A60970, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x020F21D0, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        caption_paint_prologue: &[0xE9, 0x0B, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC],
        top_inset_slot: 127,
        top_inset: 0x036F0890, // ?GetTopInset@BraveBrowserFrameViewWin@@EEBAH_N@Z
        layout_slot: 23,
        layout: 0x03A0BCD0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        layout_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40],
        set_bounds_rect: 0x05EE66C0, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x110EA540, // ??_7BraveBrowserView@@6BLayerDelegate@ui@@@
        frame_vtable: 0x110ED3A0, // ??_7BraveBrowserFrameViewWin@@6BLayerDelegate@ui@@@
        layout_immediately: 0x05EE76D0, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55],
        set_layer: 0x00852850, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28],
        set_radius: 0x01B70D50, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30],
        set_masks: 0x02F42E30, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8B, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0BEFA460, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_opaque: 0x0244FE30, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20],
        frame_browser_window_offset: 0x340,
        browser_window_in_view: 0x1F0,
        caption_ink_ids: &[0x67D, 0x67E, 0x67F],
        top_inset_adjust: -12,
        provider_verified: true,
    },
};

/// Samsung Internet 143.0.4.29 (Chromium 143.0.7499.194). The browser code is in
/// `samsunginternet.dll`, and `samsunginternet.exe` is the browser process itself, so there is no
/// versioned launcher. The file version resource names the Samsung release, not Chromium's.
/// Only the frame paint, the client-view bounds and the top inset are mapped; everything else stays off.
pub static SAMSUNG: Build = Build {
    location_bar: None,
    browser: "Samsung Internet",
    module: "samsunginternet.dll",
    version: [30, 0, 4, 29],
    toolbar: Toolbar {
        skip: "toolbar",
        ..UNCHECKED_TOOLBAR
    },
    vertical: VerticalTabs {
        // The tabbed layout's function (0x13c8310) inlines `BrowserView::ShouldDrawTabStrip`, whose
        // copies at 0x2b44ed0 and 0x2b44420 it never calls, and starts that test by asking this
        // function for feature 2, the tab strip, at 0x13c8510. Its other callers, the frame's
        // top inset among them, must keep the browser's answer or the caption buttons go with the
        // strip. The prologue is 12 bytes and ends on an instruction boundary.
        window_feature: 0x001F_BB10,
        window_feature_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC3, 0x89, 0xD6],
        window_feature_tab_strip: 2,
        window_feature_callers: (0x013C_8310, 0x013C_A742),
        set_bounds_rect: 0x047C_EA60,
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        toolbar_in_view: 0x5B0,
        // The toggle ends 36 DIPs in and the toolbar keeps 8 before its first button; the back button
        // then sits 30 DIPs from the toggle's centre, as Chrome's does.
        toolbar_leading: 32,
        detour_strip: true,
        top_row: Some(TopRow {
            parent_offset: 0x190,
            children_offset: 0x198,
            bounds_offset: 0x1C0,
            caption: 0xe310268,
            caption_button: 0xe30fea8,
            content: 0xe2f0a28,
            toolbar: 0xe2f0d68,
            separator: 0xe2def88,
            container: 0xe26a968,
            corner: 0xe2800a8,
            inner: 0xe26acc8,
            buttons: &[0xe266528, 0xe268e88],
            field: 0xe2a3168,
            page: 0xe2800a8,
            browser_row: 38,
            panel_top_trim: 1,
            paint_slots: [88, 89],
            painted: &[(0xe25fda8, [0x23b0b30, 0x593450]), (0xe2f0d68, [0x2b44ac0, 0x593450]), (0xe30fea8, [0x593140, 0x2930de0])],
            bare: &[(0xe270568, 89, 0x593450), (0xe2dec28, 89, 0x593450), (0xe2826e8, 90, 0x593580), (0xe2ee868, 89, 0x593450), (0xe2def88, 89, 0x593450)],
            holder: 0xe284ec8,
            holder_parent: 0xe2dec28,
            sidebar_root: (0xe27e8e8, [0x593140, 0x2928ee0]),
            root_widget: 0x340,
            widget_native: 0xF8,
            native_vtable: 0xe21ddc0,
            native_hide_slot: 46,
            native_hide: 0x9032e30,
            native_hide_prologue: &[0x48, 0x8B, 0x89, 0xA8, 0x00, 0x00, 0x00, 0x48, 0x85, 0xC9, 0x0F, 0x85],
            // Samsung's row and everything behind it take the design's paper.
            colours: &[
                (309, 0xFFFB_FAF7), (514, 0xFFFB_FAF7), (520, 0xFFFB_FAF7), (1321, 0xFFFB_FAF7), (1322, 0xFFFB_FAF7),
                (1450, 0xFFFB_FAF7), (1650, 0xFFFB_FAF7), (1319, 0xFFFB_FAF7), (1320, 0xFFFB_FAF7),
                // The close button's hover and the caption glyphs.
                (1512, 0xFFC4_2B1C), (1527, 0xFF1A_1A1A), (1541, 0xFF1A_1A1A),
            ],
            // Found by its body, which matches Whale's: it reads the icon's two fields, scales the dip
            // size by the canvas and paints the path. Samsung's vector icons start with command 28.
            paint_vector_icon: 0x0134_17A0,
            paint_vector_icon_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x38],
            canvas_command: 28,
            button_paint_slot: 114,
            button_paints: &[(0xe266528, 0x5f9e0), (0xe268e88, 0x1bbb2b0), (0xe2708a8, 0x5f9e0), (0xe30fea8, 0x1cd8580)],
            button_image: 0xe252008,
            chip_children: &[0xe252008, 0xe258a68, 0xe24eaa8],
            // Both match Chrome 150's functions instruction for instruction.
            draw_string: 0x09D1_8BF0,
            draw_string_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x44, 0x89, 0xCE],
            string_width: 0x00D5_F170,
            string_width_prologue: &[0x48, 0x83, 0xEC, 0x38, 0x48, 0x8B, 0x05],
            // Reload's own paint reads the mode and the state at these two offsets.
            button_state_offset: 0x3B4,
            reload_mode_offset: 0x89C,
            location_bar: 0xe270568,
            // Seen rising from 0 to 1 in the live view when the pointer enters the field.
            location_hover: 0x4E8,
            location_icon: 0xe2708a8,
            omnibox: 0xe2826e8,
            omnibox_paint: (89, 0x171fb60),
            omnibox_focus: (97, 0x379f9d0),
            omnibox_blur: (98, 0x379faa0),
            // Its body is `ApplyWeight`'s neighbour at 0x2998d00 with the colours' list at +0xA0.
            apply_color: 0x0299_8C60,
            apply_color_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0x00, 0x00, 0x00, 0xE8],
            textfield_render_text: 0x360,
            textfield_set_font: 0x0292_CA50,
            textfield_set_font_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0x60, 0x03, 0x00, 0x00],
            font_new: 0x09D2_7510,
            font_new_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2, 0xE8],
            font_list_new: 0x09D2_7410,
            font_list_new_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE, 0xB9, 0x50],
        }),
        ..UNCHECKED_VERTICALTABS
    },
    navigation: Navigation {
        supported: true, // opening the start page in a new tab checked on Samsung Internet 30.0.4.29
        content_window_for_hwnd: 0x00B5_A010,
        browser_view_for_window: 0x0D9A_0A50,
        active_web_contents: 0x0D9A_0C60,
        // `GURL::GURL(std::u16string_view)`, a wrapper that sets the fields and calls `InitCanonical`; the
        // default constructor; the destructor; the constructor of `OpenURLParams` whose arguments are
        // the address, referrer, disposition, transition and renderer flag, and its destructor; and
        // `WebContentsImpl::OpenURL`. The objects have the layout of Chrome 150's.
        gurl_from_text: 0x0126_9890,
        gurl_wide: true,
        gurl_empty: 0x0519_4F80,
        gurl_drop: 0x03BE_67D0,
        open_url_params: 0x05C4_F410,
        open_url_params_drop: 0x05C4_F9C0,
        open_url: 0x076F_51A0,
        signatures: &[
            (0x00B5_A010, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0D9A_0A50, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0D9A_0C60, &[0x48, 0x8B, 0x81, 0x50, 0x05, 0x00, 0x00, 0x48, 0x8B, 0x88, 0xE0, 0x01, 0x00, 0x00, 0xE9]),
            (0x0126_9890, &[0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05]),
            (0x0519_4F80, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0, 0x0F, 0x11, 0x41, 0x09, 0x0F, 0x11, 0x01]),
            (0x03BE_67D0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70]),
            (0x05C4_F410, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xCF, 0x4C, 0x89, 0xC3, 0x48, 0x89, 0xCE]),
            (0x05C4_F9C0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05]),
            (0x076F_51A0, &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x04, 0x00, 0x00, 0x4C, 0x89, 0xC6, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCF]),
        ],
    },
    tabs: Tabs {
        supported: true, // list, switch, close and new tabs
        content_window_for_hwnd: 0x00B5_A010,
        browser_view_for_window: 0x0D9A_0A50,
        // `BrowserView::GetActiveWebContents` reads the browser at +0x550 and its tab strip model at
        // +0x1e0, then jumps to `TabStripModel::GetActiveWebContents`.
        active_web_contents: 0x0D9A_0C60,
        browser_in_view: 0x550,
        groups: false,
        get_tab_model: 0x02EB_E870, // `mov rax, [rcx + 0x1e0]; ret`, the browser's tab strip model
        tab_count: 0x0322_A1F0,
        // 143 has no function of its own for the web contents at an index. The tab collection answers
        // the tab object at an index, and the object keeps its web contents at +0x60, which is where
        // `TabStripModel::GetActiveWebContents` reads it.
        tab_at: ABSENT,
        tab_object: 0x0476_99D0,
        tab_collection: 0x8,
        tab_contents: 0x60,
        activate_tab: 0x0877_7350,
        close_tab: 0x0877_A5C0,
        title: 0x007D_4FD0, // slot 57 of the web contents vtable at 0xe0fd4a8
        // `chrome::NewTab(Browser*, NewTabTypes)`, found by the window feature 2 test it makes on its first
        // argument and the check of its second against 5. The prologue ends with the stack cookie load
        // the detour relocates.
        new_tab_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x08, 0x01, 0x00, 0x00, 0x89, 0xD7, 0x48, 0x89,
            0xCE, 0x48, 0x8B, 0x05, 0xC8, 0x5B, 0x16, 0x02,
        ],
        // Its first argument is the browser itself, which the browser view holds.
        new_tab_address: Some(0),
        // Samsung starts loading its own page for the tab after the command returns.
        new_tab_delay_ms: 600,
        new_tab: 0x0D81_4460,
        last_committed_url: 0x0553_29B0, // slot 19 of the same vtable
        // `GetVisibleURL` is slot 18 of the vtable. The address list asks for it only when the
        // committed address is empty.
        visible_url: 0x0468_D1A0,
        get_group: 0x0323_3340,
        signatures: &[
            (0x00B5_A010, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0D9A_0A50, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0D9A_0C60, &[0x48, 0x8B, 0x81, 0x50, 0x05, 0x00, 0x00, 0x48, 0x8B, 0x88, 0xE0, 0x01, 0x00, 0x00, 0xE9]),
            (0x02EB_E870, &[0x48, 0x8B, 0x81, 0xE0, 0x01, 0x00, 0x00, 0xC3]),
            (0x0322_A1F0, &[0x48, 0x8B, 0x41, 0x08, 0x8B, 0x40, 0x18, 0xC3]),
            (0x0476_99D0, &[0x48, 0x8B, 0x89, 0x98, 0x00, 0x00, 0x00, 0xEB, 0x11]),
            (0x0877_7350, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x68, 0x01, 0x00, 0x00]),
            (0x0877_A5C0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x48, 0x8B, 0x05]),
            (0x007D_4FD0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81]),
            (0x0553_29B0, &[0x48, 0x63, 0x81, 0x9C, 0x02, 0x00, 0x00, 0x48, 0x83, 0xF8, 0xFF, 0x74]),
            (0x0468_D1A0, &[0x48, 0x8B, 0x81, 0x78, 0x02, 0x00, 0x00, 0x48, 0x85, 0xC0, 0x0F, 0x85]),
            (0x0323_3340, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x78, 0x0E, 0x48]),
        ],
        ..UNCHECKED_TABS
    },
    split: UNCHECKED_SPLIT,
    edge_vertical: None,
    edge_page: None,

    hidden_views: Some(HiddenViews {
        slots: [15, 74, 88, 89],
        parent_offset: 0x190,
        children_offset: 0x198,
        // `views::View::InvalidateLayout`, found by the fields it writes: the layout flag at +0x1f2, the
        // counter at +0x200 and the list at +0x150.
        invalidate_layout: 0x041F_1E70,
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58,
        ],
        // Samsung's sidebar is the root view of a widget of its own, so no layout function of the page
        // asks it for a minimum.
        sidebar_minimum: ABSENT,
        sidebar_minimum_prologue: &[],
        classes: &[
            // Each class is told apart by the vtable the live view carries; its four slots are the
            // functions the vtable holds in the installed build.
            // The button the tab strip's row keeps at its left end, which would draw over Rivet's toggle.
            (0xe2ee508, [0x48877c0, 0x5251150, 0x593140, 0x593450]),
            // The home, split view, favourites, profile and menu buttons of the toolbar.
            (0xe267ae8, [0x5252520, 0x51318c0, 0x593140, 0x2930de0]),
            (0xe2a2908, [0x5252520, 0x51318c0, 0x593140, 0x2930de0]),
            (0xe2a1568, [0x5252520, 0x51318c0, 0x593140, 0x2930de0]),
            (0xe2bf7a8, [0x5252520, 0x51318c0, 0x593140, 0x2930de0]),
            (0xe266988, [0x5252520, 0x51318c0, 0x593140, 0x2930de0]),
            // The container of the location bar's page action icons, among them the bookmark star.
            (0xe272a28, [0x3295c60, 0x5251150, 0x593140, 0x593450]),
            // The bookmarks bar.
            (0xe3027c8, [0x3819ab0, 0x2b49510, 0x381cda0, 0x593450]),
        ],
        // The profile button's picture and its notification dot paint on layers of their own.
        inside: &[(0xe252008, [0x593140, 0x121bca0]), (0xe286f88, [0x593140, 0x90aa180])],
    }),
    whale_row: None,
    side_panel: UNCHECKED_SIDEPANEL,
    layout: Layout {
        on_paint_slot: 89,
        on_paint: 0x1a6f760,
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        // `View::GetColorProvider`, the function the frame paint calls twice before it draws.
        get_color_provider: 0x4717be0,
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x01, 0x48, 0x8B, 0x40, 0x58, 0x48],
        draw_color: 0x3497d50,
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38, 0x48, 0x8B],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        // One virtual function fewer than Chrome 150 sits before the top inset.
        top_inset_slot: 126,
        top_inset: 0x742bb0,
        frame_vtable: 0xe2dc828,
        // The tree walk is checked on 143.0.4.29: the frame colours swap live. `toolbar.get_color` is
        // not mapped, so no colour is created.
        provider_verified: true,
        // The browser view's `views::View` lies this far inside the object `GetBrowserViewForNativeWindow`
        // answers, read from the parent of its children.
        browser_window_in_view: 0x1F8,
        ..UNCHECKED_LAYOUT
    },
};

row_struct! {
/// Brave's location bar asks for a width that leaves too little of the row beside the appbar: its
/// toolbar then drops the forward button and shows its overflow chevron. In Rivet windows it asks for
/// `width` DIPs at most, as its minimum and preferred size, and still takes the room that is left.
#[derive(Clone, Copy)]
pub struct LocationBarMinimum {
    pub vtable: usize,
    pub slot: usize,
    pub function: usize,
    pub preferred_slot: usize,
    pub preferred: usize,
    pub width: i32,
    /// Where the toolbar ends against the caption buttons, which lie above it. The bar stops 9 DIPs
    /// inside the toolbar, so a toolbar 1 DIP under them leaves the 8 DIP gap of the design.
    pub toolbar_past_caption: i32,
}
}

/// The location bar change of a build, for the builds that need one.
pub fn location_bar_minimum(build: &Build) -> Option<LocationBarMinimum> {
    build.location_bar
}

/// The builds Rivet supports, in the order they are tried.
static TABLE: [&Build; 8] = [&CHROME_156_74, &CHROME, &EDGE, &CHROME_150, &WHALE, &BRAVE, &BRAVE_63, &SAMSUNG];

static CURRENT: OnceLock<Option<&'static Build>> = OnceLock::new();

/// The row for the browser module loaded in this process, if any row matches its version.
pub fn current() -> Option<&'static Build> {
    *CURRENT.get_or_init(|| {
        crate::chrome_layout::image::recorded_build()
    })
}

/// The row of the verified build. Only code that already holds a recorded image calls this.
pub fn get() -> &'static Build {
    #[cfg(test)]
    return &CHROME;
    #[cfg(not(test))]
    current().expect("a recorded build is loaded before its table is read")
}

/// The rows that came as data with this binary, which needed no code change to add.
pub const CHECKED_IN: &str = include_str!("../rows/table.json");

/// The rows in the order they are tried: a signed row first, since it can replace one that shipped.
pub fn in_order(signed: Vec<&'static Build>, compiled: &[&'static Build]) -> Vec<&'static Build> {
    signed.into_iter().chain(compiled.iter().copied()).collect()
}

struct Loaded {
    builds: Vec<&'static Build>,
    frames: Vec<rivet_platform_windows::chrome_build::ChromeBuild>,
    note: String,
}

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        use rivet_platform_windows::{chrome_build, signed_table};
        let (signed, note) = match signed_table::default_store().filter(|_| !cfg!(test)) {
            Some(store) => crate::row_data::signed_rows(&store, signed_table::RELEASE_KEY),
            None => Default::default(),
        };
        let checked_in = signed_table::parse(CHECKED_IN.as_bytes())
            .map(|table| crate::row_data::table_rows(&table))
            .unwrap_or_default();
        let compiled: Vec<_> = checked_in.builds.into_iter().chain(TABLE).collect();
        let mut frames = signed.frames;
        frames.extend(checked_in.frames);
        frames.extend(chrome_build::builds(chrome_build::TABLE));
        Loaded { builds: in_order(signed.builds, &compiled), frames, note }
    })
}

/// Every supported row, in the order they are tried.
pub fn all() -> &'static [&'static Build] {
    &loaded().builds
}

/// The frame layout entries of every supported row, in the order they are tried.
pub fn frame_rows() -> &'static [rivet_platform_windows::chrome_build::ChromeBuild] {
    &loaded().frames
}

/// What happened to the signed table in this process.
pub fn table_note() -> &'static str {
    &loaded().note
}

/// The rows built into this binary.
pub fn compiled() -> &'static [&'static Build] {
    &TABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads `bytes.len()` bytes of the file image at `rva` from a PE file on disk.
    fn file_bytes(file: &[u8], rva: usize, length: usize) -> Option<Vec<u8>> {
        let pe = u32::from_le_bytes(file.get(0x3C..0x40)?.try_into().ok()?) as usize;
        let sections = u16::from_le_bytes(file.get(pe + 6..pe + 8)?.try_into().ok()?) as usize;
        let optional = u16::from_le_bytes(file.get(pe + 20..pe + 22)?.try_into().ok()?) as usize;
        for index in 0..sections {
            let header = pe + 24 + optional + 40 * index;
            let field = |at: usize| -> Option<usize> {
                Some(u32::from_le_bytes(file.get(header + at..header + at + 4)?.try_into().ok()?) as usize)
            };
            let (address, size, raw) = (field(12)?, field(16)?, field(20)?);
            if (address..address + size).contains(&rva) {
                let start = rva - address + raw;
                return file.get(start..start + length).map(<[u8]>::to_vec);
            }
        }
        None
    }

    #[test]
    fn fr12_edge_page_hooks_are_recorded_and_chrome_has_none() {
        let page = EDGE.edge_page.as_ref().expect("Edge insets and outlines its page");
        // Both functions start with a prologue the detour can copy whole.
        assert!(page.paint_border_prologue.len() >= 12);
        assert!(page.container_margin_prologue.len() >= 12);
        assert!(page.update_corners_prologue.len() >= 12);
        assert!(CHROME.edge_page.is_none());
        assert!(CHROME_150.edge_page.is_none());
    }

    #[test]
    fn fr3_edge_new_tab_hook_is_recorded_and_chrome_keeps_its_own() {
        let tabs = &EDGE.tabs;
        assert!(!tabs.new_tab_prologue.is_empty());
        // The prologue ends with the stack cookie load the detour relocates.
        let n = tabs.new_tab_prologue.len();
        assert_eq!(tabs.new_tab_prologue[n - 7..n - 4], [0x48, 0x8B, 0x05]);
        assert!(EDGE.navigation.supported);
        assert!(CHROME.tabs.new_tab_prologue.is_empty());
        assert!(CHROME_150.tabs.new_tab_prologue.is_empty());
    }

    #[test]
    fn fr3_whale_new_tab_hook_is_recorded_with_its_address_argument() {
        let tabs = &WHALE.tabs;
        let n = tabs.new_tab_prologue.len();
        assert_eq!(tabs.new_tab_prologue[n - 7..n - 4], [0x48, 0x8B, 0x05]);
        assert!(tabs.new_tab_address == Some(0x68) && tabs.new_tab != ABSENT);
        assert!(EDGE.tabs.new_tab_address.is_none() && CHROME.tabs.new_tab_address.is_none());
        // Every function the address navigation calls has a recorded signature.
        let nav = &WHALE.navigation;
        for rva in [nav.gurl_from_text, nav.gurl_empty, nav.gurl_drop, nav.open_url_params, nav.open_url_params_drop, nav.open_url] {
            assert!(nav.signatures.iter().any(|(at, _)| *at == rva), "{rva:#x}");
        }
    }

    #[test]
    fn nfr3_chrome_156_frame_paint_prologues_match_the_installed_build() {
        let path = r"C:\Program Files\Google\Chrome\Application\156.0.8078.48\chrome.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let layout = &CHROME.layout;
        for (name, rva, prologue) in [
            ("frame paint", layout.on_paint, layout.on_paint_prologue),
            ("caption paint", layout.caption_paint, layout.caption_paint_prologue),
            ("draw colour", layout.draw_color, layout.draw_color_prologue),
            ("colour provider", layout.get_color_provider, layout.get_color_provider_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match chrome.dll"
            );
        }
    }

    #[test]
    fn nfr3_chrome_156_74_hooks_match_the_installed_build() {
        let path = r"C:\Program Files\Google\Chrome\Application\156.0.8078.74\chrome.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let (toolbar, layout, vertical, split) =
            (&CHROME_156_74.toolbar, &CHROME_156_74.layout, &CHROME_156_74.vertical, &CHROME_156_74.split);
        for (name, rva, prologue) in [
            ("layout constant", toolbar.layout_constant, toolbar.layout_constant_prologue),
            ("theme changed", toolbar.theme_changed, toolbar.theme_changed_prologue),
            ("paint layer", toolbar.paint_layer, toolbar.paint_layer_prologue),
            ("set visible", toolbar.set_visible, toolbar.set_visible_prologue),
            ("chip text", toolbar.chip_text, toolbar.chip_text_prologue),
            ("formatted url", toolbar.formatted_url, toolbar.formatted_url_prologue),
            ("update text style", toolbar.update_text_style, toolbar.update_text_style_prologue),
            ("frame paint", layout.on_paint, layout.on_paint_prologue),
            ("caption paint", layout.caption_paint, layout.caption_paint_prologue),
            ("toolbar layout", toolbar.toolbar_layout, toolbar.toolbar_layout_prologue),
            ("colour provider", layout.get_color_provider, layout.get_color_provider_prologue),
            ("destructor", vertical.destructor, vertical.destructor_prologue),
            ("rounded corners", split.set_rounded_corners, split.set_rounded_corners_prologue),
        ] {
            assert_eq!(file_bytes(&file, rva, prologue.len()).as_deref(), Some(prologue), "{name}");
        }
    }

    #[test]
    fn nfr3_chrome_rows_of_one_milestone_are_separate() {
        assert_eq!(CHROME_156_74.version, [156, 0, 8078, 74]);
        assert_eq!(CHROME.version, [156, 0, 8078, 48]);
        assert_ne!(CHROME_156_74.layout.frame_vtable, CHROME.layout.frame_vtable);
        // The relocated hook copies this prologue whole, so it stops on an instruction boundary.
        assert_eq!(CHROME_156_74.toolbar.chip_text_prologue.len(), 13);
    }

    #[test]
    fn fr12_caption_hover_fill_base_matches_the_native_tint() {
        // Chromium fills a hovered button with this colour at 0x1A over the paper. The glyph has its
        // own colour from the paint hook, so only the fill sees this value.
        let fill = |base: u32, paper: u32| (base * 0x1A + paper * (255 - 0x1A) + 127) / 255;
        assert_eq!((fill(0x9F, 0xFB), fill(0x9F, 0xFA), fill(0x9D, 0xF7)), (0xF2, 0xF1, 0xEE));
        for build in [&CHROME_156_74, &BRAVE, &BRAVE_63] {
            let ink = build.layout.caption_ink_ids[0] as i32;
            let base = build.toolbar.colours.iter().find(|(id, _)| *id as i32 == ink);
            assert_eq!(base.map(|(_, colour)| *colour), Some(0xFF9F_9F9D), "{}", build.browser);
        }
    }
    #[test]
    fn fr12_chrome_156_74_row_is_44_dips_with_no_chevron_and_a_field_that_can_shrink() {
        let toolbar = &CHROME_156_74.toolbar;
        // Chrome's own 34 DIP field makes the row, with the separator: only its fill is 28.
        assert!(!toolbar.overrides.iter().any(|(key, _)| *key == 16));
        assert_eq!(toolbar.toolbar_top + 34 + toolbar.toolbar_bottom + 1, 44);
        assert_eq!(28 + 2 * toolbar.field_fill_margin, 34);
        // A 28 DIP button around its 18 DIP icon, which with the row's 2 DIP space gives the 30 DIP pitch.
        assert_eq!(18 + 2 * toolbar.button_padding, 28);
        // Without this hook the browser view stops the toolbar short of the caption buttons.
        assert_eq!(&toolbar.toolbar_layout_prologue[11..14], &[0x48, 0x8B, 0x05]);
        assert!(toolbar.hidden.contains(&(0x0FD1_9CB0, 0x00C9_16B0)), "OverflowButton");
        assert!(location_bar_minimum(&CHROME_156_74).is_some_and(|bar| bar.width <= 120));
    }

    #[test]
    fn nfr3_edge_156_toolbar_functions_and_tables_match_the_installed_build() {
        let path = r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.50\msedge.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let toolbar = &EDGE.toolbar;
        for (name, rva, prologue) in [
            ("layout constant", toolbar.layout_constant, toolbar.layout_constant_prologue),
            ("layout insets", toolbar.layout_insets, toolbar.layout_insets_prologue),
            ("icon update", toolbar.update_location_icon, toolbar.update_location_icon_prologue),
            ("security chip text", toolbar.chip_text, toolbar.chip_text_prologue),
            ("background refresh", toolbar.refresh_background, toolbar.refresh_background_prologue),
            ("formatted url", toolbar.formatted_url, toolbar.formatted_url_prologue),
            ("omnibox update", toolbar.omnibox_update, toolbar.omnibox_update_prologue),
            ("set visible", toolbar.set_visible, toolbar.set_visible_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match msedge.dll"
            );
        }
        // A vtable slot holds the function's absolute address.
        let image_base = 0x1_8000_0000usize;
        let pointer = |rva: usize| {
            file_bytes(&file, rva, 8).map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()) as usize - image_base)
        };
        assert_eq!(pointer(toolbar.browser_delegate_vtable + toolbar.active_web_contents_slot * 8), Some(toolbar.active_web_contents));
        assert_eq!(pointer(toolbar.web_contents_vtable + toolbar.web_contents_title_slot * 8), Some(toolbar.web_contents_title));
        // Each navigation icon carries the name recorded for it.
        for (rva, name) in [toolbar.back_icon, toolbar.forward_icon, toolbar.reload_icon].into_iter().zip(toolbar.nav_icon_names) {
            let name_pointer = pointer(rva + 16).expect("icon is mapped");
            assert_eq!(file_bytes(&file, name_pointer, name.len()).as_deref(), Some(*name));
        }
        // The edge vertical functions and the vtables Rivet reshapes.
        let edge = EDGE.edge_vertical.as_ref().expect("Edge has its own vertical tabs");
        assert_eq!(file_bytes(&file, edge.set_bounds_rect, edge.set_bounds_rect_prologue.len()).as_deref(), Some(edge.set_bounds_rect_prologue));
        for vtable in edge.button_vtables.iter().copied().chain([edge.toolbar_vtable, edge.field_vtable, edge.icon_vtable, edge.text_vtable, edge.separator_vtable, edge.top_container_vtable]) {
            assert!(pointer(vtable).is_some(), "vtable {vtable:#x} lies inside msedge.dll");
        }
    }

    #[test]
    fn nfr3_every_row_names_one_module_and_version() {
        for (index, build) in all().iter().enumerate() {
            assert!(build.module.ends_with(".dll"));
            for other in &all()[index + 1..] {
                assert!(
                    (build.module, build.version) != (other.module, other.version),
                    "{} {:?} is listed twice",
                    build.module,
                    build.version
                );
            }
        }
    }

    #[test]
    fn nfr3_chrome_and_edge_are_separate_rows() {
        assert_eq!(CHROME.module, "chrome.dll");
        assert_eq!(CHROME.version, [156, 0, 8078, 48]);
        assert_eq!(EDGE.module, "msedge.dll");
        assert_eq!(EDGE.version, [156, 0, 4314, 50]);
        assert_ne!(CHROME.toolbar.layout_constant, EDGE.toolbar.layout_constant);
    }

    #[test]
    fn nfr3_chrome_150_checks_only_the_frame_hooks() {
        assert_eq!(CHROME_150.version, [150, 0, 7871, 187]);
        assert_eq!(CHROME_150.toolbar.skip, "toolbar");
        assert!(!CHROME_150.vertical.supported && !CHROME_150.tabs.supported);
        assert_ne!(CHROME_150.layout.frame_vtable, CHROME.layout.frame_vtable);
        // Colour ids and View member offsets moved between milestones 150 and 156.
        assert_eq!(CHROME_150.layout.caption_ink_ids, &[1593, 1594, 1595]);
        assert_eq!(CHROME_150.layout.view_bounds_offset, 0x1C0);
    }

    #[test]
    fn nfr3_provider_colours_are_only_touched_on_verified_builds() {
        // Whale's provider is only read: creating a colour needs a recorded GetColor, which it lacks.
        assert!(WHALE.layout.provider_verified);
        assert!(CHROME.layout.provider_verified && EDGE.layout.provider_verified);
        for build in all() {
            if build.layout.provider_verified {
                assert_ne!(build.layout.get_color_provider, ABSENT, "{}", build.browser);
            }
        }
        // Creating a colour calls GetColor, so it needs a recorded address.
        assert_eq!(WHALE.toolbar.get_color, ABSENT);
        assert_ne!(CHROME.toolbar.get_color, ABSENT);
        assert_ne!(EDGE.toolbar.get_color, ABSENT);
    }

    #[test]
    fn nfr3_whale_maps_only_the_frame_hooks() {
        assert_eq!(WHALE.module, "whale.dll");
        assert_eq!(WHALE.version, [5, 39, 412, 57]);
        assert!(!WHALE.vertical.supported && WHALE.tabs.supported && !WHALE.tabs.groups && !WHALE.split.supported);
        assert!(WHALE.navigation.supported && !WHALE.side_panel.supported);
        assert!(WHALE.vertical.detour_strip && WHALE.vertical.should_draw_tab_strip_prologue.len() >= 14);
        assert!(all().iter().filter(|build| build.vertical.detour_strip).count() == 2);
        assert_eq!(WHALE.toolbar.skip, "toolbar");
        // Hooks without a verified address are absent, so their checks fail.
        assert_eq!(WHALE.layout.corner_paint, ABSENT);
        assert_eq!(WHALE.layout.caption_paint, ABSENT);
        assert_eq!(WHALE.layout.layout, ABSENT);
        assert_eq!((WHALE.layout.frame_browser_window_offset, WHALE.layout.browser_window_in_view), (0x338, 0x210));
    }

    /// The chrome.dll of a Brave row, when that version is the one installed.
    fn brave_dll(build: &Build) -> Option<Vec<u8>> {
        let [a, b, c, d] = build.version;
        std::fs::read(format!(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\{a}.{b}.{c}.{d}\chrome.dll")).ok()
    }

    #[test]
    fn nfr3_brave_156_frame_hooks_match_the_installed_build() {
        assert_eq!(BRAVE.version, [156, 1, 98, 62]);
        assert_eq!(BRAVE_63.version, [156, 1, 98, 63]);
        check_brave_frame_hooks(&BRAVE);
        check_brave_frame_hooks(&BRAVE_63);
    }

    fn check_brave_frame_hooks(brave: &Build) {
        assert!(brave.vertical.supported && brave.tabs.supported && !brave.tabs.groups && brave.navigation.supported);
        assert!(!brave.split.supported && !brave.side_panel.supported && !brave.vertical.detour_strip);
        // Brave's frame is its own subclass, so the vtable is not Chrome's.
        assert_ne!(brave.layout.frame_vtable, CHROME.layout.frame_vtable);
        let Some(file) = brave_dll(brave) else {
            return;
        };
        let layout = &brave.layout;
        for (name, rva, prologue) in [
            ("frame paint", layout.on_paint, layout.on_paint_prologue),
            ("colour provider", layout.get_color_provider, layout.get_color_provider_prologue),
            ("draw colour", layout.draw_color, layout.draw_color_prologue),
            ("corner paint", layout.corner_paint, layout.corner_paint_prologue),
            ("caption paint", layout.caption_paint, layout.caption_paint_prologue),
            ("frame layout", layout.layout, layout.layout_prologue),
            ("set bounds", layout.set_bounds_rect, layout.set_bounds_rect_prologue),
            ("layout immediately", layout.layout_immediately, layout.layout_immediately_prologue),
            ("paint to layer", layout.set_layer, layout.set_layer_prologue),
            ("rounded corners", layout.set_radius, layout.set_radius_prologue),
            ("masks to bounds", layout.set_masks, layout.set_masks_prologue),
            ("stack at top", layout.stack_at_top, layout.stack_at_top_prologue),
            ("fills opaquely", layout.set_opaque, layout.set_opaque_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match Brave's chrome.dll"
            );
        }
        for (rva, bytes) in brave.tabs.signatures.iter().chain(brave.navigation.signatures) {
            assert_eq!(file_bytes(&file, *rva, bytes.len()).as_deref(), Some(*bytes), "signature at {rva:#x}");
        }
        // Each replaced slot holds the function the row names, at the vtable the objects carry.
        for (name, vtable, slot, function) in [
            ("frame paint", layout.frame_vtable, layout.on_paint_slot, layout.on_paint),
            ("top inset", layout.frame_vtable, layout.top_inset_slot, layout.top_inset),
            ("frame layout", layout.frame_vtable, layout.layout_slot, layout.layout),
            ("corner paint", layout.corner_vtable, layout.on_paint_slot, layout.corner_paint),
            ("caption size", layout.caption_vtable, layout.caption_size_slot, layout.caption_size),
            ("caption paint", layout.caption_vtable, layout.caption_paint_slot, layout.caption_paint),
        ] {
            let word = file_bytes(&file, vtable + slot * 8, 8).expect("vtable inside the image");
            let target = u64::from_le_bytes(word.try_into().unwrap()) as usize;
            assert_eq!(target, 0x1_8000_0000 + function, "{name} slot");
        }
    }

    #[test]
    fn fr12_brave_156_toolbar_and_vertical_tab_hooks_match_the_installed_build() {
        check_brave_toolbar_hooks(&BRAVE);
        check_brave_toolbar_hooks(&BRAVE_63);
    }

    #[test]
    fn fr12_brave_row_is_44_dips_with_its_hairline() {
        for brave in [&BRAVE, &BRAVE_63] {
            // Its 28 DIP content, the 4 DIPs Brave adds itself, the row's insets and the hairline.
            assert_eq!(brave.toolbar.toolbar_top + 28 + 4 + brave.toolbar.toolbar_bottom + 1, 44);
        }
    }

    #[test]
    fn fr12_brave_hides_the_overflow_chevron_and_draws_its_lock_over_the_tune_mark() {
        for (brave, overflow, leo, tune) in [(&BRAVE, 0x1118FC20, 0x122F0540, 0x12193220), (&BRAVE_63, 0x1114DB60, 0x122B1260, 0x12153EF0)] {
            assert!(brave.toolbar.hidden.iter().any(|(vtable, _)| *vtable == overflow));
            assert!(brave.toolbar.extra_lock_icons.contains(&leo) && brave.toolbar.extra_lock_icons.contains(&tune));
            assert!(location_bar_minimum(brave).is_some_and(|bar| bar.width <= 120));
        }
        assert!(location_bar_minimum(&CHROME).is_none());
    }

    fn check_brave_toolbar_hooks(brave: &Build) {
        // Brave shifts Chromium's colour ids after the first Chrome one by its own 17.
        assert!(!brave.toolbar.skip.contains("toolbar"));
        assert_eq!(brave.toolbar.skip, "");
        assert_eq!((brave.toolbar.is_drawn_slot, brave.toolbar.get_widget_slot, brave.toolbar.anchor_tracker_offset), (19, 11, 0x300));
        // A 28 DIP button is its icon and the padding either side, and the fill of the field is as tall as the field.
        // Brave draws its toolbar icons at 20 DIPs and the row leaves that size alone, so 4 either side make 28.
        assert!(!brave.toolbar.overrides.iter().any(|(id, _)| *id == 47));
        let icon = Some(20);
        assert_eq!(icon.map(|size| size + 2 * brave.toolbar.button_padding), Some(28));
        assert_eq!(brave.toolbar.field_fill_margin, 0);
        assert_eq!(brave.layout.caption_ink_ids, &[CHROME.layout.caption_ink_ids[0] + 17, CHROME.layout.caption_ink_ids[1] + 17, CHROME.layout.caption_ink_ids[2] + 17]);
        for (brave, chrome) in brave.toolbar.colours.iter().zip(CHROME_156_74.toolbar.colours) {
            let shifted = if chrome.0 >= 574 { chrome.0 + 17 } else { chrome.0 };
            assert_eq!(brave.0, shifted, "colour {} of Chrome", chrome.0);
            assert_eq!(brave.1, chrome.1);
        }
        let Some(file) = brave_dll(brave) else {
            return;
        };
        let toolbar = &brave.toolbar;
        for (name, rva, prologue) in [
            ("layout constant", toolbar.layout_constant, toolbar.layout_constant_prologue),
            ("theme changed", toolbar.theme_changed, toolbar.theme_changed_prologue),
            ("paint layer", toolbar.paint_layer, toolbar.paint_layer_prologue),
            ("set visible", toolbar.set_visible, toolbar.set_visible_prologue),
            ("layout insets", toolbar.layout_insets, toolbar.layout_insets_prologue),
            ("anchor rect", toolbar.anchor_rect, toolbar.anchor_rect_prologue),
            ("anchor view access", toolbar.anchor_rect + toolbar.anchor_view_access_at, toolbar.anchor_view_access),
            ("is drawn", toolbar.is_drawn, toolbar.is_drawn_bytes),
            ("bounds in screen", toolbar.bounds_in_screen, toolbar.bounds_bytes),
            ("colour", toolbar.get_color, toolbar.get_color_bytes),
            ("view invalidation", brave.vertical.invalidate_layout, brave.vertical.invalidate_layout_prologue),
            ("region minimum size", brave.vertical.region_minimum_size, brave.vertical.region_minimum_prologue),
            ("from vector icon", toolbar.from_vector_icon, toolbar.from_vector_icon_prologue),
            ("set transform", toolbar.set_transform, toolbar.set_transform_prologue),
            ("set brightness", toolbar.set_brightness, toolbar.set_brightness_prologue),
            ("update icon", toolbar.update_icon, toolbar.update_icon_prologue),
            ("update location icon", toolbar.update_location_icon, toolbar.update_location_icon_prologue),
            ("toolbar layout", toolbar.toolbar_layout, toolbar.toolbar_layout_prologue),
            ("update location", toolbar.update_location, toolbar.update_location_prologue),
            ("textfield theme", toolbar.textfield_theme, toolbar.textfield_theme_prologue),
            ("refresh background", toolbar.refresh_background, toolbar.refresh_background_prologue),
            ("formatted url", toolbar.formatted_url, toolbar.formatted_url_prologue),
            ("update text style", toolbar.update_text_style, toolbar.update_text_style_prologue),
            ("title was set", toolbar.title_was_set, toolbar.title_was_set_prologue),
            ("omnibox update", toolbar.omnibox_update, toolbar.omnibox_update_prologue),
            ("omnibox revert all", toolbar.omnibox_revert_all, toolbar.omnibox_revert_all_prologue),
            ("omnibox tab changed", toolbar.omnibox_tab_changed, toolbar.omnibox_tab_changed_prologue),
            ("chip text", toolbar.chip_text, toolbar.chip_text_prologue),
            ("controller destructor", brave.vertical.destructor, brave.vertical.destructor_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match Brave's chrome.dll"
            );
        }
        let image_base = 0x1_8000_0000usize;
        let pointer = |rva: usize| {
            file_bytes(&file, rva, 8).map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()) as usize - image_base)
        };
        let vertical = &brave.vertical;
        assert_eq!(pointer(vertical.slot), Some(vertical.should_display));
        assert_eq!(pointer(vertical.slot - 16), Some(vertical.destructor));
        assert_eq!(pointer(vertical.region_preferred_size_slot), Some(vertical.region_preferred_size));
        assert_eq!(pointer(toolbar.browser_widget_vtable + toolbar.paint_slot * 8), Some(toolbar.paint));
        assert_eq!(pointer(toolbar.browser_widget_vtable + toolbar.layout_slot * 8), Some(toolbar.layout));
        assert_eq!(pointer(toolbar.plain_widget_vtable + toolbar.paint_slot * 8), Some(toolbar.paint));
        assert_eq!(pointer(toolbar.plain_widget_vtable + toolbar.layout_slot * 8), Some(toolbar.layout));
        assert_eq!(pointer(toolbar.browser_delegate_vtable + toolbar.active_web_contents_slot * 8), Some(toolbar.active_web_contents));
        assert_eq!(pointer(toolbar.web_contents_vtable + toolbar.web_contents_title_slot * 8), Some(toolbar.web_contents_title));
        // Each hidden view's vtable holds the theme function the row names, at the slot Rivet replaces.
        for (vtable, theme) in toolbar.hidden {
            assert_eq!(pointer(vtable + toolbar.theme_slot * 8), Some(*theme), "hidden view at {vtable:#x}");
        }
        assert_eq!(pointer(toolbar.infobar_vtable + toolbar.infobar_layout_slot * 8), Some(toolbar.infobar_layout));
        let bar = location_bar_minimum(brave).expect("Brave's location bar is recorded");
        assert_eq!(pointer(bar.vtable + bar.slot * 8), Some(bar.function));
        assert_eq!(pointer(bar.vtable + bar.preferred_slot * 8), Some(bar.preferred));
        assert!(brave.layout.set_bounds_rect != ABSENT && brave.layout.view_bounds_offset == 0x1C8);
        // Each navigation icon carries the name recorded for it.
        for (rva, name) in [toolbar.back_icon, toolbar.forward_icon, toolbar.reload_icon].into_iter().zip(toolbar.nav_icon_names) {
            let name_pointer = pointer(rva + 16).expect("icon is mapped");
            assert_eq!(file_bytes(&file, name_pointer, name.len()).as_deref(), Some(*name));
        }
    }

    #[test]
    fn nfr3_samsung_frame_and_tab_hooks_match_the_installed_build() {
        assert_eq!(SAMSUNG.module, "samsunginternet.dll");
        assert_eq!(SAMSUNG.version, [30, 0, 4, 29]);
        assert_eq!(SAMSUNG.toolbar.skip, "toolbar");
        assert!(!SAMSUNG.vertical.supported && SAMSUNG.tabs.supported && !SAMSUNG.tabs.groups && !SAMSUNG.split.supported);
        // The vtable has one function fewer than Chrome 150's before the top inset.
        assert_eq!(SAMSUNG.layout.top_inset_slot, CHROME_150.layout.top_inset_slot - 1);
        // Hooks without a verified address are absent, so their checks fail.
        assert_eq!(SAMSUNG.layout.corner_paint, ABSENT);
        assert_eq!(SAMSUNG.layout.caption_paint, ABSENT);
        let path = r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let layout = &SAMSUNG.layout;
        for (name, rva, prologue) in [
            ("frame paint", layout.on_paint, layout.on_paint_prologue),
            ("draw colour", layout.draw_color, layout.draw_color_prologue),
            ("colour provider", layout.get_color_provider, layout.get_color_provider_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match samsunginternet.dll"
            );
        }
        // The inlined tab strip test: `call` of the window feature function from the layout.
        assert_eq!(file_bytes(&file, 0x13c8510, 5).as_deref(), Some(&[0xE8, 0xFB, 0x35, 0xE3, 0xFE][..]));
        let callers = SAMSUNG.vertical.window_feature_callers;
        assert!(callers.0 <= 0x13c8510 && 0x13c8515 <= callers.1);
        let bounds = SAMSUNG.vertical.set_bounds_rect_prologue;
        assert_eq!(file_bytes(&file, SAMSUNG.vertical.set_bounds_rect, bounds.len()).as_deref(), Some(bounds));
        let bytes = SAMSUNG.vertical.window_feature_prologue;
        assert_eq!(file_bytes(&file, SAMSUNG.vertical.window_feature, bytes.len()).as_deref(), Some(bytes));
        for (rva, bytes) in SAMSUNG.tabs.signatures {
            assert_eq!(
                file_bytes(&file, *rva, bytes.len()).as_deref(),
                Some(*bytes),
                "{rva:#x} does not match samsunginternet.dll"
            );
        }
        // The web contents vtable keeps the title, committed address and visible address getters.
        for (slot, target) in [(57, SAMSUNG.tabs.title), (19, SAMSUNG.tabs.last_committed_url), (18, SAMSUNG.tabs.visible_url)] {
            let word = file_bytes(&file, 0xe0fd4a8 + slot * 8, 8).unwrap();
            assert_eq!(u64::from_le_bytes(word.try_into().unwrap()), 0x1_8000_0000u64 + target as u64);
        }
        for (slot_rva, target) in [
            (layout.frame_vtable + layout.on_paint_slot * 8, layout.on_paint),
            (layout.frame_vtable + layout.top_inset_slot * 8, layout.top_inset),
        ] {
            let word = file_bytes(&file, slot_rva, 8).unwrap();
            let image_base = 0x1_8000_0000u64;
            assert_eq!(u64::from_le_bytes(word.try_into().unwrap()), image_base + target as u64);
        }
    }

    #[test]
    fn fr3_samsung_new_tab_hook_and_navigation_match_the_installed_build() {
        let tabs = &SAMSUNG.tabs;
        let n = tabs.new_tab_prologue.len();
        // The prologue ends with the stack cookie load the detour relocates.
        assert_eq!(tabs.new_tab_prologue[n - 7..n - 4], [0x48, 0x8B, 0x05]);
        assert!(tabs.new_tab_address == Some(0) && !tabs.new_tab_takes_address && tabs.new_tab != ABSENT);
        assert!(SAMSUNG.navigation.supported && tabs.new_tab_delay_ms > 0 && SAMSUNG.navigation.gurl_wide);
        assert!(all().iter().all(|build| build.navigation.gurl_wide == (build.module == "samsunginternet.dll")));
        assert!(WHALE.tabs.new_tab_delay_ms == 0 && EDGE.tabs.new_tab_delay_ms == 0);
        let path = r"C:\Program Files\Samsung\Internet\Applicationc.0.4.29\samsunginternet.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        assert_eq!(file_bytes(&file, tabs.new_tab, n).as_deref(), Some(tabs.new_tab_prologue));
        for (rva, bytes) in SAMSUNG.navigation.signatures {
            assert_eq!(file_bytes(&file, *rva, bytes.len()).as_deref(), Some(*bytes), "{rva:#x}");
        }
    }

    #[test]
    fn fr12_samsung_top_row_matches_the_installed_build() {
        let row = SAMSUNG.vertical.top_row.as_ref().expect("Samsung's row is recorded");
        let path = r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll";
        let Ok(file) = std::fs::read(path) else { return };
        let holds = |vtable: usize, slot: usize, function: usize| {
            file_bytes(&file, vtable + slot * 8, 8).map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()) - 0x1_8000_0000)
                == Some(function as u64)
        };
        for (vtable, functions) in row.painted.iter().chain(std::iter::once(&row.sidebar_root)) {
            assert!(holds(*vtable, row.paint_slots[0], functions[0]), "{vtable:#x}");
            assert!(holds(*vtable, row.paint_slots[1], functions[1]), "{vtable:#x}");
        }
        for (vtable, slot, function) in row.bare {
            assert!(holds(*vtable, *slot, *function), "{vtable:#x}");
        }
        for (vtable, function) in row.button_paints {
            assert!(holds(*vtable, row.button_paint_slot, *function), "{vtable:#x}");
        }
        for (slot, function) in [row.omnibox_paint, row.omnibox_focus, row.omnibox_blur] {
            assert!(holds(row.omnibox, slot, function), "{slot}");
        }
        for (rva, bytes) in [
            (row.apply_color, row.apply_color_prologue),
            (row.draw_string, row.draw_string_prologue),
            (row.string_width, row.string_width_prologue),
            (row.textfield_set_font, row.textfield_set_font_prologue),
            (row.font_new, row.font_new_prologue),
            (row.font_list_new, row.font_list_new_prologue),
        ] {
            assert_eq!(file_bytes(&file, rva, bytes.len()).as_deref(), Some(bytes), "{rva:#x}");
        }
        let icon = row.paint_vector_icon_prologue;
        assert_eq!(file_bytes(&file, row.paint_vector_icon, icon.len()).as_deref(), Some(icon));
        assert!(holds(row.native_vtable, row.native_hide_slot, row.native_hide));
        let hide = row.native_hide_prologue;
        assert_eq!(file_bytes(&file, row.native_hide, hide.len()).as_deref(), Some(hide));
    }

    #[test]
    fn fr12_samsung_hidden_view_classes_match_the_installed_build() {
        let views = SAMSUNG.hidden_views.as_ref().expect("Samsung's extra views are recorded");
        assert_eq!(views.slots, [15, 74, 88, 89]);
        assert_eq!(views.sidebar_minimum, ABSENT);
        assert_eq!(SAMSUNG.layout.browser_window_in_view, 0x1F8);
        let mut seen = std::collections::HashSet::new();
        for (vtable, functions) in views.classes {
            assert!(seen.insert(*vtable), "{vtable:#x} is listed twice");
        }
        let path = r"C:\Program Files\Samsung\Internet\Applicationc.0.4.29\samsunginternet.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let image_base = 0x1_8000_0000u64;
        for (vtable, functions) in views.classes {
            for (index, slot) in views.slots.iter().enumerate() {
                let word = file_bytes(&file, vtable + slot * 8, 8).unwrap();
                assert_eq!(
                    u64::from_le_bytes(word.try_into().unwrap()),
                    image_base + functions[index] as u64,
                    "{vtable:#x} slot {slot} does not hold the recorded function"
                );
            }
        }
        let prologue = views.invalidate_layout_prologue;
        assert_eq!(file_bytes(&file, views.invalidate_layout, prologue.len()).as_deref(), Some(prologue));
    }

    #[test]
    fn nfr3_recorded_addresses_lie_inside_a_plausible_image() {
        for build in all() {
            for rva in [
                build.toolbar.layout_constant,
                build.toolbar.paint,
                build.layout.frame_vtable,
                build.tabs.tab_count,
                build.split.add_to_new_split,
                build.side_panel.show,
                build.vertical.slot,
                build.navigation.open_url,
            ] {
                // A feature the row leaves unchecked records no address.
                if rva == ABSENT || rva == 0 {
                    continue;
                }
                assert!((0x1000..0x2000_0000).contains(&rva), "{rva:#x} in {}", build.module);
            }
        }
    }

    #[test]
    fn nfr3_every_checked_function_has_instruction_bytes() {
        for build in all() {
            for (rva, bytes) in build
                .tabs
                .signatures
                .iter()
                .chain(build.split.signatures)
                .chain(build.navigation.signatures)
            {
                assert!(*rva > 0 && !bytes.is_empty(), "{} {rva:#x}", build.module);
            }
            assert!(!build.toolbar.layout_constant_prologue.is_empty());
            assert!(!build.layout.on_paint_prologue.is_empty());
        }
    }

    #[test]
    fn fr12_only_edge_has_its_own_vertical_tab_container() {
        assert!(CHROME.edge_vertical.is_none());
        let edge = EDGE.edge_vertical.as_ref().expect("Edge keeps its vertical tabs in its own view");
        // A jump over a function's start replaces at least twelve bytes, and the cookie load that
        // the trampoline relocates sits where the row says.
        for (prologue, cookie) in [
            (edge.hide_titlebar_prologue, Some(edge.cookie_at)),
            (edge.container_preferred_size_prologue, None),
            (edge.container_minimum_size_prologue, Some(edge.container_minimum_cookie_at)),
            (edge.set_bounds_rect_prologue, None),
        ] {
            assert!(prologue.len() >= 12);
            if let Some(at) = cookie {
                assert_eq!(&prologue[at..at + 3], &[0x48, 0x8B, 0x05]);
            }
        }
        assert!(edge.vertical_state >= 2, "state 1 is the horizontal strip");
    }

    #[test]
    fn fr12_whale_hides_its_toolbar_buttons_and_sidebar_by_size_and_paint() {
        let views = WHALE.hidden_views.as_ref().expect("Whale's extra views are recorded");
        // Slots of GetMinimumSize, CalculatePreferredSize, PaintChildren and OnPaint in a views::View vtable.
        assert_eq!(views.slots, [15, 74, 88, 89]);
        // Seven toolbar buttons and the sidebar's root view, each class once.
        assert_eq!(views.classes.len(), 8);
        let mut seen = std::collections::HashSet::new();
        for (vtable, functions) in views.classes {
            assert!(seen.insert(*vtable), "{vtable:#x} is listed twice");
            assert!(functions.iter().all(|function| *function != 0 && *function != ABSENT));
        }
        assert_eq!((views.parent_offset, views.children_offset), (0x190, 0x198));
        assert!(views.invalidate_layout_prologue.len() >= 8 && views.sidebar_minimum_prologue.len() >= 14);
        // Only Whale and Samsung list such views; every other build keeps its toolbar and sidebar.
        for build in all() {
            assert_eq!(
                build.hidden_views.is_some(),
                build.module == "whale.dll" || build.module == "samsunginternet.dll",
                "{}",
                build.module
            );
        }
        // Whale's toolbar stays its own: no hidden toolbar views and no `SetVisible` hook.
        assert!(WHALE.toolbar.hidden.is_empty());
        assert_eq!(WHALE.toolbar.skip, "toolbar");
    }

    #[test]
    fn fr12_whale_row_records_its_classes_and_the_bounds_function() {
        let row = WHALE.whale_row.as_ref().expect("Whale's top row is recorded");
        let hidden = WHALE.hidden_views.as_ref().unwrap();
        let mut seen = std::collections::HashSet::new();
        for vtable in [row.container, row.field].into_iter().chain(row.buttons) {
            assert!(seen.insert(vtable), "{vtable:#x} is listed twice");
        }
        // The detour replaces twelve bytes and relocates the stack cookie load that ends the prologue.
        assert!(row.set_bounds_rect_prologue.len() >= 12);
        assert_eq!(row.set_bounds_rect_prologue[row.set_bounds_rect_prologue.len() - 7..][..3], [0x48, 0x8B, 0x05]);
        assert_eq!((row.parent_offset, row.bounds_offset), (0x190, 0x1C0));
        // The frame's layout and the caption buttons: their size is hooked like Chrome's, their paint is not.
        assert_eq!((row.frame_layout_slot, row.children_offset), (23, 0x198));
        assert_eq!(row.toolbar_height, 43);
        assert!(![row.container, row.field, row.caption, row.tab_search, row.toolbar].contains(&row.separator));
        assert!(![row.container, row.field, row.caption, row.tab_search].contains(&row.toolbar));
        assert_ne!(row.caption, row.tab_search);
        assert_eq!((WHALE.layout.caption_vtable, WHALE.layout.caption_size_slot), (row.caption, 74));
        assert_ne!(WHALE.layout.caption_size, ABSENT);
        assert_eq!(WHALE.layout.caption_paint, ABSENT);
        // The row sits among the classes the build already knows as its own.
        assert_eq!(row.parent_offset, hidden.parent_offset);
        for vtable in [row.container, row.field].into_iter().chain(row.buttons) {
            assert!(hidden.classes.iter().all(|(hidden, _)| *hidden != vtable), "{vtable:#x} is hidden");
        }
        // The page's corner views are drawn by their own paint, not by a shared one.
        assert_eq!((row.page_corner, row.page_corner_paint), (0xfb81360, 0x0E68_CCE0));
        assert!(hidden.classes.iter().all(|(vtable, _)| *vtable != row.page_corner));
        // The field itself is one of the views that lose their background.
        assert_eq!(row.bare[0], row.field);
        assert_eq!(row.bare[4], row.container);
        assert_eq!(row.paint_background_slot, 90);
        assert!(!row.bare.contains(&row.address_fade));
        assert_eq!(row.layer_offset, 0x20);
        // Only Whale moves its row.
        for build in all() {
            assert_eq!(build.whale_row.is_some(), build.module == "whale.dll", "{}", build.module);
        }
    }

    #[test]
    fn fr12_hidden_toolbar_views_are_listed_once_per_vtable() {
        for build in all() {
            let mut seen = std::collections::HashSet::new();
            for (vtable, _) in build.toolbar.hidden {
                assert!(seen.insert(*vtable), "{} lists {vtable:#x} twice", build.module);
            }
        }
        // Edge keeps its views visible and asks them for no room, because its toolbar lays itself out
        // again whenever a button's visibility differs from the one it asked for.
        assert_ne!(EDGE.toolbar.size_slots, (0, 0));
        assert_eq!(CHROME.toolbar.size_slots, (0, 0));
    }

    #[test]
    fn fr12_edge_row_is_44_dips_and_its_field_runs_to_the_design_gap() {
        let edge = EDGE.edge_vertical.as_ref().expect("Edge has its own vertical tabs");
        assert_eq!(edge.toolbar_height + edge.separator_height, 44);
        // The toolbar's gap, the field's margin and the caption buttons' margin.
        assert_eq!(edge.caption_gap, 2 + 4 + 4);
        // The field's padding: the text stops where the design's does.
        assert_eq!(edge.text_margin, 10);
    }

    #[test]
    fn fr12_edge_hides_every_toolbar_view_the_design_has_no_row_for() {
        let hidden: Vec<usize> = EDGE.toolbar.hidden.iter().map(|(vtable, _)| *vtable).collect();
        for (vtable, name) in [
            (0x11892678, "settings menu"),
            (0x1191FEF8, "profile"),
            (0x1191FB58, "Copilot"),
            (0x11811CB8, "favorites and collections hub"),
            (0x1181E578, "vertical tab strip"),
            (0x11918C58, "translate, reader and favorite icons"),
            (0x1191F5B8, "chat button in the address field"),
            (0x1181FC38, "horizontal tab strip"),
            (0x118118B8, "extensions"),
            (0x11810CB8, "pinned actions"),
            (0x1187DE38, "split window"),
            (0x1184E298, "Remix"),
            (0x118BBF58, "media"),
            (0x1191EAD8, "toggle button"),
            (0x11931A18, "workspaces"),
        ] {
            assert!(hidden.contains(&vtable), "{name} is not hidden");
        }
        // A hidden class reads its own theme function, which is never the zero address.
        assert!(EDGE.toolbar.hidden.iter().all(|(_, theme)| *theme != 0));
    }

    #[test]
    fn fr3_a_build_with_tabs_checks_only_functions_it_has() {
        for build in all() {
            if build.tabs.supported {
                for (rva, _) in build.tabs.signatures {
                    assert_ne!(*rva, ABSENT, "{} checks a function it does not have", build.module);
                }
                assert!(build.tabs.browser_in_view >= 0x100);
            }
        }
        // Edge keeps its Browser at another place in the browser view, and its groups are not read.
        assert_ne!(CHROME.tabs.browser_in_view, EDGE.tabs.browser_in_view);
        assert!(CHROME.tabs.groups && !EDGE.tabs.groups);
    }

    #[test]
    fn fr11_developer_colours_name_the_row_colours_of_their_build() {
        for build in [&CHROME, &EDGE, &BRAVE, &BRAVE_63] {
            let dev = &build.toolbar.dev;
            assert!(!dev.colours.is_empty(), "{}", build.browser);
            for (key, role) in [("frame", 0), ("toolbar", 1)] {
                // The ground of the row is a colour the row already replaces, and the developer one
                // replaces that value.
                let (id, _) = dev.colours[role];
                assert!(
                    build.toolbar.colours.iter().any(|(known, _)| *known == id),
                    "{} {key} colour {id}",
                    build.browser
                );
            }
            // No id is listed twice, or the later one would never be read.
            let mut ids: Vec<i32> = dev.colours.iter().map(|(id, _)| *id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), dev.colours.len(), "{}", build.browser);
        }
    }

    #[test]
    fn fr11_developer_commands_are_those_of_the_devtools_keys() {
        for build in [&CHROME, &EDGE, &BRAVE, &BRAVE_63] {
            let dev = &build.toolbar.dev;
            assert_eq!((dev.console, dev.inspect), (40005, 40023));
        }
        // A build that was never checked offers no command and no bar.
        assert_eq!(UNCHECKED_TOOLBAR.dev.inspect, 0);
    }

    #[test]
    fn fr11_edge_156_developer_font_functions_match_the_installed_build() {
        let path = r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.50\msedge.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let dev = &EDGE.toolbar.dev;
        for (name, rva, prologue) in [
            ("font", dev.font_new, dev.font_new_prologue),
            ("font list", dev.font_list_new, dev.font_list_new_prologue),
        ] {
            assert_eq!(
                file_bytes(&file, rva, prologue.len()).as_deref(),
                Some(prologue),
                "{name} prologue does not match msedge.dll"
            );
        }
    }

    #[test]
    fn fr11_chrome_156_74_developer_font_functions_match_the_installed_build() {
        let path = r"C:\Program Files\Google\Chrome\Application\156.0.8078.74\chrome.dll";
        let Ok(file) = std::fs::read(path) else {
            return;
        };
        let dev = &CHROME_156_74.toolbar.dev;
        for (name, rva, prologue) in [
            ("font", dev.font_new, dev.font_new_prologue),
            ("font list", dev.font_list_new, dev.font_list_new_prologue),
        ] {
            assert_eq!(file_bytes(&file, rva, prologue.len()).as_deref(), Some(prologue), "{name}");
        }
    }
}
