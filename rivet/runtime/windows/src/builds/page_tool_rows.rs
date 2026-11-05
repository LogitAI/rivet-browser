//! Chromium functions for opening DevTools on a panel and for reading the link under the pointer.
//!
//! They are kept out of the rows so the signed rows keep their fields. Every address here was read
//! from the build's own public symbols, and every prologue from its installed module. None of them
//! has been driven in a browser yet, so they are unverified.

use super::Build;

/// One Chromium function: its address and its first bytes, which are checked before any call or
/// detour.
pub type Function = (usize, &'static [u8]);

#[derive(Debug, Clone, Copy)]
pub struct PageTools {
    /// `DevToolsWindow::ToggleDevToolsWindow(WebContents*, Profile*, bool force_open,
    /// const DevToolsToggleAction&, const std::string& settings, DevToolsOpenedByAction,
    /// const DevToolsManagerDelegate::DevToolsOptions&)`, the private static that every way of
    /// opening DevTools ends in. A new DevTools window applies `settings` to the profile's DevTools
    /// preferences before its frontend loads.
    pub toggle_devtools: Function,
    /// `DevToolsToggleAction::Show()`. Its bytes are the whole function: it writes type 0 and no
    /// reveal parameters, which is the action Rivet builds in place.
    pub show_action: Function,
    /// `WebContents::GetBrowserContext` and `Profile::FromBrowserContext`.
    pub browser_context: Function,
    pub profile_from_context: Function,
    /// `BrowserWebContentsDelegate::UpdateTargetURL(WebContents*, const GURL&)`: the address of the
    /// link under the pointer, which the browser shows in its status bubble. The prologue ends on an
    /// instruction boundary and holds no address relative to itself.
    pub update_target_url: Function,
}

/// `DevToolsToggleAction::Show`, the same in every build here.
const SHOW: &[u8] = &[
    0x48, 0x89, 0xC8, 0xC7, 0x01, 0x00, 0x00, 0x00, 0x00, 0x48, 0xC7, 0x41, 0x08, 0x00, 0x00, 0x00,
    0x00, 0xC3,
];
const PROFILE_FROM_CONTEXT: &[u8] = &[0x48, 0x89, 0xC8, 0xC3];
const BROWSER_CONTEXT: &[u8] = &[0x48, 0x8B, 0x81, 0x90, 0x02, 0x00, 0x00, 0xC3];
/// `push` of eight registers and `sub rsp, imm32`.
const TOGGLE_156: &[u8] = &[
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xB0, 0x00,
    0x00, 0x00,
];
const TARGET_URL_156: &[u8] = &[
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81, 0xEC, 0x28,
    0x01, 0x00, 0x00,
];

/// Google Chrome 156.0.8078.80.
pub const CHROME_156_80: PageTools = PageTools {
    toggle_devtools: (0x0BA5_1330, TOGGLE_156),
    show_action: (0x05B3_C1F0, SHOW),
    browser_context: (0x0557_5630, BROWSER_CONTEXT),
    profile_from_context: (0x04BC_8250, PROFILE_FROM_CONTEXT),
    update_target_url: (0x02F4_78B0, TARGET_URL_156),
};

/// Brave 156.1.98.63 (Chromium 156.0.8078.80).
pub const BRAVE_63: PageTools = PageTools {
    toggle_devtools: (0x0CE7_7E50, TOGGLE_156),
    show_action: (0x0708_77C0, SHOW),
    browser_context: (0x06AC_7B40, BROWSER_CONTEXT),
    profile_from_context: (0x0613_B670, PROFILE_FROM_CONTEXT),
    update_target_url: (0x02E8_6660, TARGET_URL_156),
};

/// Microsoft Edge 156.0.4314.50, from Microsoft's public symbols. Only Chromium's functions.
pub const EDGE_156_50: PageTools = PageTools {
    toggle_devtools: (
        0x0C72_C02C,
        &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xD8, 0x00, 0x00, 0x00,
        ],
    ),
    show_action: (0x0659_14D0, SHOW),
    browser_context: (0x0116_4510, BROWSER_CONTEXT),
    profile_from_context: (0x00FD_EEE0, PROFILE_FROM_CONTEXT),
    update_target_url: (
        0x02F8_0280,
        &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x81,
            0xEC, 0xF8, 0x00, 0x00, 0x00,
        ],
    ),
};

