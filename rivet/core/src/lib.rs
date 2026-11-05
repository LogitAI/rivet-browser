//! Browser-launch rules shared by the native runtime and the `rivet` command.
//!
//! This crate deliberately models a browser launch rather than opening a debugging endpoint.
//! The runtime that will directly integrate with Chromium is a later Windows-only component.

use std::fmt;
use std::path::PathBuf;

pub mod credentials;
pub mod link_rules;
pub mod paths;
pub mod session;

pub use credentials::CredentialProvider;

/// The tint of the first space a window starts with.
pub const WORK_TINT: u32 = 0xd08a52;

/// The frame and accent colours designed for a tint. A tint without a design gets a frame and an
/// accent made the same way: a grey that leans a little to the tint, and the tint darkened.
const DESIGNED: [(u32, u32, u32); 3] = [
    (WORK_TINT, 0xece8e2, 0x9a5424),
    (0x5b8bab, 0xe6e9ec, 0x2f5f80),
    (0xc0577f, 0xece9e6, 0x8a2f56),
];

fn channels(colour: u32, each: impl Fn(u32) -> u32) -> u32 {
    each((colour >> 16) & 0xff) << 16 | each((colour >> 8) & 0xff) << 8 | each(colour & 0xff)
}

/// The colour of the window's ground in a space of this tint.
pub fn space_frame(tint: u32) -> u32 {
    DESIGNED.iter().find(|row| row.0 == tint).map_or_else(
        || channels(tint, |channel| (0xe9 * 19 + channel + 10) / 20),
        |row| row.1,
    )
}

/// The strong colour of marks and active controls in a space of this tint.
pub fn space_accent(tint: u32) -> u32 {
    DESIGNED.iter().find(|row| row.0 == tint).map_or_else(
        || channels(tint, |channel| channel.saturating_sub(0x30)),
        |row| row.2,
    )
}

/// The short name of the browser a program path is, or `None` for a program Rivet does not know.
pub fn browser_label(program: &std::path::Path) -> Option<&'static str> {
    let name = program.file_name()?.to_str()?.to_ascii_lowercase();
    Some(match name.as_str() {
        "chrome.exe" => "Chrome",
        "msedge.exe" => "Edge",
        "whale.exe" => "Whale",
        "brave.exe" => "Brave",
        "samsunginternet.exe" => "Samsung",
        _ => return None,
    })
}

/// URL of the local start page shipped alongside Rivet binaries. The page is a file, so its
/// space's colours travel in the address.
pub fn start_page_url(
    directory: &std::path::Path,
    space: &str,
    browser: &str,
    profile: &str,
    tint: u32,
) -> String {
    fn encode(value: &str, path: bool) -> String {
        let mut result = String::new();
        for b in value.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) || (path && b"/:".contains(&b)) {
                result.push(b as char);
            } else {
                use std::fmt::Write;
                write!(result, "%{b:02X}").unwrap();
            }
        }
        result
    }
    let path = directory
        .join("start.html")
        .to_string_lossy()
        .replace('\\', "/");
    let prefix = if path.starts_with("//") {
        "file:"
    } else if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    format!(
        "{prefix}{}?space={}&browser={}&profile={}&tint={tint:06x}&accent={:06x}",
        encode(&path, true),
        encode(space, false),
        encode(browser, false),
        encode(profile, false),
        space_accent(tint)
    )
}

/// Whether Rivet may enter a browser using this profile folder.
///
/// A person's own browser never qualifies. It runs on the browser's default folder, which every
/// Chromium browser keeps under a folder named `User Data`, or names no folder at all.
pub fn profile_allows_rivet(user_data_dir: Option<&std::path::Path>) -> bool {
    user_data_dir.is_some_and(|folder| {
        !folder.as_os_str().is_empty()
            && !folder
                .components()
                .any(|part| part.as_os_str().eq_ignore_ascii_case("User Data"))
    })
}

/// Whether Rivet may enter a process started with these arguments.
pub fn process_allows_rivet<I, S>(arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut folder = None;
    for argument in arguments {
        let argument = argument.as_ref().to_string_lossy().into_owned();
        if let Some(value) = argument.strip_prefix("--user-data-dir=") {
            folder = Some(PathBuf::from(value));
        }
    }
    profile_allows_rivet(folder.as_deref())
}

