//! The first Rivet command surface.
//!
//! It only opens a caller-selected Chromium executable. Future commands talk to the local Rivet
//! runtime over the versioned control protocol instead of adding a remote-debugging port.

use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rivet_core::session::{SessionRecord, SessionRegistry};
use rivet_core::{build_launch_plan, DisplayMode, LaunchRequest};
use rivet_platform_windows::{
    browser_windows, close_browser_window, inject_runtime, wait_for_new_browser_window,
    BrowserWindow,
};

const USAGE: &str = "\
Usage:
  rivet session start --browser <path> --url <url> [--user-data-dir <path>] \\
      [--display visible|hidden|auto] [--runtime <path>] [--state <path>] [--json]
  rivet session list [--state <path>] [--json]
  rivet session close --id <id> [--state <path>] [--json]

The browser executable is always explicit in this first release. Rivet does not guess a browser
path or open a remote debugging port.\n";

#[derive(Debug, PartialEq, Eq)]
struct StartOptions {
    browser: PathBuf,
    url: String,
    user_data_dir: Option<PathBuf>,
    display: DisplayMode,
    runtime: Option<PathBuf>,
    state: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct QueryOptions {
    state: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct CloseOptions {
    id: String,
    state: Option<PathBuf>,
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
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    match arguments.get(0..2) {
        Some(command) if command == ["session", "start"] => run_start(parse_start(arguments)?),
        Some(command) if command == ["session", "list"] => run_list(parse_query(arguments)?),
        Some(command) if command == ["session", "close"] => run_close(parse_close(arguments)?),
        _ => Err("expected `session start`, `session list` or `session close`".into()),
    }
}

fn run_start(options: StartOptions) -> Result<(), String> {
    let request = LaunchRequest {
        browser: options.browser.clone(),
        url: options.url.clone(),
        user_data_dir: options.user_data_dir.clone(),
        display: options.display,
    };
    let plan = build_launch_plan(&request).map_err(|error| error.to_string())?;
    let runtime = resolve_runtime(options.runtime.as_deref());
    let existing_windows = if runtime.is_some() && plan.display == DisplayMode::Visible {
        browser_windows(&plan.program).map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    let child = Command::new(&plan.program)
        .args(&plan.arguments)
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", plan.program.display()))?;
    let task_id = child.id();

    let window = if plan.display == DisplayMode::Visible {
        Some(
            wait_for_new_browser_window(&plan.program, &existing_windows, Duration::from_secs(10))
                .map_err(|error| {
                    format!("could not identify the launched Rivet browser window: {error}")
                })?,
        )
    } else {
        None
    };

    if let Some(runtime) = &runtime {
        let target_id = window.map_or(task_id, |window| window.process_id);
        inject_runtime(target_id, runtime).map_err(|error| {
            format!("could not load the Rivet runtime into browser task {target_id}: {error}")
        })?;
    }

    let session_id = window
        .map(|window| record_session(&options, &plan.program, &request.url, window))
        .transpose()?;

    if options.json {
        println!(
            "{{\"pid\":{},\"display\":\"{}\",\"url\":\"{}\",\"runtime_loaded\":{},\"session_id\":{}}}",
            task_id,
            plan.display.as_str(),
            escape_json(&request.url),
            runtime.is_some(),
            session_id.as_ref().map_or("null".to_owned(), |id| format!("\"{}\"", escape_json(id)))
        );
    } else {
        println!(
            "started Rivet browser task {} ({}){}",
            task_id,
            plan.display.as_str(),
            session_id.map_or(String::new(), |id| format!(", session {id}"))
        );
    }
    Ok(())
}

fn resolve_runtime(explicit: Option<&std::path::Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    let executable = env::current_exe().ok()?;
    let sibling = executable.parent()?.join("rivet_runtime_windows.dll");
    sibling.is_file().then_some(sibling)
}

fn parse_start(arguments: Vec<String>) -> Result<StartOptions, String> {
    if arguments.len() < 2 || arguments[0] != "session" || arguments[1] != "start" {
        return Err("expected `session start`".into());
    }
    let mut browser = None;
    let mut url = None;
    let mut user_data_dir = None;
    let mut display = DisplayMode::Auto;
    let mut runtime = None;
    let mut state = None;
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
            "--runtime" => {
                runtime = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--runtime",
                )?))
            }
            "--state" => {
                state = Some(PathBuf::from(next_value(
                    &arguments, &mut index, "--state",
                )?))
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
        runtime,
        state,
        json,
    })
}