/// The functions recorded for `build`, if any. Whale and Samsung publish no symbols and these
/// functions were not found by their shape, so they have none.
pub fn page_tools_for(build: &Build) -> Option<&'static PageTools> {
    let rows: [(&Build, &'static PageTools); 3] = [
        (&super::CHROME_156_80, &CHROME_156_80),
        (&super::BRAVE_63, &BRAVE_63),
        (&super::EDGE, &EDGE_156_50),
    ];
    rows.into_iter()
        .find(|(row, _)| row.browser == build.browser && row.version == build.version)
        .map(|(_, tools)| tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes of `file` at `rva`, read through its section table.
    fn file_bytes(file: &[u8], rva: usize, length: usize) -> Option<Vec<u8>> {
        let word = |at: usize| -> Option<usize> {
            Some(u32::from_le_bytes(file.get(at..at + 4)?.try_into().ok()?) as usize)
        };
        let half = |at: usize| -> Option<usize> {
            Some(u16::from_le_bytes(file.get(at..at + 2)?.try_into().ok()?) as usize)
        };
        let pe = word(0x3C)?;
        let (sections, optional) = (half(pe + 6)?, half(pe + 20)?);
        (0..sections).find_map(|index| {
            let header = pe + 24 + optional + 40 * index;
            let (address, size, raw) = (word(header + 12)?, word(header + 16)?, word(header + 20)?);
            (rva >= address && rva + length <= address + size)
                .then(|| file.get(raw + rva - address..raw + rva - address + length))
                .flatten()
                .map(<[u8]>::to_vec)
        })
    }

    fn every(tools: &PageTools) -> [(&'static str, Function); 5] {
        [
            ("ToggleDevToolsWindow", tools.toggle_devtools),
            ("DevToolsToggleAction::Show", tools.show_action),
            ("GetBrowserContext", tools.browser_context),
            ("Profile::FromBrowserContext", tools.profile_from_context),
            ("UpdateTargetURL", tools.update_target_url),
        ]
    }

    #[test]
    fn fr11_fr3_page_tool_functions_match_the_installed_modules() {
        for (path, tools) in [
            (
                r"C:\Program Files\Google\Chrome\Application\156.0.8078.80\chrome.dll",
                &CHROME_156_80,
            ),
            (
                r"C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll",
                &BRAVE_63,
            ),
            (
                r"C:\Program Files (x86)\Microsoft\Edge\Application\156.0.4314.50\msedge.dll",
                &EDGE_156_50,
            ),
        ] {
            let Ok(file) = std::fs::read(path) else {
                continue;
            };
            for (name, (rva, bytes)) in every(tools) {
                assert_eq!(
                    file_bytes(&file, rva, bytes.len()).as_deref(),
                    Some(bytes),
                    "{name} in {path}"
                );
            }
        }
    }

    #[test]
    fn fr11_fr3_page_tools_belong_to_their_rows_only() {
        assert!(page_tools_for(&super::super::CHROME_156_80).is_some());
        assert!(page_tools_for(&super::super::BRAVE_63).is_some());
        assert!(page_tools_for(&super::super::EDGE).is_some());
        assert!(page_tools_for(&super::super::WHALE).is_none());
        assert!(page_tools_for(&super::super::SAMSUNG).is_none());
        assert!(page_tools_for(&super::super::CHROME_156_74).is_none());
    }

    #[test]
    fn fr3_link_address_detour_has_room_for_its_jump() {
        for tools in [&CHROME_156_80, &BRAVE_63, &EDGE_156_50] {
            // `mov rax, hook; jmp rax` takes twelve bytes.
            assert!(tools.update_target_url.1.len() >= 12);
        }
    }
}
