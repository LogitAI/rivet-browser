//! The saved setup: which browser Rivet opens and in which Rivet profile.
//!
//! The window and `rivet setup` both end here, so they cannot disagree. Rivet profiles live below
//! `%LOCALAPPDATA%\Rivet\profiles\<browser>\<name>`. Nothing in this module lists, opens, reads or
//! copies a profile of the browser itself.

use crate::detect::{Browser, Files};
use std::path::{Path, PathBuf};

/// The profile name offered when a person does not choose one.
pub const DEFAULT_PROFILE: &str = "Default";
const LONGEST_PROFILE: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub browser: PathBuf,
    pub profile: String,
    /// Who keeps passwords and passkeys in the profile.
    pub credentials: rivet_core::CredentialProvider,
}

/// The setup file below `root`, the local application data folder.
pub fn setup_file(root: &Path) -> PathBuf {
    rivet_core::paths::Layout::under(root).setup_file()
}

/// The local application data folder, which tests replace to keep real settings untouched.
pub fn data_root() -> Result<PathBuf, String> {
    rivet_core::paths::data_home()
        .ok_or_else(|| "could not find the local application data folder.".to_owned())
}

pub fn to_json(saved: &Saved) -> String {
    serde_json::json!({
        "version": 1,
        "browser": saved.browser.to_string_lossy(),
        "profile": saved.profile,
        "credentials": saved.credentials.as_str(),
    })
    .to_string()
}

pub fn from_json(text: &str) -> Result<Saved, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let browser = value["browser"]
        .as_str()
        .filter(|path| !path.is_empty())
        .ok_or("the browser is missing")?;
    let profile = value["profile"]
        .as_str()
        .map_or_else(|| DEFAULT_PROFILE.to_owned(), clean_profile);
    let credentials = rivet_core::credentials::from_setup(&value);
    Ok(Saved {
        browser: PathBuf::from(browser),
        profile,
        credentials,
    })
}

/// The saved setup, `None` when nothing was saved, and an error when the file cannot be read.
pub fn load(root: &Path) -> Result<Option<Saved>, String> {
    let path = setup_file(root);
    match std::fs::read_to_string(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
        Ok(text) => from_json(&text)
            .map(Some)
            .map_err(|error| format!("{} is not a valid setup: {error}", path.display())),
    }
}

/// Saves the setup. The file gets its name only when it is complete.
pub fn save(root: &Path, saved: &Saved) -> Result<(), String> {
    let path = setup_file(root);
    let folder = path.parent().ok_or("the setup file has no folder")?;
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("could not make {}: {error}", folder.display()))?;
    let temporary = folder.join("setup.json.tmp");
    std::fs::write(&temporary, to_json(saved))
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, &path)
        .map_err(|error| format!("could not save {}: {error}", path.display()))
}

/// A profile name as a folder name: letters, digits, space, dash and underscore stay, anything
/// else becomes an underscore, and an empty name is the default one.
pub fn clean_profile(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .take(LONGEST_PROFILE)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        return DEFAULT_PROFILE.to_owned();
    }
    const RESERVED: [&str; 8] = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "LPT1", "LPT2"];
    if RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return format!("{name}_");
    }
    name.to_owned()
}

/// The name of a browser's folder below Rivet's profiles: its file name without `.exe`. Edge and
/// Samsung Internet keep the short names the runtime already uses for their spaces.
pub fn profile_key(browser: &Path) -> String {
    let stem = browser.file_stem().map_or_else(String::new, |stem| {
        stem.to_string_lossy().to_ascii_lowercase()
    });
    match stem.as_str() {
        "msedge" => "edge".to_owned(),
        "samsunginternet" => "samsung".to_owned(),
        "" => "browser".to_owned(),
        _ => stem,
    }
}

/// The folder of all Rivet profiles of one browser, below `root`.
pub fn profiles_folder(root: &Path, browser: &Path) -> PathBuf {
    rivet_core::paths::Layout::under(root)
        .profiles()
        .join(profile_key(browser))
}

/// The user data folder Rivet gives the browser for a profile.
pub fn profile_dir(root: &Path, browser: &Path, profile: &str) -> PathBuf {
    profiles_folder(root, browser).join(clean_profile(profile))
}

