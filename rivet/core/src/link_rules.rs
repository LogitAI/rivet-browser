//! Link rules: where a link opened from another app or a page goes.
//!
//! A rule matches a link by its host or by the start of its address, and sends it to a named
//! space, to the Mini Window or to the current space. Rules are tried from the top and the first
//! that matches wins, so a person can put a narrow rule above a broad one. A link no rule matches
//! opens as a new tab of the current space. The list is one file in the state folder, shared by
//! the `rivet` command and every Rivet window, so both follow the same rules.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The most rules a list keeps, so a damaged or hostile file cannot make routing slow.
pub const MAX_RULES: usize = 200;
/// The longest pattern a rule keeps.
pub const MAX_PATTERN: usize = 2048;

/// How a rule recognises a link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Pattern {
    /// A host and every host below it: `github.com` matches `github.com` and `gist.github.com`.
    Host(String),
    /// The start of an address, `https://example.com/docs/`. `http` and `https` are the same here,
    /// since a site that moves to `https` should keep its rule.
    Prefix(String),
}

/// Where a matched link opens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "kebab-case")]
pub enum Target {
    /// A new tab of the space with this name.
    Space(String),
    /// The Mini Window over the current page.
    MiniWindow,
    /// A new tab of the space the window shows now.
    CurrentSpace,
}

impl Target {
    /// How the target is named on the command line and in the panel.
    pub fn label(&self) -> String {
        match self {
            Target::Space(name) => name.clone(),
            Target::MiniWindow => "Mini Window".into(),
            Target::CurrentSpace => "Current space, new tab".into(),
        }
    }

    /// The target named on the command line: `mini-window`, `current-space` or a space's name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "" => Err("a target needs a space name, `mini-window` or `current-space`".into()),
            "mini-window" => Ok(Target::MiniWindow),
            "current-space" => Ok(Target::CurrentSpace),
            name => Ok(Target::Space(name.to_owned())),
        }
    }

    /// The target after this one when a person clicks it in the panel: the current space, then
    /// each space in order, then the Mini Window, then the current space again.
    pub fn next(&self, spaces: &[String]) -> Target {
        match self {
            Target::CurrentSpace => spaces
                .first()
                .map_or(Target::MiniWindow, |name| Target::Space(name.clone())),
            Target::Space(name) => match spaces.iter().position(|space| space == name) {
                Some(at) if at + 1 < spaces.len() => Target::Space(spaces[at + 1].clone()),
                _ => Target::MiniWindow,
            },
            Target::MiniWindow => Target::CurrentSpace,
        }
    }
}

/// One rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub pattern: Pattern,
    pub target: Target,
}

/// The parts of an address a rule looks at.
struct Parts<'a> {
    scheme: String,
    host: String,
    /// Everything after the host and port, starting with `/`, or empty.
    rest: &'a str,
}

fn parts(url: &str) -> Option<Parts<'_>> {
    let (scheme, after) = url.split_once("://")?;
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let authority = &after[..end];
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if host_port.starts_with('[') {
        host_port
            .split_once(']')
            .map_or(host_port, |(host, _)| host)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    Some(Parts {
        scheme: scheme.to_ascii_lowercase(),
        host: host
            .trim_start_matches('[')
            .trim_end_matches('.')
            .to_ascii_lowercase(),
        rest: &after[end..],
    })
}

/// Only web pages are routed. Other schemes open where they always do.
fn is_web(scheme: &str) -> bool {
    scheme == "http" || scheme == "https"
}

fn host_matches(host: &str, wanted: &str) -> bool {
    !wanted.is_empty()
        && (host == wanted
            || host
                .strip_suffix(wanted)
                .is_some_and(|above| above.ends_with('.')))
}

