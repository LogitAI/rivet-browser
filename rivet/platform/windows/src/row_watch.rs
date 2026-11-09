//! Finds browser builds that need a row, from what their makers publish about them.
//!
//! Google's version history lists every version a channel serves, with its rollout fraction, and a
//! version shows up there at fraction zero before anyone receives it. Microsoft's product feed
//! lists the installers of each channel. Brave's releases appear on GitHub as prereleases first.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

/// A build that may need a row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub browser: String,
    pub version: String,
    pub channel: String,
    /// People may have this build now, so it matters more than one still coming.
    pub serving: bool,
    /// The installer, when the feed names one.
    pub download: Option<String>,
}

/// A version as numbers, which compare in the right order.
pub fn parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// The versions from Google's `versions/all/releases?filter=endtime=none` of the stable channel
/// and `versions` of the beta channel. A version still at fraction zero has not reached anyone.
pub fn chrome_candidates(stable: &Value, beta: &Value) -> Vec<Candidate> {
    let mut found = Vec::new();
    for release in stable["releases"].as_array().map_or(&[][..], Vec::as_slice) {
        let Some(version) = release["version"].as_str() else {
            continue;
        };
        found.push(Candidate {
            browser: "Google Chrome".into(),
            version: version.into(),
            channel: "stable".into(),
            serving: release["fraction"].as_f64().unwrap_or(0.0) > 0.0,
            download: None,
        });
    }
    let beta = beta["versions"].as_array().map_or(&[][..], Vec::as_slice);
    if let Some(version) = beta
        .iter()
        .filter_map(|entry| entry["version"].as_str())
        .max_by_key(|version| parts(version))
    {
        found.push(Candidate {
            browser: "Google Chrome".into(),
            version: version.into(),
            channel: "beta".into(),
            serving: false,
            download: None,
        });
    }
    found
}

/// The newest x64 installers of Edge's stable and beta channels from its product feed. The newest
/// stable one is what people are getting, and the feed does not say more about the others.
pub fn edge_candidates(products: &Value) -> Vec<Candidate> {
    let mut found = Vec::new();
    for (product, channel, serving) in [("Stable", "stable", true), ("Beta", "beta", false)] {
        let Some(entry) = products
            .as_array()
            .and_then(|list| list.iter().find(|item| item["Product"] == product))
        else {
            continue;
        };
        let newest = entry["Releases"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter(|release| release["Platform"] == "Windows" && release["Architecture"] == "x64")
            .max_by_key(|release| parts(release["ProductVersion"].as_str().unwrap_or_default()));
        let Some(release) = newest else { continue };
        let Some(version) = release["ProductVersion"].as_str() else {
            continue;
        };
        let download = release["Artifacts"].as_array().and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact["ArtifactName"] == "msi")
                .and_then(|artifact| artifact["Location"].as_str())
                .map(str::to_owned)
        });
        found.push(Candidate {
            browser: "Microsoft Edge".into(),
            version: version.into(),
            channel: channel.into(),
            serving,
            download,
        });
    }
    found
}

/// The newest Release and Beta from Brave's GitHub releases, whose titles read
/// `Release v1.98.63 (Chromium 156.0.8078.80)`. Its modules carry the Chromium milestone and the
/// tag, as `156.1.98.63`. A release stays a prerelease until Brave starts to roll it out, and
/// its files may come later than its title.
pub fn brave_candidates(releases: &Value) -> Vec<Candidate> {
    let mut newest: BTreeMap<&str, Candidate> = BTreeMap::new();
    for release in releases.as_array().map_or(&[][..], Vec::as_slice) {
        let Some(name) = release["name"].as_str().map(str::trim) else {
            continue;
        };
        let channel = match name.split(' ').next() {
            Some("Release") => "stable",
            Some("Beta") => "beta",
            _ => continue,
        };
        let Some(tag) = release["tag_name"]
            .as_str()
            .and_then(|tag| tag.strip_prefix('v'))
        else {
            continue;
        };
        let Some(major) = name
            .split("(Chromium ")
            .nth(1)
            .and_then(|rest| rest.split('.').next())
        else {
            continue;
        };
        let version = format!("{major}.{tag}");
        let installer = if channel == "stable" {
            "BraveBrowserStandaloneSilentSetup.exe"
        } else {
            "BraveBrowserStandaloneSilentBetaSetup.exe"
        };
        let download = release["assets"].as_array().and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"] == installer)
                .and_then(|asset| asset["browser_download_url"].as_str())
                .map(str::to_owned)
        });
        let candidate = Candidate {
            browser: "Brave".into(),
            version,
            channel: channel.into(),
            serving: channel == "stable" && release["prerelease"] == false,
            download,
        };
        let replaces = newest
            .get(channel)
            .is_none_or(|held| parts(&held.version) < parts(&candidate.version));
        if replaces {
            newest.insert(channel, candidate);
        }
    }
    newest.into_values().collect()
}

