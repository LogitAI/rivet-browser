//! Tests that compare rows with the installed browsers.

use super::*;

/// Reads `bytes.len()` bytes of the file image at `rva` from a PE file on disk.
const WHALE_DLL: &str = r"C:\Program Files\Naver\Naver Whale\Application\5.39.412.57\whale.dll";

fn file_bytes(file: &[u8], rva: usize, length: usize) -> Option<Vec<u8>> {
    let pe = u32::from_le_bytes(file.get(0x3C..0x40)?.try_into().ok()?) as usize;
    let sections = u16::from_le_bytes(file.get(pe + 6..pe + 8)?.try_into().ok()?) as usize;
    let optional = u16::from_le_bytes(file.get(pe + 20..pe + 22)?.try_into().ok()?) as usize;
    for index in 0..sections {
        let header = pe + 24 + optional + 40 * index;
        let field = |at: usize| -> Option<usize> {
            Some(
                u32::from_le_bytes(file.get(header + at..header + at + 4)?.try_into().ok()?)
                    as usize,
            )
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
    for rva in [
        nav.gurl_from_text,
        nav.gurl_empty,
        nav.gurl_drop,
        nav.open_url_params,
        nav.open_url_params_drop,
        nav.open_url,
    ] {
        assert!(nav.signatures.iter().any(|(at, _)| *at == rva), "{rva:#x}");
    }
}

#[test]
fn fr3_whale_new_tab_and_navigation_match_the_installed_build() {
    let tabs = &WHALE.tabs;
    let Ok(file) = std::fs::read(WHALE_DLL) else {
        return;
    };
    // `mov rdi, r8` keeps the address the command hands to the tab it adds.
    assert_eq!(
        file_bytes(&file, tabs.new_tab, tabs.new_tab_prologue.len()).as_deref(),
        Some(tabs.new_tab_prologue)
    );
    assert_eq!(tabs.new_tab_prologue[12..15], [0x4C, 0x89, 0xC7]);
    for (rva, bytes) in WHALE.navigation.signatures {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "{rva:#x}"
        );
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
        (
            "caption paint",
            layout.caption_paint,
            layout.caption_paint_prologue,
        ),
        ("draw colour", layout.draw_color, layout.draw_color_prologue),
        (
            "colour provider",
            layout.get_color_provider,
            layout.get_color_provider_prologue,
        ),
    ] {
        assert_eq!(
            file_bytes(&file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{name} prologue does not match chrome.dll"
        );
    }
}

#[test]
fn nfr6_the_launcher_opens_exactly_the_complete_rows() {
    use rivet_platform_windows::hold::{module_of, table_versions, verified};
    let verified = verified();
    // Chrome 150 only keeps the frame hooks, so a window of it would not have Rivet's top row.
    let complete: Vec<_> = all()
        .iter()
        .filter(|build| !std::ptr::eq(**build, &CHROME_150))
        .collect();
    assert_eq!(
        complete.len(),
        verified.len(),
        "the launcher's list and the table differ"
    );
    for build in complete {
        let executable = match (build.browser, build.module) {
            ("Google Chrome", _) => "chrome.exe",
            ("Brave", _) => "brave.exe",
            (_, "msedge.dll") => "msedge.exe",
            (_, "whale.dll") => "whale.exe",
            (_, "samsunginternet.dll") => "samsunginternet.exe",
            other => panic!("no launcher name for {other:?}"),
        };
        assert!(
            verified.contains(&(executable, build.version)),
            "{executable} {:?}",
            build.version
        );
        assert_eq!(module_of(executable), Some(build.module));
    }
    // The signed table is read by the same names the rows carry.
    let table: serde_json::Value = serde_json::from_str(CHECKED_IN).unwrap();
    assert!(table_versions(&table, "msedge.exe").contains(&[156, 0, 4314, 58]));
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
    assert!(
        !WHALE.vertical.supported
            && WHALE.tabs.supported
            && !WHALE.tabs.groups
            && !WHALE.split.supported
    );
    assert!(WHALE.navigation.supported && WHALE.side_panel.supported);
    assert!(
        WHALE.vertical.detour_strip && WHALE.vertical.should_draw_tab_strip_prologue.len() >= 14
    );
    assert!(
        all()
            .iter()
            .filter(|build| build.vertical.detour_strip)
            .count()
            == 2
    );
    assert_eq!(WHALE.toolbar.skip, "toolbar");
    // Hooks without a verified address are absent, so their checks fail.
    assert_eq!(WHALE.layout.corner_paint, ABSENT);
    assert_eq!(WHALE.layout.caption_paint, ABSENT);
    assert_eq!(WHALE.layout.layout, ABSENT);
    assert_eq!(
        (
            WHALE.layout.frame_browser_window_offset,
            WHALE.layout.browser_window_in_view
        ),
        (0x338, 0x210)
    );
}

#[test]
fn fr2_whale_records_chromium_s_frame_paint_and_top_inset() {
    // Rivet's frame on Whale needs Chromium's `BrowserFrameViewWin` paint and top inset.
    let layout = &WHALE.layout;
    assert_eq!(
        (layout.frame_vtable, layout.on_paint, layout.top_inset),
        (0x0FB1_7440, 0x0253_D0E0, 0x0E7B_4500)
    );
    assert_eq!((layout.on_paint_slot, layout.top_inset_slot), (89, 127));
    assert!(layout.on_paint_prologue.len() >= 12);
    let Ok(file) = std::fs::read(WHALE_DLL) else {
        return;
    };
    assert_eq!(
        file_bytes(&file, layout.on_paint, layout.on_paint_prologue.len()).as_deref(),
        Some(layout.on_paint_prologue)
    );
    // The vtable slots hold the two functions.
    for (slot, function) in [
        (layout.on_paint_slot, layout.on_paint),
        (layout.top_inset_slot, layout.top_inset),
    ] {
        let entry = file_bytes(&file, layout.frame_vtable + slot * 8, 8)
            .expect("the vtable is in the file");
        let pointer = u64::from_le_bytes(entry.try_into().unwrap()) as usize;
        assert_eq!(pointer - 0x1_8000_0000, function);
    }
}

/// The chrome.dll of a Brave row, when that version is the one installed.
fn brave_dll(build: &Build) -> Option<Vec<u8>> {
    let [a, b, c, d] = build.version;
    std::fs::read(format!(
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\{a}.{b}.{c}.{d}\chrome.dll"
    ))
    .ok()
}

#[test]
fn nfr3_brave_156_frame_hooks_match_the_installed_build() {
    assert_eq!(BRAVE.version, [156, 1, 98, 62]);
    assert_eq!(BRAVE_63.version, [156, 1, 98, 63]);
    check_brave_frame_hooks(&BRAVE);
    check_brave_frame_hooks(&BRAVE_63);
}

fn check_brave_frame_hooks(brave: &Build) {
    assert!(
        brave.vertical.supported
            && brave.tabs.supported
            && !brave.tabs.groups
            && brave.navigation.supported
    );
    assert!(!brave.split.supported && !brave.vertical.detour_strip);
    // The .62 row records no frame paint or top inset. The .63 row records Chromium's own.
    if brave.version == BRAVE.version {
        assert_eq!(
            (
                brave.layout.frame_vtable,
                brave.layout.on_paint,
                brave.layout.top_inset
            ),
            (ABSENT, ABSENT, ABSENT)
        );
    } else {
        assert!(
            brave.layout.frame_vtable != ABSENT
                && brave.layout.on_paint != ABSENT
                && brave.layout.top_inset != ABSENT
        );
    }
    let Some(file) = brave_dll(brave) else {
        return;
    };
    let layout = &brave.layout;
    for (name, rva, prologue) in [
        (
            "colour provider",
            layout.get_color_provider,
            layout.get_color_provider_prologue,
        ),
        ("draw colour", layout.draw_color, layout.draw_color_prologue),
        (
            "corner paint",
            layout.corner_paint,
            layout.corner_paint_prologue,
        ),
        (
            "caption paint",
            layout.caption_paint,
            layout.caption_paint_prologue,
        ),
        ("frame layout", layout.layout, layout.layout_prologue),
        (
            "set bounds",
            layout.set_bounds_rect,
            layout.set_bounds_rect_prologue,
        ),
        (
            "layout immediately",
            layout.layout_immediately,
            layout.layout_immediately_prologue,
        ),
        (
            "paint to layer",
            layout.set_layer,
            layout.set_layer_prologue,
        ),
        (
            "rounded corners",
            layout.set_radius,
            layout.set_radius_prologue,
        ),
        (
            "masks to bounds",
            layout.set_masks,
            layout.set_masks_prologue,
        ),
        (
            "stack at top",
            layout.stack_at_top,
            layout.stack_at_top_prologue,
        ),
        (
            "fills opaquely",
            layout.set_opaque,
            layout.set_opaque_prologue,
        ),
    ] {
        assert_eq!(
            file_bytes(&file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{name} prologue does not match Brave's chrome.dll"
        );
    }
    for (rva, bytes) in brave
        .tabs
        .signatures
        .iter()
        .chain(brave.navigation.signatures)
    {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "signature at {rva:#x}"
        );
    }
    // Each replaced slot holds the function the row names, at the vtable the objects carry.
    for (name, vtable, slot, function) in [
        (
            "corner paint",
            layout.corner_vtable,
            layout.on_paint_slot,
            layout.corner_paint,
        ),
        (
            "caption size",
            layout.caption_vtable,
            layout.caption_size_slot,
            layout.caption_size,
        ),
        (
            "caption paint",
            layout.caption_vtable,
            layout.caption_paint_slot,
            layout.caption_paint,
        ),
    ] {
        let word = file_bytes(&file, vtable + slot * 8, 8).expect("vtable inside the image");
        let target = u64::from_le_bytes(word.try_into().unwrap()) as usize;
        assert_eq!(target, 0x1_8000_0000 + function, "{name} slot");
    }
}

/// Checks one row's side panel functions and vtable slots against the module on disk.
fn check_side_panel_hooks(file: &[u8], row: &SidePanel, name: &str) {
    let image_base = 0x1_8000_0000usize;
    for (what, rva, prologue) in [
        ("show", row.show, row.show_prologue),
        (
            "entry id to string",
            row.entry_id_to_string,
            row.entry_id_to_string_prologue,
        ),
        (
            "entry showing",
            row.entry_showing,
            row.entry_showing_prologue,
        ),
        (
            "registry from",
            row.registry_from,
            row.registry_from_prologue,
        ),
        (
            "colour provider of a page",
            row.get_color_provider,
            row.get_color_provider_prologue,
        ),
        (
            "style sheet of the colours",
            row.generate_colors_css,
            row.generate_colors_css_prologue,
        ),
        (
            "layout immediately",
            row.layout_immediately,
            row.layout_immediately_prologue,
        ),
    ] {
        assert_eq!(
            file_bytes(file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{what} prologue does not match {name}"
        );
    }
    let pointer = |rva: usize| {
        file_bytes(file, rva, 8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()) as usize - image_base)
    };
    // The coordinator's vtable holds Show in slot 2 and Toggle in slot 5, and the layout vtable
    // holds the function Rivet replaces in the slot the row names.
    if row.coordinator_vtable != ABSENT {
        assert_eq!(
            pointer(row.coordinator_vtable + 2 * 8),
            Some(row.show),
            "show slot of {name}"
        );
        assert_eq!(
            pointer(row.coordinator_vtable + 5 * 8),
            Some(row.toggle),
            "toggle slot of {name}"
        );
    }
    if row.layout_vtable != ABSENT {
        assert_eq!(
            pointer(row.layout_vtable + row.proposed_slot * 8),
            Some(row.proposed),
            "layout slot of {name}"
        );
    }
    // Both views start with their own vtable, which the objects are found by.
    assert!(
        pointer(row.side_panel_vtable).is_some() && pointer(row.browser_view_view_vtable).is_some()
    );
}

#[test]
fn fr10_brave_63_side_panel_row_matches_the_installed_build() {
    // The .63 row records Chromium's coordinator, toggle and page layout. The Sidebar stays off
    // until it is driven.
    assert!(!BRAVE_63.side_panel.supported && !BRAVE.side_panel.supported);
    assert_eq!(
        (
            BRAVE_63.side_panel.extension_entry,
            BRAVE_63.side_panel.entry_count
        ),
        (0x1b, 0x1c)
    );
    let Some(file) = brave_dll(&BRAVE_63) else {
        return;
    };
    check_side_panel_hooks(&file, &BRAVE_63.side_panel, "Brave's chrome.dll");
}

#[test]
fn fr10_edge_side_panel_row_matches_the_installed_build() {
    let row = &EDGE.side_panel;
    assert!(
        !row.supported,
        "Edge's only registered panel cannot be opened"
    );
    // The Edge row's layout function is slot 4 of its layout vtable.
    assert_eq!(row.proposed_slot, 4);
    assert_eq!(row.hidden_entries, &[0]);
    let Ok(file) = std::fs::read(
        r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.50\msedge.dll",
    ) else {
        return;
    };
    check_side_panel_hooks(&file, row, "Edge's msedge.dll");
}

#[test]
fn fr12_brave_156_toolbar_and_vertical_tab_hooks_match_the_installed_build() {
    check_brave_toolbar_hooks(&BRAVE);
    check_brave_toolbar_hooks(&BRAVE_63);
}

#[test]
fn nfr3_samsung_frame_and_tab_hooks_match_the_installed_build() {
    assert_eq!(SAMSUNG.module, "samsunginternet.dll");
    assert_eq!(SAMSUNG.version, [30, 0, 4, 29]);
    assert_eq!(SAMSUNG.toolbar.skip, "toolbar");
    assert!(
        !SAMSUNG.vertical.supported
            && SAMSUNG.tabs.supported
            && !SAMSUNG.tabs.groups
            && !SAMSUNG.split.supported
    );
    // Nothing outside Chromium is recorded for the frame: no paint, no colours and no top inset.
    for rva in [
        SAMSUNG.layout.on_paint,
        SAMSUNG.layout.top_inset,
        SAMSUNG.layout.corner_paint,
        SAMSUNG.layout.caption_paint,
    ] {
        assert_eq!(rva, ABSENT);
    }
    assert!(!SAMSUNG.layout.provider_verified);
    let path = r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll";
    let Ok(file) = std::fs::read(path) else {
        return;
    };
    // The inlined tab strip test: `call` of the window feature function from the layout.
    assert_eq!(
        file_bytes(&file, 0x13c8510, 5).as_deref(),
        Some(&[0xE8, 0xFB, 0x35, 0xE3, 0xFE][..])
    );
    let callers = SAMSUNG.vertical.window_feature_callers;
    assert!(callers.0 <= 0x13c8510 && 0x13c8515 <= callers.1);
    let bounds = SAMSUNG.vertical.set_bounds_rect_prologue;
    assert_eq!(
        file_bytes(&file, SAMSUNG.vertical.set_bounds_rect, bounds.len()).as_deref(),
        Some(bounds)
    );
    let bytes = SAMSUNG.vertical.window_feature_prologue;
    assert_eq!(
        file_bytes(&file, SAMSUNG.vertical.window_feature, bytes.len()).as_deref(),
        Some(bytes)
    );
    for (rva, bytes) in SAMSUNG.tabs.signatures {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "{rva:#x} does not match samsunginternet.dll"
        );
    }
    // The web contents vtable keeps the title, committed address and visible address getters.
    for (slot, target) in [
        (57, SAMSUNG.tabs.title),
        (19, SAMSUNG.tabs.last_committed_url),
        (18, SAMSUNG.tabs.visible_url),
    ] {
        let word = file_bytes(&file, 0xe0fd4a8 + slot * 8, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(word.try_into().unwrap()),
            0x1_8000_0000u64 + target as u64
        );
    }
}

#[test]
fn fr3_samsung_new_tab_hook_and_navigation_match_the_installed_build() {
    let tabs = &SAMSUNG.tabs;
    let n = tabs.new_tab_prologue.len();
    // The prologue ends with the stack cookie load the detour relocates.
    assert_eq!(tabs.new_tab_prologue[n - 7..n - 4], [0x48, 0x8B, 0x05]);
    assert!(
        tabs.new_tab_address == Some(0) && !tabs.new_tab_takes_address && tabs.new_tab != ABSENT
    );
    assert!(
        SAMSUNG.navigation.supported && tabs.new_tab_delay_ms > 0 && SAMSUNG.navigation.gurl_wide
    );
    assert!(all()
        .iter()
        .all(|build| build.navigation.gurl_wide == (build.module == "samsunginternet.dll")));
    assert!(WHALE.tabs.new_tab_delay_ms == 0 && EDGE.tabs.new_tab_delay_ms == 0);
    // Only Samsung opens a Rivet window's new tab through `OpenURL`, past its own new tab page.
    assert!(all()
        .iter()
        .all(|build| (build.tabs.new_tab_by_open_url == Some(true))
            == (build.module == "samsunginternet.dll")));
    let path = r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll";
    let Ok(file) = std::fs::read(path) else {
        return;
    };
    assert_eq!(
        file_bytes(&file, tabs.new_tab, n).as_deref(),
        Some(tabs.new_tab_prologue)
    );
    for (rva, bytes) in SAMSUNG.navigation.signatures {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "{rva:#x}"
        );
    }
}

