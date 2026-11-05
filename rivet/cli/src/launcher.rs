#![cfg_attr(windows, windows_subsystem = "windows")]

//! The Rivet launcher for people who start Rivet without a terminal.
//!
//! The first run shows the setup window. Every later run opens the saved browser and Rivet profile
//! at once, through `rivet session start`, so the graphical and CLI entry points share one path.
//! When Windows runs it for a link from another app (`--open-link <address>`), the link goes to a
//! Rivet window, which opens it by its link rules.

#[cfg(windows)]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match rivet_cli::open_link::from_arguments(&arguments) {
        None => rivet_cli::window::run_launcher(),
        Some(Err(error)) => Err(error),
        Some(Ok(url)) => rivet_cli::window::run_launcher_with_link(&url),
    };
    if let Err(error) = result {
        rivet_cli::window::show_error(&error);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rivet-launcher is only available on Windows");
}
