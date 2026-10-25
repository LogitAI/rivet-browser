# Rivet Windows installer

This directory adapts DSA's Windows installer for Rivet. `rivet.wxs` is the package (MSI), and
`bundle.wxs` wraps it in `setup.exe` with the setup screens in `setup-ui/` (WPF on .NET
Framework 4.7.2, which every supported Windows 10 and 11 has). The screens come from DSA's
installer with the Rivet name, one program folder and Rivet's own text.

## Build

Run `build.ps1` from a Windows PowerShell session with Rust, the .NET SDK and WiX 5 available.
The script writes `rivet-<version>.msi` and `setup-<version>.exe` below `rivet/target/installer/`.
Ship `setup.exe`; the MSI is inside it.

`setup-ui/assets/logo.png` and `logo.ico` are still DSA's logo until Rivet has its own.

## First prototype run

After installing `setup.exe`, open a new terminal so the system PATH change is visible. Start an
installed Chromium executable explicitly:

```powershell
rivet session start `
  --browser "C:\Program Files\Google\Chrome\Application\chrome.exe" `
  --url "https://example.com/"
```

The installed CLI finds `rivet_runtime_windows.dll` beside itself. The prototype opens Chrome in
app mode, loads the native Rivet panel into that app window, and records the window as a Rivet
session. `rivet session list` lists recorded visible sessions and
`rivet session close --id <id>` closes only the identified app window.

The installer does not copy browser profiles, cookies, passkeys or passwords.
