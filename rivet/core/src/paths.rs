//! Where every file Rivet makes at run time lives. This module is the only place that builds those
//! paths, so the layout cannot drift between the CLI, the platform crate and the runtime.
//!
//! Everything sits below one root, `%LOCALAPPDATA%\Rivet`:
//!
//! - `config`: the saved setup and the debug switches a tester places by hand.
//! - `profiles`: the browser profiles Rivet made. Their contents belong to the browser.
//! - `state`: the session registry and the spaces of each browser profile.
//! - `cache`: what can be fetched or rebuilt, the signed build table and the held browser copies.
//! - `logs`: traces, written only when a tester asks for them.
//!
//! The install folder is never written, because the person's account cannot write it and an update
//! replaces it. The OS temp folder is shared with every other program and is not cleaned
//! reliably, so Rivet's own files never go there.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The name of the folder below the local application data folder.
pub const ROOT_NAME: &str = "Rivet";

/// A trace file may grow to this size before it is rotated to a single `.old` copy.
pub const LOG_LIMIT_BYTES: u64 = 4 * 1024 * 1024;
/// Logs not written for this long are removed.
pub const LOG_KEEP: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// A spaces file kept aside because it could not be read is removed after this long.
pub const CORRUPT_KEEP: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The local application data folder of the current user.
pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The folders below one Rivet root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// The layout of the current user, `None` when the local application data folder is unknown.
    pub fn current() -> Option<Self> {
        local_app_data().map(|local| Self::under(&local))
    }

    /// The layout below a local application data folder, `<local>\Rivet`.
    pub fn under(local_app_data: &Path) -> Self {
        Self {
            root: local_app_data.join(ROOT_NAME),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn config(&self) -> PathBuf {
        self.root.join("config")
    }
    pub fn setup_file(&self) -> PathBuf {
        self.config().join("setup.json")
    }
    /// Hand-placed switches for tests and traces. Their names are the old ones from beside the
    /// runtime, so a test script only changes the folder it writes them to.
    pub fn debug(&self) -> PathBuf {
        self.config().join("debug")
    }
    pub fn profiles(&self) -> PathBuf {
        self.root.join("profiles")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }
    pub fn sessions_file(&self) -> PathBuf {
        self.state().join("sessions.json")
    }
    pub fn spaces(&self) -> PathBuf {
        self.state().join("spaces")
    }
    /// Where links from other apps and sites open, one list for every browser and profile.
    pub fn link_rules_file(&self) -> PathBuf {
        self.state().join("link-rules.json")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn builds(&self) -> PathBuf {
        self.cache().join("builds")
    }
    pub fn table_file(&self) -> PathBuf {
        self.builds().join("table.bin")
    }
    pub fn held_browsers(&self) -> PathBuf {
        self.cache().join("browsers")
    }
    /// Short-lived files of one process, such as a launcher's error report.
    pub fn scratch(&self) -> PathBuf {
        self.cache().join("scratch")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Where a trace switched on by `name` writes. A switch is the file `debug\<name>` or the
    /// name listed in `RIVET_TRACE`, separated by commas.
    pub fn trace_file(&self, name: &str) -> Option<PathBuf> {
        self.trace_file_with(name, std::env::var("RIVET_TRACE").ok().as_deref(), None)
    }

    /// As `trace_file`, with `RIVET_TRACE`'s value given and the session's own `rivet-debug`
    /// folder. A switch in that folder writes its trace beside it, so a test run on its own profile
    /// keeps its trace apart from every other run. Nothing else turns a trace on.
    pub fn trace_file_with(
        &self,
        name: &str,
        rivet_trace: Option<&str>,
        own_debug: Option<&Path>,
    ) -> Option<PathBuf> {
        if let Some(own) = own_debug.map(|dir| dir.join(name)) {
            if own.is_file() {
                return Some(own);
            }
        }
        let listed = rivet_trace.is_some_and(|value| {
            value
                .split(',')
                .any(|item| item.trim().eq_ignore_ascii_case(name))
        });
        (listed || self.debug().join(name).is_file()).then(|| self.logs().join(name))
    }
}

/// The name of the debug folder inside a session's profile that tests use.
pub const PROFILE_DEBUG_NAME: &str = "rivet-debug";

/// The profile a browser process was started with, from its `--user-data-dir` switch only. The
/// runtime never guesses the maker's default profile folder, because that is the person's own.
pub fn session_user_data_dir(arguments: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    arguments
        .into_iter()
        .filter_map(|argument| {
            argument
                .strip_prefix("--user-data-dir=")
                .map(|dir| dir.trim_matches('"').to_owned())
        })
        .find(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// The `rivet-debug` folder of the session's profile, when the session has a profile and the
/// folder exists.
pub fn session_debug_dir(arguments: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    session_user_data_dir(arguments)
        .map(|profile| profile.join(PROFILE_DEBUG_NAME))
        .filter(|dir| dir.is_dir())
}

/// Held browser copies kept for each browser. Older ones are removed.
pub const HELD_PER_BROWSER: usize = 2;
/// An unfinished copy of a held browser, or a scratch file, is removed after this long.
pub const UNFINISHED_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// What one migration step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    Moved {
        from: PathBuf,
        to: PathBuf,
    },
    /// The old place was in use or both places exist. It stays, and the next start tries again.
    Kept {
        from: PathBuf,
        reason: String,
    },
    Removed(PathBuf),
}

/// Moves files from the places older builds used into the layout. Every move is a rename on the
/// same volume, so a large held browser is never copied, and a folder with an open file stays
/// where it is until a later start. Nothing inside a profile is opened.
pub fn migrate(local_app_data: &Path) -> Vec<Moved> {
    let layout = Layout::under(local_app_data);
    let root = layout.root().to_path_buf();
    let mut done = Vec::new();
    let steps = [
        (root.join("setup.json"), layout.setup_file()),
        (root.join("browsers"), layout.held_browsers()),
        (root.join("builds"), layout.builds()),
        (root.join("spaces"), layout.spaces()),
        (
            local_app_data
                .join("LogitAI")
                .join("Rivet")
                .join("sessions.json"),
            layout.sessions_file(),
        ),
    ];
    for (from, to) in steps {
        if std::fs::symlink_metadata(&from).is_err() {
            continue;
        }
        if std::fs::symlink_metadata(&to).is_ok() {
            done.push(Moved::Kept {
                from,
                reason: format!("{} already exists", to.display()),
            });
            continue;
        }
        let made = to.parent().map_or(Ok(()), std::fs::create_dir_all);
        match made.and_then(|()| std::fs::rename(&from, &to)) {
            Ok(()) => done.push(Moved::Moved { from, to }),
            Err(error) => done.push(Moved::Kept {
                from,
                reason: error.to_string(),
            }),
        }
    }
    // Folders that older builds made and left empty. `remove_dir` refuses a folder with anything in it.
    for empty in [
        root.join("settings"),
        local_app_data.join("LogitAI").join("Rivet"),
        local_app_data.join("LogitAI"),
    ] {
        if std::fs::remove_dir(&empty).is_ok() {
            done.push(Moved::Removed(empty));
        }
    }
    done
}

/// Removes what is no longer needed: spaces of profiles that no longer exist, unreadable spaces
/// files after `CORRUPT_KEEP`, logs past their limits, scratch files and unfinished held copies
/// after `UNFINISHED_KEEP`, and held browsers beyond `HELD_PER_BROWSER` for each browser.
pub fn prune(layout: &Layout, now: SystemTime) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for entry in read_dir(&layout.spaces()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let gone = if name.ends_with(".json.corrupt") {
            older_than(&path, now, CORRUPT_KEEP)
        } else if name.ends_with(".json.tmp") {
            older_than(&path, now, Duration::from_secs(24 * 60 * 60))
        } else if name.ends_with(".json") && name != "default.json" {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| recorded_profile(&text))
                .is_some_and(|profile| !Path::new(&profile).exists())
        } else {
            false
        };
        if gone && std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    for entry in read_dir(&layout.logs()) {
        let path = entry.path();
        if older_than(&path, now, LOG_KEEP) && std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    for entry in read_dir(&layout.scratch()) {
        let path = entry.path();
        if older_than(&path, now, UNFINISHED_KEEP) && remove_any(&path) {
            removed.push(path);
        }
    }
    for browser in read_dir(&layout.held_browsers()) {
        if browser.file_type().is_ok_and(|kind| kind.is_dir()) {
            removed.extend(prune_held(&browser.path(), now));
        }
    }
    removed
}

/// Removes the unfinished copies past `UNFINISHED_KEEP` and the oldest finished holds beyond
/// `HELD_PER_BROWSER` in one browser's folder of held copies. A hold is renamed aside before it is
/// deleted: a browser running from it blocks the rename, so a hold in use is never half deleted.
fn prune_held(folder: &Path, now: SystemTime) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let mut holds = Vec::new();
    for entry in read_dir(folder) {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".partial-") || name.contains(".removing-") {
            if older_than(&path, now, UNFINISHED_KEEP) && remove_any(&path) {
                removed.push(path);
            }
        } else if let Some(version) = held_version(&name) {
            holds.push((version, path));
        }
    }
    holds.sort();
    let extra = holds.len().saturating_sub(HELD_PER_BROWSER);
    for (_, path) in holds.into_iter().take(extra) {
        let mut aside = path.as_os_str().to_owned();
        aside.push(format!(".removing-{}", std::process::id()));
        let aside = PathBuf::from(aside);
        if std::fs::rename(&path, &aside).is_ok() {
            let _ = std::fs::remove_dir_all(&aside);
            removed.push(path);
        }
    }
    removed
}

/// The four numbers of a held build's folder name, such as `156.0.8078.48`.
fn held_version(name: &str) -> Option<[u32; 4]> {
    let mut parts = name.split('.');
    let mut version = [0; 4];
    for slot in &mut version {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(version)
}

fn remove_any(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path).is_ok(),
        Ok(_) => std::fs::remove_file(path).is_ok(),
        Err(_) => false,
    }
}