/// What still needs a row, most urgent first: what people have now, then what stable is about to
/// serve, then beta, which brings the next milestone weeks ahead and gives a person time to review
/// what symbols cannot show. A version older than every row and not served is history, not news.
pub fn pending(candidates: Vec<Candidate>, known: &BTreeSet<(String, String)>) -> Vec<Candidate> {
    let mut newest_known: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (browser, version) in known {
        let entry = newest_known.entry(browser).or_default();
        if *entry < parts(version) {
            *entry = parts(version);
        }
    }
    let mut left: Vec<Candidate> = candidates
        .into_iter()
        .filter(|candidate| {
            !known.contains(&(candidate.browser.clone(), candidate.version.clone()))
        })
        .filter(|candidate| {
            candidate.serving
                || newest_known
                    .get(candidate.browser.as_str())
                    .is_none_or(|newest| parts(&candidate.version) > *newest)
        })
        .collect();
    left.sort_by_key(|candidate| {
        (
            !candidate.serving,
            candidate.channel != "stable",
            std::cmp::Reverse(parts(&candidate.version)),
        )
    });
    left.dedup_by(|a, b| a.browser == b.browser && a.version == b.version);
    left
}

/// The feature checks of `scripts/tests/windows-features.ps1`, which every supported browser must
/// pass. A browser that misses one is not listed as supported, so features stay the same on all.
pub const FEATURES: &[&str] = &[
    "split",
    "sidebar",
    "newtab",
    "space",
    "split-start",
    "split-menu",
    "split-menu-start",
    "split-drag",
    "split-drag-start",
    "split-shortcut",
    "split-one-tab",
    "split-maximised",
    "split-space",
    "split-click",
    "split-rows",
    "split-drop-top",
    "split-drop-bottom",
    "split-grid-3",
    "split-grid-4",
    "bar-layout",
    "bar-folded",
    "mini-pop-out",
    "mini-move",
    "mini-return",
    "mini-close",
    "tab-sections",
    "tab-close-select",
    "tab-reopen",
    "tab-archive",
    "ctrl-t-setting",
    "link-new-tab",
    "command-bar",
    "link-preview",
    "rivet-settings",
    "link-rules",
    "address-suggest",
    "appbar-width",
    "appbar-peek",
    "open-link-running",
    "open-link-start",
    "credentials-browser",
    "credentials-system",
    "view-click",
    "view-click-other",
    "view-keys",
    "view-ime",
    "tab-moves",
    "first-frame",
    "fullscreen-sidebar",
    "taskbar-identity",
    "taskbar-relaunch",
    "sidebar-scale-monitor",
    "split-panel-look",
    "sidebar-floats",
    "sidebar-open-button",
    "dev-row",
    "address-editing",
    "security-chip",
    "permission-bubble",
    "password-bubble",
    "typing-keeps-active",
    "cli-control",
    "cli-input",
    "cli-hidden",
];

/// The features a test desktop can never drive, each with its reason, kept in one file. They are
/// still reported as not driven and never count as passed.
const TEST_DESKTOP_LIMITS: &str = include_str!("../test-desktop-limits.json");

