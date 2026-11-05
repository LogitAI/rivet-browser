//! `rivet link-rules` and `rivet session open`: the link rules a person or agent keeps, and links
//! sent into a Rivet window, which routes them by the same rules as links from its pages.

use std::path::{Path, PathBuf};

use rivet_core::link_rules::{LinkRules, Pattern, Target};

/// What `rivet link-rules` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RulesRequest {
    List { json: bool },
    Set { pattern: Pattern, target: Target },
    Remove { pattern: Pattern },
    /// Where a link would open, without opening it.
    Route { url: String, json: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RulesOptions {
    pub request: RulesRequest,
    pub rules: Option<PathBuf>,
}

/// Reads `link-rules list|set <pattern> <target>|remove <pattern>|route <url>` after the first
/// word, with `--rules <path>` for another file and `--json` for list and route.
pub(crate) fn parse_rules(arguments: &[String]) -> Result<RulesOptions, String> {
    let mut rules = None;
    let mut json = false;
    let mut words = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--rules" => {
                index += 1;
                rules = Some(PathBuf::from(
                    arguments.get(index).ok_or("--rules needs a value")?,
                ));
            }
            "--json" => json = true,
            option if option.starts_with("--") => {
                return Err(format!("unknown option {option:?}"))
            }
            word => words.push(word.to_owned()),
        }
        index += 1;
    }
    let request = match words.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["list"] => RulesRequest::List { json },
        ["set", pattern, target] => RulesRequest::Set {
            pattern: Pattern::parse(pattern)?,
            target: Target::parse(target)?,
        },
        ["remove", pattern] => RulesRequest::Remove {
            pattern: Pattern::parse(pattern)?,
        },
        ["route", url] => RulesRequest::Route {
            url: url.to_owned(),
            json,
        },
        _ => {
            return Err(
                "expected `link-rules list`, `set <pattern> <space>|mini-window|current-space`, `remove <pattern>` or `route <url>`"
                    .into(),
            )
        }
    };
    Ok(RulesOptions { request, rules })
}

fn target_json(target: &Target) -> serde_json::Value {
    match target {
        Target::Space(name) => serde_json::json!({ "kind": "space", "name": name }),
        Target::MiniWindow => serde_json::json!({ "kind": "mini-window" }),
        Target::CurrentSpace => serde_json::json!({ "kind": "current-space" }),
    }
}

/// Runs a request against the rules file at `path` and returns what to print.
pub(crate) fn run_rules_at(path: &Path, request: RulesRequest) -> Result<String, String> {
    let mut rules = LinkRules::load(path);
    match request {
        RulesRequest::List { json: true } => Ok(serde_json::Value::Array(
            rules
                .rules
                .iter()
                .map(|rule| {
                    serde_json::json!({
                        "pattern": rule.pattern.label(),
                        "target": target_json(&rule.target),
                    })
                })
                .collect(),
        )
        .to_string()),
        RulesRequest::List { json: false } => {
            let mut out: Vec<String> = rules
                .rules
                .iter()
                .map(|rule| format!("{}\t{}", rule.pattern.label(), rule.target.label()))
                .collect();
            out.push(format!("Everything else\t{}", Target::CurrentSpace.label()));
            Ok(out.join("\n"))
        }
        RulesRequest::Set { pattern, target } => {
            let shown = format!("{}\t{}", pattern.label(), target.label());
            rules.set(pattern, target)?;
            rules.save(path)?;
            Ok(shown)
        }
        RulesRequest::Remove { pattern } => {
            if !rules.remove(&pattern) {
                return Err(format!("no rule matches {:?}", pattern.label()));
            }
            rules.save(path)?;
            Ok(format!("removed {}", pattern.label()))
        }
        RulesRequest::Route { url, json } => {
            let target = rules.route(&url);
            Ok(if json {
                target_json(&target).to_string()
            } else {
                target.label()
            })
        }
    }
}

