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
}
