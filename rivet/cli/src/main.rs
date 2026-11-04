//! The first Rivet command surface.
//!
//! It only opens a caller-selected Chromium executable. Future commands talk to the local Rivet
//! runtime over the versioned control protocol instead of adding a remote-debugging port.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rivet_core::session::{SessionRecord, SessionRegistry};
use rivet_core::{build_launch_plan, profile_allows_rivet, DisplayMode, LaunchRequest};
use rivet_platform_windows::hold::{self, Launch};
use rivet_platform_windows::signed_table;
use rivet_platform_windows::{
    browser_windows, close_browser_window, post_window_command, inject_runtime, launch_clock, process_tree_is_alive,
    process_was_launched, resolve_browser_executable, wait_for_new_browser_window,
    BrowserWindow,
};

const USAGE: &str = "\
Usage:
  rivet setup [--browser <path> [--profile <name>]] [--list] [--json]
  rivet session start --browser <path> [--url <url>] [--user-data-dir <path>] \\
      [--display visible|hidden|auto] [--runtime <path>] [--state <path>] [--hold <path>] [--json]
  rivet session list [--state <path>] [--json]
  rivet session close --id <id> [--state <path>] [--json]
  rivet session mini-window --id <id> [--state <path>] \\
      pop-out | close | return left|right|top|bottom | move <x> <y> | resize <width> <height>
  rivet hold refresh --browser <path> [--hold <path>]

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
    /// Where held browser builds live, instead of the folder below local application data.
    hold: Option<PathBuf>,
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
    tidy_files();
    let result = run(env::args().skip(1));
    if let Err(error) = result {
        eprintln!("rivet: {error}");
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.first().is_some_and(|command| command == "setup") {
        return run_setup(&arguments[1..]);
    }
    match arguments.get(0..2) {
        Some(command) if command == ["session", "start"] => run_start(parse_start(arguments)?),
        Some(command) if command == ["session", "list"] => run_list(parse_query(arguments)?),
        Some(command) if command == ["session", "close"] => run_close(parse_close(arguments)?),
        Some(command) if command == ["session", "mini-window"] => run_mini_window(parse_mini_window(arguments)?),
        Some(command) if command == ["builds", "update"] => run_builds_update(&arguments[2..]),
        Some(command) if command == ["builds", "apply"] => run_builds_apply(&arguments[2..]),
        Some(command) if command == ["hold", "refresh"] => run_hold_refresh(arguments),
        _ => Err("expected `setup`, `session start`, `session list`, `session close`, `session mini-window`, `builds update`, `builds apply` or `hold refresh`".into()),
    }
}

/// What `rivet setup` was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum SetupRequest {
    /// Open the setup window.
    Window,
    List {
        json: bool,
    },
    Choose {
        browser: PathBuf,
        profile: String,
        json: bool,
    },
}

fn parse_setup(arguments: &[String]) -> Result<SetupRequest, String> {
    let mut browser = None;
    let mut profile = None;
    let mut list = false;
    let mut json = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--browser" => {
                browser = Some(PathBuf::from(next_value(
                    arguments,
                    &mut index,
                    "--browser",
                )?))
            }
            "--profile" => profile = Some(next_value(arguments, &mut index, "--profile")?),
            "--list" => list = true,
            "--json" => json = true,
            option => return Err(format!("unknown option {option:?}")),
        }
        index += 1;
    }
    match (list, browser, profile) {
        (true, None, None) => Ok(SetupRequest::List { json }),
        (true, _, _) => Err("--list cannot be combined with --browser or --profile".into()),
        (false, Some(browser), profile) => Ok(SetupRequest::Choose {
            browser,
            profile: profile.unwrap_or_else(|| rivet_cli::config::DEFAULT_PROFILE.to_owned()),
            json,
        }),
        (false, None, Some(_)) => Err("--profile needs --browser".into()),
        (false, None, None) => Ok(SetupRequest::Window),
    }
}

