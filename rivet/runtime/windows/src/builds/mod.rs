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

mod brave;
mod chrome;
mod edge;
mod omnibox_rows;
mod samsung;
mod whale;

pub use brave::{BRAVE, BRAVE_63};
pub use chrome::{CHROME, CHROME_150, CHROME_156_74, CHROME_156_80};
pub use edge::EDGE;
pub use samsung::SAMSUNG;
pub use whale::{WHALE, WHALE_ADD_TAB_AT};

#[allow(unused_imports)]
use crate::features::look::{ACCENT_TINT, FOCUS_RING, HAIRLINE, INK, PAPER, SELECTION, SOFT_INK};

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
    /// Chromium's tab model and page view functions an app window needs, for the builds whose
    /// addresses were found and checked.
    pub app_window: Option<AppWindow>,
    /// Chromium's address bar functions, for the builds whose addresses were found.
    pub omnibox: Option<Omnibox>,
}
}

row_struct! {
/// Chromium's functions and member offsets behind the address bar Rivet draws in an app window:
/// what typed text opens, the browser's suggestions, the page's security level and its page info
/// bubble. Every function is Chromium's, so every browser has the same address bar.
pub struct Omnibox {
    /// `Profile::FromBrowserContext` and `content::WebContentsImpl::GetBrowserContext`.
    pub profile_from_context: usize,
    pub browser_context: usize,
    /// `AutocompleteClassifierFactory::GetForProfile` and `AutocompleteClassifier::Classify`.
    pub classifier_for_profile: usize,
    pub classify: usize,
    /// `AutocompleteMatch::AutocompleteMatch()` and its destructor, its size and where it keeps
    /// `destination_url`.
    pub match_new: usize,
    pub match_drop: usize,
    pub match_size: usize,
    pub match_destination: usize,
    /// `chrome_security_state::GetVisibleSecurityState`, `security_state::GetSecurityLevel`, the
    /// state's destructor and `operator delete`.
    pub visible_security_state: usize,
    pub security_level: usize,
    pub security_state_drop: usize,
    pub delete: usize,
    /// `ShowPageInfoDialog(contents, closed callback, anchor, highlighted setting)`.
    pub show_page_info: usize,
    /// `ChromeAutocompleteProviderClient(Profile*)`, `ChromeAutocompleteSchemeClassifier(Profile*)`,
    /// `AutocompleteClassifier::DefaultOmniboxProviders`, `AutocompleteController(client, config)`
    /// and where it keeps `published_result_`, `AutocompleteInput(text, page, classifier, https,
    /// port, fake https)` and its destructor with its flags, `AutocompleteController::Start` and
    /// `operator new`. A match keeps `contents` and `description` as `std::u16string`.
    pub provider_client_new: usize,
    pub provider_client_size: usize,
    pub scheme_classifier_new: usize,
    pub scheme_classifier_size: usize,
    pub default_providers: usize,
    pub controller_new: usize,
    pub controller_size: usize,
    pub controller_published_result: usize,
    pub input_new: usize,
    pub input_drop: usize,
    pub input_size: usize,
    pub input_prevent_inline: usize,
    pub input_omit_asynchronous: usize,
    pub start: usize,
    pub allocate: usize,
    pub match_contents: usize,
    pub match_description: usize,
    /// The vtable of `BrowserViewAppLayoutImpl`, the slot of its `CalculateProposedLayout` and that
    /// function, which lays out an app window.
    pub app_layout_vtable: usize,
    pub app_layout_slot: usize,
    pub app_layout_proposed: usize,
    /// The first bytes of every function above, compared with the loaded module before any call.
    pub signatures: &'static [(usize, &'static [u8])],
}
}