#[test]
fn fr3_a_build_with_tabs_checks_only_functions_it_has() {
    for build in all() {
        if build.tabs.supported {
            for (rva, _) in build.tabs.signatures {
                assert_ne!(
                    *rva, ABSENT,
                    "{} checks a function it does not have",
                    build.module
                );
            }
            assert!(build.tabs.browser_in_view >= 0x100);
        }
    }
    // The Edge row reads the Browser at another offset in the browser view, and not its groups.
    assert_ne!(CHROME.tabs.browser_in_view, EDGE.tabs.browser_in_view);
    assert!(CHROME.tabs.groups && !EDGE.tabs.groups);
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
        assert_eq!(
            file_bytes(&file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{name}"
        );
    }
}

#[test]
fn fr9_samsung_split_records_only_the_calls_it_verified() {
    let row = &SAMSUNG.split;
    assert!(!row.supported && row.split_adds_active && row.split_vector_ends_by_pointer);
    assert_ne!(row.add_to_new_split, ABSENT);
    // Nothing else is guessed: the lookup, the removal and the layout hooks have no address.
    assert_eq!(
        (
            row.remove_split,
            row.get_split_for_tab,
            row.calculate_layout
        ),
        (ABSENT, ABSENT, ABSENT)
    );
    let path = r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll";
    let Ok(file) = std::fs::read(path) else {
        return;
    };
    for (rva, bytes) in row.signatures {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "{rva:#x}"
        );
    }
}