/// The features of `TEST_DESKTOP_LIMITS` that apply to `browser`, with their reasons. A limit with
/// a `browsers` list applies only to those browsers, matched as a word of the browser's name.
pub fn test_desktop_limits(browser: &str) -> Vec<(String, String)> {
    let browser = browser.to_ascii_lowercase();
    let limits: Value = serde_json::from_str(TEST_DESKTOP_LIMITS).unwrap_or_default();
    limits
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|limit| match limit["browsers"].as_array() {
            None => true,
            Some(only) => only
                .iter()
                .filter_map(Value::as_str)
                .any(|name| browser.split_whitespace().any(|word| word == name)),
        })
        .filter_map(|limit| {
            Some((
                limit["feature"].as_str()?.to_owned(),
                limit["reason"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// Whether a browser may be published as supported, from the rows of a feature run
/// (`results.json`: browser, feature, result). Every feature must read `PASS` for that browser,
/// except a feature a test desktop can never drive, which must read `NOT_DRIVEN`. A feature that
/// failed, was not driven for any other reason or is missing keeps the browser off the list,
/// because a check that could not run proves nothing. The error lists each reason. On success the
/// notes name each feature left undriven by the test desktop, and a listed feature that now
/// passes, so the list is looked at again.
pub fn publish_gate(results: &Value, browser: &str) -> Result<Vec<String>, Vec<String>> {
    let rows: Vec<&Value> = match results {
        Value::Array(rows) => rows.iter().collect(),
        // ConvertTo-Json writes a single row as an object.
        Value::Object(_) => vec![results],
        _ => return Err(vec!["the feature results are not a list".to_owned()]),
    };
    let browser = browser.to_ascii_lowercase();
    let mut seen: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for row in rows {
        if row["browser"]
            .as_str()
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some(&browser)
        {
            continue;
        }
        if let Some(feature) = row["feature"].as_str() {
            seen.entry(feature)
                .or_default()
                .push(row["result"].as_str().unwrap_or("NOT_DRIVEN"));
        }
    }
    let limits = test_desktop_limits(&browser);
    let mut reasons = Vec::new();
    let mut notes = Vec::new();
    for feature in FEATURES {
        let limit = limits.iter().find(|(name, _)| name == feature);
        match (seen.get(feature), limit) {
            (None, _) => reasons.push(format!("{feature}: not run")),
            (Some(results), None) => {
                if let Some(other) = results.iter().find(|result| **result != "PASS") {
                    reasons.push(format!("{feature}: {other}"));
                }
            }
            (Some(results), Some((_, why))) => {
                if results.iter().all(|result| *result == "NOT_DRIVEN") {
                    notes.push(format!("{feature}: NOT_DRIVEN on the test desktop: {why}"));
                } else if results.iter().all(|result| *result == "PASS") {
                    notes.push(format!(
                        "{feature}: passed although it is listed as a test desktop limit; look at the list again"
                    ));
                } else {
                    let other = results
                        .iter()
                        .find(|result| **result != "NOT_DRIVEN" && **result != "PASS")
                        .copied()
                        .unwrap_or("mixed results");
                    reasons.push(format!(
                        "{feature}: {other}, though it is listed as a test desktop limit"
                    ));
                }
            }
        }
    }
    if reasons.is_empty() {
        Ok(notes)
    } else {
        Err(reasons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn known(rows: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        rows.iter()
            .map(|(browser, version)| ((*browser).to_owned(), (*version).to_owned()))
            .collect()
    }

    /// As Google answered on 2026-10-31.
    fn google() -> (Value, Value) {
        let release =
            |version: &str, fraction: f64| json!({"version": version, "fraction": fraction});
        (
            json!({"releases": [
                release("156.0.8078.74", 0.495), release("156.0.8078.76", 0.2475),
                release("156.0.8078.79", 0.2475), release("156.0.8078.80", 1.0),
                release("156.0.8078.75", 0.005), release("157.0.8103.26", 0.005),
                release("157.0.8103.27", 0.0), release("156.0.8078.49", 0.0),
            ]}),
            json!({"versions": [{"version": "158.0.8127.2"}, {"version": "157.0.8103.15"}]}),
        )
    }

    #[test]
    fn nfr3_chrome_builds_are_found_before_they_reach_anyone() {
        let (stable, beta) = google();
        let left = pending(
            chrome_candidates(&stable, &beta),
            &known(&[("Google Chrome", "156.0.8078.74")]),
        );
        let versions: Vec<_> = left
            .iter()
            .map(|candidate| (candidate.version.as_str(), candidate.serving))
            .collect();
        assert_eq!(
            versions,
            [
                ("157.0.8103.26", true),
                ("156.0.8078.80", true),
                ("156.0.8078.79", true),
                ("156.0.8078.76", true),
                ("156.0.8078.75", true),
                // Ahead of its rollout.
                ("157.0.8103.27", false),
                ("158.0.8127.2", false),
            ]
        );
        // .49 is old and at fraction zero: nobody will get it.
        assert!(left
            .iter()
            .all(|candidate| candidate.version != "156.0.8078.49"));
    }

    #[test]
    fn nfr3_edge_takes_the_newest_x64_installer_of_a_channel() {
        let release = |version: &str, architecture: &str| {
            json!({"ProductVersion": version, "Platform": "Windows", "Architecture": architecture,
                   "Artifacts": [{"ArtifactName": "msi", "Location": format!("https://example.invalid/{version}-{architecture}.msi")}]})
        };
        let feed = json!([
            {"Product": "Dev", "Releases": [release("158.0.4357.0", "x64")]},
            {"Product": "Beta", "Releases": [release("157.0.4341.30", "x64"), release("157.0.4341.24", "x64")]},
            {"Product": "Stable", "Releases": [release("156.0.4314.58", "x86"), release("156.0.4314.58", "x64"), release("156.0.4314.56", "x64")]},
        ]);
        let left = pending(
            edge_candidates(&feed),
            &known(&[("Microsoft Edge", "156.0.4314.50")]),
        );
        assert_eq!(left.len(), 2);
        assert_eq!(
            (left[0].version.as_str(), left[0].serving),
            ("156.0.4314.58", true)
        );
        assert_eq!(
            left[0].download.as_deref(),
            Some("https://example.invalid/156.0.4314.58-x64.msi")
        );
        assert_eq!(
            (left[1].version.as_str(), left[1].channel.as_str()),
            ("157.0.4341.30", "beta")
        );
    }

    #[test]
    fn nfr3_brave_versions_are_made_from_the_title_and_tag() {
        let release = |name: &str, tag: &str, prerelease: bool, assets: Value| json!({"name": name, "tag_name": tag, "prerelease": prerelease, "assets": assets});
        let installer = json!([{"name": "BraveBrowserStandaloneSilentSetup.exe", "browser_download_url": "https://example.invalid/b.exe"}]);
        let feed = json!([
            release(
                "Nightly v1.101.10 (Chromium 157.0.8103.30)",
                "v1.101.10",
                true,
                json!([])
            ),
            release(
                "Release v1.98.64 (Chromium 156.0.8078.80) \n",
                "v1.98.64",
                true,
                json!([])
            ),
            release(
                "Release v1.98.63 (Chromium 156.0.8078.80)",
                "v1.98.63",
                false,
                installer
            ),
            release(
                "Beta v1.100.45 (Chromium 156.0.8078.74)",
                "v1.100.45",
                true,
                json!([])
            ),
        ]);
        let found = brave_candidates(&feed);
        let stable = found
            .iter()
            .find(|candidate| candidate.channel == "stable")
            .unwrap();
        assert_eq!(stable.version, "156.1.98.64");
        assert!(!stable.serving, "a prerelease has not reached anyone");
        assert_eq!(stable.download, None);
        assert_eq!(
            found
                .iter()
                .find(|candidate| candidate.channel == "beta")
                .unwrap()
                .version,
            "156.1.100.45"
        );
        assert!(found
            .iter()
            .all(|candidate| !candidate.version.contains(".101.")));
    }

    #[test]
    fn nfr3_a_build_with_a_row_needs_nothing() {
        let candidate = |version: &str| Candidate {
            browser: "Brave".into(),
            version: version.into(),
            channel: "stable".into(),
            serving: true,
            download: None,
        };
        assert!(pending(
            vec![candidate("156.1.98.63")],
            &known(&[("Brave", "156.1.98.63")])
        )
        .is_empty());
    }

    fn results(rows: &[(&str, &str, &str)]) -> Value {
        Value::Array(
            rows.iter()
                .map(|(browser, feature, result)| {
                    json!({"browser": browser, "feature": feature, "result": result})
                })
                .collect(),
        )
    }

    fn all_pass(browser: &str) -> Vec<(&str, &'static str, &'static str)> {
        FEATURES
            .iter()
            .map(|feature| (browser, *feature, "PASS"))
            .collect()
    }

    #[test]
    fn nfr3_a_browser_that_passed_every_feature_is_published() {
        assert!(publish_gate(&results(&all_pass("chrome")), "Chrome").is_ok());
    }

    #[test]
    fn nfr3_one_failed_or_undriven_feature_keeps_a_browser_off_the_list() {
        let mut rows = all_pass("edge");
        rows[3].2 = "FAIL";
        rows[7].2 = "NOT_DRIVEN";
        let reasons = publish_gate(&results(&rows), "edge").unwrap_err();
        assert_eq!(reasons.len(), 2);
        assert!(reasons[0].contains("FAIL") || reasons[1].contains("FAIL"));
        // A pass on another run of the same feature does not hide the failure.
        let mut twice = all_pass("edge");
        twice.push(("edge", "split", "FAIL"));
        assert!(publish_gate(&results(&twice), "edge").is_err());
    }

    /// The rows of a run on a test desktop: every feature passes but the listed limits.
    fn test_desktop_run(browser: &str) -> Vec<(&str, &'static str, &'static str)> {
        let limits = test_desktop_limits(browser);
        FEATURES
            .iter()
            .map(|feature| {
                let undriven = limits.iter().any(|(name, _)| name == feature);
                (
                    browser,
                    *feature,
                    if undriven { "NOT_DRIVEN" } else { "PASS" },
                )
            })
            .collect()
    }

    #[test]
    fn nfr3_the_test_desktop_limits_are_the_eleven_known_features_with_reasons() {
        let limits = test_desktop_limits("google chrome");
        let names: Vec<&str> = limits.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "split-shortcut",
                "split-maximised",
                "credentials-system",
                "view-click-other",
                "view-ime",
                "address-editing",
                "taskbar-identity",
                "sidebar-scale-monitor",
                "password-bubble",
                "cli-input",
                "cli-hidden"
            ]
        );
        for (name, reason) in &limits {
            assert!(FEATURES.contains(&name.as_str()), "{name} is a feature");
            assert!(!reason.is_empty(), "{name} has a reason");
        }
    }

    #[test]
    fn nfr3_a_test_desktop_run_publishes_and_still_reports_its_limits_as_not_driven() {
        let notes = publish_gate(&results(&test_desktop_run("chrome")), "chrome").unwrap();
        assert_eq!(notes.len(), 11);
        assert!(notes
            .iter()
            .all(|note| note.contains("NOT_DRIVEN on the test desktop")));
    }

    #[test]
    fn nfr3_any_other_undriven_feature_or_any_failure_blocks_publication() {
        // An undriven feature that is not a test desktop limit.
        let mut rows = test_desktop_run("edge");
        let at = rows.iter().position(|row| row.1 == "sidebar").unwrap();
        rows[at].2 = "NOT_DRIVEN";
        assert_eq!(
            publish_gate(&results(&rows), "edge"),
            Err(vec!["sidebar: NOT_DRIVEN".to_owned()])
        );
        // A listed limit that failed.
        let mut rows = test_desktop_run("edge");
        let at = rows.iter().position(|row| row.1 == "view-ime").unwrap();
        rows[at].2 = "FAIL";
        let reasons = publish_gate(&results(&rows), "edge").unwrap_err();
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].starts_with("view-ime: FAIL"));
    }

    #[test]
    fn nfr3_brave_alone_may_leave_credentials_browser_not_driven() {
        let brave = test_desktop_limits("brave");
        assert_eq!(brave.len(), 12);
        assert!(brave
            .iter()
            .any(|(name, why)| name == "credentials-browser" && why.contains("wait_for_username")));
        let notes = publish_gate(&results(&test_desktop_run("brave")), "brave").unwrap();
        assert_eq!(notes.len(), 12);
        // Any other browser that leaves it not driven is blocked.
        let mut rows = test_desktop_run("google chrome");
        let at = rows
            .iter()
            .position(|row| row.1 == "credentials-browser")
            .unwrap();
        rows[at].2 = "NOT_DRIVEN";
        assert_eq!(
            publish_gate(&results(&rows), "google chrome"),
            Err(vec!["credentials-browser: NOT_DRIVEN".to_owned()])
        );
    }

    #[test]
    fn nfr3_a_listed_limit_that_passes_is_reported() {
        let mut rows = test_desktop_run("brave");
        let at = rows.iter().position(|row| row.1 == "cli-hidden").unwrap();
        rows[at].2 = "PASS";
        let notes = publish_gate(&results(&rows), "brave").unwrap();
        assert!(notes
            .iter()
            .any(|note| note.starts_with("cli-hidden: passed although")));
    }
    #[test]
    fn nfr3_a_feature_that_was_not_run_does_not_count_as_passed() {
        let mut rows = all_pass("brave");
        rows.retain(|row| row.1 != "sidebar");
        rows.extend(all_pass("chrome"));
        assert_eq!(
            publish_gate(&results(&rows), "brave"),
            Err(vec!["sidebar: not run".to_owned()])
        );
        assert!(publish_gate(&results(&all_pass("chrome")), "brave").is_err());
        assert!(publish_gate(&json!("nonsense"), "brave").is_err());
    }

    #[test]
    fn nfr3_the_gate_asks_for_every_feature_the_feature_script_drives() {
        let script = include_str!("../../../../scripts/tests/windows-features.ps1");
        let line = script
            .lines()
            .find(|line| line.trim_start().starts_with("[string[]]$Features"))
            .expect("the script lists its features");
        let listed: BTreeSet<&str> = line.split('\'').skip(1).step_by(2).collect();
        let gated: BTreeSet<&str> = FEATURES.iter().copied().collect();
        assert_eq!(listed, gated);
    }
}
