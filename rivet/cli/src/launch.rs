//! Opening a session from a saved setup, the way the launcher always did: through
//! `rivet session start`, so the window and the command line share one path.

use crate::config::{profile_dir, profiles_folder, Saved};
use crate::detect::Browser;
use rivet_platform_windows::hold;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The arguments of `rivet session start` for a browser and its Rivet profile folder. Every launch
/// argument is chosen here.
pub fn session_arguments(browser: &Path, user_data_dir: &Path) -> Vec<String> {
    // No URL: a session without one opens the Rivet start page.
    vec![
        "session".to_owned(),
        "start".to_owned(),
        "--browser".to_owned(),
        browser.to_string_lossy().into_owned(),
        "--user-data-dir".to_owned(),
        user_data_dir.to_string_lossy().into_owned(),
        "--display".to_owned(),
        "visible".to_owned(),
    ]
}

/// Where `rivet.exe` is: beside whichever Rivet program is running.
fn rivet_executable() -> Result<PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|error| format!("could not resolve the Rivet launcher location: {error}"))?
        .with_file_name("rivet.exe"))
}

/// The browsers that were already opened with Rivet: the detected ones with a Rivet profile folder.
pub fn browsers_in_use<'a>(root: &Path, detected: &'a [Browser]) -> Vec<&'a Browser> {
    detected
        .iter()
        .filter(|browser| {
            std::fs::read_dir(profiles_folder(root, &browser.path)).is_ok_and(|mut entries| {
                entries.any(|entry| entry.is_ok_and(|entry| entry.path().is_dir()))
            })
        })
        .collect()
}

/// Keeps the installed build of every browser this person already opens with Rivet, so that a copy
/// exists before the browser's next update replaces the build. Nothing waits for the copy.
pub fn keep_used_builds(root: &Path, detected: &[Browser]) {
    let Ok(rivet) = rivet_executable() else {
        return;
    };
    for browser in browsers_in_use(root, detected) {
        let _ = Command::new(&rivet)
            .args(["hold", "refresh", "--browser"])
            .arg(&browser.path)
            .arg("--hold")
            .arg(root.join("Rivet").join("browsers"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Opens the session of a saved setup. A refusal of the hold policy comes back as the reason, and
/// nothing opens in that case, because a window without Rivet's surface is never an answer.
pub fn start(saved: &Saved, root: &Path) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let held = root.join("Rivet").join("browsers");
    hold::plan(&saved.browser, &held, now).map_err(|refusal| refusal.to_string())?;
    Command::new(rivet_executable()?)
        .args(session_arguments(
            &saved.browser,
            &profile_dir(root, &saved.browser, &saved.profile),
        ))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not start Rivet: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Status;

    #[test]
    // The paths here are Windows paths, which only Windows splits into folders.
    #[cfg(windows)]
    fn fr3_setup_opens_the_start_page_in_the_chosen_rivet_profile() {
        let root = Path::new(r"C:\Data");
        let browser = Path::new(r"C:\B\chrome.exe");
        let arguments = session_arguments(browser, &profile_dir(root, browser, "Work"));
        assert!(!arguments.iter().any(|argument| argument == "--url"));
        let at = arguments
            .iter()
            .position(|argument| argument == "--user-data-dir")
            .unwrap();
        assert_eq!(arguments[at + 1], r"C:\Data\Rivet\profiles\chrome\Work");
        let at = arguments
            .iter()
            .position(|argument| argument == "--browser")
            .unwrap();
        assert_eq!(arguments[at + 1], r"C:\B\chrome.exe");
        assert_eq!(&arguments[..2], ["session", "start"]);
    }

    #[test]
    fn nfr6_only_browsers_already_opened_with_rivet_keep_their_build() {
        let root = rivet_core::paths::TestDir::new("rivet-launch");
        let _ = std::fs::remove_dir_all(&root);
        let used = Browser {
            path: PathBuf::from(r"C:\B\chrome.exe"),
            name: "Chrome".into(),
            version: String::new(),
            proof: PathBuf::new(),
            status: Status::Ready,
        };
        let unused = Browser {
            path: PathBuf::from(r"C:\E\msedge.exe"),
            name: "Edge".into(),
            ..used.clone()
        };
        std::fs::create_dir_all(profile_dir(&root, &used.path, "Work")).unwrap();
        let both = [used.clone(), unused];
        let kept = browsers_in_use(&root, &both);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].path, used.path);
        let _ = std::fs::remove_dir_all(&root);
    }
}
