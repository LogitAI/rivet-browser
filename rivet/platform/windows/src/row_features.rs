//! Which feature of a row each value belongs to, and how a row turns that feature off.
//!
//! A row value that could not be read from its own build must never reach a browser. When it
//! belongs to a feature the runtime can do without, the row turns that feature off and clears the
//! value, and the rest of the row can still ship once its checks pass. A value of a part the
//! window cannot do without (the frame and the page area layout) has no switch here, so it still
//! stops the row and the browser opens with Rivet's held build.
//!
//! The switches are the ones the runtime already obeys: a section's `supported` flag, an optional
//! part left `null`, an empty list of colour ids, and the frame colour's `provider_verified`. Each
//! one was checked against every reader of the values it covers, so a feature that is off reads
//! none of them. The same switches work in older runtimes, which read the same row fields.

use serde_json::{json, Value};

/// The address a build records for what it does not have.
const ABSENT: &str = "0x7fff0000";

/// What turns a feature off in a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// A `supported` or `provider_verified` flag set to false.
    Flag(&'static str),
    /// An optional part set to `null`.
    Part(&'static str),
}

/// What a value of a feature that is off becomes, so the row holds nothing of another build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    /// Every number in it becomes zero, in the form it was written.
    Zero,
    /// The address the build does not have, which a reader takes as nothing there.
    Absent,
    /// An empty list.
    Empty,
}

/// A feature of a row and the values only it reads.
pub struct RowFeature {
    /// The feature's name, as the runtime and the report give it.
    pub name: &'static str,
    /// Row paths of its values. A path that ends with `.` covers the whole section.
    pub paths: &'static [&'static str],
    pub off: &'static [Off],
    pub clear: Clear,
    /// The checks of `scripts/tests/windows-features.ps1` that need the feature. A row that has the
    /// feature off is not expected to pass them. A prefix ending with `-` covers its family.
    pub checks: &'static [&'static str],
}

pub const ROW_FEATURES: &[RowFeature] = &[
    RowFeature {
        name: "split view",
        paths: &["split."],
        off: &[Off::Flag("split.supported")],
        clear: Clear::Zero,
        checks: &["split", "split-"],
    },
    RowFeature {
        name: "vertical tabs",
        paths: &[
            "vertical.update_collapse_action",
            "vertical.browser_offset",
            "vertical.mode_offset",
            "vertical.width_offset",
        ],
        off: &[Off::Flag("vertical.supported")],
        clear: Clear::Zero,
        checks: &[],
    },
    RowFeature {
        name: "sidebar",
        paths: &[
            "side_panel.color_source",
            "side_panel.source_in_widget",
            "side_panel.entry_count",
            "side_panel.extension_entry",
            "side_panel.features_coordinator",
            "side_panel.features_offset",
            "side_panel.browser_offset",
        ],
        off: &[Off::Flag("side_panel.supported")],
        clear: Clear::Zero,
        checks: &["sidebar", "sidebar-", "fullscreen-sidebar"],
    },
    RowFeature {
        name: "sidebar",
        paths: &["side_panel.hidden_entries"],
        off: &[Off::Flag("side_panel.supported")],
        clear: Clear::Empty,
        checks: &["sidebar", "sidebar-", "fullscreen-sidebar"],
    },
    RowFeature {
        // Fields of the browser's layout object that only the Sidebar's part of the layout reads.
        // The layout reads an absent field as no view.
        name: "sidebar",
        paths: &[
            "side_panel.side_panel",
            "side_panel.side_panel_animation",
            "side_panel.contents_outline",
            "side_panel.vertical_tabs",
        ],
        off: &[Off::Flag("side_panel.supported")],
        clear: Clear::Absent,
        checks: &["sidebar", "sidebar-", "fullscreen-sidebar"],
    },
    RowFeature {
        // The Sidebar's own functions and class tables. Only `features/side_panel.rs` reads them,
        // and only once the Sidebar's targets are all recorded.
        name: "sidebar",
        paths: &[
            "side_panel.side_panel_vtable",
            "side_panel.coordinator_vtable",
            "side_panel.browser_view_view_vtable",
            "side_panel.show",
            "side_panel.entry_id_to_string",
            "side_panel.toggle",
            "side_panel.entry_showing",
            "side_panel.registry_from",
            "side_panel.get_color_provider",
            "side_panel.generate_colors_css",
            "side_panel.event_handler_for_point",
        ],
        off: &[Off::Flag("side_panel.supported")],
        clear: Clear::Absent,
        checks: &["sidebar", "sidebar-", "fullscreen-sidebar"],
    },
    RowFeature {
        // The vertical tab controller's functions and class tables. Only
        // `features/vertical_tabs.rs` reads them, and only once its targets are all recorded.
        name: "vertical tabs",
        paths: &[
            "vertical.slot",
            "vertical.should_display",
            "vertical.destructor",
            "vertical.browser_view_for_browser",
            "vertical.hwnd_for_view",
            "vertical.notify_mode_changed",
            "vertical.browser_view_view_vtable",
            "vertical.region_preferred_size",
            "vertical.region_preferred_size_slot",
            "vertical.region_minimum_size",
            "vertical.invalidate_layout",
        ],
        off: &[Off::Flag("vertical.supported")],
        clear: Clear::Absent,
        checks: &[],
    },
    RowFeature {
        name: "app window",
        paths: &["app_window."],
        off: &[Off::Part("app_window")],
        clear: Clear::Zero,
        checks: &["mini-", "tab-moves"],
    },
    RowFeature {
        name: "address suggestions",
        paths: &["omnibox."],
        off: &[Off::Part("omnibox")],
        clear: Clear::Zero,
        checks: &["address-suggest", "security-chip"],
    },
    RowFeature {
        // An empty list is a build without developer colours, whose page keeps the browser's own.
        name: "developer colours",
        paths: &["toolbar.dev.colours"],
        off: &[],
        clear: Clear::Empty,
        checks: &["dev-row"],
    },
    RowFeature {
        // Font functions recorded beside the developer colours. No runtime calls them.
        name: "developer colours",
        paths: &["toolbar.dev.font_list_new", "toolbar.dev.font_new"],
        off: &[],
        clear: Clear::Absent,
        checks: &["dev-row"],
    },
    RowFeature {
        name: "frame colour",
        paths: &["layout.frame_ids", "layout.caption_ink_ids"],
        off: &[Off::Flag("layout.provider_verified")],
        clear: Clear::Empty,
        checks: &[],
    },
];