/// Every address a data row names beside its recorded bytes, checked against the module on disk.
fn check_row_bytes(file: &[u8], node: &serde_json::Value, path: &str, checked: &mut usize) {
    use serde_json::Value;
    let address = |value: &Value| {
        value
            .as_str()?
            .strip_prefix("0x")
            .and_then(|text| usize::from_str_radix(text, 16).ok())
    };
    let hex = |value: &Value| -> Option<Vec<u8>> {
        let text = value.as_str()?;
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
            .collect()
    };
    match node {
        Value::Object(map) => {
            for (key, value) in map {
                let base = ["_prologue", "_bytes", "_head"]
                    .iter()
                    .find_map(|suffix| key.strip_suffix(suffix));
                if let (Some(base), Some(bytes)) = (base, hex(value)) {
                    // A prologue that is not beside an address of the same name belongs to a table.
                    if let Some(rva) = map.get(base).and_then(address).filter(|rva| *rva != ABSENT)
                    {
                        assert_eq!(
                            file_bytes(file, rva, bytes.len()),
                            Some(bytes),
                            "{path}.{key} at {rva:#x}"
                        );
                        *checked += 1;
                    }
                } else {
                    check_row_bytes(file, value, &format!("{path}.{key}"), checked);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if let [first, second] = item.as_array().map(Vec::as_slice).unwrap_or_default() {
                    if let (Some(rva), Some(bytes)) = (address(first), hex(second)) {
                        assert_eq!(
                            file_bytes(file, rva, bytes.len()),
                            Some(bytes),
                            "{path}[{index}] at {rva:#x}"
                        );
                        *checked += 1;
                        continue;
                    }
                }
                check_row_bytes(file, item, &format!("{path}[{index}]"), checked);
            }
        }
        _ => {}
    }
}

#[test]
fn nfr3_edge_156_58_data_row_matches_the_installed_build() {
    let table: serde_json::Value =
        serde_json::from_str(CHECKED_IN).expect("the checked-in table is JSON");
    let row = table["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["build"]["browser"] == "Microsoft Edge"
                    && row["build"]["version"] == serde_json::json!([156, 0, 4314, 58])
            })
        })
        .expect("the table holds a row for Edge 156.0.4314.58");
    // What the .50 row switches on or off, which the update must not change.
    assert_eq!(
        row["build"]["side_panel"]["supported"], false,
        "the Sidebar stays off by policy"
    );
    assert_eq!(row["build"]["split"]["supported"], true);
    assert_eq!(row["build"]["tabs"]["supported"], true);
    assert_eq!(row["build"]["navigation"]["supported"], true);
    // With the gap's repaint scheduled after a focus change, a click into the other pane no longer made
    // it the active one on this build. The row leaves the repaint out, as the 4314.50 row did by
    // carrying a prologue that never matched.
    assert_eq!(
        row["build"]["split"]["schedule_paint"],
        format!("{ABSENT:#x}")
    );
    // The installed file is gone once Edge finishes updating, and then there is nothing to compare.
    let Ok(file) = std::fs::read(
        r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.58\msedge.dll",
    ) else {
        return;
    };
    let mut checked = 0;
    check_row_bytes(&file, &row["build"], "build", &mut checked);
    assert!(
        checked > 60,
        "only {checked} recorded byte runs were compared"
    );
}