/// Rotates a trace file that reached `LOG_LIMIT_BYTES`, so one trace never holds more than twice
/// that. Called before a trace is opened for writing.
pub fn cap_log(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() >= LOG_LIMIT_BYTES) {
        let mut old = path.as_os_str().to_owned();
        old.push(".old");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(path, &old);
    }
}

fn read_dir(path: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default()
}

fn older_than(path: &Path, now: SystemTime, age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| {
            now.duration_since(modified)
                .is_ok_and(|elapsed| elapsed > age)
        })
}

/// The `user_data_dir` a spaces file records, read without a full JSON parser.
fn recorded_profile(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value["user_data_dir"]
        .as_str()
        .filter(|dir| !dir.is_empty())
        .map(str::to_owned)
}

/// A folder of one test below the build's target folder, removed when it is dropped. Tests use it
/// instead of the OS temp folder so that nothing they make outlives the build tree.
pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(name: &str) -> Self {
        // A test binary runs from `target\<profile>\deps`, so its folder's parent is in the target tree.
        let exe = std::env::current_exe().expect("the test binary has a path");
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                exe.parent()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new("."))
                    .join("rivet-test-tmp")
            });
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = base.join(format!("{name}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the test folder can be made");
        Self(path)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TestDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn nfr7_every_kind_of_file_has_one_folder_below_one_root() {
        let layout = Layout::under(Path::new(r"C:\Local"));
        assert_eq!(
            layout.setup_file(),
            Path::new(r"C:\Local\Rivet\config\setup.json")
        );
        assert_eq!(layout.profiles(), Path::new(r"C:\Local\Rivet\profiles"));
        assert_eq!(
            layout.sessions_file(),
            Path::new(r"C:\Local\Rivet\state\sessions.json")
        );
        assert_eq!(layout.spaces(), Path::new(r"C:\Local\Rivet\state\spaces"));
        assert_eq!(
            layout.table_file(),
            Path::new(r"C:\Local\Rivet\cache\builds\table.bin")
        );
        assert_eq!(
            layout.held_browsers(),
            Path::new(r"C:\Local\Rivet\cache\browsers")
        );
        assert_eq!(layout.logs(), Path::new(r"C:\Local\Rivet\logs"));
    }

    #[test]
    fn nfr7_old_places_move_into_the_layout_by_rename() {
        let local = TestDir::new("nfr7-migrate");
        let old = local.join("Rivet");
        std::fs::create_dir_all(old.join("browsers").join("chrome").join("1.2.3.4")).unwrap();
        std::fs::write(
            old.join("browsers")
                .join("chrome")
                .join("1.2.3.4")
                .join("chrome.exe"),
            "x",
        )
        .unwrap();
        std::fs::create_dir_all(old.join("spaces")).unwrap();
        std::fs::write(old.join("spaces").join("default.json"), "{}").unwrap();
        std::fs::create_dir_all(old.join("builds")).unwrap();
        std::fs::write(old.join("setup.json"), "{}").unwrap();
        std::fs::create_dir_all(old.join("settings")).unwrap();
        std::fs::create_dir_all(old.join("profiles").join("chrome").join("Default")).unwrap();
        std::fs::write(
            old.join("profiles")
                .join("chrome")
                .join("Default")
                .join("Cookies"),
            "browser",
        )
        .unwrap();
        let logit = local.join("LogitAI").join("Rivet");
        std::fs::create_dir_all(&logit).unwrap();
        std::fs::write(logit.join("sessions.json"), "[]").unwrap();

        migrate(&local);
        let layout = Layout::under(&local);
        assert!(layout
            .held_browsers()
            .join("chrome")
            .join("1.2.3.4")
            .join("chrome.exe")
            .is_file());
        assert!(layout.spaces().join("default.json").is_file());
        assert!(layout.builds().is_dir());
        assert!(layout.setup_file().is_file());
        assert_eq!(
            std::fs::read_to_string(layout.sessions_file()).unwrap(),
            "[]"
        );
        assert!(!old.join("settings").exists() && !local.join("LogitAI").exists());
        assert!(!old.join("browsers").exists() && !old.join("setup.json").exists());
        assert_eq!(
            std::fs::read_to_string(
                layout
                    .profiles()
                    .join("chrome")
                    .join("Default")
                    .join("Cookies")
            )
            .unwrap(),
            "browser",
            "profiles already sit in their place and are not touched"
        );
        assert!(
            migrate(&local).is_empty(),
            "a second start has nothing to do"
        );
    }

    #[test]
    fn nfr7_a_place_that_cannot_move_stays_for_the_next_start() {
        let local = TestDir::new("nfr7-kept");
        let layout = Layout::under(&local);
        std::fs::create_dir_all(local.join("Rivet").join("spaces")).unwrap();
        std::fs::create_dir_all(layout.spaces()).unwrap();
        std::fs::create_dir_all(local.join("Rivet").join("settings")).unwrap();
        std::fs::write(local.join("Rivet").join("settings").join("kept.txt"), "x").unwrap();
        let done = migrate(&local);
        assert!(matches!(&done[..], [Moved::Kept { .. }]), "{done:?}");
        assert!(local.join("Rivet").join("spaces").is_dir());
        assert!(
            local
                .join("Rivet")
                .join("settings")
                .join("kept.txt")
                .is_file(),
            "a folder with files is not removed"
        );
    }

    #[cfg(windows)]
    #[test]
    fn nfr7_a_held_build_in_use_is_not_moved_and_not_copied() {
        use std::os::windows::fs::OpenOptionsExt;
        let local = TestDir::new("nfr7-locked");
        let exe = local
            .join("Rivet")
            .join("browsers")
            .join("chrome")
            .join("chrome.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "x").unwrap();
        // A running browser opens its files without sharing delete, which blocks a folder rename.
        let open = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&exe)
            .unwrap();
        let done = migrate(&local);
        assert!(matches!(&done[..], [Moved::Kept { .. }]), "{done:?}");
        assert!(
            !Layout::under(&local).held_browsers().exists(),
            "nothing was copied"
        );
        drop(open);
        assert!(matches!(&migrate(&local)[..], [Moved::Moved { .. }]));
    }

    #[test]
    fn nfr7_spaces_of_deleted_profiles_and_old_logs_are_pruned() {
        let local = TestDir::new("nfr7-prune");
        let layout = Layout::under(&local);
        std::fs::create_dir_all(layout.spaces()).unwrap();
        std::fs::create_dir_all(layout.logs()).unwrap();
        let alive = local.join("alive-profile");
        std::fs::create_dir_all(&alive).unwrap();
        let record = |dir: &Path| {
            serde_json::json!({ "version": 1, "user_data_dir": dir.to_string_lossy() }).to_string()
        };
        std::fs::write(layout.spaces().join("aaaa.json"), record(&alive)).unwrap();
        std::fs::write(
            layout.spaces().join("bbbb.json"),
            record(&local.join("gone")),
        )
        .unwrap();
        std::fs::write(layout.spaces().join("default.json"), "{}").unwrap();
        std::fs::write(layout.spaces().join("cccc.json.corrupt"), "{").unwrap();
        std::fs::write(layout.logs().join("rivet-nav.log"), "x").unwrap();

        assert!(prune(&layout, SystemTime::now())
            .iter()
            .all(|path| path.ends_with("bbbb.json")));
        assert!(!layout.spaces().join("bbbb.json").exists());
        assert!(
            layout.spaces().join("aaaa.json").is_file()
                && layout.spaces().join("default.json").is_file()
        );
        assert!(
            layout.spaces().join("cccc.json.corrupt").is_file()
                && layout.logs().join("rivet-nav.log").is_file()
        );

        let later = SystemTime::now() + CORRUPT_KEEP + Duration::from_secs(60);
        prune(&layout, later);
        assert!(!layout.spaces().join("cccc.json.corrupt").exists());
        assert!(!layout.logs().join("rivet-nav.log").exists());
        assert!(layout.spaces().join("aaaa.json").is_file());
    }

    #[test]
    fn nfr7_a_full_log_is_rotated_to_one_old_copy() {
        let dir = TestDir::new("nfr7-cap");
        let log = dir.join("rivet-nav.log");
        std::fs::write(&log, vec![b'x'; LOG_LIMIT_BYTES as usize]).unwrap();
        std::fs::write(dir.join("rivet-nav.log.old"), "older").unwrap();
        cap_log(&log);
        assert!(!log.exists());
        assert_eq!(
            std::fs::metadata(dir.join("rivet-nav.log.old"))
                .unwrap()
                .len(),
            LOG_LIMIT_BYTES
        );
        std::fs::write(&log, "small").unwrap();
        cap_log(&log);
        assert!(log.is_file());
    }

    #[test]
    fn nfr7_a_trace_is_written_to_logs_only_when_switched_on() {
        let local = TestDir::new("nfr7-trace");
        let layout = Layout::under(&local);
        assert_eq!(layout.trace_file("rivet-test-switch.log"), None);
        std::fs::create_dir_all(layout.debug()).unwrap();
        std::fs::write(layout.debug().join("rivet-test-switch.log"), "").unwrap();
        assert_eq!(
            layout.trace_file("rivet-test-switch.log"),
            Some(layout.logs().join("rivet-test-switch.log"))
        );
    }

    #[test]
    fn nfr7_held_browsers_keep_two_per_browser_and_unfinished_copies_go_after_a_day() {
        let local = TestDir::new("nfr7-held");
        let layout = Layout::under(&local);
        let chrome = layout.held_browsers().join("chrome");
        for version in ["150.0.1.1", "156.0.8078.48", "152.0.2.3", "156.0.8078.9"] {
            std::fs::create_dir_all(chrome.join(version)).unwrap();
        }
        std::fs::create_dir_all(chrome.join("157.0.1.1.partial-7")).unwrap();
        std::fs::write(chrome.join("hold-state.json"), "{}").unwrap();
        let edge = layout.held_browsers().join("edge");
        std::fs::create_dir_all(edge.join("140.0.1.1")).unwrap();
        std::fs::create_dir_all(layout.scratch()).unwrap();
        std::fs::write(layout.scratch().join("launch-error.txt"), "x").unwrap();

        prune(&layout, SystemTime::now());
        assert!(chrome.join("156.0.8078.48").is_dir() && chrome.join("156.0.8078.9").is_dir());
        assert!(!chrome.join("150.0.1.1").exists() && !chrome.join("152.0.2.3").exists());
        assert!(
            edge.join("140.0.1.1").is_dir(),
            "each browser keeps its own"
        );
        assert!(chrome.join("hold-state.json").is_file());
        assert!(
            chrome.join("157.0.1.1.partial-7").is_dir()
                && layout.scratch().join("launch-error.txt").is_file(),
            "a copy still being made is kept"
        );

        prune(
            &layout,
            SystemTime::now() + UNFINISHED_KEEP + Duration::from_secs(60),
        );
        assert!(!chrome.join("157.0.1.1.partial-7").exists());
        assert!(!layout.scratch().join("launch-error.txt").exists());
        assert!(chrome.join("156.0.8078.48").is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn nfr7_a_held_browser_in_use_is_not_half_deleted() {
        use std::os::windows::fs::OpenOptionsExt;
        let local = TestDir::new("nfr7-held-busy");
        let layout = Layout::under(&local);
        let chrome = layout.held_browsers().join("chrome");
        for version in ["150.0.1.1", "151.0.1.1", "152.0.1.1"] {
            std::fs::create_dir_all(chrome.join(version)).unwrap();
            std::fs::write(chrome.join(version).join("chrome.exe"), "x").unwrap();
        }
        let open = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(chrome.join("150.0.1.1").join("chrome.exe"))
            .unwrap();
        prune(&layout, SystemTime::now());
        assert!(chrome.join("150.0.1.1").join("chrome.exe").is_file());
        drop(open);
        prune(&layout, SystemTime::now());
        assert!(!chrome.join("150.0.1.1").exists());
    }

    #[test]
    fn nfr7_a_trace_turns_on_only_by_a_switch_file_or_rivet_trace() {
        let local = TestDir::new("nfr7-trace-with");
        let layout = Layout::under(&local);
        let name = "rivet-nav.log";
        assert_eq!(layout.trace_file_with(name, None, None), None);
        assert_eq!(layout.trace_file_with(name, Some("other.log"), None), None);
        assert_eq!(
            layout.trace_file_with(name, Some("x.log, RIVET-NAV.log"), None),
            Some(layout.logs().join(name))
        );
        let own = local.join("profile").join(PROFILE_DEBUG_NAME);
        std::fs::create_dir_all(&own).unwrap();
        assert_eq!(layout.trace_file_with(name, None, Some(&own)), None);
        std::fs::write(own.join(name), "").unwrap();
        assert_eq!(
            layout.trace_file_with(name, None, Some(&own)),
            Some(own.join(name))
        );
    }

    #[test]
    fn nfr7_the_runtime_reads_only_the_sessions_user_data_dir() {
        let args = |list: &[&str]| list.iter().map(|item| item.to_string()).collect::<Vec<_>>();
        assert_eq!(session_user_data_dir(args(&["chrome.exe"])), None);
        assert_eq!(
            session_user_data_dir(args(&["chrome.exe", "--profile-directory=Default"])),
            None,
            "no maker default folder is guessed"
        );
        assert_eq!(session_user_data_dir(args(&["--user-data-dir="])), None);
        assert_eq!(
            session_user_data_dir(args(&["--user-data-dir=\"D:\\P\\chrome\"", "--app=x"])),
            Some(PathBuf::from("D:\\P\\chrome"))
        );
        let profile = TestDir::new("nfr7-session-debug");
        let arg = format!("--user-data-dir={}", profile.display());
        assert_eq!(session_debug_dir([arg.clone()]), None);
        std::fs::create_dir_all(profile.join(PROFILE_DEBUG_NAME)).unwrap();
        assert_eq!(
            session_debug_dir([arg]),
            Some(profile.join(PROFILE_DEBUG_NAME))
        );
    }

    #[test]
    fn nfr7_a_test_folder_is_inside_the_build_tree_and_removed() {
        let path = {
            let dir = TestDir::new("nfr7-self");
            assert!(!dir.starts_with(std::env::temp_dir()));
            dir.to_path_buf()
        };
        assert!(!path.exists());
    }
}
