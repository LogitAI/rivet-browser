//! Rows for Microsoft Edge builds.

use super::chrome::DEV_CHROME_156;
use super::*;

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
    font_new_prologue: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2,
    ],
    font_list_new: 0x02B0_FDCE, // ??0FontList@gfx@@QEAA@AEBVFont@1@@Z
    font_list_new_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE,
    ],
    ..DEV_CHROME_156
};

pub static EDGE: Build = Build {
    omnibox: None,
    app_window: None,
    browser: "Microsoft Edge",
    module: "msedge.dll",
    version: [156, 0, 4314, 50],
    toolbar: Toolbar {
        get_widget_slot: 10,
        dev: DEV_EDGE_156,
        get_color: 0x0100309E,
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89,
            0xD7, 0x48,
        ],
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x028C8D78, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "updateicon anchor visible", // the address field, icons and bubble anchors are not recorded for Edge yet
        toolbar_layout: ABSENT,
    },
    vertical: VerticalTabs {
        supported: false,                     // not checked on Edge yet
        slot: 0x1172CF10, // VerticalTabStripStateControllerImpl vtable, ShouldDisplayVerticalTabs slot
        should_display: 0x0A3D40D0, // ?loaded@TopSitesImpl@history@@UEBA_NXZ
        browser_view_for_browser: 0x010CDA9A, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x01078A59,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0A3D3176, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x0,     // none in Edge
        destructor: 0x0A3D4830, // ??_GVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8,
        ],
        browser_offset: 0x1B8, // Chrome value, not checked on Edge
        mode_offset: 0x1E8,    // Chrome value, not checked on Edge
        width_offset: 0x1CC,   // Chrome value, not checked on Edge
        browser_view_view_vtable: 0x11937B18, // BrowserView View vtable
        region_preferred_size: 0x0CAE1ED0, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@
        region_preferred_size_slot: 0x1181EC28, // VerticalTabStripRegionView vtable, CalculatePreferredSize slot 86
        invalidate_layout: 0x00FFA880,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38,
        ],
        region_minimum_size: 0x0CAE1E50, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7,
            0x02, 0, 0, 0, 0,
        ], // Chrome value, not checked on Edge
        should_draw_tab_strip: 0x0100A550, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
        should_draw_tab_strip_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0x18, 0x06, 0x00,
            0x00,
        ],
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
        gurl_empty: 0x010FF97C,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x00FDECAA,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x066E261C, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTra
        open_url_params_drop: 0x066E2C84, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x07A598E0, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallbac
        signatures: &[
            (0x026A4DC6, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x010CDAE8, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (
                0x01016852,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x010FF97C, &[0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0]),
            (0x00FDECAA, &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x28]),
            (0x066E261C, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x066E2C84, &[0x56, 0x57, 0x48, 0x83, 0xEC]),
            (
                0x07A598E0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x0CAE5550, &[0x48, 0x8B, 0x81, 0x18, 0x06, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true, // tab list, switching, closing and new tabs checked on Edge 156.0.4314.50; groups are not
        content_window_for_hwnd: 0x026A4DC6, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x010CDAE8, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0CAE5550, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        browser_in_view: 0x618,          // where the browser view keeps the browser
        groups: false,                   // the group model's layout is not checked on Edge
        get_tab_model: 0x055E0CE0,
        tab_count: 0x010CBBCE, // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0105E38E,    // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x0A404C92, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A4080EA,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x01119470, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?
        new_tab_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0x18, 0x01, 0x00, 0x00, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x71,
            0xD7, 0xF3, 0x09,
        ],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab_by_open_url: None,
        new_tab: 0x0A2B68B0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x0115B9E0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x01056C50,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02698560, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x0A3F1DFE, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x0A3F1E94, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: ABSENT,     // not found in Edge
        allocate: 0x036CDF90,     // ??2@YAPEAX_K@Z
        add_to_new_group: ABSENT, // not found in Edge
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9],
        add_to_new_group_prologue: &[0x00], // function not found in Edge
        signatures: &[
            (
                0x02698560,
                &[
                    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x0F,
                ],
            ),
            (
                0x0A3F1DFE,
                &[
                    0x48, 0x8B, 0x41, 0x08, 0x48, 0x85, 0xC0, 0x74, 0x3F, 0x48, 0x83, 0xC1,
                ],
            ),
            (
                0x0A3F1E94,
                &[
                    0x48, 0x8B, 0x41, 0x08, 0x48, 0x85, 0xC0, 0x0F, 0x84, 0x80, 0x00, 0x00,
                ],
            ),
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
        supported: true,
        add_to_new_split: 0x0A40C036, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x0A40E340, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x02BFCE34, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x036CDF90,          // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x02E268A2, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0xF3, 0x0F, 0x10, 0x02,
        ],
        set_rounded_corners_changed: ABSENT, // Edge's start has no short jump, so it takes a plain detour
        set_rounded_corners_parity: ABSENT,
        update_border: 0x02E26912, // ?UpdateBorderAndOverlay@ContentsContainerView@@QEAAX_N00@Z
        update_border_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x50, 0x89, 0xD5,
        ],
        set_border_slot: 0xf8 / 8, // ?SetBorder@View@views in the ContentsContainerView vtable
        set_visible_slot: 0x98 / 8, // ?SetVisible@View@views in the same vtable
        outline_member: 0x498,
        calculate_layout: 0x02C01AD0, // ?CalculateProposedLayout@MultiContentsView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x340,
        split_insets_member: 0x45c,
        containers_member: 0x368,
        container_count_member: 0x370,
        active_member: 0x420, // read by ?GetActiveContentsContainerView@MultiContentsView
        schedule_paint: 0x010038F0, // ?SchedulePaint@View@views@@QEAAXXZ
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        // ?UpdateContentsBorderAndOverlay@MultiContentsView, which also runs when the focused page changes.
        web_contents_focused: 0x02E267AA,
        web_contents_focused_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        container_layout: 0x0A668A80, // ?CalculateProposedLayout@ContentsContainerView
        container_delegate_offset: 0x338,
        mini_toolbar_member: 0x490,
        flex_layout: 0x0100A8E0, // ?CalculateProposedLayout@FlexLayout@views
        flex_layout_host: 0x38,
        location_bar_vtable: 0x11874078, // ??_7LocationBarView@@6BLayerDelegate@ui@@@
        view_size: 0x1D0,                // Chrome value, not checked on Edge
        signatures: &[
            (
                0x0A40C036,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x0A40E340,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x48],
            ),
            (
                0x02BFCE34,
                &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x45, 0x85, 0xC0, 0x0F],
            ),
            (
                0x036CDF90,
                &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9],
            ),
        ],
    },
    side_panel: SidePanel {
        supported: false, // no entry of Edge's side panel opens: see `unchecked_reason`
        side_panel_vtable: 0x1180D4B8, // ??_7SidePanel@@6BLayerDelegate@ui@@@
        browser_offset: 0x618,
        features_offset: 0xd8,
        features_coordinator: 0x2c0,
        browser_view_view_vtable: 0x11937B18, // ??_7BrowserView@@6BLayerDelegate@ui@@@
        coordinator_vtable: 0x11797F78,       // ??_7SidePanelCoordinator@@6BSidePanelUIBase@@@
        show: 0x09402420, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@
        show_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x44, 0x89, 0xCB,
        ],
        entry_id_to_string: 0x02971720, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@
        entry_id_to_string_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x8B, 0x05, 0x14, 0x29, 0x88, 0x11,
        ],
        extension_entry: 0x18,
        entry_count: 0x19,
        hidden_entries: &[0], // the Reading list: it is registered, and Edge has no page behind it
        toggle: 0x09351EF0, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x60, 0x44, 0x89, 0xC3,
        ],
        entry_showing: 0x094027E0, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8B, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02ABEF42, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xC1, 0xB0, 0x00, 0x00, 0x00],
        get_color_provider: 0x0115D4C0, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[
            0x48, 0x83, 0xEC, 0x28, 0x48, 0x83, 0xE9, 0x80, 0xE8, 0x53, 0xCA, 0xE8, 0xFF, 0x48,
        ],
        source_in_widget: 0x48,          // Chrome value, not checked on Edge
        color_source: 0x98,              // Chrome value, not checked on Edge
        generate_colors_css: 0x08966BAC, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: ABSENT,
        proposed_slot: 4,
        proposed: 0x028C7270, // ?Layout@BrowserViewLayoutImplOld@@UEAAXPEAVView@views@@@Z
        old_layout: Some(true),
        registry_end: None,
        host_view: 0x38,
        page_area: 0,
        contents_container: 0xd8,
        vertical_tabs: ABSENT,
        toolbar: ABSENT, // the pages of a split fit their toolbar in Chrome's layout only
        contents_outline: ABSENT,
        side_panel: 0xb8, // ??_7SidePanel@@6BLayerDelegate@ui@@@
        side_panel_animation: ABSENT,
        layout_immediately: 0x01044270, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
            0xBE, 0xFD, 0x1A, 0x13,
        ],
        event_handler_for_point: 0x0109FB78, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 101,
        provider_verified: true,
        on_paint: 0x02B44110, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x01003060, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x02A3F86A,         // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144, 3406], // 3406 is what BrowserFrameViewWin::PaintTitlebar fills the caption row with
        corner_vtable: 0x118B7ED8,                           // CustomFloatingCorner vtable
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
        caption_container_offset: 0x440, // caption_button_container_, read by CaptionButtonsRegionWidth
        view_bounds_offset: 0x1C8,       // views::View::bounds_, unchanged
        browser_view_view_vtable: 0x11937B18, // BrowserView View vtable
        frame_vtable: 0x11857718,        // the frame view vtable
        layout_immediately: 0x01044270,  // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48,
        ],
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
};