/// The Rivet profiles that already exist for a browser, by name.
pub fn existing_profiles(root: &Path, browser: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(profiles_folder(root, browser)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// What the choices in the window or on the command line come to when they are finished.
pub fn finish(browser: &Browser, profile: &str) -> Result<Saved, String> {
    if !browser.status.can_open() {
        return Err(browser.status.sentence(&browser.name));
    }
    Ok(Saved {
        browser: browser.path.clone(),
        profile: clean_profile(profile),
        credentials: Default::default(),
    })
}

/// What a run does: open the saved setup, or ask first and say why.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    Open(Saved),
    Setup { reason: Option<String> },
}

pub fn start(root: &Path, files: &dyn Files) -> Start {
    match load(root) {
        Ok(None) => Start::Setup { reason: None },
        Err(error) => Start::Setup {
            reason: Some(error),
        },
        Ok(Some(saved)) if !files.is_file(&saved.browser) => Start::Setup {
            reason: Some(format!(
                "The browser you chose is not at {} any more. Choose it again.",
                saved.browser.display()
            )),
        },
        Ok(Some(saved)) => Start::Open(saved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::fake::Memory;
    use crate::detect::Status;

    fn scratch(name: &str) -> rivet_core::paths::TestDir {
        rivet_core::paths::TestDir::new(&format!("rivet-setup-{name}"))
    }

    fn browser(status: Status) -> Browser {
        Browser {
            path: PathBuf::from(r"C:\B\chrome.exe"),
            name: "Chrome".into(),
            version: "1.2.3.4".into(),
            proof: PathBuf::new(),
            status,
        }
    }

    #[test]
    fn nfr8_the_chosen_provider_is_saved_with_the_setup() {
        let saved = Saved {
            browser: PathBuf::from(r"C:\B\chrome.exe"),
            profile: "Work".into(),
            credentials: rivet_core::CredentialProvider::System,
        };
        assert_eq!(from_json(&to_json(&saved)), Ok(saved));
        let old =
            from_json(r#"{"version":1,"browser":"C:\\B\\chrome.exe","profile":"Work"}"#).unwrap();
        assert_eq!(old.credentials, rivet_core::CredentialProvider::Browser);
    }

    #[test]
    fn fr1_a_saved_setup_comes_back_as_it_was_saved() {
        let root = scratch("roundtrip");
        assert_eq!(load(&root), Ok(None));
        let saved = Saved {
            browser: PathBuf::from(r"C:\B\chrome.exe"),
            profile: "한국어 작업".into(),
            credentials: Default::default(),
        };
        save(&root, &saved).unwrap();
        assert_eq!(load(&root), Ok(Some(saved)));
        assert!(!setup_file(&root).with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fr1_an_unreadable_setup_file_is_reported_and_not_replaced_by_a_guess() {
        let root = scratch("broken");
        std::fs::create_dir_all(setup_file(&root).parent().unwrap()).unwrap();
        std::fs::write(setup_file(&root), "{ nope").unwrap();
        assert!(load(&root).unwrap_err().contains("setup.json"));
        let files = Memory::default();
        assert!(matches!(
            start(&root, &files),
            Start::Setup { reason: Some(_) }
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fr1_profile_names_become_safe_folder_names() {
        assert_eq!(clean_profile(""), "Default");
        assert_eq!(clean_profile("   "), "Default");
        assert_eq!(clean_profile(r"..\..\Windows"), "______Windows");
        assert_eq!(clean_profile("Work 2"), "Work 2");
        assert_eq!(clean_profile("작업"), "작업");
        assert_eq!(clean_profile("con"), "con_");
        assert_eq!(clean_profile(&"a".repeat(100)).len(), 40);
    }

    #[test]
    // The paths here are Windows paths, which only Windows splits into folders.
    #[cfg(windows)]
    fn fr1_profiles_live_below_rivets_own_folder_by_browser() {
        let root = Path::new(r"C:\Data");
        let chrome = Path::new(r"C:\B\Chrome.exe");
        assert_eq!(
            profile_dir(root, chrome, "Work"),
            Path::new(r"C:\Data\Rivet\profiles\chrome\Work")
        );
        assert_eq!(
            profile_dir(root, Path::new(r"C:\E\msedge.exe"), "Lab"),
            Path::new(r"C:\Data\Rivet\profiles\edge\Lab")
        );
        let dir = profile_dir(root, chrome, r"..\..\Google\Chrome\User Data");
        assert!(dir.starts_with(r"C:\Data\Rivet\profiles\chrome"));
        assert_eq!(
            dir.components().count(),
            Path::new(r"C:\Data\Rivet\profiles\chrome\x")
                .components()
                .count()
        );
    }

    #[test]
    fn fr1_existing_rivet_profiles_are_offered_and_nothing_else() {
        let root = scratch("existing");
        let chrome = Path::new(r"C:\B\chrome.exe");
        assert!(existing_profiles(&root, chrome).is_empty());
        for name in ["Work", "Lab"] {
            std::fs::create_dir_all(profiles_folder(&root, chrome).join(name)).unwrap();
        }
        std::fs::write(profiles_folder(&root, chrome).join("notes.txt"), "x").unwrap();
        std::fs::create_dir_all(
            profiles_folder(&root, Path::new(r"C:\E\msedge.exe")).join("Other"),
        )
        .unwrap();
        assert_eq!(existing_profiles(&root, chrome), ["Lab", "Work"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fr1_finishing_needs_a_browser_rivet_can_open() {
        let ready = finish(&browser(Status::Ready), "").unwrap();
        assert_eq!(ready.profile, "Default");
        let refused = finish(
            &browser(Status::NotYet("Rivet has not verified 9.0.0.0 yet.".into())),
            "Work",
        )
        .unwrap_err();
        assert!(refused.contains("9.0.0.0"));
    }

    #[test]
    fn fr1_later_runs_open_the_saved_browser_without_a_window() {
        let root = scratch("later");
        let mut files = Memory::default();
        files.data(r"C:\B\chrome.exe");
        assert_eq!(start(&root, &files), Start::Setup { reason: None });
        let saved = Saved {
            browser: PathBuf::from(r"C:\B\chrome.exe"),
            profile: "Work".into(),
            credentials: Default::default(),
        };
        save(&root, &saved).unwrap();
        assert_eq!(start(&root, &files), Start::Open(saved.clone()));
        let nothing = Memory::default();
        let Start::Setup {
            reason: Some(reason),
        } = start(&root, &nothing)
        else {
            panic!("a missing browser must ask again")
        };
        assert!(reason.contains(r"C:\B\chrome.exe"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