/// The installed module of each build whose app window part was measured on this machine.
const APP_WINDOW_MODULES: &[(&str, [u16; 4], &str)] = &[
    (
        "Brave",
        [156, 1, 98, 63],
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll",
    ),
    (
        "Naver Whale",
        [5, 39, 412, 57],
        r"C:\Program Files\Naver\Naver Whale\Application\5.39.412.57\whale.dll",
    ),
    (
        "Samsung Internet",
        [30, 0, 4, 29],
        r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll",
    ),
    (
        "Microsoft Edge",
        [156, 0, 4314, 58],
        r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.58\msedge.dll",
    ),
    (
        "Google Chrome",
        [156, 0, 8078, 80],
        r"C:\Program Files\Google\Chrome\Application\156.0.8078.80\chrome.dll",
    ),
    ("Google Chrome", [157, 0, 8103, 38], CHROME_157_DLL),
    ("Brave", [157, 1, 99, 55], BRAVE_157_DLL),
];

#[test]
fn nfr3_chrome_156_80_row_matches_the_installed_build() {
    // The row was ported by name from the .74 row with the .80 public symbols. Every recorded first
    // byte must be the installed module's.
    let Ok(file) =
        std::fs::read(r"C:\Program Files\Google\Chrome\Application\156.0.8078.80\chrome.dll")
    else {
        return;
    };
    let mut checked = 0;
    check_row_bytes(
        &file,
        &crate::row_data::RowValue::to_row(&CHROME_156_80),
        "build",
        &mut checked,
    );
    assert!(
        checked > 60,
        "only {checked} recorded byte runs were compared"
    );
}

#[test]
fn fr12_every_address_bar_row_matches_its_installed_build() {
    let modules = [
        (
            &CHROME_156_80,
            r"C:\Program Files\Google\Chrome\Application\156.0.8078.80\chrome.dll",
        ),
        (
            &BRAVE_63,
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll",
        ),
        (&WHALE, WHALE_DLL),
        (&CHROME_157_38, CHROME_157_DLL),
        (&BRAVE_157_55, BRAVE_157_DLL),
    ];
    for (build, path) in modules {
        let row = build
            .omnibox
            .as_ref()
            .expect("the row records its address bar");
        let Ok(file) = std::fs::read(path) else {
            continue;
        };
        for (rva, bytes) in row.signatures {
            assert_eq!(
                file_bytes(&file, *rva, bytes.len()).as_deref(),
                Some(*bytes),
                "{} {rva:#x}",
                build.browser
            );
        }
    }
}

