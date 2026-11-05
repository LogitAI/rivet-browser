//! Rows for Naver Whale builds.

use super::*;

/// Chromium's `chrome::AddTabAt(Browser*, const GURL&, int index, bool foreground,
/// std::optional<TabGroupId>*, bool, bool)` in Whale 5.39.412.57, found statically and unverified.
/// In this build `chrome::NewTab` sends an app window's new tab to a tabbed window, and `AddTabAt`
/// is the Chromium function that adds it to the app window instead. The prologue ends before the
/// stack cookie load. It is kept out of the row so the signed rows keep their fields.
pub const WHALE_ADD_TAB_AT: (usize, &[u8]) = (
    0x0E93_5CA0,
    &[
        0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x38, 0x05, 0x00, 0x00,
    ],
);

/// Naver Whale 5.39.412.57 (Chromium 150.0.7871.252). Only Chromium's functions are recorded.
pub static WHALE: Build = Build {
    omnibox: Some(super::omnibox_rows::OMNIBOX_WHALE_57),
    app_window: Some(AppWindow {
        // `WindowFeatureController::SupportsWindowFeatureImpl`, found statically and unverified: the
        // function `BrowserView::ShouldDrawTabStrip` asks through the thunk at 0x8104DF0
        // (`xor r8d, r8d; jmp`), next to `CanSupportWindowFeature` at 0x8104E00 (`mov r8b, 1; jmp`).
        // It switches on the browser type at +0x10. The prologue ends with the stack cookie load.
        feature_impl: Some(0x02273790),
        feature_impl_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xA1, 0x68, 0xA4,
            0x11,
        ],
        browser_type: 0x10,
        detach: Some(0x08181E70),
        detach_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xD6, 0x48, 0x8B, 0x05, 0xBF,
            0x81, 0xB3,
        ],
        insert: Some(0x081814D0),
        insert_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x50,
            0x44, 0x89,
        ],
        view_parent: 0x190,
        view_children: 0x198,
        operator_new: 0x3E4AB40,
        operator_new_prologue: &[
            0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xEB, 0x0F, 0x48, 0x8B, 0xCB,
            0xE8, 0x5D,
        ],
        web_view_ctor: 0xE6849C0,
        web_view_ctor_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x30, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x6F,
            0x56, 0x63,
        ],
        web_view_size: 0x538,
        browser_context: None,
        browser_context_prologue: &[],
        browser_context_offset: Some(0x00000268),
        add_child: 0x8F86720,
        add_child_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83,
            0xEC, 0x78,
        ],
        set_contents: 0xE684F30,
        set_contents_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0xE8,
            0x6E, 0x05,
        ],
        set_bounds: Some(0x08F7C700),
        set_bounds_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xB8,
        ],
        focus_behavior: None,
        focus_behavior_prologue: &[],
        request_focus: None,
        request_focus_prologue: &[],
        request_focus_slot: Some(0x00000031),
        destructor_slot: 0x9,
        bounds_changed_slot: Some(0x4B), // WebView::OnBoundsChanged, the same function as Brave 156.1.98.63 slot 0x4B
        // The window feature numbers of this build, read statically from the switch in the feature
        // function and unverified: title bar 1, tab strip 5, toolbar 6. A popup answers yes to 1 and 7
        // only, a normal window to 5, 6 and 7 unless fullscreen, and to 8.
        window_features: Some((1, 5, 6)),
    }),
    browser: "Naver Whale",
    module: "whale.dll",
    version: [5, 39, 412, 57],
    toolbar: Toolbar {
        skip: "toolbar",
        ..UNCHECKED_TOOLBAR
    },
    vertical: VerticalTabs {
        // Chromium's `BrowserView::ShouldDrawTabStrip` in this build. The
        // prologue ends on an instruction boundary and holds no relative operand.
        should_draw_tab_strip: 0x0E6CBE60,
        should_draw_tab_strip_prologue: &[
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x89, 0xC0, 0x05, 0x00,
            0x00,
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
            (
                0x03677F60,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x037537F0,
                &[
                    0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0, 0x0F, 0x11, 0x41, 0x0B, 0x0F, 0x11, 0x01,
                ],
            ),
            (
                0x02C212F0,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70,
                ],
            ),
            (
                0x05357FB0,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xCF,
                ],
            ),
            (
                0x053585F0,
                &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE],
            ),
            (
                0x06775BE0,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xA0, 0x04, 0x00,
                    0x00,
                ],
            ),
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
        // Chromium's `chrome::NewTab(Browser*, NewTabTypes, const GURL&)`, mapped from Chrome 150's by the
        // calls it makes. The prologue ends with the stack cookie load the detour relocates.
        new_tab_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x18, 0x01, 0x00, 0x00, 0x4C, 0x89,
            0xC7, 0x89, 0xD3, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0x75, 0x03, 0x75, 0x11,
        ],
        new_tab_address: Some(0x68),
        new_tab_takes_address: true,
        new_tab_delay_ms: 0,
        new_tab_by_open_url: None,
        new_tab: 0x02569CB0,
        last_committed_url: 0x06762170,
        visible_url: 0x06762120,
        get_group: 0x0818C770,
        signatures: &[
            (0x08F36620, &[0x56, 0x48, 0x83, 0xEC, 0x20, 0xE8]),
            (0x0E6CB8A0, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0E6CC090, &[0x48, 0x8B, 0x81, 0xC0, 0x05, 0x00, 0x00]),
            (
                0x03E24F10,
                &[0x48, 0x8B, 0x81, 0x28, 0x02, 0x00, 0x00, 0xC3],
            ),
            (0x08181300, &[0x48, 0x8B, 0x01, 0x8B, 0x40, 0x18, 0xC3]),
            (
                0x0818A300,
                &[
                    0x48, 0x83, 0xEC, 0x28, 0x85, 0xD2, 0x78, 0x08, 0x48, 0x8B, 0x09, 0x3B,
                ],
            ),
            (
                0x08187360,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
                ],
            ),
            (
                0x0818BE60,
                &[
                    0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x44, 0x89, 0xC6, 0x89, 0xD3,
                ],
            ),
            (
                0x06764D90,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
                ],
            ),
            (
                0x0818C770,
                &[
                    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x78,
                ],
            ),
            (
                0x06762170,
                &[
                    0x48, 0x63, 0x81, 0xAC, 0x02, 0x00, 0x00, 0x48, 0x83, 0xF8, 0xFF, 0x74,
                ],
            ),
            (
                0x06762120,
                &[
                    0x48, 0x83, 0xEC, 0x28, 0x48, 0x81, 0xC1, 0x58, 0x02, 0x00, 0x00, 0xE8,
                ],
            ),
        ],
        ..UNCHECKED_TABS
    },
    // This build carries Chromium's split tabs. Its functions were found from Chrome 150.0.7871.187's
    // symbols by the calls they make and checked in the installed DLL: `AddToNewSplit` is the
    // only caller of `RecordSplitTabCreated`, and `RemoveSplit` is the function that runs the same three
    // calls as Chrome's. The member offsets read the same as in Chrome 150.
    split: Split {
        // Off: the addresses below are right, and the split they make is in the tab model only. The
        // browser shows no second page for it.
        supported: false,
        add_to_new_split: 0x0819_0FB0,
        split_adds_active: true,
        split_vector_ends_by_pointer: true,
        remove_split: 0x0819_3990,
        get_split_for_tab: 0x0818_C800,
        allocate: 0x03E4_AB40,
        // Whale and Chrome 150 build the corners in `UpdateBorderRoundedCorners` with no setter to
        // detour, so the pages keep the browser's corners.
        set_rounded_corners: crate::builds::ABSENT,
        set_rounded_corners_prologue: &[0],
        set_rounded_corners_changed: crate::builds::ABSENT,
        set_rounded_corners_parity: crate::builds::ABSENT,
        update_border: 0x0E68_70C0,
        update_border_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x48, 0x48, 0x89, 0xCE,
        ],
        set_border_slot: 0xf0 / 8,
        set_visible_slot: 0x90 / 8,
        // `UpdateBorderAndOverlay` hides the views at +0x430 and +0x438 when there is no border, and
        // keeps a mini toolbar at +0x440 that may be absent.
        outline_member: 0x438,
        calculate_layout: 0x0E67_4C50,
        calculate_layout_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_delegate_offset: 0x320,
        split_insets_member: 0x3f4,
        containers_member: 0x370,
        container_count_member: 0x378,
        active_member: 0x3b8,
        schedule_paint: 0x08F7_E530,
        schedule_paint_prologue: &[0x48, 0x83, 0xEC, 0x38, 0x48, 0x8B],
        // `UpdateContentsBorderAndOverlay`, which every change of the pages and of the focused one ends in.
        web_contents_focused: 0x0E67_44D0,
        web_contents_focused_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x28,
        ],
        container_layout: 0x0E68_7AA0,
        container_delegate_offset: 0x318,
        mini_toolbar_member: 0x430,
        // The address in the top row does not follow the focused page here, so the flex layout and
        // the location bar are not needed.
        flex_layout: crate::builds::ABSENT,
        flex_layout_host: 0,
        location_bar_vtable: crate::builds::ABSENT,
        view_size: 0,
        signatures: &[
            (
                0x0819_0FB0,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
                ],
            ),
            (
                0x0819_3990,
                &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x48],
            ),
            (
                0x0818_C800,
                &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x45, 0x85, 0xC0, 0x78],
            ),
            (
                0x03E4_AB40,
                &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B],
            ),
        ],
    },
    // Found statically by the shape of Chrome 156's functions and members, and unverified. Whale
    // publishes no symbols. Its entry ids stop at 0x15, the extension entry, and its registry keeps
    // begin and end pointers. The layout's members sit 0x20 below Chrome's from the page area on.
    side_panel: SidePanel {
        supported: true,
        side_panel_vtable: 0x0FAA_BE20,
        browser_offset: 0x5C0,
        features_offset: 0x98,
        features_coordinator: 0x130,
        browser_view_view_vtable: 0x0FAB_7000,
        coordinator_vtable: 0x0F9A_1950,
        show: 0x07C3_DD90,
        show_prologue: &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x58],
        entry_id_to_string: 0x076C_4570,
        entry_id_to_string_prologue: &[0x56, 0x48, 0x83, 0xEC, 0x20, 0x83, 0xFA, 0x16, 0x73, 0x21],
        extension_entry: 0x15,
        entry_count: 0x16,
        hidden_entries: &[],
        toggle: 0x07C4_CCF0,
        toggle_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x60,
        ],
        entry_showing: 0x07C3_E290,
        entry_showing_prologue: &[0x48, 0x8B, 0x49, 0x10, 0x80, 0x79, 0x30, 0x01, 0x75, 0x09],
        registry_from: 0x07C3_D380,
        registry_from_prologue: &[0x48, 0x81, 0xC1, 0xE0, 0x04, 0x00, 0x00],
        get_color_provider: 0x0679_5E10,
        get_color_provider_prologue: &[
            0x48, 0x83, 0xEC, 0x28, 0x48, 0x83, 0xE9, 0x80, 0xE8, 0xE3, 0x19, 0xA5, 0xFD, 0x48,
        ],
        source_in_widget: 0x48,
        color_source: 0x98,
        generate_colors_css: 0x0E95_5CF0,
        generate_colors_css_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        layout_vtable: 0x0F9F_BB38,
        proposed_slot: 15,
        proposed: 0x0252_E780,
        old_layout: None,
        registry_end: Some(true),
        registry_in_coordinator: None,
        registry_entries: None,
        coordinator_browser: None,
        entry_key_size: None,
        extensions_by_toggle: None,
        host_view: 0x20,
        page_area: 0x30,
        contents_container: 0x98,
        vertical_tabs: 0x60,
        toolbar: 0x40,
        contents_outline: 0x38,
        side_panel: 0xA0,
        side_panel_animation: 0xA8,
        layout_immediately: 0x08F7_CD40,
        layout_immediately_prologue: &[
            0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x89, 0xCE,
        ],
        event_handler_for_point: 0x08F8_1F60,
    },
    layout: Layout {
        on_paint_slot: 89,
        provider_verified: true, // the tree walk is checked on 5.39.412.57; `toolbar.get_color` is not mapped, so no colour is created
        // `BrowserFrameViewWin::OnPaint` and `GetTopInset`, slots 89 and 127 of its vtable, found
        // statically and unverified. whale.dll has no class name string, so the vtable was found by
        // its other slots, matched one by one to Chrome 150.0.7871.187's from its public symbols.
        // Both bodies keep Chromium's prologue and member reads; the top inset adds a check of its own.
        on_paint: 0x0253_D0E0,
        on_paint_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
        ],
        get_color_provider: 0x8f819d0,
        get_color_provider_prologue: &[0x48, 0x83, 0xEC, 0x28, 0x48, 0x8B],
        draw_color: 0x9bc45b0,
        draw_color_prologue: &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x38],
        frame_ids: &[0x13C, 0x13D, 0x140, 321, 0x144],
        top_inset_slot: 127,
        top_inset: 0x0E7B_4500,
        frame_vtable: 0x0FB1_7440,
        // Slot 74 of this caption vtable reads members Chromium's `WindowsCaptionButton` does not
        // have, so the caption buttons' size is the browser's own and stays out.
        caption_vtable: 0xfb15d60,
        caption_size_slot: 74,
        caption_size: ABSENT,
        frame_browser_window_offset: 0x338,
        browser_window_in_view: 0x210,
        view_bounds_offset: 0x1C0,
        ..UNCHECKED_LAYOUT
    },
};
