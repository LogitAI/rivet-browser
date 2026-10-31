//! Adds a Chromium build to Rivet's supported-build table from the build's public symbols.
//!
//! `chrome-builds pdb-key <module>` prints the symbol-server path of the module's PDB.
//! `chrome-builds extract <module> <pdb-directory> <browser> <version> <table> [frame-class]`
//! resolves the layout hook's addresses with DbgHelp, checks them against the module file and
//! updates the table. The frame class is `BrowserFrameViewWin` unless the browser builds its
//! windows from a subclass, as Microsoft Edge does with `EdgeBrowserFrameViewWin`.
//! `chrome-builds port <prior-row.json> <prior.pdb> <new.pdb> <new-module> <version> <row.json> <report.json>`
//! makes a whole row for a new build from the row of a prior build, and exits 3 when the row needs
//! a person's review.
//! `chrome-builds pending <chrome-stable.json> <chrome-beta.json> <edge.json> <brave.json> <known.json>`
//! prints the builds that have no row yet, most urgent first.
//! `chrome-builds table-key` prints a new signing key pair.
//! `chrome-builds table-add <table.json> <row.json> <module>` puts a row into a table, with the
//! symbol key the next build's row will need.
//! `chrome-builds table-read <signed-file> <out.json>` writes out a table that verifies under
//! `RIVET_TABLE_PUBLIC_KEY`.
//! `chrome-builds table-merge <table.json> <part.json>...` adds the rows of other tables to a table.
//! `chrome-builds table-sign <table.json> <signed-file> [published-file]` signs a table with the
//! key in `RIVET_TABLE_SIGNING_KEY`, on top of the published rows, which must verify under
//! `RIVET_TABLE_PUBLIC_KEY`.

use std::path::Path;
use std::process::ExitCode;

