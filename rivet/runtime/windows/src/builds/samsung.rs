//! Rows for Samsung Internet builds.

use super::*;

/// Samsung Internet 143.0.4.29 (Chromium 143.0.7499.194). The browser code is in
/// `samsunginternet.dll`, and `samsunginternet.exe` is the browser process itself, so there is no
/// versioned launcher. The file version resource names the Samsung release, not Chromium's.
pub static SAMSUNG: Build = Build {
    omnibox: None,
    app_window: Some(AppWindow {
        feature_impl: Some(0x001FBB10),
        feature_impl_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC3, 0x89, 0xD6, 0x48, 0x89,
            0xCF,
        ],
        browser_type: 0x1B0,
        detach: None,
        detach_prologue: &[],
        insert: None,
        insert_prologue: &[],
        view_parent: 0x190,
        view_children: 0x198,
        operator_new: 0x55841C8,
        operator_new_prologue: &[
            0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xEB, 0x0F, 0x48, 0x8B, 0xCB,
            0xE8, 0x65,
        ],
        web_view_ctor: 0x1AEB440,
        web_view_ctor_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B,
            0x05, 0xED,
        ],
        web_view_size: 0x4C0,
        browser_context: None,
        browser_context_prologue: &[],
        browser_context_offset: Some(0x00000258),
        add_child: 0x4C0BB50,
        add_child_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0x88,
        ],
        set_contents: 0x1FD2EA0,
        set_contents_prologue: &[
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xD6, 0x48, 0x89,
            0xCF, 0xE8,
        ],
        // `views::View::SetBoundsRect`, found statically and unverified: the only function that names
        // both "View::Layout(set_bounds)" and "View::Layout(bounds_changed)", as Chrome's does.
        set_bounds: Some(0x047CEA60),
        set_bounds_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xE8,
        ],
        focus_behavior: None,
        focus_behavior_prologue: &[],
        request_focus: None,
        request_focus_prologue: &[],
        request_focus_slot: Some(0x00000031),
        destructor_slot: 0x9,
        bounds_changed_slot: Some(0x4B), // WebView::OnBoundsChanged, the same function as Brave 156.1.98.63 slot 0x4B
        window_features: None,
    }),
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
        window_feature_prologue: &[
            0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xC3, 0x89, 0xD6,
        ],
        window_feature_tab_strip: 2,
        window_feature_callers: (0x013C_8310, 0x013C_A742),
        set_bounds_rect: 0x047C_EA60,
        set_bounds_rect_prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53,
        ],
        toolbar_in_view: 0x5B0,
        // The toggle ends 36 DIPs in and the toolbar keeps 8 before its first button; the back button
        // then sits 30 DIPs from the toggle's centre, as Chrome's does.
        toolbar_leading: 32,
        detour_strip: true,
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
            (
                0x0D9A_0C60,
                &[
                    0x48, 0x8B, 0x81, 0x50, 0x05, 0x00, 0x00, 0x48, 0x8B, 0x88, 0xE0, 0x01, 0x00,
                    0x00, 0xE9,
                ],
            ),
            (
                0x0126_9890,
                &[
                    0x56, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
                ],
            ),
            (
                0x0519_4F80,
                &[
                    0x48, 0x89, 0xC8, 0x0F, 0x57, 0xC0, 0x0F, 0x11, 0x41, 0x09, 0x0F, 0x11, 0x01,
                ],
            ),
            (
                0x03BE_67D0,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x79, 0x70,
                ],
            ),
            (
                0x05C4_F410,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x44, 0x89, 0xCF,
                    0x4C, 0x89, 0xC3, 0x48, 0x89, 0xCE,
                ],
            ),
            (
                0x05C4_F9C0,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05,
                ],
            ),
            (
                0x076F_51A0,
                &[
                    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x04, 0x00,
                    0x00, 0x4C, 0x89, 0xC6, 0x48, 0x89, 0xD3, 0x48, 0x89, 0xCF,
                ],
            ),
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
            0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x08, 0x01, 0x00, 0x00, 0x89, 0xD7,
            0x48, 0x89, 0xCE, 0x48, 0x8B, 0x05, 0xC8, 0x5B, 0x16, 0x02,
        ],
        // Its first argument is the browser itself, which the browser view holds.
        new_tab_address: Some(0),
        // The new tab starts loading a page after the command returns.
        new_tab_delay_ms: 600,
        // The browser's own new tab page stops the browser beside Rivet's second view, on a view
        // it expects and does not find. Found statically and unverified: the call of
        // `WebContents::OpenURL` recorded for navigation, with a new foreground tab.
        new_tab_by_open_url: Some(true),
        new_tab: 0x0D81_4460,
        last_committed_url: 0x0553_29B0, // slot 19 of the same vtable
        // `GetVisibleURL` is slot 18 of the vtable. The address list asks for it only when the
        // committed address is empty.
        visible_url: 0x0468_D1A0,
        get_group: 0x0323_3340,
        signatures: &[
            (0x00B5_A010, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (0x0D9A_0A50, &[0x48, 0x83, 0xEC, 0x28, 0xE8]),
            (
                0x0D9A_0C60,
                &[
                    0x48, 0x8B, 0x81, 0x50, 0x05, 0x00, 0x00, 0x48, 0x8B, 0x88, 0xE0, 0x01, 0x00,
                    0x00, 0xE9,
                ],
            ),
            (
                0x02EB_E870,
                &[0x48, 0x8B, 0x81, 0xE0, 0x01, 0x00, 0x00, 0xC3],
            ),
            (
                0x0322_A1F0,
                &[0x48, 0x8B, 0x41, 0x08, 0x8B, 0x40, 0x18, 0xC3],
            ),
            (
                0x0476_99D0,
                &[0x48, 0x8B, 0x89, 0x98, 0x00, 0x00, 0x00, 0xEB, 0x11],
            ),
            (
                0x0877_7350,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x68, 0x01, 0x00, 0x00,
                ],
            ),
            (
                0x0877_A5C0,
                &[0x56, 0x57, 0x48, 0x83, 0xEC, 0x48, 0x48, 0x8B, 0x05],
            ),
            (
                0x007D_4FD0,
                &[
                    0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0x48, 0x8B, 0x81,
                ],
            ),
            (
                0x0553_29B0,
                &[
                    0x48, 0x63, 0x81, 0x9C, 0x02, 0x00, 0x00, 0x48, 0x83, 0xF8, 0xFF, 0x74,
                ],
            ),
            (
                0x0468_D1A0,
                &[
                    0x48, 0x8B, 0x81, 0x78, 0x02, 0x00, 0x00, 0x48, 0x85, 0xC0, 0x0F, 0x85,
                ],
            ),
            (
                0x0323_3340,
                &[
                    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xD6, 0x45, 0x85, 0xC0, 0x78, 0x0E,
                    0x48,
                ],
            ),
        ],
        ..UNCHECKED_TABS
    },
    // Only what was verified is recorded. `AddToNewSplit` is the one function that calls the recorder of
    // the `TabStrip.SplitView.Created` histogram, and it takes the same call as Chrome 150's: the other
    // tab's index alone, in a vector of begin, end and capacity pointers. Calling it on a window of two
    // tabs stops inside the tab collection, so the feature stays off.
    split: Split {
        add_to_new_split: 0x0877_CD60,
        split_adds_active: true,
        split_vector_ends_by_pointer: true,
        // The global `operator new`.
        allocate: 0x0558_41C8,
        signatures: &[
            (
                0x0877_CD60,
                &[
                    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00,
                    0x4C,
                ],
            ),
            (
                0x0558_41C8,
                &[
                    0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xEB, 0x0F,
                ],
            ),
        ],
        ..UNCHECKED_SPLIT
    },

    side_panel: UNCHECKED_SIDEPANEL,
    // The frame paint and the top inset are not recorded: nothing shows that this build's functions in
    // those vtable slots are Chromium's, and its frame vtable does not have Chromium's layout.
    layout: Layout {
        // The browser view's `views::View` lies this far inside the object `GetBrowserViewForNativeWindow`
        // answers, read from the parent of its children.
        browser_window_in_view: 0x1F8,
        ..UNCHECKED_LAYOUT
    },
};
