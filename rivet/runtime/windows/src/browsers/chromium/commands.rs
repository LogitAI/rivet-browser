//! Chromium's browser command ids, which a browser window runs from its own `WM_COMMAND`.
//!
//! The window passes the id from `HWNDMessageHandler::OnCommand` through
//! `DesktopWindowTreeHostWin::HandleAppCommand` and `BrowserView::ExecuteWindowsCommand` to
//! `chrome::ExecuteCommand`, as its own menu does, so no build address is needed. The ids are
//! defined in `chrome/app/chrome_command_ids.h` and some move between milestones. Each value below
//! was read from that header at the milestone's branch on chromium.googlesource.com. A milestone
//! without a read value has no id, and what needs it stays off, because a wrong id runs another
//! command.

/// `IDC_MOVE_TAB_TO_NEW_WINDOW` per Chromium milestone, with the branch the header was read at
/// (`refs/branch-heads/<branch>/chrome/app/chrome_command_ids.h`). It was 34054 at milestone 130.
pub(crate) const MOVE_TAB_TO_NEW_WINDOW: &[(u16, u16, usize)] = &[
    (150, 7871, 34056),
    (151, 7922, 34056),
    (152, 7977, 34056),
    (153, 8010, 34056),
    (154, 8037, 34056),
    (155, 8059, 34056),
    (156, 8078, 34056),
    (157, 8103, 34056),
    (158, 8127, 34056),
];

/// The Chromium milestone a build is made from. Chrome, Edge and Brave carry it as the first part
/// of their version. Whale and Samsung Internet number their own releases, so their base is the
/// Chromium version their module names, kept here per release.
pub(crate) fn chromium_milestone(browser: &str, version: [u16; 4]) -> Option<u16> {
    match browser {
        "Google Chrome" | "Microsoft Edge" | "Brave" => Some(version[0]),
        // Whale 5.39 is built on Chromium 150.0.7871.
        "Naver Whale" => (version[..2] == [5, 39]).then_some(150),
        // Samsung Internet 30.0 is built on Chromium 143.0.7499.
        "Samsung Internet" => (version[..2] == [30, 0]).then_some(143),
        _ => None,
    }
}

/// Whether a browser keeps Chromium's command ids without public source to confirm it. Brave
/// publishes its changes and adds its own ids only from 56000, so it keeps Chromium's. Edge, Whale
/// and Samsung Internet take the value of their Chromium milestone, which needs a regression check
/// on each build before support is claimed.
pub(crate) fn needs_regression_check(browser: &str) -> bool {
    !matches!(browser, "Google Chrome" | "Brave")
}

/// The id of `IDC_MOVE_TAB_TO_NEW_WINDOW` in a build, or why it has none.
pub(crate) fn move_tab_to_new_window(browser: &str, version: [u16; 4]) -> Result<usize, String> {
    let milestone = chromium_milestone(browser, version)
        .ok_or_else(|| format!("the Chromium milestone of {browser} {version:?} is not known"))?;
    MOVE_TAB_TO_NEW_WINDOW
        .iter()
        .find(|(known, _, _)| *known == milestone)
        .map(|(_, _, id)| *id)
        .ok_or_else(|| {
            format!("no confirmed move-tab-to-new-window command id for Chromium {milestone}")
        })
}

/// Whether the browser can run the move-tab-to-new-window command in a window of a process started
/// with `arguments`, or why it cannot.
///
/// `chrome::MoveTabsToNewWindow` asks an app window's `web_app::AppBrowserController` whether it
/// has a tab strip, without checking that there is one (`browser_commands.cc`). Only a window of an
/// installed web app or hosted app has that controller: `MaybeCreateAppBrowserController` finds the
/// app by the window's app name, and `GetAppIdFromApplicationName` reads an id only from a name that
/// starts with `_crx_`. A window opened with `--app=<url>` gets a name made from the address
/// (`GenerateApplicationNameFromURL`), so it has no controller and the command reads through a null
/// pointer. The command is enabled as soon as the window holds more than one tab.
pub(crate) fn move_tab_command_is_safe(
    arguments: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    let mut url_app = false;
    let mut installed_app = false;
    for argument in arguments {
        url_app |= argument.starts_with("--app=");
        installed_app |= argument.starts_with("--app-id=");
    }
    if url_app && !installed_app {
        return Err(
            "an --app=<url> window has no app controller, which Chromium's \
                    move-tab-to-new-window reads without a check"
                .to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr9_mini_window_uses_the_move_tab_command() {
        // Every milestone Rivet supports has the value its header defines.
        for milestone in 150..=158u16 {
            assert_eq!(
                move_tab_to_new_window("Google Chrome", [milestone, 0, 1, 1]),
                Ok(34056),
                "Chromium {milestone}"
            );
        }
        assert_eq!(move_tab_to_new_window("Brave", [157, 1, 99, 55]), Ok(34056));
        assert_eq!(
            move_tab_to_new_window("Microsoft Edge", [156, 0, 4314, 58]),
            Ok(34056)
        );
        // Whale 5.39 takes Chromium 150's value.
        assert_eq!(
            move_tab_to_new_window("Naver Whale", [5, 39, 412, 57]),
            Ok(34056)
        );
        // Each milestone appears once, with the branch it was read at.
        let mut milestones: Vec<u16> = MOVE_TAB_TO_NEW_WINDOW.iter().map(|row| row.0).collect();
        milestones.dedup();
        assert_eq!(milestones, (150..=158).collect::<Vec<_>>());
        assert!(MOVE_TAB_TO_NEW_WINDOW
            .windows(2)
            .all(|pair| pair[0].1 < pair[1].1));
    }

    #[test]
    fn fr9_unknown_milestone_turns_the_mini_window_off() {
        // Milestone 130 used 34054, so an unread milestone never borrows a neighbour's value.
        assert!(move_tab_to_new_window("Google Chrome", [149, 0, 1, 1]).is_err());
        assert!(move_tab_to_new_window("Google Chrome", [159, 0, 1, 1]).is_err());
        // Samsung Internet 30 is built on Chromium 143, which has no read value.
        assert!(move_tab_to_new_window("Samsung Internet", [30, 0, 4, 29]).is_err());
        assert!(move_tab_to_new_window("Naver Whale", [5, 40, 1, 1]).is_err());
        assert!(move_tab_to_new_window("Opera", [130, 0, 1, 1]).is_err());
    }

    #[test]
    fn fr9_browsers_without_public_command_ids_need_a_regression_check() {
        assert!(!needs_regression_check("Google Chrome"));
        assert!(!needs_regression_check("Brave"));
        assert!(needs_regression_check("Microsoft Edge"));
        assert!(needs_regression_check("Naver Whale"));
        assert!(needs_regression_check("Samsung Internet"));
    }

    #[test]
    fn fr9_an_app_url_window_keeps_the_mini_window_off() {
        let args = |list: &[&str]| list.iter().map(|text| text.to_string()).collect::<Vec<_>>();
        assert!(
            move_tab_command_is_safe(args(&["chrome.exe", "--app=https://example.test/"])).is_err()
        );
        assert!(move_tab_command_is_safe(args(&[
            "chrome.exe",
            "--app=https://example.test/",
            "--app-id=abcdefghijklmnopabcdefghijklmnop",
        ]))
        .is_ok());
        assert!(move_tab_command_is_safe(args(&["chrome.exe"])).is_ok());
    }
}