#[test]
fn fr13_every_build_that_shows_a_second_view_records_the_same_chromium_functions() {
    let chrome = ("Google Chrome", [156, 0, 8078, 74], "");
    for (_, version, _) in APP_WINDOW_MODULES.iter().chain([&chrome]) {
        let build = all()
            .iter()
            .find(|build| build.version == *version)
            .unwrap_or_else(|| panic!("{version:?} has a row"));
        let row = build
            .app_window
            .as_ref()
            .unwrap_or_else(|| panic!("{version:?} records its app window"));
        // Every build makes, fills and places the second view with the same Chromium calls.
        for name in [
            "new",
            "ctor",
            "size",
            "add_child",
            "set_contents",
            "vparent",
            "vchildren",
            "dtor_slot",
        ] {
            assert!(
                crate::features::app_window::row_value(row, name).is_some(),
                "{version:?} {name}"
            );
        }
        // The page's browser context comes from a function or from where it is kept.
        assert!(
            row.browser_context.is_some() != row.browser_context_offset.is_some(),
            "{version:?}"
        );
        // The view takes the keyboard by address or by its vtable slot.
        assert!(
            row.request_focus.is_some() || row.request_focus_slot.is_some(),
            "{version:?}"
        );
        for (name, _, bytes) in crate::features::app_window::prologues(row) {
            assert!(bytes.len() >= 12, "{version:?} {name}");
        }
    }
    // The window feature detour copies exactly its prologue.
    let brave = BRAVE_63.app_window.as_ref().unwrap();
    assert_eq!(
        crate::features::app_window::row_value(brave, "feature_len"),
        Some(12)
    );
    assert_eq!(
        crate::features::app_window::row_value(SAMSUNG.app_window.as_ref().unwrap(), "kind"),
        Some(0x1B0)
    );
    // Whale's window feature detour copies its prologue with the stack cookie load at its end.
    assert_eq!(
        crate::features::app_window::row_value(WHALE.app_window.as_ref().unwrap(), "feature_len"),
        Some(15)
    );
    // Samsung's second view takes its bounds through `SetBoundsRect`.
    assert!(SAMSUNG.app_window.as_ref().unwrap().set_bounds.is_some());
}

#[test]
fn fr13_app_window_functions_match_the_installed_builds() {
    for (browser, version, path) in APP_WINDOW_MODULES {
        let Ok(file) = std::fs::read(path) else {
            continue;
        };
        check_app_window_bytes(browser, version, &file);
    }
    // Chrome 156.0.8078.74 is the build Rivet holds while Chrome itself has moved on.
    let held = rivet_core::paths::Layout::current().map(|layout| {
        layout
            .held_browsers()
            .join("chrome")
            .join("156.0.8078.74")
            .join("156.0.8078.74")
            .join("chrome.dll")
    });
    if let Some(file) = held.and_then(|path| std::fs::read(path).ok()) {
        check_app_window_bytes("Google Chrome", &[156, 0, 8078, 74], &file);
    }
    // Chrome 156.0.8078.80 and Brave 156.1.98.63 are held the same way.
    for (browser, folder, version, path) in [
        (
            "Google Chrome",
            "chrome",
            [156, 0, 8078, 80],
            "156.0.8078.80",
        ),
        ("Brave", "brave", [156, 1, 98, 63], "156.1.98.63"),
    ] {
        let held = rivet_core::paths::Layout::current().map(|layout| {
            layout
                .held_browsers()
                .join(folder)
                .join(path)
                .join(path)
                .join("chrome.dll")
        });
        if let Some(file) = held.and_then(|path| std::fs::read(path).ok()) {
            check_app_window_bytes(browser, &version, &file);
        }
    }
}

#[test]
fn fr9_app_window_rows_keep_only_the_way_back_of_the_mini_window() {
    // The Mini Window goes out by the browser's command, which needs no address, and comes back by
    // the tab strip's detach and insert, which every row with an app window part keeps.
    let expected: [(&str, [u16; 4]); 8] = [
        ("Google Chrome", [157, 0, 8103, 38]),
        ("Brave", [157, 1, 99, 55]),
        ("Google Chrome", [156, 0, 8078, 74]),
        ("Google Chrome", [156, 0, 8078, 80]),
        ("Microsoft Edge", [156, 0, 4314, 58]),
        ("Brave", [156, 1, 98, 63]),
        ("Naver Whale", [5, 39, 412, 57]),
        ("Samsung Internet", [30, 0, 4, 29]),
    ];
    for (browser, version) in expected {
        let build = all()
            .iter()
            .find(|build| build.browser == browser && build.version == version)
            .unwrap_or_else(|| panic!("{browser} {version:?} has a row"));
        let row = build.app_window.as_ref().unwrap();
        assert!(row.detach.is_some(), "{browser} {version:?}");
        assert!(row.insert.is_some(), "{browser} {version:?}");
        assert_eq!(
            crate::features::app_window::row_value(row, "move_new"),
            None,
            "{browser} {version:?}"
        );
    }
}

/// The installed modules of the Chromium 157 builds.
const CHROME_157_DLL: &str = r"C:\Program Files\Google\Chrome\Application\157.0.8103.38\chrome.dll";
const BRAVE_157_DLL: &str =
    r"C:\Program Files\BraveSoftware\Brave-Browser\Application\157.1.99.55\chrome.dll";

#[test]
fn nfr3_chromium_157_rows_match_the_installed_builds() {
    // Both rows were ported by name from the 156 rows with each build's own public symbols. Every
    // recorded first byte must be the installed module's.
    for (build, path) in [
        (&CHROME_157_38, CHROME_157_DLL),
        (&BRAVE_157_55, BRAVE_157_DLL),
    ] {
        let Ok(file) = std::fs::read(path) else {
            continue;
        };
        let mut checked = 0;
        check_row_bytes(
            &file,
            &crate::row_data::RowValue::to_row(build),
            "build",
            &mut checked,
        );
        assert!(
            checked > 60,
            "{}: only {checked} recorded byte runs were compared",
            build.browser
        );
    }
}