/// Whether a browser task creates a visible Rivet surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Create an app window for a person or visual UI automation.
    Visible,
    /// Run a Chromium page without a visible browser window or frame stream.
    Hidden,
    /// Let a future policy layer choose. The first CLI resolves this to `Visible`.
    Auto,
}

impl DisplayMode {
    /// Parses the stable CLI spelling.
    pub fn parse(value: &str) -> Result<Self, LaunchError> {
        match value {
            "visible" => Ok(Self::Visible),
            "hidden" => Ok(Self::Hidden),
            "auto" => Ok(Self::Auto),
            _ => Err(LaunchError::InvalidDisplayMode(value.to_owned())),
        }
    }

    /// Returns the stable CLI spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::Auto => "auto",
        }
    }
}

/// Inputs selected by the caller for one browser task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub browser: PathBuf,
    pub url: String,
    pub user_data_dir: Option<PathBuf>,
    pub display: DisplayMode,
    /// Who keeps the profile's passwords and passkeys.
    pub credentials: CredentialProvider,
}

/// The exact process invocation Rivet gives to a Chromium browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub display: DisplayMode,
}

/// A rejected caller input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    InvalidDisplayMode(String),
    MissingBrowser,
    InvalidUrl(String),
    UnsafeArgument(String),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDisplayMode(value) => {
                write!(f, "display must be visible, hidden or auto, not {value:?}")
            }
            Self::MissingBrowser => {
                write!(f, "a browser executable is required; pass --browser <path>")
            }
            Self::InvalidUrl(value) => {
                write!(
                    f,
                    "URL must use https://, http://, file:// or about:, not {value:?}"
                )
            }
            Self::UnsafeArgument(value) => {
                write!(f, "value contains a control character: {value:?}")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Builds a Chromium invocation without opening a debugging port.
pub fn build_launch_plan(request: &LaunchRequest) -> Result<LaunchPlan, LaunchError> {
    if request.browser.as_os_str().is_empty() {
        return Err(LaunchError::MissingBrowser);
    }
    reject_control_characters(&request.url)?;
    if !is_supported_url(&request.url) {
        return Err(LaunchError::InvalidUrl(request.url.clone()));
    }

    let display = match request.display {
        // `auto` is intentionally deterministic until the visual-evidence policy exists.
        DisplayMode::Auto => DisplayMode::Visible,
        mode => mode,
    };
    let mut arguments = vec![
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
    ];
    if let Some(dir) = &request.user_data_dir {
        let value = dir.to_string_lossy().into_owned();
        reject_control_characters(&value)?;
        arguments.push(format!("--user-data-dir={value}"));
    }
    arguments.extend(request.credentials.launch_arguments());
    match display {
        DisplayMode::Visible => {
            // An app window has no maker tab strip or toolbar.
            arguments.push(format!("--app={}", request.url));
        }
        DisplayMode::Hidden => {
            arguments.push("--headless=new".to_owned());
            arguments.push(request.url.clone());
        }
        DisplayMode::Auto => unreachable!("auto is resolved above"),
    }
    Ok(LaunchPlan {
        program: request.browser.clone(),
        arguments,
        display,
    })
}

fn is_supported_url(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("file://")
        || value.starts_with("about:")
}

fn reject_control_characters(value: &str) -> Result<(), LaunchError> {
    if value.chars().any(char::is_control) {
        Err(LaunchError::UnsafeArgument(value.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(display: DisplayMode) -> LaunchRequest {
        LaunchRequest {
            browser: PathBuf::from("C:/Browser/chrome.exe"),
            url: "https://example.test/".into(),
            user_data_dir: Some(PathBuf::from("C:/Profiles/rivet")),
            display,
            credentials: CredentialProvider::Browser,
        }
    }

    #[test]
    fn nfr8_launch_carries_the_chosen_provider_switches() {
        let mut input = request(DisplayMode::Visible);
        input.credentials = CredentialProvider::System;
        let plan = build_launch_plan(&input).unwrap();
        assert!(plan
            .arguments
            .contains(&"--disable-features=WebAuthenticationEnclaveAuthenticator".into()));
        let plan = build_launch_plan(&request(DisplayMode::Visible)).unwrap();
        assert!(!plan
            .arguments
            .iter()
            .any(|argument| argument.starts_with("--disable-features")));
    }

    #[cfg(windows)]
    #[test]
    fn fr1_rivet_never_enters_the_persons_own_profile() {
        let allowed = |path: &str| profile_allows_rivet(Some(std::path::Path::new(path)));
        assert!(!profile_allows_rivet(None));
        assert!(!allowed(""));
        assert!(!allowed(
            r"C:\Users\me\AppData\Local\Naver\Naver Whale\User Data"
        ));
        assert!(!allowed(
            r"C:\Users\me\AppData\Local\Google\Chrome\user data\Default"
        ));
        assert!(allowed(
            r"C:\Users\me\AppData\Local\Rivet\profiles\whale\Personal"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn fr1_rivet_reads_the_profile_from_the_process_arguments() {
        assert!(!process_allows_rivet(["whale.exe"]));
        assert!(!process_allows_rivet(["whale.exe", "--type=renderer"]));
        assert!(!process_allows_rivet([
            "whale.exe",
            r"--user-data-dir=C:\Local\Naver\Naver Whale\User Data",
        ]));
        assert!(process_allows_rivet([
            "whale.exe",
            r"--user-data-dir=C:\Local\Rivet\profiles\whale\My Space",
            "about:blank",
        ]));
    }

    #[test]
    fn fr3_start_page_url_encodes_path_and_space() {
        assert_eq!(
            start_page_url(std::path::Path::new("C:/Rivet files"), "Work & Play", "Edge", "Research", 0x5b8bab),
            "file:///C:/Rivet%20files/start.html?space=Work%20%26%20Play&browser=Edge&profile=Research&tint=5b8bab&accent=2f5f80"
        );
    }

    #[test]
    fn fr3_space_colours_match_the_design_and_other_tints_get_their_own() {
        for (tint, frame, accent) in [
            (0xd08a52, 0xece8e2, 0x9a5424),
            (0x5b8bab, 0xe6e9ec, 0x2f5f80),
            (0xc0577f, 0xece9e6, 0x8a2f56),
        ] {
            assert_eq!((space_frame(tint), space_accent(tint)), (frame, accent));
        }
        assert_eq!(space_accent(0x6a9f58), 0x3a6f28);
        assert_ne!(space_frame(0x6a9f58), space_frame(0x8a6fc4));
    }

    #[cfg(windows)]
    #[test]
    fn fr3_browser_label_names_all_five_browsers() {
        for (program, label) in [
            (
                r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                Some("Chrome"),
            ),
            (r"C:\Edge\MSEDGE.EXE", Some("Edge")),
            (r"C:\Naver\whale.exe", Some("Whale")),
            (r"C:\Brave\brave.exe", Some("Brave")),
            (r"C:\Samsung\samsunginternet.exe", Some("Samsung")),
            (r"C:\Other\notepad.exe", None),
        ] {
            assert_eq!(browser_label(std::path::Path::new(program)), label);
        }
    }

    #[test]
    fn fr12_visible_opens_an_app_window_without_a_debugging_port() {
        let plan = build_launch_plan(&request(DisplayMode::Visible)).unwrap();
        assert!(plan
            .arguments
            .contains(&"--app=https://example.test/".into()));
        assert!(!plan
            .arguments
            .iter()
            .any(|argument| argument.starts_with("--remote-debugging")));
    }

    #[test]
    fn fr2_browser_frame_does_not_require_fullscreen() {
        let plan = build_launch_plan(&request(DisplayMode::Visible)).unwrap();
        assert!(!plan.arguments.contains(&"--start-fullscreen".into()));
        assert!(!plan.arguments.contains(&"--kiosk".into()));
    }

    #[test]
    fn fr5_hidden_uses_headless_without_a_visible_app_window() {
        let plan = build_launch_plan(&request(DisplayMode::Hidden)).unwrap();
        assert!(plan.arguments.contains(&"--headless=new".into()));
        assert!(!plan
            .arguments
            .iter()
            .any(|argument| argument.starts_with("--app=")));
        assert!(plan.arguments.contains(&"https://example.test/".into()));
    }

    #[test]
    fn fr1_rejects_an_unsupported_navigation_target() {
        let mut input = request(DisplayMode::Visible);
        input.url = "javascript:alert(1)".into();
        assert_eq!(
            build_launch_plan(&input),
            Err(LaunchError::InvalidUrl(input.url))
        );
    }

    #[test]
    fn fr5_auto_is_visible_until_a_policy_engine_can_decide() {
        let plan = build_launch_plan(&request(DisplayMode::Auto)).unwrap();
        assert_eq!(plan.display, DisplayMode::Visible);
    }
}
