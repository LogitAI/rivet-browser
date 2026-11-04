//! The loaded browser module, checked before any recorded address is read.
//!
//! Every address Rivet patches or calls is an offset into one exact browser build. Reading such an
//! offset in another build can leave the mapped image and crash the browser, so the module's own
//! version resource and size are checked first and every offset is bounds checked against the
//! mapped size.

use std::sync::OnceLock;

use crate::builds::{self, Build};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

#[derive(Clone, Copy, Debug)]
pub struct Image {
    pub base: usize,
    pub size: usize,
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// `SizeOfImage` of a mapped 64-bit PE, from its first bytes.
pub fn size_of_image(head: &[u8]) -> Option<usize> {
    if u16_at(head, 0)? != 0x5a4d {
        return None;
    }
    let nt = u32_at(head, 0x3c)? as usize;
    if head.get(nt..nt.checked_add(4)?)? != b"PE\0\0" || u16_at(head, nt + 24)? != 0x20b {
        return None;
    }
    Some(u32_at(head, nt + 24 + 56)? as usize)
}

/// The file version from the version resource of a mapped 64-bit PE, where every RVA is an
/// offset into `image`.
pub fn file_version(image: &[u8]) -> Option<[u16; 4]> {
    let nt = u32_at(image, 0x3c)? as usize;
    // Data directory 2 is the resource table.
    let root = u32_at(image, nt.checked_add(24 + 112 + 16)?)? as usize;
    if root == 0 {
        return None;
    }
    // Entry `index` of the directory at `at`: its id and the offset of what it names.
    let entry = |at: usize, index: usize| -> Option<(u32, u32)> {
        let count = u16_at(image, at.checked_add(12)?)? as usize
            + u16_at(image, at.checked_add(14)?)? as usize;
        if index >= count {
            return None;
        }
        let entry = at.checked_add(16 + index * 8)?;
        Some((u32_at(image, entry)?, u32_at(image, entry.checked_add(4)?)?))
    };
    let directory = |offset: u32| root.checked_add((offset & 0x7fff_ffff) as usize);
    // RT_VERSION is resource type 16.
    let mut kind = None;
    for index in 0..64 {
        let Some((id, offset)) = entry(root, index) else {
            break;
        };
        if id == 16 && offset & 0x8000_0000 != 0 {
            kind = directory(offset);
            break;
        }
    }
    let name = directory(entry(kind?, 0)?.1)?;
    let (_, data) = entry(name, 0)?;
    if data & 0x8000_0000 != 0 {
        return None;
    }
    let data = root.checked_add(data as usize)?;
    let start = u32_at(image, data)? as usize;
    let length = u32_at(image, data.checked_add(4)?)? as usize;
    let info = image.get(start..start.checked_add(length)?)?;
    // VS_VERSIONINFO: three words, the key `VS_VERSION_INFO`, padding to four bytes, then
    // VS_FIXEDFILEINFO.
    let key: Vec<u16> = (0..15)
        .map(|index| u16_at(info, 6 + index * 2))
        .collect::<Option<_>>()?;
    if String::from_utf16(&key).ok()? != "VS_VERSION_INFO" || u32_at(info, 40)? != 0xfeef_04bd {
        return None;
    }
    let (most, least) = (u32_at(info, 48)?, u32_at(info, 52)?);
    Some([
        (most >> 16) as u16,
        most as u16,
        (least >> 16) as u16,
        least as u16,
    ])
}

impl Image {
    /// The address of `length` bytes at `rva`, or `None` when they are not inside the image.
    pub fn at(&self, rva: usize, length: usize) -> Option<usize> {
        (rva.checked_add(length)? <= self.size).then(|| self.base + rva)
    }

    /// The pointer-sized value stored at `rva`.
    pub unsafe fn word(&self, rva: usize) -> Option<usize> {
        Some((self.at(rva, 8)? as *const usize).read_unaligned())
    }

    /// The bytes at `rva`.
    pub unsafe fn bytes(&self, rva: usize, length: usize) -> Option<&'static [u8]> {
        Some(std::slice::from_raw_parts(
            self.at(rva, length)? as *const u8,
            length,
        ))
    }

    /// Whether the bytes at `rva` are exactly `expected`.
    pub unsafe fn has(&self, rva: usize, expected: &[u8]) -> bool {
        self.bytes(rva, expected.len()) == Some(expected)
    }

    /// Whether the pointer-sized value at `rva` is the address of the function at `target`.
    pub unsafe fn points_to(&self, rva: usize, target: usize) -> bool {
        self.word(rva) == Some(self.base + target)
    }
}

