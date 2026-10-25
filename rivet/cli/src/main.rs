//! The first Rivet command surface.
//!
//! It only opens a caller-selected Chromium executable. Future commands talk to the local Rivet
//! runtime over the versioned control protocol instead of adding a remote-debugging port.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use rivet_core::{build_launch_plan, DisplayMode, LaunchRequest};

const USAGE: &str = "\
Usage:
  rivet session start --browser <path> --url <url> [--user-data-dir <path>] \\
      [--display visible|hidden|auto] [--json]

The browser executable is always explicit in this first release. Rivet does not guess a browser
path or open a remote debugging port.\n";

#[derive(Debug, PartialEq, Eq)]
struct StartOptions {
    browser: PathBuf,
    url: String,
    user_data_dir: Option<PathBuf>,
    display: DisplayMode,
    json: bool,
}

fn main() {
    let result = run(env::args().skip(1));
    if let Err(error) = result {
        eprintln!("rivet: {error}");
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let options = parse_start(arguments.into_iter().collect())?;
    let request = LaunchRequest {
        browser: options.browser,
        url: options.url,
        user_data_dir: options.user_data_dir,
        display: options.display,
    };
    let plan = build_launch_plan(&request).map_err(|error| error.to_string())?;
    let child = Command::new(&plan.program)
        .args(&plan.arguments)
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", plan.program.display()))?;

    if options.json {
        println!(
            "{{\"pid\":{},\"display\":\"{}\",\"url\":\"{}\"}}",
            child.id(),
            plan.display.as_str(),
            escape_json(&request.url)
        );
    } else {
        println!(
            "started Rivet browser task {} ({})",
            child.id(),
            plan.display.as_str()
        );
    }
    Ok(())
}

fn parse_start(arguments: Vec<String>) -> Result<StartOptions, String> {
    if arguments.len() < 2 || arguments[0] != "session" || arguments[1] != "start" {
        return Err("expected `session start`".into());
    }
    let mut browser = None;
    let mut url = None;
    let mut user_data_dir = None;
    let mut display = DisplayMode::Auto;
    let mut json = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--browser" => {
                browser = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--browser",
                )?))
            }
            "--url" => url = Some(next_value(&arguments, &mut index, "--url")?),
            "--user-data-dir" => {
                user_data_dir = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--user-data-dir",
                )?))
            }
            "--display" => {
                display = DisplayMode::parse(&next_value(&arguments, &mut index, "--display")?)
                    .map_err(|error| error.to_string())?
            }
            "--json" => json = true,
            option => return Err(format!("unknown option {option:?}")),
        }
        index += 1;
    }
    Ok(StartOptions {
        browser: browser.ok_or_else(|| "--browser is required".to_owned())?,
        url: url.ok_or_else(|| "--url is required".to_owned())?,
        user_data_dir,
        display,
        json,
    })
}

fn next_value(arguments: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| format!("{option} needs a value"))
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr4_parses_machine_readable_hidden_start() {
        let options = parse_start(vec![
            "session".into(),
            "start".into(),
            "--browser".into(),
            "C:/Chrome/chrome.exe".into(),
            "--url".into(),
            "https://example.test/".into(),
            "--display".into(),
            "hidden".into(),
            "--json".into(),
        ])
        .unwrap();
        assert_eq!(options.display, DisplayMode::Hidden);
        assert!(options.json);
    }

    #[test]
    fn fr4_requires_an_explicit_browser_path() {
        let error = parse_start(vec![
            "session".into(),
            "start".into(),
            "--url".into(),
            "https://example.test/".into(),
        ])
        .unwrap_err();
        assert_eq!(error, "--browser is required");
    }
}
