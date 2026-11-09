//! A link from another app, which Windows hands to Rivet when a person chose Rivet as their
//! browser. Windows runs `rivet-launcher.exe --open-link "<address>"`. The link goes to a running
//! Rivet window of the saved browser, or to the window the saved session opens, as the same
//! open-link message `rivet session open` sends, so the window's link rules decide where it opens.

#[cfg(windows)]
use std::collections::HashSet;
use std::path::Path;

use rivet_core::session::SessionRecord;
#[cfg(windows)]
use rivet_core::session::SessionRegistry;

#[cfg(windows)]
use crate::config::Saved;

/// The option Windows passes the address with. The installer writes it into the open command.
pub const OPEN_LINK: &str = "--open-link";

/// The address of a launch that carries a link: `None` when the launch has no `--open-link`, and
/// an error when it has one without a web address.
pub fn from_arguments(arguments: &[String]) -> Option<Result<String, String>> {
    let at = arguments
        .iter()
        .position(|argument| argument == OPEN_LINK)?;
    let Some(url) = arguments.get(at + 1) else {
        return Some(Err(format!("{OPEN_LINK} needs an address")));
    };
    let url = if url.contains("://") {
        url.clone()
    } else {
        file_url(url)
    };
    Some(
        rivet_protocol::window::encode_open_link(&url)
            .map(|_| url)
            .map_err(|error| format!("Rivet cannot open this link: {error}")),
    )
}

/// The `file:` address of a page file Explorer passes as a path, such as `C:\a b\page.html`.
pub fn file_url(path: &str) -> String {
    let mut out = String::from("file:///");
    for character in path.trim_matches('"').chars() {
        match character {
            '\\' => out.push('/'),
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            other => out.push(other),
        }
    }
    out
}

/// The Rivet sessions of `browser` that may take a link, newest window first by session id, so
/// the window a person used last is tried first. A window that runs from Rivet's held copy of that
/// browser, below `held`, is one of them too: its session names the copy, not the installed one.
pub fn candidates<'a>(
    sessions: impl Iterator<Item = &'a SessionRecord>,
    browser: &Path,
    held: &Path,
) -> Vec<&'a SessionRecord> {
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    let wanted = lower(browser);
    let name = browser.file_name().map(|name| lower(Path::new(name)));
    let copies = browser
        .file_stem()
        .map(|stem| format!("{}{}", lower(&held.join(stem)), std::path::MAIN_SEPARATOR));
    // A launcher stub such as Whale's runs the program in a version folder beside it, and the
    // session names that program.
    let beside = browser.parent().map(lower);
    let of_browser = |session: &&SessionRecord| {
        let path = lower(&session.browser);
        let same_name = session
            .browser
            .file_name()
            .map(|found| lower(Path::new(found)))
            == name;
        path == wanted
            || (same_name
                && beside.is_some()
                && session.browser.parent().and_then(Path::parent).map(lower) == beside)
            || (copies
                .as_ref()
                .is_some_and(|copies| path.starts_with(copies.as_str()))
                && same_name)
    };
    let mut found: Vec<&SessionRecord> = sessions.filter(of_browser).collect();
    found.sort_by(|a, b| b.id.cmp(&a.id));
    found
}

#[cfg(windows)]
fn send(session: &SessionRecord, url: &str) -> bool {
    let Ok(bytes) = rivet_protocol::window::encode_open_link(url) else {
        return false;
    };
    rivet_platform_windows::send_window_data(
        &session.browser,
        rivet_platform_windows::BrowserWindow {
            handle: session.window_handle,
            process_id: session.process_id,
        },
        rivet_protocol::window::OPEN_LINK_DATA,
        &bytes,
    )
    .is_ok()
}