row_struct! {
/// What a Rivet app window calls in Chromium: the window feature answer that keeps new tabs in the
/// app window's own tab strip model, the tab moves into it, and the second `views::WebView` that
/// shows a split or sidebar page. Every function is Chromium's. An address the build does not
/// have is `None`, and the part that needs it stays off. Each prologue is compared with the loaded
/// module before anything is called or patched.
pub struct AppWindow {
    /// The window feature function: `WindowFeatureController::SupportsWindowFeatureImpl(this,
    /// feature, check)`, or `Browser::SupportsWindowFeatureImpl` on builds that keep it there.
    /// Its prologue is exactly the bytes the detour copies.
    pub feature_impl: Option<usize>,
    pub feature_impl_prologue: &'static [u8],
    /// Where the object the feature function is called on keeps its browser type.
    pub browser_type: usize,
    /// `TabStripModel::DetachTabAtForInsertion(this, out, index, reason)` and
    /// `TabStripModel::InsertDetachedTabAt(this, index, tab, flags, group)`.
    pub detach: Option<usize>,
    pub detach_prologue: &'static [u8],
    pub insert: Option<usize>,
    pub insert_prologue: &'static [u8],
    /// A `views::View` keeps its parent at this offset and its children vector right after it.
    pub view_parent: usize,
    pub view_children: usize,
    /// `operator new(size)`, `views::WebView::WebView(this, context)` and the object's size.
    pub operator_new: usize,
    pub operator_new_prologue: &'static [u8],
    pub web_view_ctor: usize,
    pub web_view_ctor_prologue: &'static [u8],
    pub web_view_size: usize,
    /// `WebContentsImpl::GetBrowserContext(page)`, or, where the build inlines it, the offset in the
    /// web contents where the browser context is kept.
    pub browser_context: Option<usize>,
    pub browser_context_prologue: &'static [u8],
    pub browser_context_offset: Option<usize>,
    /// `views::View::AddChildViewAtImpl(this, view, index)`.
    pub add_child: usize,
    pub add_child_prologue: &'static [u8],
    /// `views::WebView::SetWebContents(this, contents)`.
    pub set_contents: usize,
    pub set_contents_prologue: &'static [u8],
    /// `views::View::SetBoundsRect(this, &rect)`. A build without it gets bounds written directly.
    pub set_bounds: Option<usize>,
    pub set_bounds_prologue: &'static [u8],
    /// `views::View::SetFocusBehavior(this, behavior)`.
    pub focus_behavior: Option<usize>,
    pub focus_behavior_prologue: &'static [u8],
    /// `views::View::RequestFocus(this)`, by address or by its vtable slot.
    pub request_focus: Option<usize>,
    pub request_focus_prologue: &'static [u8],
    pub request_focus_slot: Option<usize>,
    /// The vtable slot of the view's deleting destructor.
    pub destructor_slot: usize,
    /// The vtable slot of `views::View::OnBoundsChanged`, which lets the second view follow the
    /// bounds the browser gives the window's page view. Without it the pages are not laid out side
    /// by side.
    pub bounds_changed_slot: Option<usize>,
    /// The build's `WindowFeature` values for the title bar, the tab strip and the toolbar, when they
    /// are not Chromium's 1, 2 and 3.
    pub window_features: Option<(u32, u32, u32)>,
}
}

row_struct! {
/// Values for `chrome_toolbar.rs`.
pub struct Toolbar {
    /// `views::View::GetWidget` is the twelfth entry of every view's vtable.
    pub get_widget_slot: usize,
    /// What this build needs to show a page in developer mode.
    pub dev: DevMode,
    /// `ui::ColorProvider::GetColor(this, id)`. A provider makes a colour the first time it is asked
    /// for it, so one that was never asked for is asked for before its value is replaced.
    pub get_color: usize,
    pub get_color_bytes: &'static [u8],
    /// `views::DesktopWindowTreeHostWin::GetContentWindowForHWND` and
    /// `views::Widget::GetWidgetForNativeWindow`.
    pub content_window_for_hwnd: usize,
    pub widget_for_window: usize,
    /// Features this build leaves alone: "toolbar" (the whole top row), "address", "icon" and "updateicon".
    pub skip: &'static str,
    /// The toolbar view's own `Layout`, when the browser lays the toolbar out outside the window's
    /// layout pass. `ABSENT` when not needed.
    pub toolbar_layout: usize,
}
}

