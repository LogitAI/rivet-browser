# Rivet Windows installer

This directory adapts the DSA staged Windows installer flow for Rivet. It packages the Rust CLI
and the Windows runtime DLL into a per-machine MSI.

## Build

Run `build.ps1` from a Windows PowerShell session with Rust and WiX 5 available. The script
writes `rivet-<version>.msi` below `rivet/target/installer/`.

## First prototype run

After installing the MSI, open a new terminal so the system PATH change is visible. Start an
installed Chromium executable explicitly and point Rivet at its installed runtime DLL:

```powershell
rivet session start `
  --browser "C:\Program Files\Google\Chrome\Application\chrome.exe" `
  --url "https://example.com/" `
  --runtime "C:\Program Files\Rivet Browser\rivet_runtime_windows.dll"
```

The prototype opens Chrome in app mode, loads the native Rivet panel into that app window, and
records the window as a Rivet session. `rivet session list` lists recorded visible sessions and
`rivet session close --id <id>` closes only the identified app window.

The installer does not copy browser profiles, cookies, passkeys or passwords.
