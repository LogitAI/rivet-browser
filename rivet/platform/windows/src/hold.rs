//! Held browser builds: a copy of the last build Rivet verified, which Rivet windows open from while
//! the installed browser runs a build Rivet does not support yet.
//!
//! A hold contains program files only. Browser profiles stay out of it. The installed browser keeps
//! updating on its own: a hold is an unregistered folder its updater does not know about. A held
//! build misses security fixes, so it is used for a limited time from when Rivet first saw the
//! unsupported build. After that Rivet explains why and opens nothing, because a window without
//! Rivet's surface is never an acceptable answer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How long a held build may open windows after Rivet first saw the unsupported build.
pub const LIMIT_SECONDS: u64 = 7 * 24 * 60 * 60;

/// The file that marks a finished copy, inside its version folder.
pub const MARKER: &str = "rivet-held.json";
const STATE: &str = "hold-state.json";
const KEPT_HOLDS: usize = 2;

pub type Version = [u16; 4];

/// All application packages and all restricted application packages may read and run a hold.
const HOLD_GRANTS: [&str; 2] = ["*S-1-15-2-1:(OI)(CI)(RX)", "*S-1-15-2-2:(OI)(CI)(RX)"];

/// The builds with a complete and checked table row. The runtime's tests compare this list with its
/// table, so the launcher and the runtime cannot disagree.
pub const VERIFIED: [(&str, Version); 13] = [
    ("chrome.exe", [157, 0, 8103, 39]),
    ("chrome.exe", [157, 0, 8103, 38]),
    ("chrome.exe", [156, 0, 8078, 82]),
    ("chrome.exe", [156, 0, 8078, 80]),
    ("chrome.exe", [156, 0, 8078, 74]),
    ("chrome.exe", [156, 0, 8078, 48]),
    ("msedge.exe", [156, 0, 4314, 50]),
    ("msedge.exe", [156, 0, 4314, 58]),
    ("whale.exe", [5, 39, 412, 57]),
    ("brave.exe", [156, 1, 98, 62]),
    ("brave.exe", [156, 1, 98, 63]),
    ("brave.exe", [157, 1, 99, 55]),
    ("samsunginternet.exe", [30, 0, 4, 29]),
];

/// The module that holds the browser's code, for the browsers Rivet integrates with.
pub fn module_of(executable: &str) -> Option<&'static str> {
    match executable.to_ascii_lowercase().as_str() {
        "chrome.exe" | "brave.exe" => Some("chrome.dll"),
        "msedge.exe" => Some("msedge.dll"),
        "whale.exe" => Some("whale.dll"),
        "samsunginternet.exe" => Some("samsunginternet.dll"),
        _ => None,
    }
}

/// The builds of `executable` that have a usable row: the ones built into Rivet, and the ones in the signed
/// table Rivet applied since. A build the signed table adds needs no hold, and can be held once it is old.
pub fn supported_versions(executable: &str) -> Vec<Version> {
    let mut versions = built_in_versions(executable);
    if let (Some(store), Some(key)) = (
        crate::signed_table::default_store(),
        crate::signed_table::RELEASE_KEY,
    ) {
        if let Ok(Some(table)) = crate::signed_table::read_store(&store, key) {
            versions.extend(table_versions(&table, executable));
        }
    }
    versions
}

fn built_in_versions(executable: &str) -> Vec<Version> {
    let executable = executable.to_ascii_lowercase();
    VERIFIED
        .iter()
        .filter(|(name, _)| *name == executable)
        .map(|(_, version)| *version)
        .collect()
}

/// The versions a table has rows for, for the browser that `executable` starts.
pub fn table_versions(table: &serde_json::Value, executable: &str) -> Vec<Version> {
    let (Some(module), name) = (module_of(executable), display_name(executable)) else {
        return Vec::new();
    };
    let rows = table["rows"].as_array().map_or(&[][..], Vec::as_slice);
    rows.iter()
        .map(|row| &row["build"])
        .filter(|build| build["browser"] == name && build["module"] == module)
        .filter_map(|build| serde_json::from_value::<Version>(build["version"].clone()).ok())
        .collect()
}