use rivet_platform_windows::chrome_build::{self, ChromeBuild, PeFile};
use rivet_platform_windows::row_port::{self, NewBuild};
use rivet_platform_windows::{row_watch, signed_table};

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
        ["port", prior, old_pdb, new_pdb, module, version, row, report] => {
            return match port(
                Path::new(prior),
                Path::new(old_pdb),
                Path::new(new_pdb),
                Path::new(module),
                version,
                Path::new(row),
                Path::new(report),
            ) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(3),
                Err(error) => {
                    eprintln!("chrome-builds: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        ["pending", stable, beta, edge, brave, known] => {
            pending(Path::new(stable), Path::new(beta), Path::new(edge), Path::new(brave), Path::new(known))
        }
        ["table-key"] => signed_table::generate_key().map(|(private, public)| {
            println!("private {private}");
            println!("public {public}");
        }),
        ["table-add", table, row, module] => table_add(Path::new(table), Path::new(row), Path::new(module)),
        ["table-read", signed, out] => table_read(Path::new(signed), Path::new(out)),
        ["table-merge", table, parts @ ..] => table_merge(Path::new(table), parts),
        ["table-sign", table, signed] => table_sign(Path::new(table), Path::new(signed), None),
        ["table-sign", table, signed, published] => {
            table_sign(Path::new(table), Path::new(signed), Some(Path::new(published)))
        }
        _ => Err("usage: chrome-builds pdb-key <module> | extract <module> <pdb-directory> <browser> <version> <table> [frame-class] | port <prior-row.json> <prior.pdb> <new.pdb> <new-module> <version> <row.json> <report.json> | pending <chrome-stable.json> <chrome-beta.json> <edge.json> <brave.json> <known.json> | table-key | table-add <table.json> <row.json> <module> | table-read <signed-file> <out.json> | table-merge <table.json> <part.json>... | table-sign <table.json> <signed-file> [published-file]".into()),
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

fn port(
    prior: &Path,
    old_pdb: &Path,
    new_pdb: &Path,
    module: &Path,
    version: &str,
    row_path: &Path,
    report_path: &Path,
) -> Result<bool, String> {
    let prior: serde_json::Value =
        serde_json::from_slice(&read(prior)?).map_err(|error| format!("the prior row is not JSON: {error}"))?;
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("not a 64-bit PE image")?;
    let mut old = Vec::new();
    symbols::publics(old_pdb, &mut |address, name| old.push((address, name.to_owned())))?;
    old.sort();
    // The frame entry keeps plain numbers.
    let mut targets = row_port::addresses_of(&prior["build"]);
    for key in ["frame_view_vtable", "client_view_bounds", "hwnd_for_view"] {
        targets.extend(prior["frame"][key].as_u64().map(|value| value as u32));
    }
    let named = row_port::name_addresses(&targets, &old);
    let wanted = row_port::wanted_names(&named);
    drop(old);
    let mut new = std::collections::HashMap::<String, Vec<u32>>::new();
    symbols::publics(new_pdb, &mut |address, name| {
        let name = row_port::stable_name(name);
        if wanted.contains(name) {
            new.entry(name.to_owned()).or_default().push(address);
        }
    })?;
    let unnamed = row_port::unnamed_code(&prior, &named, &pe);
    let names = symbols::private_names(old_pdb, &unnamed)?;
    let places = symbols::private_places(new_pdb, &names.values().cloned().collect::<Vec<_>>())?;
    let private = names
        .iter()
        .filter_map(|(address, name)| match places.get(name).map(Vec::as_slice) {
            Some([place]) => Some((*address, *place)),
            _ => None,
        })
        .collect();
    let (row, report) = row_port::port_row(&prior, &named, &NewBuild { version, pe: &pe, symbols: &new, private: &private });
    let text = |value: &dyn erased::Json| value.pretty();
    std::fs::write(row_path, text(&row)).map_err(|error| error.to_string())?;
    std::fs::write(report_path, text(&report)).map_err(|error| error.to_string())?;
    println!(
        "{} {}: {} of {} addresses, {} for review",
        report.browser,
        report.version,
        report.mapped,
        report.addresses,
        report.review.len()
    );
    Ok(report.review.is_empty())
}

mod erased {
    pub trait Json {
        fn pretty(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn pretty(&self) -> String {
            serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
        }
    }
}

fn json_file(path: &Path) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&read(path)?).map_err(|error| format!("{} is not JSON: {error}", path.display()))
}

fn pending(stable: &Path, beta: &Path, edge: &Path, brave: &Path, known: &Path) -> Result<(), String> {
    let mut candidates = row_watch::chrome_candidates(&json_file(stable)?, &json_file(beta)?);
    candidates.extend(row_watch::edge_candidates(&json_file(edge)?));
    candidates.extend(row_watch::brave_candidates(&json_file(brave)?));
    let known = json_file(known)?
        .as_array()
        .ok_or("the known builds are not a list")?
        .iter()
        .filter_map(|pair| Some((pair[0].as_str()?.to_owned(), pair[1].as_str()?.to_owned())))
        .collect();
    let left = row_watch::pending(candidates, &known);
    println!("{}", serde_json::to_string_pretty(&left).map_err(|error| error.to_string())?);
    Ok(())
}

fn table_add(table_path: &Path, row: &Path, module: &Path) -> Result<(), String> {
    let row = json_file(row)?;
    let mut table = json_file(table_path)?;
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("not a 64-bit PE image")?;
    let key = pe.pdb_key().ok_or("the image has no CodeView record")?;
    let name = format!(
        "{} {}",
        row["frame"]["browser"].as_str().unwrap_or_default(),
        row["frame"]["version"].as_str().unwrap_or_default()
    );
    table["symbols"][name] = key.into();
    signed_table::upsert_row(&mut table, row)?;
    let text = serde_json::to_string_pretty(&table).map_err(|error| error.to_string())? + "\n";
    std::fs::write(table_path, text).map_err(|error| error.to_string())
}

fn table_read(signed: &Path, out: &Path) -> Result<(), String> {
    let public = std::env::var("RIVET_TABLE_PUBLIC_KEY").map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
    let file = read(signed)?;
    let table = signed_table::parse(signed_table::open(&public, &file)?)?;
    let text = serde_json::to_string(&table).map_err(|error| error.to_string())?;
    std::fs::write(out, text).map_err(|error| error.to_string())
}

fn table_merge(table_path: &Path, parts: &[&str]) -> Result<(), String> {
    let mut table = json_file(table_path)?;
    for part in parts {
        // A sequence of zero: only signing sets the real one.
        table = signed_table::merge_for_signing(&json_file(Path::new(part))?, Some(&table), 0)?;
        table["sequence"] = 0.into();
    }
    let text = serde_json::to_string_pretty(&table).map_err(|error| error.to_string())? + "\n";
    std::fs::write(table_path, text).map_err(|error| error.to_string())
}

fn table_sign(table: &Path, signed: &Path, published: Option<&Path>) -> Result<(), String> {
    let private = std::env::var("RIVET_TABLE_SIGNING_KEY").map_err(|_| "RIVET_TABLE_SIGNING_KEY is not set")?;
    let published = match published {
        Some(path) => {
            let public = std::env::var("RIVET_TABLE_PUBLIC_KEY").map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
            let file = read(path)?;
            Some(signed_table::parse(signed_table::open(&public, &file)?)?)
        }
        None => None,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let merged = signed_table::merge_for_signing(&json_file(table)?, published.as_ref(), now)?;
    let payload = serde_json::to_vec(&merged).map_err(|error| error.to_string())?;
    let file = signed_table::sign(&private, &payload)?;
    std::fs::write(signed, file).map_err(|error| error.to_string())?;
    println!("sequence {}, {} rows", merged["sequence"], merged["rows"].as_array().map_or(0, Vec::len));
    Ok(())
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
    use std::collections::{BTreeMap, HashMap};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::System::Diagnostics::Debug::{
        SymCleanup, SymEnumSymbolsW, SymFromAddrW, SymFromNameW, SymInitializeW, SymLoadModuleExW, SymSetOptions,
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

    /// Where a PDB opened on its own is placed: wide enough for every browser's module.
    const IMAGE_SIZE: u32 = 0x2000_0000;
    /// Reads only the linker's public names, which are decorated and stable between builds.
    const SYMOPT_PUBLICS_ONLY: u32 = 0x4000;

    /// Visits every public symbol of a PDB, with its address in the module and its decorated name.
    pub fn publics(pdb: &Path, visit: &mut dyn FnMut(u32, &str)) -> Result<(), String> {
        unsafe extern "system" fn each(
            info: *const SYMBOL_INFOW,
            _size: u32,
            context: *const core::ffi::c_void,
        ) -> i32 {
            let info = &*info;
            let visit = &mut *(context as *mut &mut dyn FnMut(u32, &str));
            let name = std::slice::from_raw_parts(info.Name.as_ptr(), info.NameLen as usize);
            if let Ok(address) = u32::try_from(info.Address - BASE) {
                visit(address, &String::from_utf16_lossy(name));
            }
            1
        }
        unsafe {
            SymSetOptions(SYMOPT_LOAD_ANYTHING | SYMOPT_PUBLICS_ONLY);
            if SymInitializeW(GetCurrentProcess(), std::ptr::null(), 0) == 0 {
                return Err("DbgHelp could not start".into());
            }
            let base = SymLoadModuleExW(
                GetCurrentProcess(),
                std::ptr::null_mut(),
                wide(pdb).as_ptr(),
                std::ptr::null(),
                BASE,
                IMAGE_SIZE,
                std::ptr::null(),
                0,
            );
            if base == 0 {
                SymCleanup(GetCurrentProcess());
                return Err(format!("DbgHelp could not load {}", pdb.display()));
            }
            let mask: Vec<u16> = "*".encode_utf16().chain(Some(0)).collect();
            let mut visit = visit;
            let done = SymEnumSymbolsW(
                GetCurrentProcess(),
                BASE,
                mask.as_ptr(),
                Some(each),
                (&raw mut visit).cast(),
            );
            SymCleanup(GetCurrentProcess());
            if done == 0 {
                return Err(format!("DbgHelp could not list {}", pdb.display()));
            }
        }
        Ok(())
    }

    fn load_all(pdb: &Path) -> Result<(), String> {
        unsafe {
            SymSetOptions(SYMOPT_LOAD_ANYTHING | SYMOPT_UNDNAME);
            if SymInitializeW(GetCurrentProcess(), std::ptr::null(), 0) == 0 {
                return Err("DbgHelp could not start".into());
            }
            let base = SymLoadModuleExW(
                GetCurrentProcess(),
                std::ptr::null_mut(),
                wide(pdb).as_ptr(),
                std::ptr::null(),
                BASE,
                IMAGE_SIZE,
                std::ptr::null(),
                0,
            );
            if base == 0 {
                SymCleanup(GetCurrentProcess());
                return Err(format!("DbgHelp could not load {}", pdb.display()));
            }
        }
        Ok(())
    }

    /// The function that starts at each address, from the PDB's private symbols.
    pub fn private_names(pdb: &Path, addresses: &[u32]) -> Result<BTreeMap<u32, String>, String> {
        load_all(pdb)?;
        let mut names = BTreeMap::new();
        for address in addresses {
            unsafe {
                // SYMBOL_INFOW ends in the name, whose room the caller provides.
                let mut storage = vec![0u64; (std::mem::size_of::<SYMBOL_INFOW>() + 2048) / 8];
                let info = storage.as_mut_ptr().cast::<SYMBOL_INFOW>();
                (*info).SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
                (*info).MaxNameLen = 1000;
                let mut distance = 0u64;
                if SymFromAddrW(GetCurrentProcess(), BASE + u64::from(*address), &mut distance, info) != 0 && distance == 0 {
                    let name = std::slice::from_raw_parts((*info).Name.as_ptr(), (*info).NameLen as usize);
                    names.insert(*address, String::from_utf16_lossy(name).trim_end_matches(['\0', ' ']).to_owned());
                }
            }
        }
        unsafe { SymCleanup(GetCurrentProcess()) };
        Ok(names)
    }

    /// Where each function name lies, from the PDB's private symbols.
    pub fn private_places(pdb: &Path, names: &[String]) -> Result<HashMap<String, Vec<u32>>, String> {
        unsafe extern "system" fn each(
            info: *const SYMBOL_INFOW,
            _size: u32,
            context: *const core::ffi::c_void,
        ) -> i32 {
            let info = &*info;
            let found = &mut *(context as *mut Vec<(String, u32)>);
            let name = std::slice::from_raw_parts(info.Name.as_ptr(), info.NameLen as usize);
            if let Ok(address) = u32::try_from(info.Address - BASE) {
                found.push((String::from_utf16_lossy(name), address));
            }
            1
        }
        load_all(pdb)?;
        let mut places: HashMap<String, Vec<u32>> = HashMap::new();
        for name in names {
            let mask: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let mut found: Vec<(String, u32)> = Vec::new();
            unsafe {
                SymEnumSymbolsW(GetCurrentProcess(), BASE, mask.as_ptr(), Some(each), (&raw mut found).cast());
            }
            let mut exact: Vec<u32> = found
                .into_iter()
                .filter(|(other, _)| other.trim_end_matches(['\0', ' ']) == name)
                .map(|(_, place)| place)
                .collect();
            exact.sort_unstable();
            exact.dedup();
            places.insert(name.clone(), exact);
        }
        unsafe { SymCleanup(GetCurrentProcess()) };
        Ok(places)
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

    pub fn publics(_pdb: &Path, _visit: &mut dyn FnMut(u32, &str)) -> Result<(), String> {
        Err("symbol extraction needs Windows DbgHelp".into())
    }
    pub fn private_names(_pdb: &Path, _addresses: &[u32]) -> Result<std::collections::BTreeMap<u32, String>, String> {
        Err("symbol extraction needs Windows DbgHelp".into())
    }
    pub fn private_places(_pdb: &Path, _names: &[String]) -> Result<std::collections::HashMap<String, Vec<u32>>, String> {
        Err("symbol extraction needs Windows DbgHelp".into())
    }

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