/// Values the runtime finds in the browser's own memory by Chromium's class names and the shape of
/// the view tree, so a row needs none of them. Where a row has one, the runtime only compares it
/// with what it found. A value no instruction of the new build gave is left out of the row rather
/// than carried from the prior build, and does not stop the row: the browser view, the area below
/// the top row and the view of the pages are the layout's views named `BrowserView`,
/// `MainBackgroundRegionView` and `MultiContentsView`; the frame view's field is the one that holds
/// that same browser view; and a view's widget is the one the root of its view tree names. A
/// runtime from before this cannot read a row without them, so it refuses the row and opens its
/// held build instead of guessing.
pub const REFLECTED: &[&str] = &[
    "layout.frame_browser_window_offset",
    "side_panel.contents_container",
    "side_panel.host_view",
    "side_panel.page_area",
    "toolbar.get_widget_slot",
];

/// Whether the runtime finds a value by reflection, so a row may leave it out.
pub fn is_reflected(path: &str) -> bool {
    REFLECTED.contains(&field_of(path))
}

/// Leaves the field a value path names out of a row's build part.
pub fn leave_out(build: &mut Value, path: &str) {
    let field = field_of(path);
    let (parent, key) = match field.rsplit_once('.') {
        Some((parent, key)) => (Some(parent), key),
        None => (None, field),
    };
    let holder = match parent {
        Some(parent) => at_mut(build, parent),
        None => Some(build),
    };
    if let Some(object) = holder.and_then(Value::as_object_mut) {
        object.remove(key);
    }
}

/// Values the runtime does not read while another value of the row says so: then they are not
/// values of the build at all, and a row holds zero for them.
pub struct UnusedWhen {
    pub paths: &'static [&'static str],
    /// The value of the row that decides it.
    pub when: When,
    pub why: &'static str,
}