impl Pattern {
    /// The pattern a person typed. Text with `://` is the start of an address. Otherwise it is a
    /// host, written as `github.com`, `*.notion.so` or `github.com/*`; a host with a path, such as
    /// `github.com/LogitAI`, is the start of that address on `https`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("a pattern needs a host or the start of an address".into());
        }
        if text.len() > MAX_PATTERN {
            return Err(format!("a pattern is at most {MAX_PATTERN} characters"));
        }
        if text.contains("://") {
            let parsed = parts(text).ok_or("the address has no host")?;
            if !is_web(&parsed.scheme) || parsed.host.is_empty() {
                return Err("only http and https addresses can be matched".into());
            }
            return Ok(Pattern::Prefix(text.trim_end_matches('*').to_owned()));
        }
        let text = text.trim_end_matches('*').trim_end_matches('/');
        let (host, path) = text.split_once('/').unwrap_or((text, ""));
        let host = host.trim_start_matches("*.").to_ascii_lowercase();
        if host.is_empty() || host.contains(['*', ' ', '@', ':']) {
            return Err(format!("{host:?} is not a host"));
        }
        if path.is_empty() {
            Ok(Pattern::Host(host))
        } else {
            Ok(Pattern::Prefix(format!("https://{host}/{path}")))
        }
    }

    /// How the pattern is shown: a host as `*.host`, an address start as it was given.
    pub fn label(&self) -> String {
        match self {
            Pattern::Host(host) => format!("*.{host}"),
            Pattern::Prefix(start) => format!("{start}*"),
        }
    }

    /// Whether the pattern matches an address.
    pub fn matches(&self, url: &str) -> bool {
        let Some(link) = parts(url) else {
            return false;
        };
        if !is_web(&link.scheme) {
            return false;
        }
        match self {
            Pattern::Host(host) => host_matches(&link.host, host),
            Pattern::Prefix(start) => {
                let Some(wanted) = parts(start) else {
                    return false;
                };
                is_web(&wanted.scheme)
                    && link.host == wanted.host
                    && link.rest.starts_with(wanted.rest)
            }
        }
    }
}

