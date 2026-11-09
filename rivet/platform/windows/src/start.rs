//! How Rivet starts a browser process.
//!
//! Chromium shows its first browser window with the show command its own process was started
//! with. `BrowserDesktopWindowTreeHostWin::GetInitialShowState` returns `wShowWindow` from
//! `GetStartupInfo`, and `HWNDMessageHandler::Show` uses that for a window whose first show state
//! is the default one. When that command is `SW_HIDE`, which is what a process started without
//! `STARTF_USESHOWWINDOW` reads, Chromium shows the window with `SW_SHOWNORMAL` and then calls
//! `Activate`, which ends in `SetForegroundWindow`. A process started by the foreground process
//! may take the foreground, so a test browser started from a terminal took the person's keyboard
//! before the Rivet runtime had even attached.
//!
//! This holds only for the default show state. `--window-position` and `--window-size` give the
//! window the normal show state, which `HWNDMessageHandler::Show` turns into `SW_SHOWNORMAL` and
//! `Activate` whatever the startup information says, so a test run starts without them.
//!
//! Under a test run (`RIVET_KEEP_OFFSCREEN`) the browser is therefore started with
//! `STARTF_USESHOWWINDOW` and `SW_SHOWMINNOACTIVE`. Chromium then shows its first window minimized
//! without activating it and skips `Activate`. A minimized window lies on no monitor, so nothing of
//! it shows at the default place Chromium picked, and the Rivet runtime restores it off every
//! monitor, without activation, as it attaches. Chromium does not read `STARTF_USEPOSITION`: its
//! window always has bounds of its own, and saved bounds are pulled back until part of the window
//! is on a monitor. Later windows of the same process read the same startup information and show
//! minimized too; the runtime restores each one off every monitor once.
//!
//! Outside a test run the browser starts exactly as before, through `std::process::Command`.

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::path::Path;

/// `STARTF_USESHOWWINDOW`: `wShowWindow` of the startup information is used.
pub const STARTF_USESHOWWINDOW: u32 = 0x0000_0001;
/// `SW_SHOWNOACTIVATE`: shown in its normal place and size, without activation.
pub const SW_SHOWNOACTIVATE: u16 = 4;
/// `SW_SHOWMINNOACTIVE`: shown minimized, without activation.
pub const SW_SHOWMINNOACTIVE: u16 = 7;
/// `CREATE_UNICODE_ENVIRONMENT`: the environment block is UTF-16.
pub const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;

/// The startup show command a started browser gets, or `None` to leave the startup information as
/// Windows makes it. Only a test run sets one.
pub fn first_show(keep_offscreen: bool) -> Option<u16> {
    keep_offscreen.then_some(SW_SHOWMINNOACTIVE)
}

/// Appends `argument` to a command line the way the Microsoft C runtime splits it again.
pub fn push_argument(line: &mut Vec<u16>, argument: &[u16]) {
    const QUOTE: u16 = b'"' as u16;
    const SLASH: u16 = b'\\' as u16;
    let plain = !argument.is_empty()
        && !argument
            .iter()
            .any(|&unit| matches!(unit, 0x20 | 0x09 | 0x0a | 0x0b | QUOTE));
    if plain {
        line.extend_from_slice(argument);
        return;
    }
    line.push(QUOTE);
    let mut slashes = 0usize;
    for &unit in argument {
        if unit == SLASH {
            slashes += 1;
        } else {
            if unit == QUOTE {
                // Every backslash before a quote is doubled, and the quote itself is escaped.
                line.extend(std::iter::repeat_n(SLASH, slashes + 1));
            }
            slashes = 0;
        }
        line.push(unit);
    }
    // Backslashes before the closing quote are doubled so they stay backslashes.
    line.extend(std::iter::repeat_n(SLASH, slashes));
    line.push(QUOTE);
}

/// The command line for `program` and `arguments`, as UTF-16 units without the final zero. The
/// program is always quoted, since a path holds no quote but may hold a space.
pub fn command_line(program: &[u16], arguments: &[Vec<u16>]) -> Vec<u16> {
    let mut line = vec![b'"' as u16];
    line.extend_from_slice(program);
    line.push(b'"' as u16);
    for argument in arguments {
        line.push(b' ' as u16);
        push_argument(&mut line, argument);
    }
    line
}

