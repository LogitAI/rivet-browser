//! Persistent metadata for the browser windows that Rivet started.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The minimal window identity Rivet needs to list and close one owned app window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub browser: PathBuf,
    pub window_handle: isize,
    pub process_id: u32,
    pub url: String,
}

/// A file-backed collection of Rivet-owned browser windows.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionRegistry {
    sessions: BTreeMap<String, SessionRecord>,
}

impl SessionRegistry {
    /// Reads a registry. A missing file is an empty registry.
    pub fn load(path: &Path) -> Result<Self, SessionRegistryError> {
        match fs::read(path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map_err(|error| SessionRegistryError::InvalidState(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SessionRegistryError::Read(error.to_string())),
        }
    }

    /// Writes the complete registry after replacing a temporary file in the same directory.
    pub fn save(&self, path: &Path) -> Result<(), SessionRegistryError> {
        let parent = path
            .parent()
            .ok_or_else(|| SessionRegistryError::Write("registry path has no parent".into()))?;
        fs::create_dir_all(parent)
            .map_err(|error| SessionRegistryError::Write(error.to_string()))?;
        let temporary = path.with_extension("new");
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| SessionRegistryError::Write(error.to_string()))?;
        fs::write(&temporary, contents)
            .map_err(|error| SessionRegistryError::Write(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| SessionRegistryError::Write(error.to_string()))
    }

    /// Adds a session. The caller supplies an ID that is unique in its own namespace.
    pub fn insert(&mut self, record: SessionRecord) -> Result<(), SessionRegistryError> {
        if self.sessions.contains_key(&record.id) {
            return Err(SessionRegistryError::DuplicateId(record.id));
        }
        self.sessions.insert(record.id.clone(), record);
        Ok(())
    }

    /// Returns stored sessions in deterministic ID order.
    pub fn list(&self) -> impl Iterator<Item = &SessionRecord> {
        self.sessions.values()
    }

    /// Removes and returns a stored session.
    pub fn remove(&mut self, id: &str) -> Option<SessionRecord> {
        self.sessions.remove(id)
    }
}

/// A registry I/O or consistency failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRegistryError {
    Read(String),
    Write(String),
    InvalidState(String),
    DuplicateId(String),
}

impl std::fmt::Display for SessionRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "could not read the Rivet session registry: {error}"),
            Self::Write(error) => write!(f, "could not write the Rivet session registry: {error}"),
            Self::InvalidState(error) => {
                write!(f, "the Rivet session registry is invalid: {error}")
            }
            Self::DuplicateId(id) => write!(f, "the Rivet session ID already exists: {id}"),
        }
    }
}

impl std::error::Error for SessionRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr1_registry_lists_and_removes_only_the_requested_session() {
        let mut registry = SessionRegistry::default();
        registry
            .insert(SessionRecord {
                id: "one".into(),
                browser: "C:/Chrome/chrome.exe".into(),
                window_handle: 1,
                process_id: 10,
                url: "https://one.test/".into(),
            })
            .unwrap();
        registry
            .insert(SessionRecord {
                id: "two".into(),
                browser: "C:/Chrome/chrome.exe".into(),
                window_handle: 2,
                process_id: 10,
                url: "https://two.test/".into(),
            })
            .unwrap();
        assert_eq!(registry.remove("one").unwrap().window_handle, 1);
        assert_eq!(
            registry
                .list()
                .map(|session| &session.id)
                .collect::<Vec<_>>(),
            ["two"]
        );
    }
}