pub fn format_version(version: Version) -> String {
    format!(
        "{}.{}.{}.{}",
        version[0], version[1], version[2], version[3]
    )
}

pub fn parse_version(text: &str) -> Option<Version> {
    let parts = text
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<Vec<u16>>>()?;
    parts.try_into().ok()
}

/// The clock of one browser, saved between launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    /// When Rivet first saw an unsupported build, in seconds since the Unix epoch.
    pub first_detected: u64,
    /// The unsupported build Rivet saw last.
    pub installed: String,
}

/// What a saved clock file holds. A file that cannot be read must not restart the limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedClock {
    None,
    Running(Clock),
    Unreadable,
}

/// Why no window opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The installed build is unsupported and Rivet never kept a verified one.
    NoHeldBuild { browser: String, installed: Version },
    /// The held build was used for the whole limit.
    Expired {
        browser: String,
        installed: Version,
        held: Option<Version>,
    },
    /// The limit cannot be enforced without its saved clock.
    Unwritable { browser: String, reason: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHeldBuild { browser, installed } => write!(
                f,
                "Rivet does not support {browser} {} yet and has no verified copy of an earlier build. \
                 Rivet will not open it without its window surface. Update Rivet, or choose another browser.",
                format_version(*installed)
            ),
            Self::Expired { browser, installed, held } => write!(
                f,
                "Rivet has not supported {browser} {} for 7 days. Rivet no longer opens new windows from the \
                 older verified build{}, because it misses security fixes. Update Rivet, or choose another browser.",
                format_version(*installed),
                held.map_or(String::new(), |held| format!(" {}", format_version(held)))
            ),
            Self::Unwritable { browser, reason } => {
                write!(f, "Rivet could not record the {browser} update state: {reason}")
            }
        }
    }
}

/// Where a Rivet window opens from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The installed browser is supported, or Rivet does not integrate with this executable.
    Installed,
    Held {
        version: Version,
        clock: Clock,
    },
    Refused(Refusal),
}

/// Decides where a window opens from. This is the whole policy and touches nothing.
pub fn decide(
    executable: &str,
    installed: Option<Version>,
    supported: &[Version],
    held: &[Version],
    saved: &SavedClock,
    now: u64,
) -> Decision {
    let Some(installed) = installed else {
        return Decision::Installed;
    };
    if supported.contains(&installed) {
        return Decision::Installed;
    }
    let browser = display_name(executable).to_owned();
    let newest = held
        .iter()
        .copied()
        .filter(|version| supported.contains(version))
        .max();
    let first_detected = match saved {
        SavedClock::None => now,
        // A newer unsupported build must not restart the limit. Only a supported one ends it.
        SavedClock::Running(clock) => clock.first_detected,
        SavedClock::Unreadable => {
            return Decision::Refused(Refusal::Expired {
                browser,
                installed,
                held: newest,
            });
        }
    };
    let Some(version) = newest else {
        return Decision::Refused(Refusal::NoHeldBuild { browser, installed });
    };
    // A clock that went backwards cannot prove the limit still holds.
    if now < first_detected || now - first_detected >= LIMIT_SECONDS {
        return Decision::Refused(Refusal::Expired {
            browser,
            installed,
            held: Some(version),
        });
    }
    let clock = Clock {
        first_detected,
        installed: format_version(installed),
    };
    Decision::Held { version, clock }
}

fn display_name(executable: &str) -> &str {
    match executable.to_ascii_lowercase().as_str() {
        "chrome.exe" => "Google Chrome",
        "msedge.exe" => "Microsoft Edge",
        "brave.exe" => "Brave",
        "whale.exe" => "Naver Whale",
        "samsunginternet.exe" => "Samsung Internet",
        _ => "this browser",
    }
}

/// `chrome`, `msedge` and so on, the name of a browser's folders below Rivet's own.
pub fn browser_key(executable: &str) -> String {
    executable
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_owned()
}