/// Sends `url` to a running window of the saved browser. Without one, the saved session starts
/// and the link goes to its window once the runtime answers.
#[cfg(windows)]
pub fn open(url: &str, saved: &Saved, root: &Path) -> Result<(), String> {
    let registry_file = rivet_core::paths::Layout::under(root).sessions_file();
    let load = || SessionRegistry::load(&registry_file).unwrap_or_default();
    let registry = load();
    let before: HashSet<String> = registry.list().map(|session| session.id.clone()).collect();
    let held = rivet_core::paths::Layout::under(root).held_browsers();
    if candidates(registry.list(), &saved.browser, &held)
        .into_iter()
        .any(|session| send(session, url))
    {
        return Ok(());
    }
    crate::launch::start(saved, root)?;
    // The new window registers itself, then its runtime attaches. Until then a send fails.
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let registry = load();
        let fresh: Vec<&SessionRecord> = candidates(registry.list(), &saved.browser, &held)
            .into_iter()
            .filter(|session| !before.contains(&session.id))
            .collect();
        if fresh.into_iter().any(|session| send(session, url)) {
            return Ok(());
        }
    }
    Err("Rivet started, but its window did not take the link.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn fr3_a_link_from_another_app_is_read_from_the_open_command() {
        assert_eq!(from_arguments(&words(&[])), None);
        assert_eq!(
            from_arguments(&words(&["--open-link", "https://github.com/"])),
            Some(Ok("https://github.com/".into()))
        );
        assert!(matches!(
            from_arguments(&words(&["--open-link"])),
            Some(Err(_))
        ));
        assert!(matches!(
            from_arguments(&words(&["--open-link", "file:///C:/x.ini"])),
            Some(Err(_))
        ));
        assert_eq!(
            from_arguments(&words(&["--open-link", r"C:\My Pages\a#1.html"])),
            Some(Ok("file:///C:/My%20Pages/a%231.html".into())),
            "Explorer passes an HTML file as a path"
        );
        assert!(matches!(
            from_arguments(&words(&["--open-link", r"C:\Windows\win.ini"])),
            Some(Err(_))
        ));
    }

    #[test]
    fn fr3_a_link_from_another_app_goes_to_a_window_of_the_saved_browser() {
        let record = |id: &str, browser: &str| SessionRecord {
            id: id.into(),
            browser: PathBuf::from(browser),
            window_handle: 1,
            process_id: 2,
            url: String::new(),
            display: "visible".into(),
        };
        let all = [
            record("1700000001-a", r"C:\B\chrome.exe"),
            record("1700000003-c", r"C:\B\msedge.exe"),
            record("1700000002-b", r"C:\b\CHROME.exe"),
        ];
        let found = candidates(
            all.iter(),
            Path::new(r"C:\B\chrome.exe"),
            Path::new(r"C:\H"),
        );
        let ids: Vec<&str> = found.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(ids, ["1700000002-b", "1700000001-a"]);
    }

    #[cfg(windows)]
    #[test]
    fn fr3_a_link_goes_to_a_window_that_runs_from_the_held_copy_of_the_saved_browser() {
        let record = |id: &str, browser: &str| SessionRecord {
            id: id.into(),
            browser: PathBuf::from(browser),
            window_handle: 1,
            process_id: 2,
            url: String::new(),
            display: "visible".into(),
        };
        let held = Path::new(r"C:\Local\Rivet\cache\browsers");
        let all = [
            record(
                "1700000001-a",
                r"C:\Local\Rivet\cache\browsers\msedge\156.0.4314.58\156.0.4314.58\msedge.exe",
            ),
            record(
                "1700000002-b",
                r"C:\Local\Rivet\cache\browsers\chrome\156.0.1.2\chrome.exe",
            ),
            record(
                "1700000003-c",
                r"C:\Local\Rivet\cache\browsers\msedge\156.0.4314.58\setup.exe",
            ),
            record("1700000004-d", r"C:\Elsewhere\msedge\msedge.exe"),
        ];
        let found = candidates(
            all.iter(),
            Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            held,
        );
        let ids: Vec<&str> = found.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(ids, ["1700000001-a"]);
    }

    #[cfg(windows)]
    #[test]
    fn fr3_a_link_goes_to_a_window_of_the_program_a_launcher_stub_runs() {
        let record = |id: &str, browser: &str| SessionRecord {
            id: id.into(),
            browser: PathBuf::from(browser),
            window_handle: 1,
            process_id: 2,
            url: String::new(),
            display: "visible".into(),
        };
        let all = [
            record(
                "1700000001-a",
                r"C:\Naver Whale\Application\5.39.412.57\whale.exe",
            ),
            record(
                "1700000002-b",
                r"C:\Naver Whale\Application\5.39.412.57\setup.exe",
            ),
            record(
                "1700000003-c",
                r"C:\Naver Whale\Application\5.39.412.57\x\whale.exe",
            ),
        ];
        let found = candidates(
            all.iter(),
            Path::new(r"C:\Naver Whale\Application\whale.exe"),
            Path::new(r"C:\Local\Rivet\cache\browsers"),
        );
        let ids: Vec<&str> = found.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(ids, ["1700000001-a"]);
    }
}
