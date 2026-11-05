//! The first setup of Rivet: finding Chromium browsers, choosing one with a Rivet profile, saving
//! the choice and opening the session. The `rivet` command and the launcher share this crate, so
//! the window and the command line cannot disagree.

pub mod config;
pub mod detect;
pub mod launch;
pub mod model;
pub mod open_link;
pub mod pe;
pub mod system;
#[cfg(windows)]
pub use rivet_platform_windows::textservices;
#[cfg(windows)]
pub mod window;
