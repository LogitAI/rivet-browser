//! Rows for Brave builds.

use super::chrome::DEV_CHROME_156;
use super::*;

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

/// Brave 156.1.98.62 (Chromium 156.0.8078.74). Brave publishes its symbols, so this row is filled from
/// them. Only Chromium's functions are recorded: the frame paint, top inset, page layout, side panel
/// toggle and page border of this build are left off. Chromium's colour ids
/// after the frame ones are shifted, so `caption_ink_ids` are read from this build's
/// `PaintButtonContents`.
pub static BRAVE: Build = Build {
    omnibox: None,
    app_window: None,
    browser: "Brave",
    module: "chrome.dll",
    version: [156, 1, 98, 62],
    toolbar: Toolbar {
        get_widget_slot: 11,
        dev: DEV_BRAVE_156,
        get_color: 0x06A9A890, // ?GetColor@ColorProvider@ui@@QEBAIH@Z
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
            0x89, 0xD7,
        ],
        content_window_for_hwnd: 0x02EE93A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02EEBD70, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "",
        toolbar_layout: ABSENT,
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x11066E60, // the vtable of tabs::VerticalTabStripStateControllerImpl, at ShouldDisplayVerticalTabs
        should_display: 0x039AC440, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x00365C40, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x03BF81A0,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0B4B7890, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x02916550, // ?UpdateCollapseActionItem@VerticalTabStripStateControllerImpl@tabs@@AEAAXXZ
        destructor: 0x02A3E050, // the scalar deleting destructor, two words before ShouldDisplayVerticalTabs
        destructor_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48,
        ],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x1112C600, // the browser view vtable
        region_preferred_size: 0x0CF05490, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x11131130, // slot 74 of ??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@
        invalidate_layout: 0x05CB4A60,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        region_minimum_size: 0x0CF05400, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0xC7,
            0x02, 0x00, 0x00, 0x00, 0x00,
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
        gurl_empty: 0x06A5E800,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x0515D0D0,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x0857FB20, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui@@_N@Z
        open_url_params_drop: 0x01FC71B0, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x0972ED60, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@...@Z
        signatures: &[
            (0x02EE93A0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CF09280, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (
                0x0609D7A0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x06A5E800, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x0515D0D0, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x0857FB20, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x01FC71B0, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (
                0x0972ED60,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
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
        tab_count: 0x039AD7B0,     // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x00B0DDC0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0B4DAF50, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0B4DDC20,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x01029360, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@23@@__Cr@std@@XZ
        new_tab: 0x0B3DA400, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x06B1CB70, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00B27C70,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
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
            (
                0x0B3DA400,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x06B1CB70, &[0x48, 0x63, 0x81, 0xD4, 0x02, 0x00, 0x00]),
            (0x00B27C70, &[0x48, 0x8B, 0x81, 0xB0, 0x02, 0x00, 0x00]),
        ],
        ..UNCHECKED_TABS
    },
    split: Split {
        supported: false,
        add_to_new_split: 0x0B4E1CF0, // ?AddToNewSplit@TabStripModel
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x0B4E3870,        // ?RemoveSplit@TabStripModel
        get_split_for_tab: 0x02E99E30,   // ?GetSplitForTab@TabStripModel
        allocate: 0x06FA7DE0,            // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x03A01B60, // ?SetRoundedCorners@ContentsContainerView
        set_rounded_corners_prologue: &[
            0xF3, 0x0F, 0x10, 0x02, 0x0F, 0x2E, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
        ],
        set_rounded_corners_changed: 0x03A01BA0,
        set_rounded_corners_parity: 0x03A01B6D,
        update_border: ABSENT,
        update_border_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x60, 0x0F, 0x29, 0x74, 0x24,
            0x50,
        ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        outline_member: 0x448,
        calculate_layout: 0x0220F6B0, // ?CalculateProposedLayout@MultiContentsView
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x068CD5C0, // ?SchedulePaint@View@views
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        web_contents_focused: 0x03A03C60, // MultiContentsView::OnWebContentsFocused, found by its bytes beside the class's other functions
        web_contents_focused_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x98, 0x03, 0x00, 0x00,
        ],
        container_layout: 0x01F96F00, // ?CalculateProposedLayout@ContentsContainerView
        container_delegate_offset: 0x320,
        mini_toolbar_member: 0x440,
        flex_layout: 0x067B5A30, // ?CalculateProposedLayout@FlexLayout@views
        flex_layout_host: 0x38,
        location_bar_vtable: ABSENT,
        view_size: 0x1D0,
        signatures: &[
            (
                0x0B4E1CF0,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x0B4E3870,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58],
            ),
            (
                0x02E99E30,
                &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x45, 0x85, 0xC0, 0x78],
            ),
            (
                0x06FA7DE0,
                &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9],
            ),
        ],
    },
    // Only the browser's layout, which the pages of a split need. The Sidebar stays off.
    side_panel: SidePanel {
        layout_vtable: ABSENT,
        proposed_slot: 16,
        proposed: ABSENT,
        old_layout: None,
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x003E5B50, // ?LayoutImmediately@View@views
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55,
        ],
        ..UNCHECKED_SIDEPANEL
    },
    layout: Layout {
        on_paint_slot: 89,
        on_paint: ABSENT,
        on_paint_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00,
        ],
        get_color_provider: 0x05836160, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x01],
        draw_color: 0x03C29C10, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x111B08C0, // ??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@
        corner_paint: 0x0B762730,  // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_vtable: 0x111E7580, // ??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@
        caption_size_slot: 74,
        caption_size: 0x03A4ABD0, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x02EBEF10, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        caption_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        top_inset_slot: 127,
        top_inset: ABSENT,
        layout_slot: 23,
        layout: 0x039F55C0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        layout_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40],
        set_bounds_rect: 0x06008490, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x1112C600, // the browser view vtable
        frame_vtable: ABSENT,
        layout_immediately: 0x003E5B50, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE,
        ],
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
    omnibox: Some(super::omnibox_rows::OMNIBOX_BRAVE_63),
    app_window: Some(AppWindow {
        feature_impl: Some(0x00107B60),
        feature_impl_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x8B, 0x41, 0x10, 0x48, 0x83, 0xF8, 0x05,
        ],
        browser_type: 0x10,
        detach: Some(0x0B492AC0), // Chromium, found statically by name in the public symbols and unverified
        detach_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6, 0x48, 0x8B, 0x05, 0x6F,
            0xC5, 0xCC,
        ],
        insert: Some(0x0B4925A0), // Chromium, found statically by name in the public symbols and unverified
        insert_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83,
            0xEC, 0x68,
        ],
        view_parent: 0x198,
        view_children: 0x1A0,
        operator_new: 0x6F4DE00,
        operator_new_prologue: &[
            0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xE8, 0xD2, 0xCB, 0x22, 0xFE,
            0x48, 0x85,
        ],
        web_view_ctor: 0x222A9B0,
        web_view_ctor_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x7F,
            0x46, 0xF3,
        ],
        web_view_size: 0x528,
        browser_context: Some(0x06AC7B40),
        browser_context_prologue: &[
            0x48, 0x8B, 0x81, 0x90, 0x02, 0x00, 0x00, 0xC3, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC,
        ],
        browser_context_offset: None,
        add_child: 0x6651DF0,
        add_child_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0x88,
        ],
        set_contents: 0x27540A0,
        set_contents_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8,
            0xAE, 0x5E,
        ],
        set_bounds: Some(0x05EE66C0),
        set_bounds_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xE8,
        ],
        focus_behavior: Some(0x0085D450),
        focus_behavior_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
            0xDE, 0x1B,
        ],
        request_focus: Some(0x027542B0),
        request_focus_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x01, 0x48, 0x8B,
            0x40, 0x50,
        ],
        request_focus_slot: None,
        destructor_slot: 0x9,
        bounds_changed_slot: Some(0x0000004B),
        window_features: None,
    }),
    browser: "Brave",
    module: "chrome.dll",
    version: [156, 1, 98, 63],
    toolbar: Toolbar {
        get_widget_slot: 11,
        dev: DEV_BRAVE_156,
        get_color: 0x06A5CB00, // ?GetColor@ColorProvider@ui@@QEBAIH@Z
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
            0x89, 0xD7,
        ],
        content_window_for_hwnd: 0x02F353F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02F37C00, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "",
        toolbar_layout: 0x01378F50, // ?Layout@ToolbarView, found statically and unverified
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x11024BD8, // the vtable of tabs::VerticalTabStripStateControllerImpl, at ShouldDisplayVerticalTabs
        should_display: 0x039C3900, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x00227A40, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x03C06C50,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0B4738F0, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x029F0BE0, // ?UpdateCollapseActionItem@VerticalTabStripStateControllerImpl@tabs@@AEAAXXZ
        destructor: 0x02B00B60, // the scalar deleting destructor, two words before ShouldDisplayVerticalTabs
        destructor_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48,
        ],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x110EA540, // the browser view vtable
        region_preferred_size: 0x0CEC7C70, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x110EF070, // slot 74 of ??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@
        invalidate_layout: 0x05A276E0,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        region_minimum_size: 0x0CEC7BE0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0xC7,
            0x02, 0x00, 0x00, 0x00, 0x00,
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
        gurl_empty: 0x06A10280,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x0513AB00,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x08537500, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui@@_N@Z
        open_url_params_drop: 0x020CAE20, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x096EA220, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@...@Z
        signatures: &[
            (0x02F353F0, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0CECB9F0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (
                0x05C641F0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x06A10280, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x0513AB00, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x08537500, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x020CAE20, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (
                0x096EA220,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
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
        tab_count: 0x039C4B40,     // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x00526C00, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        activate_tab: 0x0B496FB0, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0B499C40,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00AE2AE0, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator@_S@23@@__Cr@std@@XZ
        new_tab: 0x0B3962E0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x06AC7B50, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00A77600,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
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
            (
                0x0B3962E0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x06AC7B50, &[0x48, 0x63, 0x81, 0xD4, 0x02, 0x00, 0x00]),
            (0x00A77600, &[0x48, 0x8B, 0x81, 0xB0, 0x02, 0x00, 0x00]),
        ],
        ..UNCHECKED_TABS
    },
    split: Split {
        supported: false,
        add_to_new_split: 0x0B49DD10, // ?AddToNewSplit@TabStripModel
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x0B49F890,        // ?RemoveSplit@TabStripModel
        get_split_for_tab: 0x02EED4F0,   // ?GetSplitForTab@TabStripModel
        allocate: 0x06F4DE00,            // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x03A17F90, // ?SetRoundedCorners@ContentsContainerView
        set_rounded_corners_prologue: &[
            0xF3, 0x0F, 0x10, 0x02, 0x0F, 0x2E, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
        ],
        set_rounded_corners_changed: 0x03A17FD0,
        set_rounded_corners_parity: 0x03A17F9D,
        update_border: ABSENT,
        update_border_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x60, 0x0F, 0x29, 0x74, 0x24,
            0x50,
        ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        outline_member: 0x448,
        calculate_layout: 0x0230EB00, // ?CalculateProposedLayout@MultiContentsView
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x06736040, // ?SchedulePaint@View@views
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        web_contents_focused: 0x03A1A090, // MultiContentsView::OnWebContentsFocused, found by its bytes beside the class's other functions
        web_contents_focused_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B, 0x81, 0x98, 0x03, 0x00, 0x00,
        ],
        container_layout: 0x0208C360, // ?CalculateProposedLayout@ContentsContainerView
        container_delegate_offset: 0x320,
        mini_toolbar_member: 0x440,
        flex_layout: 0x06746FC0, // ?CalculateProposedLayout@FlexLayout@views
        flex_layout_host: 0x38,
        location_bar_vtable: 0x11133840, // ??_7LocationBarView@@6BLayerDelegate@ui@@@, found statically and unverified
        view_size: 0x1D0,
        signatures: &[
            (
                0x0B49DD10,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x0B49F890,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58],
            ),
            (
                0x02EED4F0,
                &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x45, 0x85, 0xC0, 0x78],
            ),
            (
                0x06F4DE00,
                &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9],
            ),
        ],
    },
    side_panel: SidePanel {
        supported: false,
        side_panel_vtable: 0x1110CF80, // ??_7SidePanel@@6BLayerDelegate@ui@@@
        browser_offset: 0x560,
        features_offset: 0xd0,
        features_coordinator: 0x338,
        browser_view_view_vtable: 0x110EA540, // the browser view vtable
        coordinator_vtable: 0x110DA588, // ??_7SidePanelCoordinator@@6BSidePanelUIBase@@@, found statically and unverified
        show: 0x0AC1BC60, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@std@@_N@Z
        show_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x44, 0x89, 0xCB,
        ],
        entry_id_to_string: 0x01EBF1A0, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@std@@W4SidePanelEntryId@@@Z
        entry_id_to_string_prologue: &[
            0x83, 0xFA, 0x1B, 0x0F, 0x87, 0x45, 0x06, 0x00, 0x00, 0x48, 0x89, 0xC8, 0x89, 0xD1,
        ],
        extension_entry: 0x1B,
        entry_count: 0x1C,
        hidden_entries: &[],
        // Slot 5 of the coordinator vtable.
        toggle: 0x03760A50, // ?Toggle@SidePanelCoordinator, found statically and unverified
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x68, 0x48,
            0x89, 0xCF,
        ],
        entry_showing: 0x0375E960, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8B, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02E7BED0, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xC1, 0xA8, 0x00, 0x00, 0x00],
        get_color_provider: 0x034779D0, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[
            0x48, 0x8B, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8B, 0x01, 0x48, 0x8B, 0x40, 0x08,
        ],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x06B03030, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@@__Cr@std@@AEBVColorProvider@ui@@AEBVGURL@@_N2@Z
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: 0x11056600, // ??_7BrowserViewTabbedLayoutImpl@@6B@, found statically and unverified
        proposed_slot: 16,
        proposed: 0x038CA250, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl, slot 16, found statically and unverified
        old_layout: None,
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x05EE76D0, // ?LayoutImmediately@View@views
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55,
        ],
        event_handler_for_point: 0x018960D0, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        on_paint: 0x01701270, // ?OnPaint@BrowserFrameViewWin, slot 89, found statically and unverified
        on_paint_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
        ],
        get_color_provider: 0x06735E30, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78, 0x48],
        draw_color: 0x03C35B00, // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        draw_color_prologue: &[0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x1116E800, // ??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@
        corner_paint: 0x0B71E3A0,  // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_vtable: 0x111A54C0, // ??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@
        caption_size_slot: 74,
        caption_size: 0x03A60970, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x020F21D0, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        caption_paint_prologue: &[0xE9, 0x0B, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC],
        top_inset_slot: 127,
        top_inset: 0x02465480, // ?GetTopInset@BrowserFrameViewWin, slot 127, found statically and unverified
        layout_slot: 23,
        layout: 0x03A0BCD0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        layout_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40],
        set_bounds_rect: 0x05EE66C0, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x110EA540, // the browser view vtable
        frame_vtable: 0x110ED840, // ??_7BrowserFrameViewWin@@6BLayerDelegate@ui@@@, found statically and unverified
        layout_immediately: 0x05EE76D0, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55,
        ],
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
        top_inset_adjust: 0, // Chromium's own GetTopInset, as on Chrome
        provider_verified: true,
    },
};