/// The warning a window opened from a held build shows.
pub fn warning_text(installed: &str, first_detected: u64, now: u64) -> String {
    let left = (first_detected + LIMIT_SECONDS).saturating_sub(now);
    let days = left.div_ceil(24 * 60 * 60);
    let unit = if days == 1 { "day" } else { "days" };
    format!("Older browser build without the latest security fixes. Rivet does not support {installed} yet. {days} {unit} left.")
}
/// The root of a default hold, the held browsers folder of the layout.
/// The root of a default hold: `%LOCALAPPDATA%\Rivet\browsers`.
pub fn default_root() -> Option<PathBuf> {
    rivet_core::paths::Layout::current().map(|layout| layout.held_browsers())
}

/// The folder that holds a browser's version folders and its clock.
fn browser_folder(root: &Path, executable: &str) -> PathBuf {
    root.join(browser_key(executable))
}

/// The finished holds of a browser.
pub fn held_versions(root: &Path, executable: &str) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(browser_folder(root, executable)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join(MARKER).is_file())
        .filter_map(|entry| parse_version(entry.file_name().to_str()?))
        .collect()
}

pub fn read_clock(root: &Path, executable: &str) -> SavedClock {
    read_clock_file(&browser_folder(root, executable).join(STATE))
}

fn read_clock_file(path: &Path) -> SavedClock {
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SavedClock::None,
        Err(_) => SavedClock::Unreadable,
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_or(SavedClock::Unreadable, SavedClock::Running)
        }
    }
}

/// The saved clock is written next to a temporary name first, so a crash cannot leave half a file.
fn write_clock(root: &Path, executable: &str, clock: &Clock) -> std::io::Result<()> {
    let folder = browser_folder(root, executable);
    std::fs::create_dir_all(&folder)?;
    let temporary = folder.join(format!("{STATE}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_vec(clock).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(temporary, folder.join(STATE))
}

fn clear_clock(root: &Path, executable: &str) {
    let _ = std::fs::remove_file(browser_folder(root, executable).join(STATE));
}

/// The executable inside a hold.
pub fn held_executable(root: &Path, executable: &str, version: Version) -> PathBuf {
    browser_folder(root, executable)
        .join(format_version(version))
        .join(executable)
}

/// What a launch uses after the policy and its saved state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Open the path the caller gave. `keep` asks for a hold of that build once it is open.
    Installed {
        keep: bool,
    },
    Held {
        program: PathBuf,
        version: Version,
    },
}

/// Applies the policy for one browser path and saves the clock.
pub fn plan(browser: &Path, root: &Path, now: u64) -> Result<Launch, Refusal> {
    let supported = browser
        .file_name()
        .and_then(|name| name.to_str())
        .map(supported_versions)
        .unwrap_or_default();
    plan_with(browser, root, now, &supported)
}

/// `plan` with the builds that have a usable row given by the caller.
pub fn plan_with(
    browser: &Path,
    root: &Path,
    now: u64,
    supported: &[Version],
) -> Result<Launch, Refusal> {
    let Some(executable) = browser.file_name().and_then(|name| name.to_str()) else {
        return Ok(Launch::Installed { keep: false });
    };
    // A path inside the hold already is the held build.
    if browser.starts_with(root) {
        return Ok(Launch::Installed { keep: false });
    }
    let installed = installed_version(browser);
    let held = held_versions(root, executable);
    match decide(
        executable,
        installed,
        supported,
        &held,
        &read_clock(root, executable),
        now,
    ) {
        Decision::Installed => {
            let keep = installed.is_some_and(|version| !held.contains(&version));
            if installed.is_some() {
                clear_clock(root, executable);
            }
            Ok(Launch::Installed { keep })
        }
        Decision::Held { version, clock } => {
            write_clock(root, executable, &clock).map_err(|error| Refusal::Unwritable {
                browser: display_name(executable).to_owned(),
                reason: error.to_string(),
            })?;
            Ok(Launch::Held {
                program: held_executable(root, executable, version),
                version,
            })
        }
        Decision::Refused(refusal) => {
            // The clock starts at the first sight even when nothing can be opened yet.
            if let (Refusal::NoHeldBuild { installed, .. }, SavedClock::None) =
                (&refusal, read_clock(root, executable))
            {
                let clock = Clock {
                    first_detected: now,
                    installed: format_version(*installed),
                };
                let _ = write_clock(root, executable, &clock);
            }
            Err(refusal)
        }
    }
}

/// The status a runtime reads about the process it lives in: the unsupported build and when Rivet
/// first saw it, if `executable` belongs to a hold.
pub fn held_status(executable: &Path) -> Option<Clock> {
    let version_folder = executable
        .ancestors()
        .skip(1)
        .take(2)
        .find(|folder| folder.join(MARKER).is_file())?;
    match read_clock_file(&version_folder.parent()?.join(STATE)) {
        SavedClock::Running(clock) => Some(clock),
        // A held build always warns, even if its clock cannot be read.
        _ => Some(Clock {
            first_detected: 0,
            installed: "the installed build".to_owned(),
        }),
    }
}

/// The newest version of `module` beside a browser executable, by each folder's version resource.
pub fn installed_version(browser: &Path) -> Option<Version> {
    let module = module_of(browser.file_name()?.to_str()?)?;
    let folder = browser.parent()?;
    // An unpacked browser keeps its module beside the executable, without a version folder.
    let beside = file_version(&folder.join(module));
    let entries = std::fs::read_dir(folder).ok()?;
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(module))
        .filter(|path| path.is_file())
        .filter_map(|path| file_version(&path))
        .chain(beside)
        .max()
}

