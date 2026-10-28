//! Browser-launch rules shared by the native runtime and the `rivet` command.
//!
//! This crate deliberately models a browser launch rather than opening a debugging endpoint.
//! The runtime that will directly integrate with Chromium is a later Windows-only component.

use std::fmt;
use std::path::PathBuf;

pub mod session;

/// URL of the local start page shipped alongside Rivet binaries.
pub fn start_page_url(directory: &std::path::Path, space: &str) -> String {
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
        "{prefix}{}?space={}",
        encode(&path, true),
        encode(space, false)
    )
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
    match display {
        DisplayMode::Visible => {
            // An app window has no tab strip or address bar. The runtime adds the Rivet sidebar
            // to this window, so it stays a normal resizable window rather than fullscreen.
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
        }
    }

    #[test]
    fn fr3_start_page_url_encodes_path_and_space() {
        assert_eq!(
            start_page_url(std::path::Path::new("C:/Rivet files"), "Work & Play"),
            "file:///C:/Rivet%20files/start.html?space=Work%20%26%20Play"
        );
    }

    #[test]
    fn fr5_visible_uses_an_app_window_without_a_debugging_port() {
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
