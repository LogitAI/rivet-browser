#![cfg_attr(windows, windows_subsystem = "windows")]

//! The Rivet launcher for people who start Rivet without a terminal.
//!
//! The first run shows the setup window. Every later run opens the saved browser and Rivet profile
//! at once, through `rivet session start`, so the graphical and CLI entry points share one path.

#[cfg(windows)]
fn main() {
    if let Err(error) = rivet_cli::window::run_launcher() {
        rivet_cli::window::show_error(&error);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rivet-launcher is only available on Windows");
}