/// The folder beside `browser` with the module of `version`.
fn version_folder(browser: &Path, module: &str, version: Version) -> Option<PathBuf> {
    std::fs::read_dir(browser.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|folder| file_version(&folder.join(module)) == Some(version))
}

/// Copies the installed build into the hold unless it already is there. Only the launcher files and
/// the one version folder are program files of this build. The copy gets its final name only when
/// it is complete, so a hold is never half a browser.
pub fn refresh(browser: &Path, root: &Path) -> Result<Option<Version>, String> {
    let executable = browser
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the browser path has no file name")?;
    refresh_with(browser, root, &supported_versions(executable))
}

/// `refresh` with the builds that have a usable row given by the caller.
pub fn refresh_with(
    browser: &Path,
    root: &Path,
    supported: &[Version],
) -> Result<Option<Version>, String> {
    let executable = browser
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the browser path has no file name")?;
    let module = module_of(executable).ok_or("Rivet does not integrate with this browser")?;
    let version = installed_version(browser).ok_or("the browser's version could not be read")?;
    if !supported.contains(&version) {
        return Ok(None);
    }
    let folder = browser_folder(root, executable);
    let finished = folder.join(format_version(version));
    if finished.join(MARKER).is_file() {
        return Ok(None);
    }
    let source = browser.parent().ok_or("the browser path has no folder")?;
    let inner = version_folder(browser, module, version)
        .ok_or("the browser's version folder is missing")?;
    let partial = folder.join(format!(
        "{}.partial-{}",
        format_version(version),
        std::process::id()
    ));
    let fail = |error: std::io::Error| format!("could not copy {}: {error}", browser.display());
    std::fs::create_dir_all(&partial).map_err(fail)?;
    allow_sandboxed_reads(&partial);
    let copied = copy_build(source, &inner, &partial, &format_version(version));
    if let Err(error) = copied.and_then(|()| {
        // A concurrent launch may have finished the same copy first, which is just as good.
        if finished.exists() {
            let _ = std::fs::remove_dir_all(&finished);
        }
        std::fs::rename(&partial, &finished)
    }) {
        let _ = std::fs::remove_dir_all(&partial);
        return if finished.join(MARKER).is_file() {
            Ok(None)
        } else {
            Err(fail(error))
        };
    }
    prune(root, executable);
    Ok(Some(version))
}

/// The accounts of an app container, as the browser's installer grants them on its own folder. The
/// browser's more restricted sandboxes read program files through them. Files copied later inherit it.
fn allow_sandboxed_reads(folder: &Path) {
    let _ = crate::background::hidden(&mut std::process::Command::new("icacls"))
        .arg(folder)
        .args(HOLD_GRANTS.iter().flat_map(|sid| ["/grant", *sid]))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn copy_build(source: &Path, inner: &Path, target: &Path, version: &str) -> std::io::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    let name = inner
        .file_name()
        .ok_or_else(|| std::io::Error::other("no folder name"))?;
    copy_tree(inner, &target.join(name))?;
    std::fs::write(
        target.join(MARKER),
        format!("{{\"version\":\"{version}\"}}"),
    )
}

fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        // The installer archive only reinstalls the browser, and a hold never installs anything.
        if entry.file_name().eq_ignore_ascii_case("Installer") {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Keeps the newest holds. A hold with open windows cannot be removed and stays for the next time.
fn prune(root: &Path, executable: &str) {
    let mut versions = held_versions(root, executable);
    versions.sort_unstable();
    let extra = versions.len().saturating_sub(KEPT_HOLDS);
    for version in versions.into_iter().take(extra) {
        let _ =
            std::fs::remove_dir_all(browser_folder(root, executable).join(format_version(version)));
    }
    remove_stale_partials(&browser_folder(root, executable), PARTIAL_KEEP_SECONDS);
}

/// A copy that a stopped process left unfinished is never used, so it goes once it is older than `keep`.
fn remove_stale_partials(folder: &Path, keep: u64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age.as_secs() >= keep));
        if stale && entry.file_name().to_string_lossy().contains(".partial-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// How long an unfinished copy may stay before a later copy removes it.
const PARTIAL_KEEP_SECONDS: u64 = 24 * 60 * 60;

#[cfg(windows)]
fn file_version(path: &Path) -> Option<Version> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    if !path.is_file() {
        return None;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut info = std::ptr::null_mut();
        let mut length = 0u32;
        let root = [u16::from(b'\\'), 0];
        if VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut info, &mut length) == 0
            || info.is_null()
            || (length as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
        {
            return None;
        }
        let info = &*(info as *const VS_FIXEDFILEINFO);
        Some([
            (info.dwFileVersionMS >> 16) as u16,
            info.dwFileVersionMS as u16,
            (info.dwFileVersionLS >> 16) as u16,
            info.dwFileVersionLS as u16,
        ])
    }
}