row_struct! {
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
    /// Whether `AddToNewSplit` adds the active tab to the split by itself. Then it takes only the other
    /// tab's index and refuses a list that holds the active tab. Newer builds take both indices.
    pub split_adds_active: bool,
    /// How the index vector `AddToNewSplit` takes is laid out: begin, end and capacity as pointers, or
    /// begin, size and capacity. libc++ changed it between Chrome 150 and 156.
    pub split_vector_ends_by_pointer: bool,
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
    pub schedule_paint_prologue: &'static [u8],
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
    /// How many entry ids this build defines.
    pub entry_count: u32,
    /// Entries the build registers and cannot show, which the Sidebar leaves out. Opening one of
    /// them stops the browser.
    pub hidden_entries: &'static [u32],
    /// `SidePanelCoordinator::Toggle(this, key*, trigger)`, or the coordinator's own override of it in
    /// the vtable's slot 5, opens an entry or closes it when it is
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
    /// The slot holds Chromium's older `BrowserViewLayoutImplOld::Layout` in place of
    /// `CalculateProposedLayout`, so the layout hook sets bounds after the browser's layout runs.
    pub old_layout: Option<bool>,
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

const UNCHECKED_TOOLBAR: Toolbar = Toolbar {
    get_widget_slot: 0,
    dev: UNCHECKED_DEV,
    content_window_for_hwnd: ABSENT,
    widget_for_window: ABSENT,
    skip: "",
    toolbar_layout: ABSENT,
    get_color: ABSENT,
    get_color_bytes: &[0],
};

const UNCHECKED_VERTICALTABS: VerticalTabs = VerticalTabs {
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
    split_adds_active: false,
    split_vector_ends_by_pointer: false,
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
    schedule_paint_prologue: &[0],
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
    hidden_entries: &[],
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
    old_layout: None,
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

/// Why a row leaves a feature off though it may record some of its addresses. A feature whose
/// addresses are all missing needs no reason of its own.
pub fn unchecked_reason(build: &Build, feature: &str) -> &'static str {
    match (build.browser, feature) {
        ("Microsoft Edge", "vertical tabs") => "vertical tabs are not checked on Edge yet",
        ("Microsoft Edge", "sidebar") => "the only panel Edge registers has no page behind it, so the Sidebar has nothing to show on Edge",
        ("Samsung Internet", "split view") => {
            "split view is not checked on Samsung Internet: its split call stops inside the tab collection"
        }
        (_, "split view") => "split view is not checked on this browser build",
        (_, "sidebar") => "the Sidebar is not checked on this browser build",
        (_, "vertical tabs") => "vertical tabs are not checked on this browser build",
        (_, "opening addresses") => "opening addresses is not checked on this browser build",
        _ => "tab control is not checked on this browser build",
    }
}

/// The builds Rivet supports, in the order they are tried.
static TABLE: [&Build; 9] = [
    &CHROME_156_74,
    &CHROME_156_80,
    &CHROME,
    &EDGE,
    &CHROME_150,
    &WHALE,
    &BRAVE,
    &BRAVE_63,
    &SAMSUNG,
];

static CURRENT: OnceLock<Option<&'static Build>> = OnceLock::new();

/// The row for the browser module loaded in this process, if any row matches its version.
pub fn current() -> Option<&'static Build> {
    *CURRENT.get_or_init(|| crate::chromium::image::recorded_build())
}

/// The row of the verified build. Only code that already holds a recorded image calls this.
/// Chromium's `chrome::AddTabAt` for a build whose `chrome::NewTab` sends an app window's new tab to
/// another window without asking the window feature question, as its address and prologue.
pub fn add_tab_at_for(build: &Build) -> Option<(usize, &'static [u8])> {
    (build.browser == WHALE.browser && build.version == WHALE.version).then_some(WHALE_ADD_TAB_AT)
}

pub fn get() -> &'static Build {
    #[cfg(test)]
    return &CHROME;
    #[cfg(not(test))]
    current().expect("a recorded build is loaded before its table is read")
}

/// The rows that came as data with this binary, which needed no code change to add.
pub const CHECKED_IN: &str = include_str!("../../rows/table.json");

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
        Loaded {
            builds: in_order(signed.builds, &compiled),
            frames,
            note,
        }
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
mod tests;
