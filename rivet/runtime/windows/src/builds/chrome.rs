//! Rows for Google Chrome builds.

use super::*;

pub(super) const DEV_CHROME_156: DevMode = DevMode {
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
    font_new_prologue: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2,
    ],
    font_list_new: 0x02F7_27D0, // ??0FontList@gfx@@QEAA@AEBVFont@1@@Z
    font_list_new_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE,
    ],
    ..DEV_CHROME_156
};

pub static CHROME: Build = Build {
    omnibox: None,
    app_window: None,
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [156, 0, 8078, 48],
    toolbar: Toolbar {
        get_widget_slot: 11,
        dev: DEV_CHROME_156,
        get_color: 0x04AA24F0,
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
            0x89, 0xD7,
        ],
        content_window_for_hwnd: 0x00D5_64A0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x0315_C7C0, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "",
        toolbar_layout: ABSENT,
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x0FDA_FE00, // .weak.??_7VerticalTabStripStateControllerImpl@tabs@@6BVerticalTabStripStateController@1@@.default.?R
        should_display: 0x0369_C6E0, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x0034_D3E0, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x036A_A950,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x0A1D_B330, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x020F_3D90, // ??0VerticalTabStripStateController@tabs@@IEAA@AEAVBrowserWindowInterface@@@Z
        destructor: 0x0296_C3F0, // ??_EVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x89, 0xd7, 0x48, 0x89, 0xce, 0xe8,
        ],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x0FE7_BF68, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        region_preferred_size: 0x0BC7_FA80, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x0FF1_7CF8, // .weak.??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$Cd
        invalidate_layout: 0x0411_6480,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        region_minimum_size: 0x0BC7_F9F0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7,
            0x02, 0, 0, 0, 0,
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
        gurl_empty: 0x0588_DF10,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x03E0_01F0,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x072E_8660, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui
        open_url_params_drop: 0x0204_54C0, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x0366_BC50, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@$$A6AXAE
        signatures: &[
            (0x00D5_64A0, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BC8_1E10, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (
                0x042B_C7A0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x0588_DF10, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
            (0x03E0_01F0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x072E_8660, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x0204_54C0, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (
                0x0366_BC50,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x0BC8_2340, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
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
        tab_count: 0x0369_CBE0,     // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0311_22A0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x0311_2220, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x0A20_11B0,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00D7_8440, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        new_tab_prologue: &[],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab_by_open_url: None,
        new_tab: 0x0A0B_8F50, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x058A_48A0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x05CF_8BA0,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x0311_2320, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x0a1f_10d0, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x0a1f_1130, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: 0x0a20_8550, // ?ChangeTabGroupVisuals@TabStripModel@@QEAAXAEBVTabGroupId@tab_groups@@VTabGroupVisualData@3@_N@Z
        allocate: 0x05d3_d6a0,     // ??2@YAPEAX_K@Z
        add_to_new_group: 0x0a20_60b0, // ?AddToNewGroup@TabStripModel@@QEAA?AVTabGroupId@tab_groups@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        add_to_new_group_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
        ],
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
            (0x00D5_64A0, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BC8_1E10, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (0x0BC8_2340, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
            (0x04FB_01F0, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
            (0x0369_CBE0, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
            (0x0311_22A0, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
            (0x0311_2220, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
            (0x0A20_11B0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x00D7_8440, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
            (
                0x0A0B_8F50,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x058A_48A0, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
            (0x05CF_8BA0, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
        ],
    },
    split: Split {
        supported: true,
        add_to_new_split: 0x0A20_5410, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x0A20_7580, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x0311_2370, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x05d3_d6a0,          // ??2@YAPEAX_K@Z
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
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x054A_B3B0, // ?SchedulePaint@View@views@@QEAAXXZ
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        web_contents_focused: 0x0A4C_B140, // ??_EMultiContentsViewDropTargetController@@UEAAPEAXI@Z
        web_contents_focused_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00,
        ],
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
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x0A20_7580,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
            ),
            (
                0x0311_2370,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
            ),
            (
                0x05d3_d6a0,
                &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
            ),
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
        hidden_entries: &[],
        toggle: 0x0997_F450, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60,
        ],
        entry_showing: 0x0369_1A10, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x030B_E3C0, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00],
        get_color_provider: 0x0366_DB00, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[
            0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08,
        ],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x058C_6090, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@V?$alloc
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: 0x0FDB_0278, // .weak.??_7BrowserViewTabbedLayoutImpl@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        proposed_slot: 16,
        proposed: 0x0104_8AB0, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLayoutImpl@@A
        old_layout: None,
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x04BE_C780, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        event_handler_for_point: 0x007C_6530, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x011B_5610, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x04AA_27D0, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x036B_AD80,         // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x0FEC_AFC8, // .weak.??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        corner_paint: 0x0A4C_73A0,  // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
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
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
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
};

/// Google Chrome 156.0.8078.74, filled from that build's own public symbols and instruction bytes.
pub static CHROME_156_74: Build = Build {
    omnibox: Some(super::omnibox_rows::OMNIBOX_CHROME_156_74),
    app_window: Some(AppWindow {
        feature_impl: Some(0x001BE6D0),
        feature_impl_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x8B, 0x41, 0x10, 0x85, 0xC0,
        ],
        browser_type: 0x10,
        detach: Some(0x09FB8750),
        detach_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6, 0x48, 0x8B, 0x05, 0xDF,
            0xC8, 0xA2,
        ],
        insert: Some(0x09FB8240),
        insert_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83,
            0xEC, 0x68,
        ],
        view_parent: 0x198,
        view_children: 0x1A0,
        operator_new: 0x5A1E310,
        operator_new_prologue: &[
            0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xE8, 0x12, 0xD5, 0x21, 0xFE,
            0x48, 0x85,
        ],
        web_view_ctor: 0x1D7D840,
        web_view_ctor_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xEF,
            0x77, 0xC6,
        ],
        web_view_size: 0x528,
        browser_context: Some(0x055938A0),
        browser_context_prologue: &[
            0x48, 0x8B, 0x81, 0x90, 0x02, 0x00, 0x00, 0xC3, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC,
        ],
        browser_context_offset: None,
        add_child: 0x54C1F40,
        add_child_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0x88,
        ],
        set_contents: 0x22A9840,
        set_contents_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8,
            0x3E, 0x47,
        ],
        set_bounds: Some(0x04A52AF0),
        set_bounds_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xE8,
        ],
        focus_behavior: Some(0x00F32240),
        focus_behavior_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
            0xEE, 0x2D,
        ],
        request_focus: Some(0x026F9A70),
        request_focus_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x01, 0x48, 0x8B,
            0x40, 0x50,
        ],
        request_focus_slot: None,
        destructor_slot: 0x9,
        bounds_changed_slot: None,
        window_features: None,
    }),
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [156, 0, 8078, 74],
    toolbar: Toolbar {
        get_widget_slot: 11,
        dev: DEV_CHROME_156_74,
        get_color: 0x0535_9A80,
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
            0x89, 0xD7,
        ],
        content_window_for_hwnd: 0x02F4_6B00, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02F4_8CE0, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "",
        toolbar_layout: 0x011C_98D0, // ?Layout@ToolbarView@@UEAAXVPassKey@@@Z
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x0FC2_23A8, // .weak.??_7VerticalTabStripStateControllerImpl@tabs@@6BVerticalTabStripStateController@1@@.default.?R
        should_display: 0x0345_2850, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x002E_75B0, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x0346_0F20,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x09F9_9710, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x01DB_9180, // ??0VerticalTabStripStateController@tabs@@IEAA@AEAVBrowserWindowInterface@@@Z
        destructor: 0x02AD_3050, // ??_EVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48,
        ],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x0FCE_E270, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        region_preferred_size: 0x0BAA_5650, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x0FD8_A000, // .weak.??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$Cd
        invalidate_layout: 0x046F_2F30,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        region_minimum_size: 0x0BAA_55C0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7,
            0x02, 0, 0, 0, 0,
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
        gurl_empty: 0x054D_1240,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x03BA_A460,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x0700_9CC0, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui
        open_url_params_drop: 0x020C_7F60, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x081D_4FC0, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@$$A6AXAE
        signatures: &[
            (0x02F4_6B00, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BAA_79F0, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (
                0x04AA_5180,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x054D_1240, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
            (0x03BA_A460, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x0700_9CC0, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x020C_7F60, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (
                0x081D_4FC0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x0BAA_7F20, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
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
        tab_count: 0x0345_2D40,     // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x009C_51D0, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x09FB_C5F0, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x09FB_F9D0,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x0106_B960, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        new_tab_prologue: &[],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab_by_open_url: None,
        new_tab: 0x09E7_61A0, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x0559_38B0, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00B6_3BC0,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02EF_9D70, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x09FA_F550, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x09FA_F5B0, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: 0x09FC_6D90, // ?ChangeTabGroupVisuals@TabStripModel@@QEAAXAEBVTabGroupId@tab_groups@@VTabGroupVisualData@3@_N@Z
        allocate: 0x05A1_E310,     // ??2@YAPEAX_K@Z
        add_to_new_group: 0x09FC_48F0, // ?AddToNewGroup@TabStripModel@@QEAA?AVTabGroupId@tab_groups@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        add_to_new_group_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
        ],
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
            (0x02F4_6B00, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BAA_79F0, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (0x0BAA_7F20, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
            (0x04E2_60C0, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
            (0x0345_2D40, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
            (0x009C_51D0, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
            (0x09FB_C5F0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
            (0x09FB_F9D0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x0106_B960, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
            (
                0x09E7_61A0,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x0559_38B0, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
            (0x00B6_3BC0, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
        ],
    },
    split: Split {
        supported: true,
        add_to_new_split: 0x09FC_3C40, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x09FC_5DC0, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x02EF_9DC0, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x05A1_E310,          // ??2@YAPEAX_K@Z
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
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x0480_FBC0, // ?SchedulePaint@View@views@@QEAAXXZ
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        web_contents_focused: 0x0A28_A190, // ??_EMultiContentsViewDropTargetController@@UEAAPEAXI@Z
        web_contents_focused_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00,
        ],
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
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x09FC_5DC0,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
            ),
            (
                0x02EF_9DC0,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
            ),
            (
                0x05A1_E310,
                &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
            ),
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
        hidden_entries: &[],
        toggle: 0x0971_D2C0, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60,
        ],
        entry_showing: 0x0344_74A0, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02E9_FDA0, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00],
        get_color_provider: 0x0342_7870, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[
            0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08,
        ],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x055D_5D40, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@V?$alloc
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: 0x0FC2_2820, // .weak.??_7BrowserViewTabbedLayoutImpl@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        proposed_slot: 16,
        proposed: 0x012B_2170, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLayoutImpl@@A
        old_layout: None,
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x003E_BA00, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48,
        ],
        event_handler_for_point: 0x00BA_AC10, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x0139_5BD0, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x042A_1D90, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x0347_1E40,         // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x0FD3_D2D0, // .weak.??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        corner_paint: 0x0A28_63E0,  // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
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
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE, 0x48,
        ],
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
};

/// Chrome 156.0.8078.80's font functions, found by name in its public symbols and unverified.
const DEV_CHROME_156_80: DevMode = DevMode {
    font_new: 0x02FC_50E0, // ??0Font@gfx@@QEAA@AEBV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@std@@H@Z
    font_new_prologue: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x89, 0xD1, 0x44, 0x89, 0xC2,
    ],
    font_list_new: 0x02FC_4DD0, // ??0FontList@gfx@@QEAA@AEBVFont@1@@Z
    font_list_new_prologue: &[
        0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCE,
    ],
    ..DEV_CHROME_156
};

/// Google Chrome 156.0.8078.80, ported by name from the 156.0.8078.74 row with this build's own public
/// symbols, and every recorded first byte read from the installed module. Every address here was
/// found statically and is unverified: no window of this build has been driven yet, so the launcher
/// does not open it.
pub static CHROME_156_80: Build = Build {
    omnibox: Some(super::omnibox_rows::OMNIBOX_CHROME_156_80),
    app_window: Some(AppWindow {
        feature_impl: Some(0x001E54E0),
        feature_impl_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x8B, 0x41, 0x10, 0x85, 0xC0,
        ],
        browser_type: 0x10,
        detach: Some(0x09FADA10),
        detach_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6, 0x48, 0x8B, 0x05, 0x1F,
            0x56, 0xA3,
        ],
        insert: Some(0x09FAD500),
        insert_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83,
            0xEC, 0x68,
        ],
        view_parent: 0x198,
        view_children: 0x1A0,
        operator_new: 0x59FC170,
        operator_new_prologue: &[
            0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xE8, 0xB2, 0x85, 0x20, 0xFE,
            0x48, 0x85,
        ],
        web_view_ctor: 0xBA86C0,
        web_view_ctor_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x6F,
            0xA9, 0xE3,
        ],
        web_view_size: 0x528,
        browser_context: Some(0x05575630),
        browser_context_prologue: &[
            0x48, 0x8B, 0x81, 0x90, 0x02, 0x00, 0x00, 0xC3, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC,
        ],
        browser_context_offset: None,
        add_child: 0x549D9D0,
        add_child_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0x88,
        ],
        set_contents: 0x1148B60,
        set_contents_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8,
            0x5E, 0x6B,
        ],
        set_bounds: Some(0x04970300),
        set_bounds_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xE8,
        ],
        focus_behavior: Some(0x00F90A70),
        focus_behavior_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
            0xBE, 0x25,
        ],
        request_focus: Some(0x027F0F40),
        request_focus_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x01, 0x48, 0x8B,
            0x40, 0x50,
        ],
        request_focus_slot: None,
        destructor_slot: 0x9,
        bounds_changed_slot: Some(0x4B),
        window_features: None,
    }),
    browser: "Google Chrome",
    module: "chrome.dll",
    version: [156, 0, 8078, 80],
    toolbar: Toolbar {
        get_widget_slot: 11,
        dev: DEV_CHROME_156_80,
        get_color: 0x052B_06D0,
        get_color_bytes: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20,
            0x89, 0xD7,
        ],
        content_window_for_hwnd: 0x02F9_C1F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        widget_for_window: 0x02F9_E210, // ?GetWidgetForNativeView@Widget@views@@SAPEAV12@PEAVWindow@aura@@@Z
        skip: "",
        toolbar_layout: 0x0130_94D0, // ?Layout@ToolbarView@@UEAAXVPassKey@@@Z
    },
    vertical: VerticalTabs {
        supported: true,
        slot: 0x0FC1_D120, // .weak.??_7VerticalTabStripStateControllerImpl@tabs@@6BVerticalTabStripStateController@1@@.default.?R
        should_display: 0x0348_10D0, // ?ShouldDisplayVerticalTabs@VerticalTabStripStateControllerImpl@tabs@@UEBA_NXZ
        browser_view_for_browser: 0x0021_19B0, // ?GetBrowserViewForBrowser@BrowserView@@SAPEAV1@PEBVBrowserWindowInterface@@@Z
        hwnd_for_view: 0x0348_D7F0,            // ?HWNDForView@views@@YAPEAUHWND__@@PEBVView@1@@Z
        notify_mode_changed: 0x09F8_E9D0, // ?NotifyModeChanged@VerticalTabStripStateController@tabs@@IEAAXXZ
        update_collapse_action: 0x0208_FB20, // ??0VerticalTabStripStateController@tabs@@IEAA@AEAVBrowserWindowInterface@@@Z
        destructor: 0x0289_05D0, // ??_EVerticalTabStripStateControllerImpl@tabs@@UEAAPEAXI@Z
        destructor_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x89, 0xD7, 0x48, 0x89, 0xCE, 0xE8,
        ],
        browser_offset: 0x1B8,
        mode_offset: 0x1E8,
        width_offset: 0x1CC,
        browser_view_view_vtable: 0x0FCE_91B0, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        region_preferred_size: 0x0BAA_1270, // ?CalculatePreferredSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        region_preferred_size_slot: 0x0FD8_4F40, // .weak.??_7VerticalTabStripRegionView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$Cd
        invalidate_layout: 0x044A_1090,          // ?InvalidateLayout@View@views@@QEAAX_N@Z
        invalidate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        region_minimum_size: 0x0BAA_11E0, // ?GetMinimumSize@VerticalTabStripRegionView@@UEBA?AVSize@gfx@@XZ
        region_minimum_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x89, 0xd6, 0x48, 0x89, 0xcf, 0x48, 0xc7,
            0x02, 0, 0, 0, 0,
        ],
        should_draw_tab_strip: 0x0500_F4B0, // ?ShouldDrawTabStrip@BrowserView@@QEBA_NXZ
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
        content_window_for_hwnd: 0x02F9_C1F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BAA_3630, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BAA_3B60, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        gurl_from_text: 0x0476_5290, // ??0GURL@@QEAA@V?$basic_string_view@DU?$char_traits@D@__Cr@std@@@__Cr@std@@@Z
        gurl_wide: false,
        gurl_empty: 0x054B_BA70,           // ??0GURL@@QEAA@XZ
        gurl_drop: 0x03BC_5750,            // ??1GURL@@QEAA@XZ
        open_url_params: 0x06FF_87B0, // ??0OpenURLParams@content@@QEAA@AEBVGURL@@AEBUReferrer@1@W4WindowOpenDisposition@@W4PageTransition@ui
        open_url_params_drop: 0x0214_4DD0, // ??1OpenURLParams@content@@QEAA@XZ
        open_url: 0x081C_7A40, // ?OpenURL@WebContentsImpl@content@@UEAAPEAVWebContents@2@AEBUOpenURLParams@2@V?$OnceCallback@$$A6AXAE
        signatures: &[
            (0x02F9_C1F0, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BAA_3630, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (
                0x0476_5290,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
            ),
            (0x054B_BA70, &[0x48, 0x89, 0xc8, 0x0f, 0x57, 0xc0]),
            (0x03BC_5750, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x06FF_87B0, &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53]),
            (0x0214_4DD0, &[0x41, 0x56, 0x56, 0x57, 0x53]),
            (
                0x081C_7A40,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x0BAA_3B60, &[0x48, 0x8B, 0x81, 0x60, 0x05, 0x00, 0x00]),
        ],
    },
    tabs: Tabs {
        supported: true,
        content_window_for_hwnd: 0x02F9_C1F0, // ?GetContentWindowForHWND@DesktopWindowTreeHostWin@views@@SAPEAVWindow@aura@@PEAUHWND__@@@Z
        browser_view_for_window: 0x0BAA_3630, // ?GetBrowserViewForNativeWindow@BrowserView@@SAPEAV1@PEAVWindow@aura@@@Z
        active_web_contents: 0x0BAA_3B60, // ?GetActiveWebContents@BrowserView@@QEAAPEAVWebContents@content@@XZ
        browser_in_view: 0x560,
        groups: true,
        get_tab_model: 0x04BF_5770, // ?Client@WebLocalFrameImpl@blink@@UEBAPEAVWebLocalFrameClient@2@XZ
        tab_count: 0x0348_14B0,     // ?count@TabStripModel@@QEBAHXZ
        tab_at: 0x0045_8950, // ?GetWebContentsAt@TabStripModel@@QEBAPEAVWebContents@content@@H@Z
        tab_object: ABSENT,
        tab_collection: 0,
        tab_contents: 0,
        activate_tab: 0x09FB_18B0, // ?ActivateTabAt@TabStripModel@@QEAAXHUTabStripUserGestureDetails@@@Z
        close_tab: 0x09FB_4C90,    // ?CloseWebContentsAt@TabStripModel@@QEAAXHI@Z
        title: 0x00A4_96C0, // ?GetTitle@WebContentsImpl@content@@UEAAAEBV?$basic_string@_SU?$char_traits@_S@__Cr@std@@V?$allocator
        new_tab_prologue: &[],
        new_tab_address: None,
        new_tab_takes_address: false,
        new_tab_delay_ms: 0,
        new_tab_by_open_url: None,
        new_tab: 0x09E6_B310, // ?NewTab@chrome@@YAAEAVWebContents@content@@PEAVBrowserWindowInterface@@W4NewTabTypes@@@Z
        last_committed_url: 0x0557_5640, // ?GetLastCommittedURL@WebContentsImpl@content@@UEBAAEBVGURL@@XZ
        visible_url: 0x00AC_1480,        // ?GetVisibleURL@WebContentsImpl@content@@UEAAAEBVGURL@@XZ
        get_group: 0x02F5_BD20, // ?GetTabGroupForTab@TabStripModel@@QEBA?AV?$optional@VTabGroupId@tab_groups@@@__Cr@std@@H@Z
        has_group: 0x09FA_4810, // ?ContainsTabGroup@TabGroupModel@@QEBA_NAEBVTabGroupId@tab_groups@@@Z
        lookup_group: 0x09FA_4870, // ?GetTabGroup@TabGroupModel@@QEBAPEAVTabGroup@@AEBVTabGroupId@tab_groups@@@Z
        change_group: 0x09FB_C050, // ?ChangeTabGroupVisuals@TabStripModel@@QEAAXAEBVTabGroupId@tab_groups@@VTabGroupVisualData@3@_N@Z
        allocate: 0x059F_C170,     // ??2@YAPEAX_K@Z
        add_to_new_group: 0x09FB_9BB0, // ?AddToNewGroup@TabStripModel@@QEAA?AVTabGroupId@tab_groups@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        allocate_prologue: &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
        add_to_new_group_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
        ],
        signatures: &[
            (
                0x02F5_BD20,
                &[
                    0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xd6, 0x45, 0x85, 0xc0, 0x78,
                ],
            ),
            (
                0x09FA_4810,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xec, 0x38, 0x49, 0x89, 0xd0, 0x48, 0x8b, 0x05,
                ],
            ),
            (
                0x09FA_4870,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x89, 0xd6,
                ],
            ),
            (
                0x09FB_C050,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (0x02F9_C1F0, &[0x56, 0x48, 0x83, 0xec, 0x20, 0xe8]),
            (0x0BAA_3630, &[0x48, 0x83, 0xec, 0x28, 0xe8]),
            (0x0BAA_3B60, &[0x48, 0x8b, 0x81, 0x60, 0x05, 0, 0]),
            (0x04BF_5770, &[0x48, 0x8b, 0x41, 0x30, 0xc3]),
            (0x0348_14B0, &[0x48, 0x8b, 0x01, 0x8b, 0x40, 0x20, 0xc3]),
            (0x0045_8950, &[0x48, 0x83, 0xec, 0x28, 0x85, 0xd2]),
            (0x09FB_18B0, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x38]),
            (0x09FB_4C90, &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28]),
            (0x00A4_96C0, &[0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81]),
            (
                0x09E6_B310,
                &[0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57],
            ),
            (0x0557_5640, &[0x48, 0x63, 0x81, 0xd4, 0x02, 0, 0]),
            (0x00AC_1480, &[0x48, 0x8b, 0x81, 0xb0, 0x02, 0, 0]),
        ],
    },
    split: Split {
        supported: true,
        add_to_new_split: 0x09FB_8F00, // ?AddToNewSplit@TabStripModel@@QEAA?AVSplitTabId@split_tabs@@V?$vector@HV?$allocator@H@__Cr@std@@@__C
        split_adds_active: false,
        split_vector_ends_by_pointer: false,
        remove_split: 0x09FB_B080, // ?RemoveSplit@TabStripModel@@QEAAXVSplitTabId@split_tabs@@@Z
        get_split_for_tab: 0x02F5_BD70, // ?GetSplitForTab@TabStripModel@@QEBA?AV?$optional@VSplitTabId@split_tabs@@@__Cr@std@@H@Z
        allocate: 0x059F_C170,          // ??2@YAPEAX_K@Z
        set_rounded_corners: 0x0348_39A0, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_prologue: &[
            0xf3, 0x0f, 0x10, 0x02, 0x0f, 0x2e, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
        ],
        set_rounded_corners_changed: 0x0348_39E0, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_rounded_corners_parity: 0x0348_39AD, // ?SetRoundedCorners@ContentsContainerView@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        update_border: 0x020F_3680, // ?UpdateBorderAndOverlay@ContentsContainerView@@QEAAX_N00@Z
        update_border_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x48, 0x48, 0x89, 0xce,
        ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        outline_member: 0x448,
        calculate_layout: 0x0238_A930, // ?CalculateProposedLayout@MultiContentsView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x328,
        split_insets_member: 0x404,
        containers_member: 0x350,
        container_count_member: 0x358,
        active_member: 0x3c8,
        schedule_paint: 0x047A_09D0, // ?SchedulePaint@View@views@@QEAAXXZ
        schedule_paint_prologue: &[0x56, 0x48, 0x83, 0xec, 0x40, 0x48],
        web_contents_focused: 0x0A27_EFE0, // ??_EMultiContentsViewDropTargetController@@UEAAPEAXI@Z
        web_contents_focused_prologue: &[
            0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00,
        ],
        container_layout: 0x0210_99D0, // ?CalculateProposedLayout@ContentsContainerView@@EEBA?AUProposedLayout@views@@AEBVSizeBounds@3@@Z
        container_delegate_offset: 0x320,
        mini_toolbar_member: 0x440,
        flex_layout: 0x051C_9140, // views::FlexLayout::CalculateProposedLayout
        flex_layout_host: 0x38,
        location_bar_vtable: 0x0FCF_3E30, // LocationBarView primary vtable
        view_size: 0x1D0,
        signatures: &[
            (
                0x09FB_8F00,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x09FB_B080,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
            ),
            (
                0x02F5_BD70,
                &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
            ),
            (
                0x059F_C170,
                &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9],
            ),
        ],
    },
    side_panel: SidePanel {
        supported: true,
        side_panel_vtable: 0x0FCD_4EF0, // .weak.??_7SidePanel@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        browser_offset: 0x560,
        features_offset: 0xd0,
        features_coordinator: 0x330,
        browser_view_view_vtable: 0x0FCE_91B0, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        coordinator_vtable: 0x0FCC_6688, // .weak.??_7SidePanelCoordinator@@6BSidePanelUIBase@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        show: 0x0970_5E10, // ?Show@SidePanelUIBase@@UEAAXW4SidePanelEntryId@@V?$optional@W4SidePanelOpenTrigger@@@__Cr@std@@_N@Z
        show_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58],
        entry_id_to_string: 0x01E4_DEC0, // ?SidePanelEntryIdToString@@YA?AV?$basic_string@DU?$char_traits@D@__Cr@std@@V?$allocator@D@23@@__Cr@s
        entry_id_to_string_prologue: &[0x56, 0x48, 0x83, 0xec, 0x30, 0x48, 0x89, 0xce],
        extension_entry: 0x17,
        entry_count: 0x18,
        hidden_entries: &[],
        toggle: 0x0970_E4A0, // ?Toggle@SidePanelCoordinator@@UEAAXVSidePanelEntryKey@@W4SidePanelOpenTrigger@@@Z
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x60,
        ],
        entry_showing: 0x0347_68E0, // ?IsSidePanelEntryShowing@SidePanelUIBase@@UEBA_NAEBVSidePanelEntryKey@@@Z
        entry_showing_prologue: &[0x48, 0x8b, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01],
        registry_from: 0x02F0_F940, // ?From@SidePanelRegistry@@SAPEAV1@PEAVBrowserWindowInterface@@@Z
        registry_from_prologue: &[0x48, 0x81, 0xc1, 0xa8, 0x00, 0x00, 0x00],
        get_color_provider: 0x01F3_BD40, // ?GetColorProvider@WebContentsImpl@content@@EEBAAEBVColorProvider@ui@@XZ
        get_color_provider_prologue: &[
            0x48, 0x8b, 0x89, 0x98, 0x00, 0x00, 0x00, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x08,
        ],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x055A_EFA0, // ?GenerateColorsCss@ThemeSource@@SA?AV?$optional@V?$basic_string@DU?$char_traits@D@__Cr@std@@V?$alloc
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: 0x0FC1_D598, // .weak.??_7BrowserViewTabbedLayoutImpl@@6B@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplate@
        proposed_slot: 16,
        proposed: 0x0141_3500, // ?CalculateProposedLayout@BrowserViewTabbedLayoutImpl@@MEBA?AUProposedLayout@BrowserViewLayoutImpl@@A
        old_layout: None,
        host_view: 0x38,
        page_area: 0x50,
        contents_container: 0xb8,
        vertical_tabs: 0x80,
        toolbar: 0x60,
        contents_outline: 0x58,
        side_panel: 0xc0,
        side_panel_animation: 0xc8,
        layout_immediately: 0x0497_1310, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        event_handler_for_point: 0x0196_7ED0, // ?GetEventHandlerForPoint@View@views@@QEAAPEAV12@AEBVPoint@gfx@@@Z
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true,
        on_paint: 0x014E_BF30, // ?OnPaint@BrowserFrameViewWin@@MEAAXPEAVCanvas@gfx@@@Z
        get_color_provider: 0x047A_07C0, // ?GetColorProvider@View@views@@QEBAPEBVColorProvider@ui@@XZ
        draw_color: 0x0349_CC90,         // ?DrawColor@Canvas@gfx@@QEAAXI@Z
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        corner_vtable: 0x0FD3_8210, // .weak.??_7CustomFloatingCorner@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        corner_paint: 0x0A27_B230,  // ?OnPaint@CustomFloatingCorner@@UEAAXPEAVCanvas@gfx@@@Z
        caption_vtable: 0x0FD6_F810, // .weak.??_7WindowsCaptionButton@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromi
        caption_size_slot: 74,
        caption_size: 0x02F7_9030, // ?CalculatePreferredSize@WindowsCaptionButton@@UEBA?AVSize@gfx@@AEBVSizeBounds@views@@@Z
        caption_paint_slot: 114,
        caption_paint: 0x0216_2380, // ?PaintButtonContents@WindowsCaptionButton@@UEAAXPEAVCanvas@gfx@@@Z
        top_inset_slot: 127,
        top_inset: 0x0198_BB60, // ?GetTopInset@BrowserFrameViewWin@@UEBAH_N@Z
        layout_slot: 23,
        layout: 0x0198_B5C0, // ?Layout@BrowserFrameViewWin@@MEAAXV?$NonCopyablePassKey@VView@views@@@base@@@Z
        set_bounds_rect: 0x0497_0300, // ?SetBoundsRect@View@views@@QEAAXAEBVRect@gfx@@@Z
        caption_container_offset: 0x3C8,
        view_bounds_offset: 0x1C8,
        browser_view_view_vtable: 0x0FCE_91B0, // .weak.??_7BrowserView@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromiseTemplat
        frame_vtable: 0x0FCE_1070, // .weak.??_7BrowserFrameViewWin@@6BLayerDelegate@ui@@@.default.?RejectPromiseOnDestruction@?$CdmPromis
        layout_immediately: 0x0497_1310, // ?LayoutImmediately@View@views@@AEAAXXZ
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        set_layer: 0x00F8_65B0, // ?SetPaintToLayer@View@views@@QEAAXW4LayerType@ui@@@Z
        set_layer_prologue: &[0x56, 0x57, 0x48, 0x83, 0xec, 0x28],
        set_radius: 0x01C4_6BA0, // ?SetRoundedCornerRadius@Layer@ui@@QEAAXAEBVRoundedCornersF@gfx@@@Z
        set_radius_prologue: &[0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x30],
        set_masks: 0x02FA_9360, // ?SetMasksToBounds@Layer@ui@@QEAAX_N@Z
        set_masks_prologue: &[0x48, 0x8b, 0x89, 0x70, 0x01, 0x00, 0x00],
        stack_at_top: 0x0AA9_0760, // ?StackAtTop@Layer@ui@@QEAAXPEAV12@@Z
        stack_at_top_prologue: &[0x41, 0x57, 0x41, 0x56, 0x56, 0x57],
        set_opaque: 0x00BA_1BC0, // ?SetFillsBoundsOpaquely@Layer@ui@@QEAAX_N@Z
        set_opaque_prologue: &[0x56, 0x48, 0x83, 0xec, 0x20],
        on_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        draw_color_prologue: &[0x48, 0x83, 0xec, 0x38, 0x48, 0x8b],
        get_color_provider_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x78],
        corner_paint_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        layout_prologue: &[0x41, 0x56, 0x56, 0x57, 0x55, 0x53],
        set_bounds_rect_prologue: &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54],
        frame_browser_window_offset: 0x340,
        browser_window_in_view: 0x1f0,
        caption_paint_prologue: &[0xE9, 0x0B, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC],
        caption_ink_ids: &[0x66C, 0x66D, 0x66E],
        top_inset_adjust: 0,
    },
};

/// Google Chrome 150.0.7871.187. Only the frame, caption and panel hooks are checked against this
/// build's symbols; every other feature stays off until its addresses are verified.
pub static CHROME_150: Build = Build {
    omnibox: None,
    app_window: None,
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
        layout_immediately_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
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