#[test]
fn nfr3_chromium_157_rows_record_the_members_that_moved() {
    // Chromium 157 made `views::View` eight bytes shorter after its bounds, so every view member
    // past them moved, and it took one virtual out of the frame view. The values were read again
    // from each build's instructions, not carried over.
    for build in [&CHROME_157_38, &BRAVE_157_55] {
        let name = build.browser;
        assert_eq!(build.tabs.browser_in_view, 0x558, "{name}");
        assert_eq!(build.side_panel.browser_offset, 0x558, "{name}");
        assert_eq!(build.layout.view_bounds_offset, 0x1C8, "{name}");
        assert_eq!(build.layout.frame_browser_window_offset, 0x338, "{name}");
        assert_eq!(build.layout.caption_container_offset, 0x3C0, "{name}");
        assert_eq!(
            (build.layout.top_inset_slot, build.layout.caption_paint_slot),
            (126, 113),
            "{name}"
        );
        assert_eq!(build.omnibox.as_ref().unwrap().match_size, 0x690, "{name}");
        let split = &build.split;
        assert_eq!(
            (
                split.layout_delegate_offset,
                split.containers_member,
                split.container_count_member,
                split.active_member,
                split.split_insets_member,
            ),
            (0x320, 0x348, 0x350, 0x3C0, 0x3FC),
            "{name}"
        );
        assert_eq!(
            (
                split.container_delegate_offset,
                split.mini_toolbar_member,
                split.outline_member
            ),
            (0x318, 0x438, 0x440),
            "{name}"
        );
        let row = build.app_window.as_ref().unwrap();
        assert_eq!(row.web_view_size, 0x520, "{name}");
        assert_eq!(
            (row.view_parent, row.view_children),
            (0x198, 0x1A0),
            "{name}"
        );
    }
    // Chromium 157 dropped one side panel entry. Brave keeps its own four.
    assert_eq!(
        (
            CHROME_157_38.side_panel.extension_entry,
            CHROME_157_38.side_panel.entry_count
        ),
        (22, 23)
    );
    assert_eq!(
        (
            BRAVE_157_55.side_panel.extension_entry,
            BRAVE_157_55.side_panel.entry_count
        ),
        (26, 27)
    );
    // Colour ids moved by five, and Brave's stay 17 above Chrome's past the frame ones.
    assert_eq!(
        CHROME_157_38.layout.frame_ids,
        &[0x141, 0x142, 0x145, 0x146, 0x149]
    );
    assert_eq!(
        BRAVE_157_55.layout.frame_ids,
        CHROME_157_38.layout.frame_ids
    );
    assert_eq!(CHROME_157_38.layout.caption_ink_ids, &[0x673, 0x674, 0x675]);
    assert_eq!(BRAVE_157_55.layout.caption_ink_ids, &[0x684, 0x685, 0x686]);
    // Brave's window feature object keeps its browser type eight bytes further on.
    assert_eq!(
        CHROME_157_38.app_window.as_ref().unwrap().browser_type,
        0x10
    );
    assert_eq!(BRAVE_157_55.app_window.as_ref().unwrap().browser_type, 0x18);
}

fn check_app_window_bytes(browser: &str, version: &[u16; 4], file: &[u8]) {
    let build = all()
        .iter()
        .find(|build| build.version == *version && build.browser == browser)
        .unwrap();
    let row = build.app_window.as_ref().unwrap();
    for (name, rva, bytes) in crate::features::app_window::prologues(row) {
        assert_eq!(
            file_bytes(file, rva, bytes.len()).as_deref(),
            Some(bytes),
            "{browser} {name} {rva:#x}"
        );
    }
}

#[test]
fn fr9_whale_split_functions_match_the_installed_build() {
    let row = &WHALE.split;
    // The calls are right and the feature is off, because the browser shows no second page for a
    // split of the tab model.
    assert!(!row.supported && row.split_adds_active && row.split_vector_ends_by_pointer);
    assert_eq!(row.set_rounded_corners, ABSENT);
    assert_eq!(row.flex_layout, ABSENT);
    assert_eq!(WHALE.layout.view_bounds_offset, 0x1C0);
    let Ok(file) = std::fs::read(WHALE_DLL) else {
        return;
    };
    for (rva, bytes) in row.signatures {
        assert_eq!(
            file_bytes(&file, *rva, bytes.len()).as_deref(),
            Some(*bytes),
            "{rva:#x}"
        );
    }
    for (rva, bytes) in [
        (row.calculate_layout, row.calculate_layout_prologue),
        (row.container_layout, row.calculate_layout_prologue),
        (row.web_contents_focused, row.web_contents_focused_prologue),
        (row.update_border, row.update_border_prologue),
        (row.schedule_paint, row.schedule_paint_prologue),
    ] {
        assert_eq!(
            file_bytes(&file, rva, bytes.len()).as_deref(),
            Some(bytes),
            "{rva:#x}"
        );
    }
    // The layout delegates sit in the tables whose first slot holds the layout functions.
    let image_base = 0x1_8000_0000u64;
    for (table, function) in [
        (0xF90_FEC0usize, row.calculate_layout),
        (0xF91_1780, row.container_layout),
    ] {
        let word = file_bytes(&file, table, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(word.try_into().unwrap()),
            image_base + function as u64,
            "{table:#x}"
        );
    }
}