/// The rules in the order they are tried.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRules {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl LinkRules {
    /// Where `url` opens: the target of the first rule that matches, else the current space.
    pub fn route(&self, url: &str) -> Target {
        self.rules
            .iter()
            .find(|rule| rule.pattern.matches(url))
            .map_or(Target::CurrentSpace, |rule| rule.target.clone())
    }

    /// Adds a rule at the end, or changes the target of the rule with the same pattern.
    pub fn set(&mut self, pattern: Pattern, target: Target) -> Result<(), String> {
        if let Some(rule) = self.rules.iter_mut().find(|rule| rule.pattern == pattern) {
            rule.target = target;
            return Ok(());
        }
        if self.rules.len() >= MAX_RULES {
            return Err(format!("a list holds at most {MAX_RULES} rules"));
        }
        self.rules.push(Rule { pattern, target });
        Ok(())
    }

    /// Removes the rule with this pattern. Returns whether there was one.
    pub fn remove(&mut self, pattern: &Pattern) -> bool {
        let before = self.rules.len();
        self.rules.retain(|rule| &rule.pattern != pattern);
        self.rules.len() != before
    }

    /// Reads the list. A missing file is an empty list. A file that cannot be read as a list is
    /// treated as empty too, so a damaged file never stops links from opening; it is replaced on
    /// the next save.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        let mut rules: Self = serde_json::from_slice(&bytes).unwrap_or_default();
        rules.rules.truncate(MAX_RULES);
        rules
    }

    /// Writes the list through a file beside it, so a reader never sees half a list.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("the rules path has no folder")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = path.with_extension("new");
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(list: &[(&str, Target)]) -> LinkRules {
        let mut rules = LinkRules::default();
        for (pattern, target) in list {
            rules
                .set(Pattern::parse(pattern).unwrap(), target.clone())
                .unwrap();
        }
        rules
    }

    #[test]
    fn fr3_a_link_no_rule_matches_opens_in_the_current_space() {
        assert_eq!(
            LinkRules::default().route("https://example.com/"),
            Target::CurrentSpace
        );
        let list = rules(&[("github.com", Target::Space("Work".into()))]);
        assert_eq!(list.route("https://example.com/"), Target::CurrentSpace);
        assert_eq!(list.route("mailto:a@example.com"), Target::CurrentSpace);
        assert_eq!(list.route("not an address"), Target::CurrentSpace);
    }

    #[test]
    fn fr3_a_host_rule_sends_the_host_and_its_subdomains_to_its_space() {
        let list = rules(&[
            ("github.com/*", Target::Space("Work".into())),
            ("*.notion.so", Target::Space("Research".into())),
        ]);
        assert_eq!(
            list.route("https://github.com/LogitAI"),
            Target::Space("Work".into())
        );
        assert_eq!(
            list.route("http://gist.GitHub.com:443/x"),
            Target::Space("Work".into())
        );
        assert_eq!(
            list.route("https://user@github.com/"),
            Target::Space("Work".into())
        );
        assert_eq!(
            list.route("https://www.notion.so/page"),
            Target::Space("Research".into())
        );
        assert_eq!(list.route("https://notgithub.com/"), Target::CurrentSpace);
        assert_eq!(
            list.route("https://github.com.evil.example/"),
            Target::CurrentSpace
        );
        assert_eq!(list.route("file:///C:/github.com/"), Target::CurrentSpace);
    }

    #[test]
    fn fr3_a_prefix_rule_matches_the_start_of_the_address_on_http_or_https() {
        let list = rules(&[
            ("https://calendar.google.com/event", Target::MiniWindow),
            ("example.com/docs", Target::Space("Docs".into())),
        ]);
        assert_eq!(
            list.route("https://calendar.google.com/event?eid=1"),
            Target::MiniWindow
        );
        assert_eq!(
            list.route("http://calendar.google.com/event/2"),
            Target::MiniWindow
        );
        assert_eq!(
            list.route("https://calendar.google.com/"),
            Target::CurrentSpace
        );
        assert_eq!(
            list.route("https://mail.calendar.google.com/event"),
            Target::CurrentSpace
        );
        assert_eq!(
            list.route("https://example.com/docs/a"),
            Target::Space("Docs".into())
        );
        assert_eq!(list.route("https://example.com/blog"), Target::CurrentSpace);
    }

    #[test]
    fn fr3_the_first_matching_rule_wins_and_setting_a_pattern_again_changes_it() {
        let mut list = rules(&[
            ("https://github.com/LogitAI/", Target::MiniWindow),
            ("github.com", Target::Space("Work".into())),
        ]);
        assert_eq!(
            list.route("https://github.com/LogitAI/rivet"),
            Target::MiniWindow
        );
        assert_eq!(
            list.route("https://github.com/other"),
            Target::Space("Work".into())
        );
        list.set(Pattern::Host("github.com".into()), Target::CurrentSpace)
            .unwrap();
        assert_eq!(list.rules.len(), 2, "the same pattern is one rule");
        assert_eq!(list.route("https://github.com/other"), Target::CurrentSpace);
        assert!(list.remove(&Pattern::Host("github.com".into())));
        assert!(!list.remove(&Pattern::Host("github.com".into())));
    }

    #[test]
    fn fr3_patterns_and_targets_are_read_as_people_write_them() {
        assert_eq!(
            Pattern::parse("*.Notion.so").unwrap(),
            Pattern::Host("notion.so".into())
        );
        assert_eq!(
            Pattern::parse("github.com/*").unwrap(),
            Pattern::Host("github.com".into())
        );
        assert!(Pattern::parse("").is_err());
        assert!(Pattern::parse("ftp://example.com/").is_err());
        assert!(Pattern::parse("a b").is_err());
        assert_eq!(Target::parse("mini-window").unwrap(), Target::MiniWindow);
        assert_eq!(
            Target::parse("current-space").unwrap(),
            Target::CurrentSpace
        );
        assert_eq!(Target::parse("Work").unwrap(), Target::Space("Work".into()));
        assert!(Target::parse(" ").is_err());
    }

    #[test]
    fn fr3_clicking_a_target_walks_current_space_then_each_space_then_the_mini_window() {
        let spaces = vec!["Work".to_owned(), "Research".to_owned()];
        let mut target = Target::CurrentSpace;
        let mut seen = Vec::new();
        for _ in 0..4 {
            target = target.next(&spaces);
            seen.push(target.clone());
        }
        assert_eq!(
            seen,
            vec![
                Target::Space("Work".into()),
                Target::Space("Research".into()),
                Target::MiniWindow,
                Target::CurrentSpace
            ]
        );
        assert_eq!(
            Target::Space("Gone".into()).next(&spaces),
            Target::MiniWindow
        );
        assert_eq!(Target::CurrentSpace.next(&[]), Target::MiniWindow);
    }

    #[test]
    fn fr3_the_rules_survive_the_file_and_a_damaged_file_is_an_empty_list() {
        let dir = crate::paths::TestDir::new("fr3-link-rules");
        let path = dir.join("state").join("link-rules.json");
        assert_eq!(LinkRules::load(&path), LinkRules::default());
        let list = rules(&[
            ("github.com", Target::Space("Work".into())),
            ("https://calendar.google.com/", Target::MiniWindow),
            ("example.com", Target::CurrentSpace),
        ]);
        list.save(&path).unwrap();
        assert_eq!(LinkRules::load(&path), list);
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(LinkRules::load(&path), LinkRules::default());
    }
}
