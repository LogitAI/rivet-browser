//! The supported-build table: every address, vtable, slot, member offset, instruction prologue and
//! enum value the runtime reads from one exact browser module.
//!
//! One row describes one browser build. The runtime picks the row whose module is loaded and
//! whose file version resource matches, and touches nothing when no row matches. Chrome and Edge
//! share Chromium's source, but each build is compiled on its own, so every row is filled from
//! that build's own public symbols and checked against its own instruction bytes.

#![allow(clippy::unreadable_literal)]

use std::sync::OnceLock;

#[allow(unused_imports)]
use crate::chrome_toolbar::{ACCENT_TINT, FOCUS_RING, HAIRLINE, INK, PAPER, SELECTION, SOFT_INK};

/// An address the build does not have. Every check against it fails, so the feature that needs it
/// stays off.
const ABSENT: usize = 0x7FFF_0000;

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
}

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
    /// `views::View::GetWidget` is the twelfth entry of every view's vtable.
    pub get_widget_slot: usize,
    /// `LayoutConstant` values of this build and the sizes of the Rivet top row, in DIPs.
    pub overrides: &'static [(i32, i32)],
    /// Colour ids of this build and the Rivet colours, as `0xAARRGGBB`.
    pub colours: &'static [(i32, u32)],
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
    /// Where the anchor view access starts inside `GetAnchorRect`.
    pub anchor_view_access_at: usize,
    /// Features this build leaves alone: "toolbar" (the whole top row), "address", "icon" and "updateicon".
    pub skip: &'static str,
    /// Where a `BubbleDialogDelegate` keeps the tracker of its anchor view.
    pub anchor_tracker_offset: usize,
}

/// Values for `vertical_tabs.rs`.
pub struct VerticalTabs {
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
}

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

/// Values for `chrome_tabs.rs`.
pub struct Tabs {
    /// Whether this build has been checked for this feature. When it is false the feature is left alone.
    pub supported: bool,
    pub content_window_for_hwnd: usize,
    pub browser_view_for_window: usize,
    pub active_web_contents: usize,
    /// Tab strip and browser actions for Chrome 156.0.8078.48.
    pub get_tab_model: usize,
    pub tab_count: usize,
    pub tab_at: usize,
    pub activate_tab: usize,
    pub close_tab: usize,
    pub title: usize,
    pub new_tab: usize,
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

/// Values for `chrome_layout.rs`.
pub struct Layout {
    /// Experiment, Chrome 156.0.8078.48 only. The frame view paints its title bar with colours from
    /// a colour provider that other windows of the profile share. Rivet wraps the frame view's
    /// `OnPaint` and swaps the frame colours in that provider only while a Rivet window paints.
    ///
    /// `views::View::GetColorProvider` returns the provider. It keeps generated colours in a tree
    /// whose nodes hold the colour id at `+0x20` and the colour at `+0x24`.
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
}

pub static CHROME: Build = Build {
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
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (16, 28), // kLocationBarHeight
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (47, 18), // kToolbarButtonIconSize
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ],
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
        hwnd_for_widget: 0x0AC0_B080, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28],
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9],
        source_in_widget: 0x48,
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x0315_C7C0, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: 0x10E0_2EA8, // ?kBackArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        forward_icon: 0x10E0_55C0, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: 0x10E0_7DD0, // ?kReloadChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
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
        anchor_view_access_at: 0x2a,
        skip: "",
        anchor_tracker_offset: 0x300,
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
    },
    navigation: Navigation {
        supported: true,
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BC8_1E10, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BC8_2340, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x042B_C7A0, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
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
        get_tab_model: 0x04FB_01F0, // ?Client@WebLocalFrameImpl@blink@@UEBAPEAVWebLocalFrameClient@2@XZ
        tab_count: 0x0369_CBE0, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0311_22A0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0311_2220, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A20_11B0, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00D7_8440, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
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
    },
};