/// `rivet setup` opens the setup window. With `--browser` and `--profile` it makes the same choice
/// without a window, and `--list` shows the Chromium browsers it found.
fn run_setup(arguments: &[String]) -> Result<(), String> {
    use rivet_cli::{config, model, system};
    match parse_setup(arguments)? {
        SetupRequest::Window => {
            #[cfg(windows)]
            return rivet_cli::window::open(None);
            #[cfg(not(windows))]
            Err("the setup window is only available on Windows".into())
        }
        SetupRequest::List { json } => {
            let browsers = system::detect_all();
            if json {
                let rows: Vec<_> = browsers
                    .iter()
                    .map(|browser| {
                        serde_json::json!({
                            "name": browser.name,
                            "version": browser.version,
                            "path": browser.path.to_string_lossy(),
                            "proof": browser.proof.to_string_lossy(),
                            "can_open": browser.status.can_open(),
                            "status": browser.status.sentence(&browser.name),
                        })
                    })
                    .collect();
                println!("{}", serde_json::Value::Array(rows));
            } else if browsers.is_empty() {
                println!("no Chromium browser found");
            } else {
                for browser in &browsers {
                    println!(
                        "{}  {}  {}",
                        browser.name,
                        browser.version,
                        browser.path.display()
                    );
                    println!("    {}", browser.status.sentence(&browser.name));
                }
            }
            Ok(())
        }
        SetupRequest::Choose {
            browser,
            profile,
            json,
        } => {
            let found = system::inspect(&browser)
                .map_err(|rejection| format!("{}: {rejection}", browser.display()))?;
            let root = config::data_root()?;
            let saved = model::choose(found, &profile, &root)?;
            config::save(&root, &saved)?;
            if json {
                println!("{}", config::to_json(&saved));
            } else {
                println!(
                    "saved {} with Rivet profile {}",
                    saved.browser.display(),
                    saved.profile
                );
            }
            Ok(())
        }
    }
}

/// The key this build trusts tables under.
fn table_key() -> Result<&'static str, String> {
    signed_table::RELEASE_KEY.ok_or_else(|| "this build of Rivet has no table key".to_owned())
}

fn table_store() -> Result<PathBuf, String> {
    signed_table::default_store()
        .ok_or_else(|| "the local application data folder is unknown".to_owned())
}

/// `rivet builds apply <file>` applies a signed table that is already on this computer.
fn run_builds_apply(arguments: &[String]) -> Result<(), String> {
    let [file] = arguments else {
        return Err("usage: rivet builds apply <file>".into());
    };
    let bytes = fs::read(file).map_err(|error| format!("could not read {file}: {error}"))?;
    let sequence = signed_table::install(&table_store()?, table_key()?, &bytes)?;
    println!("applied build table {sequence}");
    Ok(())
}

/// `rivet builds update [--url <address>]` downloads the signed table and applies it now.
fn run_builds_update(arguments: &[String]) -> Result<(), String> {
    let url = match arguments {
        [] => signed_table::RELEASE_URL
            .ok_or("this build of Rivet has no table address; pass --url")?,
        [flag, url] if flag == "--url" => url.as_str(),
        _ => return Err("usage: rivet builds update [--url <address>]".into()),
    };
    let anytime = std::time::Duration::ZERO;
    match signed_table::refresh(&table_store()?, table_key()?, url, anytime, 30_000)? {
        Some(sequence) => println!("applied build table {sequence}"),
        None => println!("the build table is current"),
    }
    Ok(())
}

