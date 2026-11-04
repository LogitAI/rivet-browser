//! Who keeps the passwords and passkeys of a Rivet profile.
//!
//! Rivet stores no password and no passkey. A person chooses a provider, and Rivet only tells the
//! browser which of its own Chromium switches and preferences to use for the Rivet profile:
//!
//! - `Browser`: the browser's own password manager offers to save and fills, and passkeys go where
//!   the browser sends them.
//! - `System`: the browser's password manager is turned off for the profile, and passkeys go to the
//!   Windows WebAuthn provider (Windows Hello and the passkey managers registered with Windows),
//!   because the browser's own passkey authenticator is switched off.
//!
//! Nothing here opens, reads or copies a password store. Only the `Preferences` file of a Rivet
//! profile is changed, and only the keys below.

use std::path::Path;

use serde_json::{Map, Value};

/// The provider a person chose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CredentialProvider {
    /// The browser's own password manager.
    #[default]
    Browser,
    /// The operating system's passkey provider.
    System,
}

/// Chromium's feature that adds the browser's own cloud passkey authenticator next to the Windows
/// one. With it off, Chromium hands passkey requests to the Windows WebAuthn API.
pub const BROWSER_PASSKEY_FEATURE: &str = "WebAuthenticationEnclaveAuthenticator";

/// Chromium's profile preferences behind "Offer to save passwords" and "Sign in automatically".
pub const SAVE_PASSWORDS_PREF: &str = "credentials_enable_service";
pub const AUTO_SIGN_IN_PREF: &str = "credentials_enable_autosignin";

impl CredentialProvider {
    pub const ALL: [CredentialProvider; 2] =
        [CredentialProvider::Browser, CredentialProvider::System];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "browser" => Some(Self::Browser),
            "system" | "os" | "windows" => Some(Self::System),
            _ => None,
        }
    }

    /// The words a person reads for the choice.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Browser => "Browser password manager",
            Self::System => "Windows passkeys",
        }
    }

    /// The other provider.
    pub const fn other(self) -> Self {
        match self {
            Self::Browser => Self::System,
            Self::System => Self::Browser,
        }
    }

    /// The Chromium switches the browser is started with for this provider.
    pub fn launch_arguments(self) -> Vec<String> {
        match self {
            Self::Browser => Vec::new(),
            Self::System => vec![format!("--disable-features={BROWSER_PASSKEY_FEATURE}")],
        }
    }

    /// Sets this provider's keys in a profile's `Preferences` and leaves every other key as it was.
    pub fn apply_to_preferences(self, preferences: &mut Value) {
        if !preferences.is_object() {
            *preferences = Value::Object(Map::new());
        }
        let browser = self == Self::Browser;
        let object = preferences.as_object_mut().expect("made an object above");
        object.insert(SAVE_PASSWORDS_PREF.to_owned(), Value::Bool(browser));
        object.insert(AUTO_SIGN_IN_PREF.to_owned(), Value::Bool(browser));
    }
}

/// The provider saved in a setup file's JSON, the browser's when none was saved.
pub fn from_setup(value: &Value) -> CredentialProvider {
    value["credentials"]
        .as_str()
        .and_then(CredentialProvider::parse)
        .unwrap_or_default()
}

/// Saves `provider` in the setup file at `path` and keeps every other key. The browser reads it at
/// the next start of a Rivet profile. A setup that is not JSON is left alone.
pub fn save_to_setup(path: &Path, provider: CredentialProvider) -> Result<(), String> {
    let mut setup = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(&text)
            .map_err(|_| format!("{} is not a valid setup", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    let object = setup
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a valid setup", path.display()))?;
    object.insert(
        "credentials".to_owned(),
        Value::String(provider.as_str().to_owned()),
    );
    let folder = path.parent().ok_or("the setup file has no folder")?;
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("could not make {}: {error}", folder.display()))?;
    let temporary = folder.join("setup.json.tmp");
    std::fs::write(&temporary, setup.to_string())
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("could not save {}: {error}", path.display()))
}

/// The provider saved in the setup file at `path`.
pub fn load_from_setup(path: &Path) -> CredentialProvider {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map_or_else(Default::default, |value| from_setup(&value))
}

