//! The in-browser Rivet runtime bootstrap.
//!
//! This DLL intentionally has no browser-specific hook yet. Its only current contract is a
//! stable ABI probe after it has been loaded into an explicitly selected browser process.

/// The ABI expected by the Windows launcher.
pub const RUNTIME_ABI_VERSION: u32 = 1;

/// Lets the launcher verify that it loaded a Rivet runtime, rather than an arbitrary DLL.
#[no_mangle]
pub extern "C" fn rivet_runtime_abi_version() -> u32 {
    RUNTIME_ABI_VERSION
}

/// Starts Rivet after the DLL has loaded and the Windows loader lock has been released.
///
/// The first native-frame implementation is initialized from this stable entry point. Calling it
/// again is harmless, which lets a launcher recover after a command retry.
#[cfg(windows)]
#[no_mangle]
pub extern "system" fn rivet_runtime_start() -> u32 {
    use std::sync::atomic::{AtomicBool, Ordering};

    static STARTED: AtomicBool = AtomicBool::new(false);
    STARTED.store(true, Ordering::Release);
    1
}

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: windows_module,
    _reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    // Loader-lock work must stay minimal. Runtime initialization starts only through the control
    // channel after the DLL has loaded.
    1
}

#[cfg(windows)]
#[allow(non_camel_case_types)]
type windows_module = isize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_runtime_exposes_the_expected_abi() {
        assert_eq!(rivet_runtime_abi_version(), RUNTIME_ABI_VERSION);
    }

    #[cfg(windows)]
    #[test]
    fn fr2_runtime_bootstrap_is_idempotent() {
        assert_eq!(rivet_runtime_start(), 1);
        assert_eq!(rivet_runtime_start(), 1);
    }
}