pub(crate) fn rules_path(override_path: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        return Ok(path);
    }
    rivet_core::paths::Layout::current()
        .map(|layout| layout.link_rules_file())
        .ok_or_else(|| "the local application data folder is unknown; pass --rules <path>".into())
}

/// What `rivet session open` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpenOptions {
    pub id: String,
    pub state: Option<PathBuf>,
    pub url: String,
}

/// Reads `session open --id <id> [--state <path>] <url>`.
pub(crate) fn parse_open(arguments: &[String]) -> Result<OpenOptions, String> {
    let mut id = None;
    let mut state = None;
    let mut url = None;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--id" => {
                index += 1;
                id = Some(arguments.get(index).ok_or("--id needs a value")?.clone());
            }
            "--state" => {
                index += 1;
                state = Some(PathBuf::from(
                    arguments.get(index).ok_or("--state needs a value")?,
                ));
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown option {option:?}"))
            }
            word if url.is_none() => url = Some(word.to_owned()),
            word => return Err(format!("unexpected {word:?}")),
        }
        index += 1;
    }
    let url = url.ok_or("an address to open is required")?;
    rivet_protocol::window::encode_open_link(&url)?;
    Ok(OpenOptions {
        id: id.ok_or("--id is required")?,
        state,
        url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn fr3_cli_keeps_link_rules_and_answers_where_a_link_opens() {
        let dir = rivet_core::paths::TestDir::new("fr3-cli-link-rules");
        let path = dir.join("link-rules.json");
        let run = |args: &[&str]| {
            let options = parse_rules(&words(args)).unwrap();
            run_rules_at(&path, options.request)
        };
        run(&["set", "github.com/*", "Work"]).unwrap();
        run(&["set", "https://calendar.google.com/", "mini-window"]).unwrap();
        assert_eq!(run(&["route", "https://gist.github.com/x"]).unwrap(), "Work");
        assert_eq!(
            run(&["route", "https://calendar.google.com/e", "--json"]).unwrap(),
            r#"{"kind":"mini-window"}"#
        );
        assert_eq!(
            run(&["route", "https://example.com/"]).unwrap(),
            "Current space, new tab"
        );
        assert_eq!(
            run(&["list"]).unwrap(),
            "*.github.com\tWork\nhttps://calendar.google.com/*\tMini Window\nEverything else\tCurrent space, new tab"
        );
        run(&["remove", "github.com"]).unwrap();
        assert!(run(&["remove", "github.com"]).is_err());
        assert_eq!(run(&["route", "https://github.com/"]).unwrap(), "Current space, new tab");
    }

    #[test]
    fn fr3_cli_reads_link_rule_requests() {
        assert_eq!(
            parse_rules(&words(&["--rules", "r.json", "set", "*.notion.so", "Research"])).unwrap(),
            RulesOptions {
                request: RulesRequest::Set {
                    pattern: Pattern::Host("notion.so".into()),
                    target: Target::Space("Research".into())
                },
                rules: Some(PathBuf::from("r.json")),
            }
        );
        assert!(parse_rules(&words(&["set", "github.com"])).is_err());
        assert!(parse_rules(&words(&["set", "", "Work"])).is_err());
        assert!(parse_rules(&words(&["list", "--nope"])).is_err());
    }

    #[test]
    fn fr3_cli_sends_only_web_addresses_into_a_session() {
        assert_eq!(
            parse_open(&words(&["session", "open", "--id", "s1", "https://github.com/"])).unwrap(),
            OpenOptions {
                id: "s1".into(),
                state: None,
                url: "https://github.com/".into()
            }
        );
        assert!(parse_open(&words(&["session", "open", "--id", "s1", "file:///C:/"])).is_err());
        assert!(parse_open(&words(&["session", "open", "https://github.com/"])).is_err());
        assert!(parse_open(&words(&["session", "open", "--id", "s1"])).is_err());
    }
}