/// A UTF-16 environment block: `base` with `extra` set over it, ordered by name without regard to
/// case as Windows expects, each entry ending in a zero and the block in one more.
pub fn environment_block(
    base: Vec<(Vec<u16>, Vec<u16>)>,
    extra: &[(Vec<u16>, Vec<u16>)],
) -> Vec<u16> {
    fn key(name: &[u16]) -> Vec<u16> {
        name.iter()
            .map(|&unit| {
                if (b'a' as u16..=b'z' as u16).contains(&unit) {
                    unit - 32
                } else {
                    unit
                }
            })
            .collect()
    }
    let mut entries: Vec<(Vec<u16>, Vec<u16>)> = base
        .into_iter()
        .filter(|(name, _)| !extra.iter().any(|(other, _)| key(other) == key(name)))
        .collect();
    entries.extend(extra.iter().cloned());
    entries.sort_by_key(|(name, _)| key(name));
    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend_from_slice(&name);
        block.push(b'=' as u16);
        block.extend_from_slice(&value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
fn wide(text: &OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.encode_wide().collect()
}

/// Starts the browser `program` with `arguments` and the extra environment `environment`, and
/// returns its process id. `creation_flags` are added to the process creation flags. Under a test
/// run the browser's first window shows without activation; otherwise the start is a plain
/// `std::process::Command` spawn, as it always was.
#[cfg(windows)]
pub fn spawn_browser(
    program: &Path,
    arguments: &[String],
    environment: &[(&str, &str)],
    creation_flags: u32,
) -> std::io::Result<u32> {
    match first_show(crate::focus::keep_offscreen()) {
        Some(show) => spawn_with_show(program, arguments, environment, creation_flags, show),
        None => {
            use std::os::windows::process::CommandExt;
            let mut command = std::process::Command::new(program);
            command.args(arguments);
            for (name, value) in environment {
                command.env(name, value);
            }
            if creation_flags != 0 {
                command.creation_flags(creation_flags);
            }
            command.spawn().map(|child| child.id())
        }
    }
}

/// Starts the browser `program` with `arguments` and the extra environment `environment`, and
/// returns its process id. Away from Windows there is no startup show command to set.
#[cfg(not(windows))]
pub fn spawn_browser(
    program: &std::path::Path,
    arguments: &[String],
    environment: &[(&str, &str)],
    _creation_flags: u32,
) -> std::io::Result<u32> {
    let mut command = std::process::Command::new(program);
    command.args(arguments);
    for (name, value) in environment {
        command.env(name, value);
    }
    command.spawn().map(|child| child.id())
}

/// Starts `program` through `CreateProcessW` with `show` as its startup show command. No handle
/// of this process is inherited, so the browser holds none of the caller's standard streams, as
/// in a start from the installed launcher.
#[cfg(windows)]
fn spawn_with_show(
    program: &Path,
    arguments: &[String],
    environment: &[(&str, &str)],
    creation_flags: u32,
    show: u16,
) -> std::io::Result<u32> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
    };

    let mut application = wide(program.as_os_str());
    let arguments: Vec<Vec<u16>> = arguments
        .iter()
        .map(|argument| wide(OsStr::new(argument)))
        .collect();
    let mut line = command_line(&application, &arguments);
    line.push(0);
    application.push(0);
    let base = std::env::vars_os()
        .map(|(name, value)| (wide(&name), wide(&value)))
        .collect();
    let extra: Vec<(Vec<u16>, Vec<u16>)> = environment
        .iter()
        .map(|(name, value)| (wide(OsStr::new(name)), wide(OsStr::new(value))))
        .collect();
    let block = environment_block(base, &extra);
    let mut startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = show;
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: every pointer is to a live buffer of this frame, each string ends in a zero, the
    // environment block ends in two, and `process` receives the handles that are closed below.
    let started = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            creation_flags | CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if started == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: both handles were just returned by CreateProcessW and are closed once.
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    Ok(process.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn nfr5_test_browser_starts_minimized_without_activation() {
        // A test run's first window shows minimized and inactive, so it never lies on a monitor
        // before the runtime restores it off every monitor.
        assert_eq!(first_show(true), Some(SW_SHOWMINNOACTIVE));
        assert_eq!(SW_SHOWMINNOACTIVE, 7);
        assert_eq!(SW_SHOWNOACTIVATE, 4);
        assert_eq!(STARTF_USESHOWWINDOW, 1);
    }

    #[test]
    fn nfr5_installed_browser_start_is_unchanged() {
        assert_eq!(first_show(false), None);
    }

    #[test]
    fn nfr5_test_browser_command_line_splits_back_into_its_arguments() {
        let line = command_line(
            &units(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            &[
                units("--no-first-run"),
                units(r"--user-data-dir=C:\Run 1\profile"),
                units(r#"--app=data:text/html,<p class="a">x</p>"#),
                units(r"C:\ends in slash\"),
                units(""),
            ],
        );
        assert_eq!(
            String::from_utf16(&line).unwrap(),
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --no-first-run "--user-data-dir=C:\Run 1\profile" "--app=data:text/html,<p class=\"a\">x</p>" "C:\ends in slash\\" """#
        );
    }

    #[test]
    fn nfr5_test_browser_environment_adds_the_switches_once() {
        let block = environment_block(
            vec![
                (units("Path"), units(r"C:\Windows")),
                (units("RIVET_KEEP_OFFSCREEN"), units("0")),
            ],
            &[(units("rivet_keep_offscreen"), units("1"))],
        );
        let text = String::from_utf16(&block).unwrap();
        assert_eq!(text, "Path=C:\\Windows\0rivet_keep_offscreen=1\0\0");
        assert_eq!(environment_block(Vec::new(), &[]), vec![0, 0]);
    }
}