/// Checks the module at `base`: a 64-bit PE whose size is plausible and whose version
/// resource names `version`.
pub unsafe fn verify(base: usize, version: [u16; 4]) -> Option<Image> {
    if base == 0 {
        return None;
    }
    // The first page of a loaded module is always mapped.
    let size = size_of_image(std::slice::from_raw_parts(base as *const u8, 0x400))?;
    if !(0x1_0000..=0x4000_0000).contains(&size) {
        return None;
    }
    let bytes = std::slice::from_raw_parts(base as *const u8, size);
    (file_version(bytes)? == version).then_some(Image { base, size })
}

static IMAGE: OnceLock<Option<(Image, &'static Build)>> = OnceLock::new();

fn matched() -> Option<(Image, &'static Build)> {
    *IMAGE.get_or_init(|| unsafe {
        builds::all().iter().find_map(|build| {
            let module: Vec<u16> = build.module.encode_utf16().chain(Some(0)).collect();
            let image = verify(GetModuleHandleW(module.as_ptr()) as usize, build.version)?;
            Some((image, *build))
        })
    })
}

/// The loaded browser whose offsets are recorded, or `None` for any other build or process.
pub fn recorded() -> Option<Image> {
    matched().map(|(image, _)| image)
}

/// The table row of the loaded browser.
pub fn recorded_build() -> Option<&'static Build> {
    matched().map(|(_, build)| build)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mapped image with a version resource, laid out with RVA equal to offset.
    fn fake(version: [u16; 4]) -> Vec<u8> {
        let mut image = vec![0u8; 0x2000];
        let put32 = |image: &mut Vec<u8>, at: usize, value: u32| {
            image[at..at + 4].copy_from_slice(&value.to_le_bytes())
        };
        image[0..2].copy_from_slice(b"MZ");
        put32(&mut image, 0x3c, 0x80);
        image[0x80..0x84].copy_from_slice(b"PE\0\0");
        image[0x80 + 24..0x80 + 26].copy_from_slice(&0x20bu16.to_le_bytes());
        put32(&mut image, 0x80 + 24 + 56, 0x2000);
        // Resource table at 0x400: type directory, name directory, language directory, entry.
        put32(&mut image, 0x80 + 24 + 112 + 16, 0x400);
        image[0x400 + 14] = 1;
        put32(&mut image, 0x410, 16);
        put32(&mut image, 0x414, 0x8000_0000 | 0x20);
        image[0x420 + 14] = 1;
        put32(&mut image, 0x430, 1);
        put32(&mut image, 0x434, 0x8000_0000 | 0x40);
        image[0x440 + 14] = 1;
        put32(&mut image, 0x450, 0x409);
        put32(&mut image, 0x454, 0x60);
        put32(&mut image, 0x460, 0x800);
        put32(&mut image, 0x464, 0x100);
        let key: Vec<u8> = "VS_VERSION_INFO"
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        image[0x806..0x806 + key.len()].copy_from_slice(&key);
        put32(&mut image, 0x800 + 40, 0xfeef_04bd);
        put32(
            &mut image,
            0x800 + 48,
            (version[0] as u32) << 16 | version[1] as u32,
        );
        put32(
            &mut image,
            0x800 + 52,
            (version[2] as u32) << 16 | version[3] as u32,
        );
        image
    }

    #[test]
    fn nfr3_version_resource_names_the_build() {
        let image = fake([156, 0, 8078, 48]);
        assert_eq!(file_version(&image), Some([156, 0, 8078, 48]));
        assert_eq!(size_of_image(&image), Some(0x2000));
    }

    #[test]
    fn nfr3_another_version_is_not_the_recorded_one() {
        let image = fake([157, 0, 8100, 1]);
        assert_ne!(file_version(&image), Some([156, 0, 8078, 48]));
    }

    #[test]
    fn nfr3_damaged_images_are_rejected_without_panicking() {
        let image = fake([156, 0, 8078, 48]);
        for length in [0, 1, 0x3c, 0x80, 0x120, 0x420, 0x7ff, 0x850] {
            let _ = file_version(&image[..length]);
            let _ = size_of_image(&image[..length]);
        }
        assert_eq!(file_version(&image[..0x850]), None);
        assert_eq!(size_of_image(&[0u8; 0x400]), None);
    }

    #[test]
    fn nfr3_recorded_offsets_outside_the_image_are_not_read() {
        let bytes = vec![0xabu8; 64];
        let image = Image {
            base: bytes.as_ptr() as usize,
            size: 64,
        };
        assert!(image.at(56, 8).is_some());
        assert!(image.at(57, 8).is_none());
        assert!(image.at(usize::MAX, 8).is_none());
        assert!(image.at(0x0FDA_FE00, 8).is_none());
        unsafe {
            assert_eq!(image.word(64), None);
            assert_eq!(image.bytes(60, 5), None);
            assert!(!image.has(0x1000_0000, &[0xab]));
            assert_eq!(image.word(0), Some(0xabab_abab_abab_abab));
        }
    }
}