/// Writes the provider's preferences into the `Default` profile of a Rivet user data folder before
/// the browser starts. A file that is not JSON is left alone, because it is the browser's.
pub fn prepare_profile(user_data_dir: &Path, provider: CredentialProvider) -> Result<(), String> {
    let folder = user_data_dir.join("Default");
    let path = folder.join("Preferences");
    let mut preferences = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(_) => {
                return Err(format!(
                    "{} is not JSON; the provider was not set",
                    path.display()
                ))
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    provider.apply_to_preferences(&mut preferences);
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("could not make {}: {error}", folder.display()))?;
    let temporary = folder.join("Preferences.rivet.tmp");
    std::fs::write(&temporary, preferences.to_string())
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, &path)
        .map_err(|error| format!("could not save {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr8_both_providers_can_be_chosen_by_name() {
        for provider in CredentialProvider::ALL {
            assert_eq!(CredentialProvider::parse(provider.as_str()), Some(provider));
        }
        assert_eq!(
            CredentialProvider::parse("Windows"),
            Some(CredentialProvider::System)
        );
        assert_eq!(CredentialProvider::parse("keychain"), None);
        assert_eq!(CredentialProvider::default(), CredentialProvider::Browser);
        assert_eq!(
            CredentialProvider::Browser.other(),
            CredentialProvider::System
        );
    }

    #[test]
    fn nfr8_system_provider_hands_passkeys_to_windows_by_a_chromium_switch() {
        assert!(CredentialProvider::Browser.launch_arguments().is_empty());
        assert_eq!(
            CredentialProvider::System.launch_arguments(),
            vec!["--disable-features=WebAuthenticationEnclaveAuthenticator".to_owned()]
        );
    }

    #[test]
    fn nfr8_provider_sets_only_the_password_manager_preferences() {
        let mut preferences =
            serde_json::json!({"homepage": "about:blank", "credentials_enable_service": true});
        CredentialProvider::System.apply_to_preferences(&mut preferences);
        assert_eq!(preferences["credentials_enable_service"], false);
        assert_eq!(preferences["credentials_enable_autosignin"], false);
        assert_eq!(preferences["homepage"], "about:blank");
        CredentialProvider::Browser.apply_to_preferences(&mut preferences);
        assert_eq!(preferences["credentials_enable_service"], true);
        let mut odd = Value::Null;
        CredentialProvider::Browser.apply_to_preferences(&mut odd);
        assert_eq!(odd["credentials_enable_service"], true);
    }

    #[test]
    fn nfr8_saved_provider_is_read_from_the_setup() {
        assert_eq!(
            from_setup(&serde_json::json!({"credentials": "system"})),
            CredentialProvider::System
        );
        assert_eq!(
            from_setup(&serde_json::json!({})),
            CredentialProvider::Browser
        );
        assert_eq!(
            from_setup(&serde_json::json!({"credentials": 3})),
            CredentialProvider::Browser
        );
    }

    #[test]
    fn nfr8_rivet_settings_change_the_saved_provider_and_keep_the_setup() {
        let root = crate::paths::TestDir::new("rivet-credentials-setup");
        let path = root.join("setup.json");
        assert_eq!(load_from_setup(&path), CredentialProvider::Browser);
        std::fs::write(
            &path,
            r#"{"version":1,"browser":"C:\\b.exe","profile":"Work"}"#,
        )
        .unwrap();
        save_to_setup(&path, CredentialProvider::System).unwrap();
        assert_eq!(load_from_setup(&path), CredentialProvider::System);
        let setup: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(setup["profile"], "Work");
        std::fs::write(&path, "[1]").unwrap();
        assert!(save_to_setup(&path, CredentialProvider::Browser).is_err());
    }

    #[test]
    fn nfr8_profile_preferences_are_written_and_other_keys_kept() {
        let root = crate::paths::TestDir::new("rivet-credentials");
        let _ = std::fs::remove_dir_all(&root);
        prepare_profile(&root, CredentialProvider::System).unwrap();
        let path = root.join("Default").join("Preferences");
        let first: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(first["credentials_enable_service"], false);
        std::fs::write(&path, r#"{"kept":1,"credentials_enable_service":false}"#).unwrap();
        prepare_profile(&root, CredentialProvider::Browser).unwrap();
        let second: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(second["kept"], 1);
        assert_eq!(second["credentials_enable_service"], true);
        std::fs::write(&path, "not json").unwrap();
        assert!(prepare_profile(&root, CredentialProvider::System).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        let _ = std::fs::remove_dir_all(&root);
    }
}