fn parse_query(arguments: Vec<String>) -> Result<QueryOptions, String> {
    let mut state = None;
    let mut json = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--state" => {
                state = Some(PathBuf::from(next_value(
                    &arguments, &mut index, "--state",
                )?))
            }
            "--json" => json = true,
            option => return Err(format!("unknown option {option:?}")),
        }
        index += 1;
    }
    Ok(QueryOptions { state, json })
}

fn parse_close(arguments: Vec<String>) -> Result<CloseOptions, String> {
    let mut id = None;
    let mut state = None;
    let mut json = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--id" => id = Some(next_value(&arguments, &mut index, "--id")?),
            "--state" => {
                state = Some(PathBuf::from(next_value(
                    &arguments, &mut index, "--state",
                )?))
            }
            "--json" => json = true,
            option => return Err(format!("unknown option {option:?}")),
        }
        index += 1;
    }
    Ok(CloseOptions {
        id: id.ok_or_else(|| "--id is required".to_owned())?,
        state,
        json,
    })
}

fn record_session(
    options: &StartOptions,
    browser: &std::path::Path,
    url: &str,
    window: BrowserWindow,
) -> Result<String, String> {
    let state = state_path(options.state.as_deref())?;
    let mut registry = SessionRegistry::load(&state).map_err(|error| error.to_string())?;
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    let id = format!("{}-{:x}-{created}", window.process_id, window.handle);
    registry
        .insert(SessionRecord {
            id: id.clone(),
            browser: browser.to_path_buf(),
            window_handle: window.handle,
            process_id: window.process_id,
            url: url.to_owned(),
        })
        .map_err(|error| error.to_string())?;
    registry.save(&state).map_err(|error| error.to_string())?;
    Ok(id)
}

fn run_list(options: QueryOptions) -> Result<(), String> {
    let registry = SessionRegistry::load(&state_path(options.state.as_deref())?)
        .map_err(|error| error.to_string())?;
    let sessions = registry.list().collect::<Vec<_>>();
    if options.json {
        println!(
            "{}",
            serde_json::to_string(&sessions).map_err(|error| error.to_string())?
        );
    } else if sessions.is_empty() {
        println!("no Rivet sessions");
    } else {
        for session in sessions {
            println!("{}  {}  {}", session.id, session.process_id, session.url);
        }
    }
    Ok(())
}

fn run_close(options: CloseOptions) -> Result<(), String> {
    let state = state_path(options.state.as_deref())?;
    let mut registry = SessionRegistry::load(&state).map_err(|error| error.to_string())?;
    let session = registry
        .remove(&options.id)
        .ok_or_else(|| format!("Rivet session {:?} does not exist", options.id))?;
    close_browser_window(
        &session.browser,
        BrowserWindow {
            handle: session.window_handle,
            process_id: session.process_id,
        },
    )
    .map_err(|error| error.to_string())?;
    registry.save(&state).map_err(|error| error.to_string())?;
    if options.json {
        println!("{{\"closed\":\"{}\"}}", escape_json(&session.id));
    } else {
        println!("closed Rivet session {}", session.id);
    }
    Ok(())
}

fn state_path(override_path: Option<&std::path::Path>) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    let local = env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is not set; pass --state <path>".to_owned())?;
    Ok(PathBuf::from(local)
        .join("LogitAI")
        .join("Rivet")
        .join("sessions.json"))
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
            "--runtime".into(),
            "C:/Rivet/rivet-runtime-windows.dll".into(),
            "--json".into(),
        ])
        .unwrap();
        assert_eq!(options.display, DisplayMode::Hidden);
        assert_eq!(
            options.runtime,
            Some(PathBuf::from("C:/Rivet/rivet-runtime-windows.dll"))
        );
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

    #[test]
    fn fr1_parses_session_close_without_a_browser_guess() {
        let options = parse_close(vec![
            "session".into(),
            "close".into(),
            "--id".into(),
            "123-abc".into(),
            "--state".into(),
            "C:/Rivet/sessions.json".into(),
            "--json".into(),
        ])
        .unwrap();
        assert_eq!(options.id, "123-abc");
        assert_eq!(options.state, Some(PathBuf::from("C:/Rivet/sessions.json")));
        assert!(options.json);
    }

    #[test]
    fn fr2_explicit_runtime_wins_over_an_installed_sibling() {
        let runtime = resolve_runtime(Some(std::path::Path::new("C:/Rivet/custom-runtime.dll")));
        assert_eq!(runtime, Some(PathBuf::from("C:/Rivet/custom-runtime.dll")));
    }
}