/// What a deciding value of a row must be for other values to be unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// An address the build records.
    Recorded(&'static str),
    /// An address the build does not record.
    Absent(&'static str),
    /// A flag that is false.
    False(&'static str),
}

pub const UNUSED_WHEN: &[UnusedWhen] = &[
    UnusedWhen {
        paths: &["tabs.tab_collection", "tabs.tab_contents"],
        when: When::Recorded("tabs.tab_at"),
        why: "the build has its own GetWebContentsAt",
    },
    UnusedWhen {
        paths: &[
            "vertical.window_feature_tab_strip",
            "vertical.window_feature_callers",
            "vertical.toolbar_in_view",
            "vertical.toolbar_leading",
        ],
        when: When::Absent("vertical.window_feature"),
        why: "the tab strip is left out through ShouldDrawTabStrip, not the window feature",
    },
    UnusedWhen {
        paths: &["vertical.should_draw_tab_strip"],
        when: When::False("vertical.detour_strip"),
        why: "the build does not leave its tab strip out by a detour",
    },
];

/// The field a value path names: `layout.frame_ids[2]` names `layout.frame_ids`.
pub fn field_of(path: &str) -> &str {
    path.split('[').next().unwrap_or(path)
}

fn covers(rule: &str, path: &str) -> bool {
    let field = field_of(path);
    if rule.ends_with('.') {
        field.starts_with(rule)
    } else {
        field == rule
    }
}

/// The feature a value belongs to, when the row can turn it off.
pub fn feature_of(path: &str) -> Option<&'static RowFeature> {
    ROW_FEATURES
        .iter()
        .find(|feature| feature.paths.iter().any(|rule| covers(rule, path)))
}

fn at<'a>(build: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(build, |value, key| value.get(key))
}

fn at_mut<'a>(build: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    path.split('.')
        .try_fold(build, |value, key| value.get_mut(key))
}

fn is_recorded(value: &Value) -> bool {
    crate::row_recipe::number(value).is_some_and(|value| (0x10_0000..0x7FFF_0000).contains(&value))
}

/// Why the runtime does not read a value in this row, or `None` when it does.
pub fn unused(build: &Value, path: &str) -> Option<&'static str> {
    UNUSED_WHEN
        .iter()
        .find(|rule| {
            rule.paths.iter().any(|each| covers(each, path))
                && match rule.when {
                    When::Recorded(address) => at(build, address).is_some_and(is_recorded),
                    When::Absent(address) => {
                        at(build, address).is_some_and(|value| !is_recorded(value))
                    }
                    When::False(flag) => at(build, flag).and_then(Value::as_bool) == Some(false),
                }
        })
        .map(|rule| rule.why)
}

fn zero(value: &mut Value) {
    if let Value::Array(items) = value {
        items.iter_mut().for_each(zero);
        return;
    }
    let cleared = match &*value {
        Value::String(text) if text.starts_with("0x") => json!("0x0"),
        Value::Number(_) => json!(0),
        _ => return,
    };
    *value = cleared;
}

/// Clears the field a value path names, as `clear` says.
pub fn clear(build: &mut Value, path: &str, how: Clear) {
    let Some(value) = at_mut(build, field_of(path)) else {
        return;
    };
    match how {
        Clear::Zero => zero(value),
        Clear::Absent => *value = json!(ABSENT),
        Clear::Empty => *value = json!([]),
    }
}

/// Turns a feature off in a row's build part.
pub fn switch_off(build: &mut Value, feature: &RowFeature) {
    for off in feature.off {
        match *off {
            Off::Flag(path) => {
                if let Some(flag) = at_mut(build, path) {
                    *flag = json!(false);
                }
            }
            Off::Part(name) => build[name] = Value::Null,
        }
    }
}

/// Turns off the feature of that name, through every part of the row it has, and clears each of
/// its values that a path names, so a feature that is off holds nothing of another build.
pub fn turn_off(build: &mut Value, name: &str) {
    for feature in ROW_FEATURES.iter().filter(|feature| feature.name == name) {
        switch_off(build, feature);
        for path in feature.paths.iter().filter(|path| !path.ends_with('.')) {
            clear(build, path, feature.clear);
        }
    }
}

/// How a value of a feature that is off is cleared where it sits: an address inside a list of
/// signatures becomes zero with the rest of that list, since a list holds no single address.
pub fn clear_value(build: &mut Value, path: &str, feature: &RowFeature) {
    let how = match feature.clear {
        Clear::Absent if path.contains('[') => Clear::Zero,
        how => how,
    };
    clear(build, path, how);
}