pub static EDGE: Build = Build {
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
        update_location_icon: ABSENT, // not found in Edge
        update_location_icon_prologue: &[0x00], // function not found in Edge
        layout_insets: 0x00FF3C3D, // ?GetLayoutInsets@@YA?AVInsets@gfx@@W4LayoutInset@@@Z
        layout_insets_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x88, 0x00, 0x00, 0x00],
        toolbar_button_inset: 6, // Chrome value, not checked on Edge
        toolbar_inset: 11, // Chrome value, not checked on Edge
        location_icon_inset: 3, // Chrome value, not checked on Edge
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
        omnibox_textfield: 0x10, // Chrome value, not checked on Edge
        update_text_style: 0x02824F00, // ?UpdateTextStyle@OmniboxView@@IEAAXAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$all
        update_text_style_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xF0, 0x01, 0x00, 0x00],
        apply_color: 0x028D2F82, // ?ApplyColor@RenderText@gfx@@QEAAXIAEBVRange@2@@Z
        apply_color_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0xA0, 0x00, 0x00, 0x00],
        apply_weight: 0x0B74E45C, // ?ApplyWeight@RenderText@gfx@@QEAAXW4Weight@Font@2@AEBVRange@2@@Z
        apply_weight_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x81, 0xC1, 0x00, 0x01, 0x00, 0x00],
        title_was_set: 0x033CB530, // ?TitleWasSet@OmniboxViewViews@@EEAAXPEAVNavigationEntry@content@@@Z
        title_was_set_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0x28, 0x01, 0x00, 0x00],
        observer_offset: 0x720, // Chrome value, not checked on Edge
        operator_new: 0x036CDF90, // ??2@YAPEAX_K@Z
        operator_new_bytes: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
        operator_delete: 0x0023EE60, // ??3@YAXPEAX_K@Z
        operator_delete_bytes: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6],
        model_delegate: 0x40, // Chrome value, not checked on Edge
        browser_delegate_vtable: ABSENT, // not found in Edge
        active_web_contents: 0x00FE1AC0, // ?GetActiveWebContents@BrowserLocationBarModelDelegate@@UEBAPEAVWebContents@content@@XZ
        active_web_contents_slot: 19, // Chrome value, not checked on Edge
        web_contents_vtable: ABSENT, // not found in Edge
        web_contents_title: 0x01119470, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?
        web_contents_title_slot: 53, // Chrome value, not checked on Edge
        location_bar_view_base: 0x100, // Chrome value, not checked on Edge
        hover_value: 0x5F8, // Chrome value, not checked on Edge
        field_background: 0x328, // Chrome value, not checked on Edge
        background_vtable: 0x114D6E80, // ??_7BackgroundPainter@views@@6B@
        painter_vtable: ABSENT, // not found in Edge
        painter_radii: 0x10, // Chrome value, not checked on Edge
        painter_insets: 0x20, // Chrome value, not checked on Edge
        field_state: 0x4E8, // Chrome value, not checked on Edge
        field_state_focused: 0x18, // Chrome value, not checked on Edge
        focused_fill_id: 1013, // Chrome value, not checked on Edge
        focus_ring_remove: 0x027876CC, // ?Remove@FocusRing@views@@SAXPEAVView@2@@Z
        set_transform: 0x0B38D582, // ?SetTransform@View@views@@QEAAXAEBVTransform@gfx@@@Z
        set_transform_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x48, 0x89, 0xD6],
        set_brightness: 0x02DA797C, // ?SetLayerBrightness@Layer@ui@@QEAAXM@Z
        set_brightness_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x0F, 0x29, 0x74, 0x24, 0x30, 0x0F, 0x28, 0xF1],
        view_layer: 0x20, // Chrome value, not checked on Edge
        view_size: 0x1D0, // Chrome value, not checked on Edge
        refresh_background: ABSENT, // not found in Edge
        refresh_background_prologue: &[0x00], // function not found in Edge
        chip_text: ABSENT, // not found in Edge
        chip_text_prologue: &[0x00], // function not found in Edge
        textfield_theme: 0x02D30920, // ?OnThemeChanged@Textfield@views@@UEAAXXZ
        textfield_theme_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x10, 0x37, 0x4C, 0x11],
        textfield_set_font: 0x0278A88A, // ?SetFontList@Textfield@views@@QEAAXAEBVFontList@gfx@@@Z
        textfield_set_font_bytes: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81, 0xC0, 0x03, 0x00, 0x00],
        font_size: 0x0101C850, // ?GetFontSize@FontList@gfx@@QEBAHXZ
        font_size_bytes: &[0x48, 0x8B, 0x09, 0xE9, 0x00, 0x00, 0x00, 0x00, 0x56, 0x48, 0x83, 0xEC, 0x20, 0x8B, 0x41, 0x44],
        derive_size: 0x0101C9EA, // ?DeriveWithSizeDelta@FontList@gfx@@QEBA?AV12@H@Z
        derive_size_bytes: &[0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x44, 0x89, 0xC7, 0x48, 0x89, 0xD6],
        textfield_render_text: 0x378, // Chrome value, not checked on Edge
        render_text_font_list: 0x48, // Chrome value, not checked on Edge
        gurl_destructor: 0x00FDECAA, // ??1GURL@@QEAA@XZ
        gurl_destructor_bytes: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70],
        gurl_scheme_begin: 0x20, // Chrome value, not checked on Edge
        gurl_scheme_length: 0x24, // Chrome value, not checked on Edge
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
                (0x118CBBB8, 0x026C1FA0), // page_actions::PageActionView
                (0x1181E978, 0x00FDE2C0), // VerticalTabStripRegionView
                (0x11834A18, 0x00FDE2C0), // SidePanelHeader
                (0x11890838, 0x02B70B40), // PinnedActionToolbarButton
            ],
        get_widget_slot: 10,
        overrides: &[
        (12, 8),  // kLocationBarChildCornerRadius
        (16, 28), // kLocationBarHeight
        (29, 12), // kMainBackgroundRegionCornerRadius
        (46, 28), // kToolbarButtonHeight
        (47, 18), // kToolbarButtonIconSize
        (55, 8),  // kToolbarCornerRadius
        (60, 0),  // kVerticalTabMinWidth
    ], // Chrome value, not checked on Edge
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
    ], // Chrome value, not checked on Edge
        hwnd_for_widget: 0x0B379B4D, // ?HWNDForWidget@views@@YAPEAUHWND__@@PEBVWidget@1@@Z
        hwnd_for_widget_head: &[0x48, 0x83, 0xEC, 0x28], // Chrome value, not checked on Edge
        hwnd_for_widget_tail: &[0x48, 0x89, 0xC1, 0x48, 0x83, 0xC4, 0x28, 0xE9], // Chrome value, not checked on Edge
        source_in_widget: 0x48, // Chrome value, not checked on Edge
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x028C8D78, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        back_icon: ABSENT, // not found in Edge
        forward_icon: 0x12C62308, // ?kForwardArrowChromeRefreshOldIcon@vector_icons@@3UVectorIcon@gfx@@B
        reload_icon: ABSENT, // not found in Edge
        page_info_icon: 0x12ED5050, // ?kSecurePageInfoChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        google_g_icon: ABSENT, // not found in Edge
        google_g_monochrome_icon: ABSENT, // not found in Edge
        edit_icons: &[0x10E0_5680, 0x10E0_56B8, 0x10E0_8278, 0x10E0_82C0, 0x10E0_82F8], // Chrome value, not checked on Edge
        http_icon: 0x12ED4BE8, // ?kHttpChromeRefreshOldIcon@omnibox@@3UVectorIcon@gfx@@B
        http_icon_name: b"kHttpChromeRefreshOldIcon", // Chrome value, not checked on Edge
        page_info_bytes: &[
        0x1C, 0, 0, 0, 0, 0, 0x80, 0x41, 1, 0, 0, 0, 7, 0, 0, 0,
        0x5C, 0x8F, 0x3A, 0x41, 0xEC, 0x51, 0x5C, 0x41, 0x12, 0, 0, 0, 0xD7, 0xA3, 0x30, 0xBF,
    ], // Chrome value, not checked on Edge
        page_info_reps: ABSENT, // not found in Edge
        page_info_id: ABSENT, // not found in Edge
        page_info_path: ABSENT, // not found in Edge
        anchor_view_access_at: 0x28,
        skip: "toolbar", // Edge keeps its own toolbar for now
        anchor_tracker_offset: 0x378, // BubbleDialogDelegate anchor view tracker, read by GetAnchorRect
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
    },
    navigation: Navigation {
        supported: false, // not checked on Edge yet
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x010CDAE8, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CAE5550, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x01016852, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
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
        supported: false, // not checked on Edge yet
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x010CDAE8, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CAE5550, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        get_tab_model: 0x055E0CE0, // ?PdfSize@DocumentMetadata@datamodel@edge_pdf@@QEBA_KXZ
        tab_count: 0x010CBBCE, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0105E38E, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0A404C92, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A4080EA, // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x01119470, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?
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
            (0x7FFF0000, &[0x00]),
            (0x026A4DC6, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x010CDAE8, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0CAE5550, &[0x48, 0x8B, 0x81, 0x18, 0x06, 0x00, 0x00]),
            (0x055E0CE0, &[0x48, 0x8B, 0x41, 0x30, 0xC3]),
            (0x010CBBCE, &[0x48, 0x8B, 0x41, 0x10, 0x8B, 0x40, 0x20]),
            (0x0105E38E, &[0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2]),
            (0x0A404C92, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38]),
            (0x0A4080EA, &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC]),
            (0x01119470, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48]),
            (0x0A2B68B0, &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54]),
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
        on_paint: 0x02B44110, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x01003060, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x02A3F86A, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144], // Chrome value, not checked on Edge
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
    },
};

/// The builds Rivet supports, in the order they are tried.
static TABLE: [&Build; 2] = [&CHROME, &EDGE];

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

/// Every supported row.
pub fn all() -> &'static [&'static Build] {
    &TABLE
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