fn check_brave_toolbar_hooks(brave: &Build) {
    assert_eq!(brave.toolbar.skip, "");
    assert_eq!(brave.toolbar.get_widget_slot, 11);
    // The Brave row's caption colour ids sit 17 above Chrome's.
    assert_eq!(
        brave.layout.caption_ink_ids,
        &[
            CHROME.layout.caption_ink_ids[0] + 17,
            CHROME.layout.caption_ink_ids[1] + 17,
            CHROME.layout.caption_ink_ids[2] + 17
        ]
    );
    let Some(file) = brave_dll(brave) else {
        return;
    };
    for (name, rva, prologue) in [
        (
            "colour",
            brave.toolbar.get_color,
            brave.toolbar.get_color_bytes,
        ),
        (
            "view invalidation",
            brave.vertical.invalidate_layout,
            brave.vertical.invalidate_layout_prologue,
        ),
        (
            "region minimum size",
            brave.vertical.region_minimum_size,
            brave.vertical.region_minimum_prologue,
        ),
        (
            "controller destructor",
            brave.vertical.destructor,
            brave.vertical.destructor_prologue,
        ),
    ] {
        assert_eq!(
            file_bytes(&file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{name} prologue does not match Brave's chrome.dll"
        );
    }
}

#[test]
fn nfr3_chrome_156_74_hooks_match_the_installed_build() {
    let path = r"C:\Program Files\Google\Chrome\Application\156.0.8078.74\chrome.dll";
    let Ok(file) = std::fs::read(path) else {
        return;
    };
    let (layout, vertical, split) = (
        &CHROME_156_74.layout,
        &CHROME_156_74.vertical,
        &CHROME_156_74.split,
    );
    for (name, rva, prologue) in [
        ("frame paint", layout.on_paint, layout.on_paint_prologue),
        (
            "caption paint",
            layout.caption_paint,
            layout.caption_paint_prologue,
        ),
        (
            "colour provider",
            layout.get_color_provider,
            layout.get_color_provider_prologue,
        ),
        (
            "destructor",
            vertical.destructor,
            vertical.destructor_prologue,
        ),
        (
            "rounded corners",
            split.set_rounded_corners,
            split.set_rounded_corners_prologue,
        ),
    ] {
        assert_eq!(
            file_bytes(&file, rva, prologue.len()).as_deref(),
            Some(prologue),
            "{name}"
        );
    }
}

#[test]
fn nfr3_recorded_addresses_lie_inside_a_plausible_image() {
    for build in all() {
        for rva in [
            build.toolbar.get_color,
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
            assert!(
                (0x1000..0x2000_0000).contains(&rva),
                "{rva:#x} in {}",
                build.module
            );
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
        assert!(!build.layout.on_paint_prologue.is_empty());
    }
}

#[test]
fn nfr3_chrome_and_edge_are_separate_rows() {
    assert_eq!(CHROME.module, "chrome.dll");
    assert_eq!(CHROME.version, [156, 0, 8078, 48]);
    assert_eq!(EDGE.module, "msedge.dll");
    assert_eq!(EDGE.version, [156, 0, 4314, 50]);
    assert_ne!(CHROME.layout.frame_vtable, EDGE.layout.frame_vtable);
}

#[test]
fn nfr3_chrome_rows_of_one_milestone_are_separate() {
    assert_eq!(CHROME_156_74.version, [156, 0, 8078, 74]);
    assert_eq!(CHROME.version, [156, 0, 8078, 48]);
    assert_ne!(
        CHROME_156_74.layout.frame_vtable,
        CHROME.layout.frame_vtable
    );
}

/// The function at `slot` of the first vtable that a constructor stores, read from the module file.
fn vtable_entry_after(file: &[u8], ctor: usize, slot: usize) -> Option<Vec<u8>> {
    let pe = u32::from_le_bytes(file.get(0x3C..0x40)?.try_into().ok()?) as usize;
    let image_base = u64::from_le_bytes(file.get(pe + 48..pe + 56)?.try_into().ok()?) as usize;
    let code = file_bytes(file, ctor, 0x80)?;
    // The first `lea rax, [rip + vtable]` of the constructor loads the view's own vtable.
    let at = code
        .windows(3)
        .position(|bytes| bytes[0] == 0x48 && bytes[1] == 0x8D && bytes[2] & 0xC7 == 0x05)?;
    let displacement = i32::from_le_bytes(code.get(at + 3..at + 7)?.try_into().ok()?);
    let vtable = (ctor as i64 + at as i64 + 7 + displacement as i64) as usize;
    let entry =
        u64::from_le_bytes(file_bytes(file, vtable + slot * 8, 8)?.try_into().ok()?) as usize;
    file_bytes(file, entry - image_base, 16)
}

#[test]
fn fr9_chrome_156_80_page_view_bounds_slot_is_brave_s_web_view_on_bounds_changed() {
    // Brave 156.1.98.63 is built from the same Chromium as Chrome 156.0.8078.80, and its slot is
    // measured. Without the slot the browser's layout and Rivet's placement fight over the page
    // view and its page never gets to paint at one size.
    let chrome = CHROME_156_80.app_window.as_ref().unwrap();
    let brave = BRAVE_63.app_window.as_ref().unwrap();
    assert_eq!(chrome.bounds_changed_slot, brave.bounds_changed_slot);
    let (Ok(chrome_file), Ok(brave_file)) = (
        std::fs::read(r"C:\Program Files\Google\Chrome\Application\156.0.8078.80\chrome.dll"),
        std::fs::read(
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll",
        ),
    ) else {
        return;
    };
    let slot = chrome.bounds_changed_slot.unwrap();
    let ours = vtable_entry_after(&chrome_file, chrome.web_view_ctor, slot).expect("Chrome's slot");
    let theirs = vtable_entry_after(&brave_file, brave.web_view_ctor, slot).expect("Brave's slot");
    // The first bytes up to the stack cookie's address are the same function.
    assert_eq!(ours[..13], theirs[..13]);
}

#[test]
fn fr12_edge_58_whale_57_and_samsung_29_record_the_page_view_bounds_slot() {
    // Without the slot the browser lays the page view out to the full page area again after every
    // move below the bar, and the bar keeps covering the top of the page. Each slot holds the same
    // `WebView::OnBoundsChanged` as Brave 156.1.98.63's measured slot.
    let table: serde_json::Value =
        serde_json::from_str(CHECKED_IN).expect("the checked-in table is JSON");
    let edge = table["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["build"]["browser"] == "Microsoft Edge"
                    && row["build"]["version"] == serde_json::json!([156, 0, 4314, 58])
            })
        })
        .expect("the table holds a row for Edge 156.0.4314.58");
    let edge_slot = usize::from_str_radix(
        edge["build"]["app_window"]["bounds_changed_slot"]
            .as_str()
            .expect("Edge .58 records the slot")
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let edge_ctor = usize::from_str_radix(
        edge["build"]["app_window"]["web_view_ctor"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    assert_eq!(edge_slot, 0x57);
    let whale = super::whale::WHALE.app_window.as_ref().unwrap();
    let samsung = SAMSUNG.app_window.as_ref().unwrap();
    assert_eq!(whale.bounds_changed_slot, Some(0x4B));
    assert_eq!(samsung.bounds_changed_slot, Some(0x4B));
    let brave = BRAVE_63.app_window.as_ref().unwrap();
    let Ok(brave_file) = std::fs::read(
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll",
    ) else {
        return;
    };
    let theirs = vtable_entry_after(
        &brave_file,
        brave.web_view_ctor,
        brave.bounds_changed_slot.unwrap(),
    )
    .expect("Brave's slot");
    for (path, ctor, slot) in [
        (
            r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.58\msedge.dll",
            edge_ctor,
            edge_slot,
        ),
        (WHALE_DLL, whale.web_view_ctor, 0x4B),
        (
            r"C:\Program Files\Samsung\Internet\Application\143.0.4.29\samsunginternet.dll",
            samsung.web_view_ctor,
            0x4B,
        ),
    ] {
        let Ok(file) = std::fs::read(path) else {
            continue;
        };
        let ours = vtable_entry_after(&file, ctor, slot).expect("the slot");
        // The first bytes up to the stack cookie's address are the same function.
        assert_eq!(ours[..13], theirs[..13], "{path}");
    }
}

#[test]
fn fr12_edge_58_and_whale_57_record_the_page_view_that_moves_below_the_bar() {
    // Neither build records the Sidebar's layout, so the bar's row is kept free by moving the page
    // view, which needs the second view's constructor, its allocation and `AddChildViewAt`.
    let table: serde_json::Value =
        serde_json::from_str(CHECKED_IN).expect("the checked-in table is JSON");
    let edge = table["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["build"]["browser"] == "Microsoft Edge"
                    && row["build"]["version"] == serde_json::json!([156, 0, 4314, 58])
            })
        })
        .expect("the table holds a row for Edge 156.0.4314.58");
    assert_eq!(
        edge["build"]["side_panel"]["layout_vtable"],
        format!("{ABSENT:#x}")
    );
    for name in ["operator_new", "web_view_ctor", "add_child", "set_contents"] {
        assert!(
            edge["build"]["app_window"][name].is_string(),
            "Edge .58 records {name}"
        );
    }
    let whale = &super::whale::WHALE;
    assert_eq!(whale.version, [5, 39, 412, 57]);
    assert_eq!(whale.side_panel.layout_vtable, 0x0F9F_BB38);
    let row = whale
        .app_window
        .as_ref()
        .expect("Whale .57 records the app window");
    for name in ["new", "ctor", "add_child", "set_contents"] {
        assert!(
            crate::features::app_window::row_value(row, name).is_some(),
            "Whale .57 records {name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn nfr6_newest_row_of_each_ported_browser_has_its_symbol_key() {
    // The row job ports a new build from the newest row of its browser, read through that row's
    // public symbols. A newest row without its symbol key stops every later row of the browser.
    let table: serde_json::Value =
        serde_json::from_str(CHECKED_IN).expect("the checked-in table is JSON");
    let mut rows = crate::compiled_rows();
    rows.extend(table["rows"].as_array().cloned().unwrap_or_default());
    let version_of = |row: &serde_json::Value| -> Vec<u64> {
        row["build"]["version"]
            .as_array()
            .map(|parts| parts.iter().filter_map(serde_json::Value::as_u64).collect())
            .unwrap_or_default()
    };
    for browser in ["Google Chrome", "Microsoft Edge", "Brave"] {
        let newest = rows
            .iter()
            .filter(|row| row["build"]["browser"] == browser && !row["frame"].is_null())
            .max_by_key(|row| version_of(row))
            .unwrap_or_else(|| panic!("{browser} has a row"));
        let version: Vec<String> = version_of(newest).iter().map(u64::to_string).collect();
        let key = format!("{browser} {}", version.join("."));
        assert!(
            table["symbols"][key.as_str()].is_string(),
            "{key} has no symbol key in rows/table.json"
        );
    }
}

#[cfg(windows)]
#[test]
fn nfr6_recipes_read_every_row_value_they_cover_again_from_its_own_module() {
    // Replays the row job's recipes on the modules this machine has: the installed Chrome and
    // Brave 157, the held Chrome 156 and the installed Edge 156. Each hand-made row value that a
    // recipe covers must come out the same, which is what lets a later build get them without a
    // person. Chromium 157 moved these values (0x560 to 0x558, 0x528 to 0x520, slot 127 to 126),
    // so a recipe that only copied them would fail here.
    use crate::row_data::RowValue;
    use rivet_platform_windows::{chrome_build::PeFile, row_recipe};
    let table: serde_json::Value =
        serde_json::from_str(CHECKED_IN).expect("the checked-in table is JSON");
    let edge_156 = table["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["build"]["browser"] == "Microsoft Edge"
                    && row["build"]["version"] == serde_json::json!([156, 0, 4314, 58])
            })
        })
        .map(|row| row["build"].clone());
    let mut cases: Vec<(serde_json::Value, std::path::PathBuf)> = vec![
        (
            crate::builds::CHROME_157_38.to_row(),
            r"C:\Program Files\Google\Chrome\Application\157.0.8103.38\chrome.dll".into(),
        ),
        (
            crate::builds::BRAVE_157_55.to_row(),
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\157.1.99.55\chrome.dll"
                .into(),
        ),
    ];
    if let Some(held) = rivet_core::paths::Layout::current().map(|layout| layout.held_browsers()) {
        cases.push((
            crate::builds::CHROME_156_80.to_row(),
            held.join(r"chrome\156.0.8078.80\156.0.8078.80\chrome.dll"),
        ));
    }
    if let Some(edge) = edge_156 {
        cases.push((
            edge,
            r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.58\msedge.dll".into(),
        ));
    }
    for (row, module) in cases {
        // A machine without this build has nothing to replay it on.
        let Ok(bytes) = std::fs::read(&module) else {
            continue;
        };
        let pe = PeFile::parse(&bytes).expect("a 64-bit module");
        let mut derived_row = row.clone();
        let derived = row_recipe::derive_row(&mut derived_row, &pe);
        let name = format!("{} {}", row["browser"], row["version"]);
        assert!(derived.failed.is_empty(), "{name}: {:?}", derived.failed);
        assert_eq!(derived_row, row, "{name}: {:?}", derived.read);
        // The member offset, the object size and the frame slots at least.
        assert!(derived.read.len() >= 7, "{name}: {:?}", derived.read);
    }
}