/// The key of a row that names the features it has off and the values that turned them off. The
/// runtime reads only `frame` and `build`, so it changes nothing in a browser; the feature gate
/// reads it.
pub const SWITCHED_OFF: &str = "switched_off";

/// The feature checks a row's switched-off features excuse, by the names the row gives.
pub fn excused_checks(row: &Value) -> Vec<(&'static str, &'static str)> {
    let names: Vec<&str> = row[SWITCHED_OFF]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|entry| entry["feature"].as_str())
        .collect();
    let mut out = Vec::new();
    for feature in ROW_FEATURES
        .iter()
        .filter(|feature| names.contains(&feature.name))
    {
        for check in feature.checks {
            if !out.contains(&(feature.name, *check)) {
                out.push((feature.name, *check));
            }
        }
    }
    out
}

/// The switched-off feature that excuses a feature check, if one does.
pub fn excuses(excused: &[(&'static str, &'static str)], check: &str) -> Option<&'static str> {
    excused
        .iter()
        .find(|(_, rule)| {
            if rule.ends_with('-') {
                check.starts_with(rule)
            } else {
                check == *rule
            }
        })
        .map(|(name, _)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr6_every_row_value_of_a_feature_has_one_switch() {
        assert_eq!(feature_of("split.view_size").unwrap().name, "split view");
        assert_eq!(
            feature_of("toolbar.dev.colours[3][0]").unwrap().name,
            "developer colours"
        );
        assert_eq!(
            feature_of("layout.frame_ids[1]").unwrap().name,
            "frame colour"
        );
        assert_eq!(
            feature_of("omnibox.match_size").unwrap().name,
            "address suggestions"
        );
        // The frame and the page area layout have no switch: the window cannot do without them.
        for core in ["layout.view_bounds_offset", "side_panel.toolbar"] {
            assert!(feature_of(core).is_none(), "{core}");
            assert!(!is_reflected(core), "{core}");
        }
        // The window's views and widget have no switch either; the runtime finds them itself.
        for reflected in REFLECTED {
            assert!(feature_of(reflected).is_none(), "{reflected}");
            assert!(is_reflected(reflected), "{reflected}");
        }
        // A value that only looks like a section's name is not in it.
        assert!(feature_of("splitter.width").is_none());
    }

    #[test]
    fn nfr6_a_reflected_value_is_left_out_of_the_row() {
        let mut build = json!({
            "layout": {"frame_browser_window_offset": "0x340", "view_bounds_offset": "0x1c8"},
            "toolbar": {"get_widget_slot": "0xb"},
        });
        leave_out(&mut build, "layout.frame_browser_window_offset");
        leave_out(&mut build, "toolbar.get_widget_slot");
        assert_eq!(
            build,
            json!({"layout": {"view_bounds_offset": "0x1c8"}, "toolbar": {}})
        );
    }

    #[test]
    fn nfr6_a_value_the_runtime_does_not_read_is_not_a_value_of_the_build() {
        let build = json!({
            "tabs": {"tab_at": "0x326a00", "tab_collection": "0x30"},
            "vertical": {"window_feature": "0x7fff0000", "window_feature_callers": ["0x0", "0x0"]},
        });
        assert!(unused(&build, "tabs.tab_collection").is_some());
        assert!(unused(&build, "vertical.window_feature_callers[1]").is_some());
        let build = json!({
            "tabs": {"tab_at": "0x7fff0000"},
            "vertical": {"window_feature": "0x1234560"},
        });
        assert!(unused(&build, "tabs.tab_collection").is_none());
        assert!(unused(&build, "vertical.toolbar_in_view").is_none());
    }

    #[test]
    fn nfr6_a_switched_off_feature_excuses_only_its_own_checks() {
        let row =
            json!({"switched_off": [{"feature": "split view"}, {"feature": "developer colours"}]});
        let excused = excused_checks(&row);
        assert_eq!(excuses(&excused, "split-grid-4"), Some("split view"));
        assert_eq!(excuses(&excused, "split"), Some("split view"));
        assert_eq!(excuses(&excused, "dev-row"), Some("developer colours"));
        assert_eq!(excuses(&excused, "sidebar"), None);
        assert_eq!(excuses(&excused, "bar-layout"), None);
        assert!(excused_checks(&json!({})).is_empty());
    }
}
