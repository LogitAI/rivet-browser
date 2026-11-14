//! Chromium's browser command ids that Rivet runs, per Chromium milestone.
//!
//! A browser window runs these ids from its own `WM_COMMAND`, as its own menu and keys do, so they
//! need no address or member of the build. They are defined in `chrome/app/chrome_command_ids.h`,
//! and each value below was read from that header at the milestone's branch on
//! chromium.googlesource.com (`refs/branch-heads/<branch>/chrome/app/chrome_command_ids.h`). They
//! are public and stable, so they are not values of a build row: the row generator and the
//! runtime both take them from here. A milestone without a read value has no id, and what needs it
//! stays off, because a wrong id runs another command.

/// `IDC_DEV_TOOLS_INSPECT` and `IDC_DEV_TOOLS_CONSOLE` per Chromium milestone, with the branch the
/// header was read at. They are the commands of the Ctrl+Shift+C and Ctrl+Shift+J keys.
pub const DEV_TOOLS: &[(u16, u16, u32, u32)] = &[
    (150, 7871, 40023, 40005),
    (151, 7922, 40023, 40005),
    (152, 7977, 40023, 40005),
    (153, 8010, 40023, 40005),
    (154, 8037, 40023, 40005),
    (155, 8059, 40023, 40005),
    (156, 8078, 40023, 40005),
    (157, 8103, 40023, 40005),
    (158, 8127, 40023, 40005),
    (159, 8154, 40023, 40005),
];

/// The paths of a row's values that are these command ids. A row generator never carries them:
/// it writes the value this table gives, or zero for a milestone it does not know.
pub const ROW_PATHS: &[&str] = &["toolbar.dev.inspect", "toolbar.dev.console"];

/// The inspect and console command ids of a Chromium milestone.
pub fn dev_tools(milestone: u16) -> Option<(u32, u32)> {
    DEV_TOOLS
        .iter()
        .find(|(known, _, _, _)| *known == milestone)
        .map(|(_, _, inspect, console)| (*inspect, *console))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_developer_commands_come_from_chromium_command_ids() {
        for milestone in 150..=159u16 {
            assert_eq!(dev_tools(milestone), Some((40023, 40005)), "{milestone}");
        }
        // An unread milestone never borrows a neighbour's value.
        assert_eq!(dev_tools(149), None);
        assert_eq!(dev_tools(160), None);
        // Each milestone appears once, in branch order.
        assert!(DEV_TOOLS
            .windows(2)
            .all(|pair| pair[0].0 + 1 == pair[1].0 && pair[0].1 < pair[1].1));
    }
}
