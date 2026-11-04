//! The Windows side of finding browsers: registered programs, version resources and the lookups
//! that feed `detect`.

use crate::detect::{self, Browser, Disk, Product, Rejection, Roots};
use rivet_platform_windows::hold;
use std::path::{Path, PathBuf};

/// The program an `App Paths` value names. Installers write it plain, quoted, or followed by
/// arguments.
pub fn registered_value(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let program = match value.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => match value.to_ascii_lowercase().find(".exe") {
            Some(end) => &value[..end + 4],
            None => value,
        },
    };
    program
        .to_ascii_lowercase()
        .ends_with(".exe")
        .then(|| PathBuf::from(program))
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// Every program that an `App Paths` entry of this computer or this user registers.
#[cfg(windows)]
pub fn registered_programs() -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ,
    };
    let base = wide(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths");
    let mut found: Vec<PathBuf> = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let mut paths: HKEY = std::ptr::null_mut();
            if unsafe { RegOpenKeyExW(root, base.as_ptr(), 0, KEY_READ | view, &mut paths) } != 0 {
                continue;
            }
            for index in 0.. {
                let mut name = [0u16; 260];
                let mut length = name.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        paths,
                        index,
                        name.as_mut_ptr(),
                        &mut length,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if status != 0 {
                    break;
                }
                let mut entry: HKEY = std::ptr::null_mut();
                if unsafe { RegOpenKeyExW(paths, name.as_ptr(), 0, KEY_READ | view, &mut entry) }
                    != 0
                {
                    continue;
                }
                let mut bytes = 0u32;
                let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
                let size = unsafe {
                    RegGetValueW(
                        entry,
                        std::ptr::null(),
                        std::ptr::null(),
                        flags,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut bytes,
                    )
                };
                if size == 0 && bytes >= 2 {
                    let mut value = vec![0u16; bytes as usize / 2 + 1];
                    let read = unsafe {
                        RegGetValueW(
                            entry,
                            std::ptr::null(),
                            std::ptr::null(),
                            flags,
                            std::ptr::null_mut(),
                            value.as_mut_ptr().cast(),
                            &mut bytes,
                        )
                    };
                    if read == 0 {
                        let end = value
                            .iter()
                            .position(|unit| *unit == 0)
                            .unwrap_or(value.len());
                        if let Some(program) =
                            registered_value(&String::from_utf16_lossy(&value[..end]))
                        {
                            found.push(program);
                        }
                    }
                }
                unsafe { RegCloseKey(entry) };
            }
            unsafe { RegCloseKey(paths) };
        }
    }
    found
}

#[cfg(not(windows))]
pub fn registered_programs() -> Vec<PathBuf> {
    Vec::new()
}

/// The product name and version in a program's version resource.
#[cfg(windows)]
pub fn product(path: &Path) -> Product {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(file.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return Product::default();
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(file.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return Product::default();
        }
        let query = |key: &str| -> Option<(*const u8, u32)> {
            let key = wide(key);
            let mut at = std::ptr::null_mut();
            let mut length = 0u32;
            (VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut at, &mut length) != 0
                && !at.is_null()
                && length > 0)
                .then_some((at as *const u8, length))
        };
        let mut languages: Vec<String> = Vec::new();
        if let Some((at, length)) = query(r"\VarFileInfo\Translation") {
            for pair in 0..(length as usize / 4) {
                let language = std::ptr::read_unaligned(at.add(pair * 4) as *const u16);
                let page = std::ptr::read_unaligned(at.add(pair * 4 + 2) as *const u16);
                languages.push(format!("{language:04x}{page:04x}"));
            }
        }
        languages.extend([
            "040904b0".to_owned(),
            "040904e4".to_owned(),
            "000004b0".to_owned(),
        ]);
        let text = |field: &str| -> Option<String> {
            languages.iter().find_map(|language| {
                let (at, length) = query(&format!(r"\StringFileInfo\{language}\{field}"))?;
                let units = std::slice::from_raw_parts(at as *const u16, length as usize);
                let end = units
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(units.len());
                let value = String::from_utf16_lossy(&units[..end]);
                (!value.trim().is_empty()).then_some(value)
            })
        };
        Product {
            name: text("ProductName").or_else(|| text("FileDescription")),
            version: text("ProductVersion"),
        }
    }
}

#[cfg(not(windows))]
pub fn product(_path: &Path) -> Product {
    Product::default()
}

/// A browser the files proved, with its name, version and whether Rivet can apply to it.
pub fn describe(path: PathBuf, proof: PathBuf) -> Browser {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let installed = hold::installed_version(&path);
    let supported = hold::supported_versions(&file_name);
    let kept = hold::default_root()
        .map(|root| hold::held_versions(&root, &file_name))
        .unwrap_or_default();
    let status = detect::status(&file_name, installed, &supported, &kept);
    let product = product(&path);
    Browser::new(path, proof, product, status, installed)
}

/// Every Chromium browser on this computer, in the order people see.
pub fn detect_all() -> Vec<Browser> {
    let mut browsers: Vec<Browser> =
        detect::chromium_programs(&Disk, &Roots::from_environment(), registered_programs())
            .into_iter()
            .map(|(path, proof)| describe(path, proof))
            .collect();
    detect::order(&mut browsers);
    browsers
}

/// A program chosen by hand, accepted only on the same evidence as the detected ones.
pub fn inspect(path: &Path) -> Result<Browser, Rejection> {
    let proof = detect::chromium_proof(&Disk, path)?;
    Ok(describe(path.to_path_buf(), proof))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr1_registered_values_name_the_program_in_any_form_installers_write_it() {
        assert_eq!(
            registered_value(r"C:\A\a.exe"),
            Some(PathBuf::from(r"C:\A\a.exe"))
        );
        assert_eq!(
            registered_value(r#""C:\Program Files\A\a.exe" --flag"#),
            Some(PathBuf::from(r"C:\Program Files\A\a.exe"))
        );
        assert_eq!(
            registered_value(r"C:\Program Files\A\A.EXE /x"),
            Some(PathBuf::from(r"C:\Program Files\A\A.EXE"))
        );
        assert_eq!(registered_value(""), None);
        assert_eq!(registered_value(r"C:\A\library.dll"), None);
    }
}
