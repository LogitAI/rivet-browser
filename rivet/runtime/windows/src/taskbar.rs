//! Gives a Rivet browser window Rivet's own taskbar identity.
//!
//! Chromium tags its app windows with Chrome's AppUserModelID and icon, so Windows groups a Rivet
//! window with Chrome and shows Chrome's logo. Rivet gives the window its own ID, its relaunch
//! command and its icon, and keeps the browser from replacing the icon afterwards.

use windows::core::Result;
use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
    PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, LookupIconIdFromDirectoryEx, HICON, LR_DEFAULTCOLOR,
};

/// The ID that the installed Start menu shortcut also carries, so a pinned Rivet and a running
/// Rivet window share one taskbar button.
pub const APP_ID: &str = "LogitAI.Rivet.Browser";

const ICON: &[u8] = include_bytes!("../../../installer/windows/setup-ui/assets/logo.ico");

/// Creates the Rivet icon at a pixel size from the embedded icon directory.
pub unsafe fn icon(size: i32) -> Option<HICON> {
    let offset = LookupIconIdFromDirectoryEx(ICON.as_ptr(), true, size, size, LR_DEFAULTCOLOR);
    let image = ICON.get(usize::try_from(offset).ok().filter(|offset| *offset > 0)?..)?;
    CreateIconFromResourceEx(image, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR).ok()
}

/// Applies Rivet's taskbar identity. `runtime_directory` holds the installed launcher and icon.
pub unsafe fn apply(window: isize, runtime_directory: Option<&std::path::Path>) -> Result<()> {
    let store: IPropertyStore = SHGetPropertyStoreForWindow(HWND(window as _))?;
    if let Some(directory) = runtime_directory {
        let launcher = directory.join("rivet-launcher.exe");
        let icon = directory.join("rivet.ico");
        // Relaunch details only make sense for an installed Rivet, where both files exist.
        if launcher.is_file() && icon.is_file() {
            let command = format!("\"{}\"", launcher.display());
            store.SetValue(
                &PKEY_AppUserModel_RelaunchCommand,
                &PROPVARIANT::from(command.as_str()),
            )?;
            store.SetValue(
                &PKEY_AppUserModel_RelaunchIconResource,
                &PROPVARIANT::from(icon.display().to_string().as_str()),
            )?;
            store.SetValue(
                &PKEY_AppUserModel_RelaunchDisplayNameResource,
                &PROPVARIANT::from("Rivet Browser"),
            )?;
        }
    }
    store.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(APP_ID))?;
    store.Commit()
}

/// Removes the identity before the window is destroyed, as the shell requires.
pub unsafe fn clear(window: isize) {
    if let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(HWND(window as _)) {
        for key in [
            &PKEY_AppUserModel_ID,
            &PKEY_AppUserModel_RelaunchCommand,
            &PKEY_AppUserModel_RelaunchIconResource,
            &PKEY_AppUserModel_RelaunchDisplayNameResource,
        ] {
            let _ = store.SetValue(key, &PROPVARIANT::default());
        }
        let _ = store.Commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_rivet_icon_loads_at_taskbar_sizes() {
        for size in [16, 24, 32, 48, 64] {
            assert!(unsafe { icon(size) }.is_some(), "no icon at {size}px");
        }
    }
}
