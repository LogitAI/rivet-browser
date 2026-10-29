//! Adds a Chromium build to Rivet's supported-build table from the build's public symbols.
//!
//! `chrome-builds pdb-key <module>` prints the symbol-server path of the module's PDB.
//! `chrome-builds extract <module> <pdb-directory> <browser> <version> <table> [frame-class]`
//! resolves the layout hook's addresses with DbgHelp, checks them against the module file and
//! updates the table. The frame class is `BrowserFrameViewWin` unless the browser builds its
//! windows from a subclass, as Microsoft Edge does with `EdgeBrowserFrameViewWin`.

use std::path::Path;
use std::process::ExitCode;

use rivet_platform_windows::chrome_build::{self, ChromeBuild, PeFile};

/// How many bytes of `views::HWNDForView` the runtime compares before patching.
const PROLOGUE: usize = 15;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["pdb-key", module] => pdb_key(Path::new(module)),
        ["extract", module, pdbs, browser, version, table] => extract(
            Path::new(module),
            Path::new(pdbs),
            browser,
            version,
            Path::new(table),
            "BrowserFrameViewWin",
        ),
        ["extract", module, pdbs, browser, version, table, class] => {
            extract(Path::new(module), Path::new(pdbs), browser, version, Path::new(table), class)
        }
        _ => Err("usage: chrome-builds pdb-key <module> | extract <module> <pdb-directory> <browser> <version> <table> [frame-class]".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("chrome-builds: {error}");
            ExitCode::FAILURE
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

fn pdb_key(module: &Path) -> Result<(), String> {
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("not a 64-bit PE image")?;
    let key = pe.pdb_key().ok_or("the image has no CodeView record")?;
    let pdb = format!(
        "{}.pdb",
        module
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("bad module name")?
    );
    println!("{pdb}/{key}/{pdb}");
    Ok(())
}

fn extract(
    module: &Path,
    pdbs: &Path,
    browser: &str,
    version: &str,
    table_path: &Path,
    frame_class: &str,
) -> Result<(), String> {
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("not a 64-bit PE image")?;
    let symbols = symbols::Session::open(module, pdbs)?;
    // The function is declared on `BrowserFrameViewWin` whichever subclass builds the windows.
    let client_view_bounds = symbols.address("BrowserFrameViewWin::GetBoundsForClientView")?;
    let hwnd_for_view = symbols.address("views::HWNDForView")?;
    // The class has several vtables because of multiple inheritance. Exactly one of them holds
    // the client-view bounds function.
    let (frame_view_vtable, client_view_slot) = symbols
        .vtables(frame_class)?
        .into_iter()
        .find_map(|vtable| Some((vtable, pe.vtable_slot(vtable, client_view_bounds)?)))
        .ok_or("no vtable of the frame class holds GetBoundsForClientView")?;
    let prologue = pe
        .read(hwnd_for_view, PROLOGUE)
        .ok_or("views::HWNDForView is outside the image")?;
    let build = ChromeBuild {
        browser: browser.into(),
        version: version.into(),
        module: module
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("bad module name")?
            .into(),
        frame_view_vtable,
        client_view_slot,
        client_view_bounds,
        hwnd_for_view,
        hwnd_for_view_prologue: prologue.iter().map(|byte| format!("{byte:02x}")).collect(),
    };
    let text = std::fs::read_to_string(table_path).unwrap_or_else(|_| "[]".into());
    let mut table = chrome_build::builds(&text);
    chrome_build::upsert(&mut table, build.clone());
    let text = serde_json::to_string_pretty(&table).map_err(|error| error.to_string())? + "\n";
    std::fs::write(table_path, text)
        .map_err(|error| format!("could not write {}: {error}", table_path.display()))?;
    println!(
        "{}",
        serde_json::to_string(&build).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[cfg(windows)]
mod symbols {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::System::Diagnostics::Debug::{
        SymCleanup, SymEnumSymbolsW, SymFromNameW, SymInitializeW, SymLoadModuleExW, SymSetOptions,
        SYMBOL_INFOW, SYMOPT_UNDNAME,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    /// The base the module is loaded at for symbol lookups. Results are reported as RVAs.
    const BASE: u64 = 0x1_8000_0000;
    /// Accept a PDB whose age differs from the image, which Google's symbol server serves.
    const SYMOPT_LOAD_ANYTHING: u32 = 0x40;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub struct Session;

    impl Session {
        pub fn open(module: &Path, pdbs: &Path) -> Result<Self, String> {
            unsafe {
                SymSetOptions(SYMOPT_UNDNAME | SYMOPT_LOAD_ANYTHING);
                if SymInitializeW(GetCurrentProcess(), wide(pdbs).as_ptr(), 0) == 0 {
                    return Err("DbgHelp could not start".into());
                }
                let base = SymLoadModuleExW(
                    GetCurrentProcess(),
                    std::ptr::null_mut(),
                    wide(module).as_ptr(),
                    std::ptr::null(),
                    BASE,
                    0,
                    std::ptr::null(),
                    0,
                );
                if base == 0 {
                    return Err("DbgHelp could not load the module's symbols".into());
                }
            }
            Ok(Self)
        }

        pub fn address(&self, name: &str) -> Result<u32, String> {
            let name_wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            unsafe {
                let mut info: SYMBOL_INFOW = std::mem::zeroed();
                info.SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
                if SymFromNameW(GetCurrentProcess(), name_wide.as_ptr(), &mut info) == 0 {
                    return Err(format!("the symbols have no {name}"));
                }
                u32::try_from(info.Address - BASE).map_err(|_| format!("{name} is out of range"))
            }
        }

        pub fn vtables(&self, class: &str) -> Result<Vec<u32>, String> {
            unsafe extern "system" fn visit(
                info: *const SYMBOL_INFOW,
                _size: u32,
                context: *const core::ffi::c_void,
            ) -> i32 {
                let info = &*info;
                let found = &mut *(context as *mut Vec<(String, u64)>);
                let name = std::slice::from_raw_parts(info.Name.as_ptr(), info.NameLen as usize);
                found.push((String::from_utf16_lossy(name), info.Address));
                1
            }
            let mask: Vec<u16> = format!("{class}::*")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut found: Vec<(String, u64)> = Vec::new();
            unsafe {
                SymEnumSymbolsW(
                    GetCurrentProcess(),
                    BASE,
                    mask.as_ptr(),
                    Some(visit),
                    &mut found as *mut _ as *const _,
                );
            }
            let vtables: Vec<u32> = found
                .into_iter()
                .filter(|(name, _)| {
                    name.trim_end_matches(['\0', ' ']) == format!("{class}::`vftable'")
                })
                .filter_map(|(_, address)| u32::try_from(address - BASE).ok())
                .collect();
            if vtables.is_empty() {
                return Err(format!("the symbols have no {class} vtable"));
            }
            Ok(vtables)
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe { SymCleanup(GetCurrentProcess()) };
        }
    }
}

#[cfg(not(windows))]
mod symbols {
    use std::path::Path;

    pub struct Session;

    impl Session {
        pub fn open(_module: &Path, _pdbs: &Path) -> Result<Self, String> {
            Err("symbol extraction needs Windows DbgHelp".into())
        }
        pub fn address(&self, _name: &str) -> Result<u32, String> {
            unreachable!()
        }
        pub fn vtables(&self, _class: &str) -> Result<Vec<u32>, String> {
            unreachable!()
        }
    }
}