#[cfg(windows)]
#[test]
fn nfr6_a_table_row_carries_the_page_tools_and_bubble_anchor_of_its_prior_row() {
    use crate::row_data::RowValue;
    // The generator ports every part of the built-in row it is given, so the parts kept in lists
    // of their own must be in that row.
    let rows = crate::compiled_rows();
    let chrome = rows
        .iter()
        .find(|row| {
            row["build"]["browser"] == "Google Chrome"
                && row["build"]["version"] == serde_json::json!([157, 0, 8103, 38])
        })
        .expect("Chrome 157.0.8103.38 is built in");
    assert!(chrome["build"]["page_tools"].is_object());
    assert!(chrome["build"]["bubble_anchor"].is_array());
    // A table row with those parts gives a build that has them, with no list entry for it.
    let mut row = chrome["build"].clone();
    row["version"] = serde_json::json!([157, 0, 8103, 99]);
    let build: &'static Build = Box::leak(Box::new(Build::from_row(&row).unwrap()));
    let tools = crate::builds::page_tool_rows::page_tools_for(build).expect("the row's page tools");
    assert_eq!(
        tools.update_target_url,
        crate::builds::page_tool_rows::page_tools_for(&CHROME_157_38)
            .unwrap()
            .update_target_url
    );
    assert_eq!(
        crate::builds::bubble_anchor_of(build),
        crate::builds::bubble_anchor_of(&CHROME_157_38)
    );
}