fn run_start(options: StartOptions) -> Result<(), String> {
    signed_table::refresh_before_start();
    let request = LaunchRequest {
        browser: options.browser.clone(),
        url: options.url.clone(),
        user_data_dir: options.user_data_dir.clone(),
        display: options.display,
    };
    // Only a window with Rivet's surface may open. A hidden session has no window to protect.
    let held_root = options.hold.clone().or_else(hold::default_root);
    let launch = match (
        &held_root,
        build_launch_plan(&request).map(|plan| plan.display),
    ) {
        (Some(root), Ok(DisplayMode::Visible)) => {
            hold::plan(&request.browser, root, unix_now()).map_err(|refusal| refusal.to_string())?
        }
        _ => Launch::Installed { keep: false },
    };
    let mut request = request;
    let mut held_version = None;
    if let Launch::Held { program, version } = &launch {
        request.browser.clone_from(program);
        held_version = Some(hold::format_version(*version));
        // Without one the copy would open the browser's own profile, which is not Rivet's to touch.
        if request.user_data_dir.is_none() {
            request.user_data_dir = rivet_core::paths::Layout::current()
                .and_then(|layout| default_held_profile(&layout.profiles(), program));
        }
        if request.user_data_dir.is_none() {
            return Err("a held browser build needs a Rivet profile".to_owned());
        }
    }
    let mut plan = build_launch_plan(&request).map_err(|error| error.to_string())?;
    plan.program = resolve_browser_executable(&plan.program);
    if let (Launch::Installed { keep: true }, Some(root)) = (&launch, &held_root) {
        keep_build_in_background(&options.browser, root);
    }
    let runtime = resolve_runtime(options.runtime.as_deref());
    if runtime.is_some() && !profile_allows_rivet(request.user_data_dir.as_deref()) {
        return Err(
            "Rivet runs only on a profile of its own, so that it never enters your own browser; pass --user-data-dir with a folder outside the browser's User Data"
                .into(),
        );
    }
    let launch_started = launch_clock();
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
    // Spike: load the runtime before the browser builds its first window, so window feature
    // answers are in place when the window is made.
    if std::env::var_os("RIVET_SPIKE_EARLY").is_some() {
        if let Some(runtime) = &runtime {
            let _ = inject_runtime(task_id, None, runtime);
        }
    }

    let window = if plan.display == DisplayMode::Visible {
        // Rivet enters only what this launch started. Anything else may be the person's own browser.
        let ours = |process: u32| {
            runtime.is_none() || process_was_launched(process, task_id, launch_started)
        };
        Some(
            wait_for_new_browser_window(
                &plan.program,
                &existing_windows,
                Duration::from_secs(30),
                &ours,
                &mut || !process_tree_is_alive(task_id),
            )
            .map_err(|error| {
                format!("could not identify the launched Rivet browser window: {error}")
            })?,
        )
    } else {
        None
    };

    if let Some(runtime) = &runtime {
        let target_id = window.map_or(task_id, |window| window.process_id);
        inject_runtime(target_id, window.map(|item| item.handle), runtime).map_err(|error| {
            format!("could not load the Rivet runtime into browser task {target_id}: {error}")
        })?;
    }

    let session_id = window
        .map(|window| record_session(&options, &plan.program, &request.url, window))
        .transpose()?;

    if options.json {
        println!(
            "{}",
            start_json(
                task_id,
                plan.display.as_str(),
                &request.url,
                runtime.is_some(),
                session_id.as_deref(),
                held_version.as_deref(),
            )
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
    let mut hold = None;
    let mut json = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--hold" => hold = Some(PathBuf::from(next_value(&arguments, &mut index, "--hold")?)),
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
    let browser = browser.ok_or_else(|| "--browser is required".to_owned())?;
    Ok(StartOptions {
        url: match url {
            Some(url) => url,
            None => {
                let exe = env::current_exe()
                    .map_err(|e| format!("could not locate Rivet executable: {e}"))?;
                let directory = exe
                    .parent()
                    .ok_or("Rivet executable has no parent directory")?;
                let directory = if directory.file_name().is_some_and(|name| name == "deps") {
                    directory.parent().unwrap_or(directory)
                } else {
                    directory
                };
                // The runtime replaces this page when the profile's saved space is another one.
                let label = rivet_core::browser_label(&browser).unwrap_or("Browser");
                rivet_core::start_page_url(directory, "Work", label, "Work", rivet_core::WORK_TINT)
            }
        },
        browser,
        user_data_dir,
        display,
        runtime,
        state,
        hold,
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

/// What `rivet session mini-window` was asked to do.
#[derive(Debug, PartialEq, Eq)]
struct MiniWindowOptions {
    id: String,
    state: Option<PathBuf>,
    command: rivet_protocol::window::MiniWindowCommand,
}

fn parse_mini_window(arguments: Vec<String>) -> Result<MiniWindowOptions, String> {
    let mut id = None;
    let mut state = None;
    let mut words = Vec::new();
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--id" => id = Some(next_value(&arguments, &mut index, "--id")?),
            "--state" => state = Some(PathBuf::from(next_value(&arguments, &mut index, "--state")?)),
            option if option.starts_with("--") => return Err(format!("unknown option {option:?}")),
            word => words.push(word.to_owned()),
        }
        index += 1;
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    Ok(MiniWindowOptions {
        id: id.ok_or_else(|| "--id is required".to_owned())?,
        state,
        command: rivet_protocol::window::MiniWindowCommand::parse(&words)?,
    })
}

/// Sends a Mini Window command to the window of one Rivet session, the same command the window's
/// own buttons run.
fn run_mini_window(options: MiniWindowOptions) -> Result<(), String> {
    let registry = SessionRegistry::load(&state_path(options.state.as_deref())?).map_err(|error| error.to_string())?;
    let session = registry
        .list()
        .find(|session| session.id == options.id)
        .ok_or_else(|| format!("Rivet session {:?} does not exist", options.id))?;
    let (wparam, lparam) = options.command.encode();
    post_window_command(
        &session.browser,
        BrowserWindow { handle: session.window_handle, process_id: session.process_id },
        rivet_protocol::window::MINI_WINDOW_MESSAGE,
        wparam,
        lparam,
    )
    .map_err(|error| error.to_string())
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
        println!("{}", close_json(&session.id));
    } else {
        println!("closed Rivet session {}", session.id);
    }
    Ok(())
}

fn state_path(override_path: Option<&std::path::Path>) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    rivet_core::paths::Layout::current()
        .map(|layout| layout.sessions_file())
        .ok_or_else(|| {
            "the local application data folder is unknown; pass --state <path>".to_owned()
        })
}

/// Moves files older builds left elsewhere into the layout and removes what is past its limit.
/// A move that fails because a file is open is tried again on the next start.
fn tidy_files() {
    let Some(local) = rivet_core::paths::local_app_data() else {
        return;
    };
    rivet_core::paths::migrate(&local);
    rivet_core::paths::prune(&rivet_core::paths::Layout::under(&local), SystemTime::now());
}

fn next_value(arguments: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| format!("{option} needs a value"))
}

fn start_json(
    pid: u32,
    display: &str,
    url: &str,
    runtime_loaded: bool,
    session_id: Option<&str>,
    held_version: Option<&str>,
) -> String {
    serde_json::json!({
        "pid": pid,
        "display": display,
        "url": url,
        "runtime_loaded": runtime_loaded,
        "session_id": session_id,
        "held_version": held_version,
    })
    .to_string()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// `<profiles>\<browser>\Default`, the Rivet profile a held build opens.
fn default_held_profile(profiles: &std::path::Path, program: &std::path::Path) -> Option<PathBuf> {
    let key = hold::browser_key(program.file_name()?.to_str()?);
    Some(profiles.join(key).join("Default"))
}

/// Copies the build that just proved itself, without delaying the window that opens from it.
fn keep_build_in_background(browser: &std::path::Path, root: &std::path::Path) {
    let Ok(rivet) = env::current_exe() else {
        return;
    };
    let _ = Command::new(rivet)
        .args(["hold", "refresh", "--browser"])
        .arg(browser)
        .arg("--hold")
        .arg(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn run_hold_refresh(arguments: Vec<String>) -> Result<(), String> {
    let mut browser = None;
    let mut root = None;
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
            "--hold" => root = Some(PathBuf::from(next_value(&arguments, &mut index, "--hold")?)),
            option => return Err(format!("unknown option {option:?}")),
        }
        index += 1;
    }
    let browser = browser.ok_or("--browser is required")?;
    let root = root
        .or_else(hold::default_root)
        .ok_or("could not find the local application data folder")?;
    match hold::refresh(&browser, &root)? {
        Some(version) => println!("held {}", hold::format_version(version)),
        None => println!("nothing to hold"),
    }
    Ok(())
}

fn close_json(id: &str) -> String {
    serde_json::json!({ "closed": id }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr9_cli_names_a_session_and_a_mini_window_command() {
        use rivet_protocol::window::{Edge, MiniWindowCommand};
        let words = |words: &[&str]| words.iter().map(|word| (*word).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            parse_mini_window(words(&["session", "mini-window", "--id", "s1", "return", "left"])),
            Ok(MiniWindowOptions { id: "s1".into(), state: None, command: MiniWindowCommand::Return(Edge::Left) })
        );
        assert_eq!(
            parse_mini_window(words(&["session", "mini-window", "pop-out", "--id", "s2"])).map(|options| options.command),
            Ok(MiniWindowCommand::PopOut)
        );
        assert!(parse_mini_window(words(&["session", "mini-window", "close"])).is_err(), "the session is required");
        assert!(parse_mini_window(words(&["session", "mini-window", "--id", "s1"])).is_err(), "a command is required");
    }

    #[test]
    fn fr1_setup_without_options_opens_the_window() {
        assert_eq!(parse_setup(&[]), Ok(SetupRequest::Window));
    }

    #[test]
    fn fr1_setup_chooses_a_browser_and_profile_without_a_window() {
        let words = |words: &[&str]| {
            words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse_setup(&words(&[
                "--browser",
                r"C:\B\chrome.exe",
                "--profile",
                "Work",
                "--json"
            ])),
            Ok(SetupRequest::Choose {
                browser: PathBuf::from(r"C:\B\chrome.exe"),
                profile: "Work".into(),
                json: true
            })
        );
        assert_eq!(
            parse_setup(&words(&["--browser", r"C:\B\chrome.exe"])),
            Ok(SetupRequest::Choose {
                browser: PathBuf::from(r"C:\B\chrome.exe"),
                profile: "Default".into(),
                json: false
            })
        );
        assert_eq!(
            parse_setup(&words(&["--list", "--json"])),
            Ok(SetupRequest::List { json: true })
        );
        assert!(parse_setup(&words(&["--profile", "Work"])).is_err());
        assert!(parse_setup(&words(&["--list", "--browser", "x"])).is_err());
        assert!(parse_setup(&words(&["--nope"])).is_err());
    }

    #[test]
    fn fr1_setup_refuses_a_file_that_is_not_a_chromium_browser() {
        let error = run_setup(&[
            "--browser".to_owned(),
            r"C:\Windows\System32\notepad.exe".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("notepad.exe"), "{error}");
    }

    #[test]
    fn nfr6_builds_commands_name_what_they_need() {
        let run_words = |words: &[&str]| run(words.iter().map(|word| (*word).to_owned()));
        assert!(run_words(&["builds", "apply"])
            .unwrap_err()
            .contains("usage"));
        assert!(run_words(&["builds", "update", "--nope"])
            .unwrap_err()
            .contains("usage"));
        assert!(run_words(&["builds", "apply", "no-such-file"])
            .unwrap_err()
            .contains("no-such-file"));
    }

    #[test]
    fn fr3_session_start_defaults_to_local_page() {
        let options = parse_start(vec![
            "session".into(),
            "start".into(),
            "--browser".into(),
            "chrome.exe".into(),
        ])
        .unwrap();
        assert!(options.url.starts_with("file:///"));
        assert!(options.url.ends_with(
            "/start.html?space=Work&browser=Chrome&profile=Work&tint=d08a52&accent=9a5424"
        ));
        // A Windows path only names its file on Windows.
        #[cfg(windows)]
        {
            let whale = parse_start(vec![
                "session".into(),
                "start".into(),
                "--browser".into(),
                r"C:\Naver\whale.exe".into(),
            ])
            .unwrap();
            assert!(
                whale.url.contains("&browser=Whale&"),
                "the page names the browser it opens in"
            );
        }
        assert!(build_launch_plan(&LaunchRequest {
            browser: options.browser,
            url: options.url,
            user_data_dir: None,
            display: DisplayMode::Visible
        })
        .is_ok());
    }

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
    fn nfr6_session_output_names_the_held_build() {
        let parsed: serde_json::Value = serde_json::from_str(&start_json(
            1,
            "visible",
            "u",
            true,
            None,
            Some("156.0.8078.74"),
        ))
        .unwrap();
        assert_eq!(parsed["held_version"], "156.0.8078.74");
    }

    #[test]
    fn nfr6_a_held_build_never_gets_the_browsers_own_profile() {
        let profile = default_held_profile(
            std::path::Path::new("C:/Data/Rivet/profiles"),
            std::path::Path::new("C:/Data/Rivet/browsers/chrome/156.0.8078.74/chrome.exe"),
        );
        assert_eq!(
            profile,
            Some(PathBuf::from("C:/Data/Rivet/profiles/chrome/Default"))
        );
    }

    #[test]
    fn nfr6_parses_the_hold_folder() {
        let options = parse_start(
            [
                "session",
                "start",
                "--browser",
                "C:/B/chrome.exe",
                "--url",
                "about:blank",
                "--hold",
                "C:/H",
            ]
            .map(str::to_owned)
            .to_vec(),
        )
        .unwrap();
        assert_eq!(options.hold, Some(PathBuf::from("C:/H")));
    }

    #[test]
    fn fr4_json_output_escapes_control_characters() {
        let url = "https://example.test/\"a\"\nb\t\u{1}";
        let parsed: serde_json::Value = serde_json::from_str(&start_json(
            7,
            "visible",
            url,
            true,
            Some("id\n\"x\""),
            None,
        ))
        .unwrap();
        assert_eq!(parsed["url"], url);
        assert_eq!(parsed["session_id"], "id\n\"x\"");
        assert_eq!(parsed["pid"], 7);
        let parsed: serde_json::Value = serde_json::from_str(&close_json("a\nb\"c")).unwrap();
        assert_eq!(parsed["closed"], "a\nb\"c");
        let none: serde_json::Value =
            serde_json::from_str(&start_json(1, "hidden", "u", false, None, None)).unwrap();
        assert!(none["session_id"].is_null());
    }

    #[test]
    fn fr2_explicit_runtime_wins_over_an_installed_sibling() {
        let runtime = resolve_runtime(Some(std::path::Path::new("C:/Rivet/custom-runtime.dll")));
        assert_eq!(runtime, Some(PathBuf::from("C:/Rivet/custom-runtime.dll")));
    }
}