#[cfg(not(windows))]
fn file_version(_path: &Path) -> Option<Version> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW: Version = [156, 0, 8078, 99];
    const HELD: Version = [156, 0, 8078, 74];
    const SUPPORTED: [Version; 2] = [[156, 0, 8078, 48], HELD];
    const DAY: u64 = 24 * 60 * 60;

    fn running(first_detected: u64) -> SavedClock {
        SavedClock::Running(Clock {
            first_detected,
            installed: format_version(NEW),
        })
    }

    #[test]
    fn nfr6_a_supported_build_opens_as_installed() {
        assert_eq!(
            decide(
                "chrome.exe",
                Some(HELD),
                &SUPPORTED,
                &[],
                &SavedClock::None,
                10
            ),
            Decision::Installed
        );
    }

    #[test]
    fn nfr6_a_browser_rivet_does_not_integrate_with_keeps_its_path() {
        assert_eq!(
            decide("other.exe", None, &SUPPORTED, &[], &SavedClock::None, 10),
            Decision::Installed
        );
    }

    #[test]
    fn nfr6_an_unsupported_build_opens_the_held_build_and_starts_the_clock() {
        let expected = Decision::Held {
            version: HELD,
            clock: Clock {
                first_detected: 1000,
                installed: format_version(NEW),
            },
        };
        assert_eq!(
            decide(
                "chrome.exe",
                Some(NEW),
                &SUPPORTED,
                &[HELD],
                &SavedClock::None,
                1000
            ),
            expected
        );
    }

    #[test]
    fn nfr6_the_newest_verified_hold_wins_and_an_unverified_folder_is_ignored() {
        let held = [[156, 0, 8078, 48], HELD, [157, 0, 0, 1]];
        let Decision::Held { version, .. } = decide(
            "chrome.exe",
            Some(NEW),
            &SUPPORTED,
            &held,
            &SavedClock::None,
            1,
        ) else {
            panic!("a verified hold exists");
        };
        assert_eq!(version, HELD);
    }

    #[test]
    fn nfr6_without_a_hold_nothing_opens_rather_than_a_plain_browser() {
        assert!(matches!(
            decide(
                "chrome.exe",
                Some(NEW),
                &SUPPORTED,
                &[],
                &SavedClock::None,
                1
            ),
            Decision::Refused(Refusal::NoHeldBuild { .. })
        ));
    }

    #[test]
    fn nfr6_a_held_build_opens_windows_for_seven_days_and_not_after() {
        let start = 1_000_000;
        let open = |now| {
            decide(
                "chrome.exe",
                Some(NEW),
                &SUPPORTED,
                &[HELD],
                &running(start),
                now,
            )
        };
        assert!(matches!(open(start + 7 * DAY - 1), Decision::Held { .. }));
        assert!(matches!(
            open(start + 7 * DAY),
            Decision::Refused(Refusal::Expired { .. })
        ));
        assert!(matches!(
            open(start + 30 * DAY),
            Decision::Refused(Refusal::Expired { .. })
        ));
    }

    #[test]
    fn nfr6_a_second_unsupported_build_does_not_restart_the_limit() {
        let later: Version = [156, 0, 8078, 120];
        let decision = decide(
            "chrome.exe",
            Some(later),
            &SUPPORTED,
            &[HELD],
            &running(500),
            500 + DAY,
        );
        let Decision::Held { clock, .. } = decision else {
            panic!("still inside the limit")
        };
        assert_eq!(clock.first_detected, 500);
    }

    #[test]
    fn nfr6_a_lost_clock_or_a_clock_set_back_does_not_extend_the_limit() {
        assert!(matches!(
            decide(
                "chrome.exe",
                Some(NEW),
                &SUPPORTED,
                &[HELD],
                &SavedClock::Unreadable,
                1
            ),
            Decision::Refused(Refusal::Expired { .. })
        ));
        assert!(matches!(
            decide(
                "chrome.exe",
                Some(NEW),
                &SUPPORTED,
                &[HELD],
                &running(1000),
                999
            ),
            Decision::Refused(Refusal::Expired { .. })
        ));
    }

    #[test]
    fn nfr6_the_refusal_says_why() {
        let text = Refusal::Expired {
            browser: "Google Chrome".into(),
            installed: NEW,
            held: Some(HELD),
        }
        .to_string();
        assert!(
            text.contains("156.0.8078.99")
                && text.contains("156.0.8078.74")
                && text.contains("7 days")
        );
        assert!(text.contains("security fixes"));
    }

    #[test]
    fn nfr6_the_warning_counts_whole_days_up() {
        assert!(warning_text("9.9.9.9", 0, 1).contains("7 days left"));
        assert!(warning_text("9.9.9.9", 0, 6 * DAY + 1).contains("1 day left"));
        assert!(warning_text("9.9.9.9", 0, 7 * DAY).contains("0 days left"));
    }

    fn scratch(name: &str) -> rivet_core::paths::TestDir {
        rivet_core::paths::TestDir::new(&format!("rivet-hold-{name}"))
    }

    #[test]
    fn nfr7_an_unfinished_copy_past_its_limit_is_removed_and_holds_stay() {
        let root = scratch("stale");
        let folder = root.join("chrome");
        std::fs::create_dir_all(folder.join("156.0.8078.48.partial-1")).unwrap();
        std::fs::create_dir_all(folder.join("156.0.8078.74")).unwrap();
        remove_stale_partials(&folder, PARTIAL_KEEP_SECONDS);
        assert!(
            folder.join("156.0.8078.48.partial-1").is_dir(),
            "a fresh copy may still be running"
        );
        remove_stale_partials(&folder, 0);
        assert!(!folder.join("156.0.8078.48.partial-1").exists());
        assert!(folder.join("156.0.8078.74").is_dir());
    }

    #[test]
    fn nfr6_an_unfinished_copy_is_not_a_hold() {
        let root = scratch("partial");
        let finished = root.join("chrome").join("156.0.8078.74");
        std::fs::create_dir_all(&finished).unwrap();
        std::fs::create_dir_all(root.join("chrome").join("156.0.8078.48.partial-1")).unwrap();
        assert!(held_versions(&root, "chrome.exe").is_empty());
        std::fs::write(finished.join(MARKER), "{}").unwrap();
        assert_eq!(held_versions(&root, "chrome.exe"), vec![HELD]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nfr6_a_process_in_a_hold_reads_its_clock_and_any_other_process_has_none() {
        let root = scratch("status");
        let version = root.join("whale").join("5.39.412.57");
        let inner = version.join("5.39.412.57");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(version.join(MARKER), "{}").unwrap();
        // The launcher stub's process and the inner browser process are both inside the hold.
        assert_eq!(
            held_status(&inner.join("whale.exe"))
                .unwrap()
                .first_detected,
            0
        );
        assert_eq!(
            held_status(&version.join("whale.exe"))
                .unwrap()
                .first_detected,
            0
        );
        assert!(held_status(&root.join("elsewhere").join("whale.exe")).is_none());
        let clock = Clock {
            first_detected: 77,
            installed: "5.40.0.0".into(),
        };
        write_clock(&root, "whale.exe", &clock).unwrap();
        assert_eq!(held_status(&inner.join("whale.exe")), Some(clock));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_the_installed_build_is_read_from_the_module_of_any_layout() {
        // Only checks on this machine's browsers, which tests do not change.
        let chrome = Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        if chrome.is_file() {
            assert!(installed_version(chrome).is_some(), "version folder layout");
        }
        let root = scratch("flat");
        let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap();
        // A module with a version resource beside the executable counts too.
        std::fs::copy(
            system.join("System32").join("kernel32.dll"),
            root.join("chrome.dll"),
        )
        .unwrap();
        assert!(installed_version(&root.join("chrome.exe")).is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_sandboxed_processes_can_read_a_hold() {
        let root = scratch("acl");
        allow_sandboxed_reads(&root);
        std::fs::write(root.join("file"), "x").unwrap();
        let listed = crate::background::hidden(&mut std::process::Command::new("icacls"))
            .arg(root.join("file"))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&listed.stdout);
        // The first name is the same in every language, the second one is translated.
        assert!(text.contains("ALL APPLICATION PACKAGES"), "{text}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nfr6_a_corrupt_clock_file_counts_as_unreadable() {
        let root = scratch("corrupt");
        std::fs::create_dir_all(root.join("chrome")).unwrap();
        std::fs::write(root.join("chrome").join(STATE), "not json").unwrap();
        assert_eq!(read_clock(&root, "chrome.exe"), SavedClock::Unreadable);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A browser folder with a stub launcher and a version folder whose module is a system file, so that its
    /// version resource is real. Returns the browser path and the version it reads as.
    #[cfg(windows)]
    fn fake_install(root: &Path) -> (PathBuf, Version) {
        let app = root.join("Application");
        let inner = app.join("10.0.0.1");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(app.join("chrome.exe"), "stub").unwrap();
        let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap();
        std::fs::copy(
            system.join("System32").join("kernel32.dll"),
            inner.join("chrome.dll"),
        )
        .unwrap();
        std::fs::write(inner.join("resources.pak"), "program").unwrap();
        let browser = app.join("chrome.exe");
        let version = installed_version(&browser).expect("the stub module has a version");
        (browser, version)
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_keeping_a_build_copies_program_files_and_never_a_profile() {
        let root = scratch("keep");
        let (browser, version) = fake_install(&root);
        let app = browser.parent().unwrap();
        // Things that belong to a person or an updater and sit near a browser's program files.
        for private in ["Default", "User Data", "Installer"] {
            std::fs::create_dir_all(app.join(private)).unwrap();
            std::fs::write(app.join(private).join("Login Data"), "secret").unwrap();
        }
        std::fs::create_dir_all(app.join("10.0.0.1").join("Installer")).unwrap();
        std::fs::write(
            app.join("10.0.0.1").join("Installer").join("setup.exe"),
            "setup",
        )
        .unwrap();
        let hold = root.join("browsers");
        assert_eq!(
            refresh_with(&browser, &hold, &[version]).unwrap(),
            Some(version)
        );
        let kept = hold.join("chrome").join(format_version(version));
        assert!(kept.join(MARKER).is_file() && kept.join("chrome.exe").is_file());
        assert!(kept.join("10.0.0.1").join("chrome.dll").is_file());
        for private in ["Default", "User Data", "Installer"] {
            assert!(!kept.join(private).exists(), "{private} was copied");
        }
        assert!(!kept.join("10.0.0.1").join("Installer").exists());
        // Nothing but folders and files of the build: no file in the copy is named like browser data.
        let mut names = Vec::new();
        let mut folders = vec![kept.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(folder).unwrap().flatten() {
                names.push(entry.file_name().to_string_lossy().to_ascii_lowercase());
                if entry.path().is_dir() {
                    folders.push(entry.path());
                }
            }
        }
        for private in [
            "login data",
            "cookies",
            "local state",
            "web data",
            "history",
        ] {
            assert!(
                !names.iter().any(|name| name == private),
                "{private} was copied"
            );
        }
        // A second request finds the finished copy and does nothing.
        assert_eq!(refresh_with(&browser, &hold, &[version]).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_a_build_without_a_usable_row_is_not_kept() {
        let root = scratch("keep-unsupported");
        let (browser, _) = fake_install(&root);
        let hold = root.join("browsers");
        assert_eq!(refresh_with(&browser, &hold, &[HELD]).unwrap(), None);
        assert!(!hold.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_a_launch_of_an_unsupported_install_opens_the_held_copy_then_stops_after_seven_days() {
        let root = scratch("plan");
        let (browser, _) = fake_install(&root);
        let hold = root.join("browsers");
        let held = hold.join("chrome").join(format_version(HELD));
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join(MARKER), "{}").unwrap();
        let supported = [HELD];
        let first = 5_000_000;
        let Launch::Held { program, version } =
            plan_with(&browser, &hold, first, &supported).unwrap()
        else {
            panic!("the held copy opens");
        };
        assert_eq!((program, version), (held.join("chrome.exe"), HELD));
        // The clock is on disk now, so the next launch counts from the first one.
        let Launch::Held { .. } =
            plan_with(&browser, &hold, first + 7 * DAY - 1, &supported).unwrap()
        else {
            panic!("still inside the limit");
        };
        let refusal = plan_with(&browser, &hold, first + 7 * DAY, &supported).unwrap_err();
        assert!(matches!(refusal, Refusal::Expired { .. }), "{refusal:?}");
        assert!(refusal.to_string().contains("no longer opens new windows"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn nfr6_a_supported_install_clears_the_clock_and_asks_to_be_kept() {
        let root = scratch("supported");
        let (browser, version) = fake_install(&root);
        let hold = root.join("browsers");
        write_clock(
            &hold,
            "chrome.exe",
            &Clock {
                first_detected: 1,
                installed: "1.2.3.4".into(),
            },
        )
        .unwrap();
        assert_eq!(
            plan_with(&browser, &hold, 9, &[version]).unwrap(),
            Launch::Installed { keep: true }
        );
        assert_eq!(read_clock(&hold, "chrome.exe"), SavedClock::None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nfr6_a_signed_row_makes_a_new_build_supported_without_a_hold() {
        let table = serde_json::json!({"rows": [
            {"build": {"browser": "Microsoft Edge", "module": "msedge.dll", "version": [157, 0, 1, 2]}},
            {"build": {"browser": "Google Chrome", "module": "chrome.dll", "version": [157, 0, 9, 9]}},
            {"build": {"browser": "Brave", "module": "chrome.dll", "version": [157, 1, 1, 1]}},
        ]});
        assert_eq!(table_versions(&table, "msedge.exe"), vec![[157, 0, 1, 2]]);
        assert_eq!(table_versions(&table, "chrome.exe"), vec![[157, 0, 9, 9]]);
        assert_eq!(table_versions(&table, "brave.exe"), vec![[157, 1, 1, 1]]);
        assert!(table_versions(&table, "whale.exe").is_empty());
        let supported = table_versions(&table, "msedge.exe");
        assert_eq!(
            decide(
                "msedge.exe",
                Some([157, 0, 1, 2]),
                &supported,
                &[],
                &SavedClock::None,
                1
            ),
            Decision::Installed
        );
    }
}
