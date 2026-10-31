//! The supported-build table as a signed file, which reaches a user without reinstalling Rivet.
//!
//! A file is a magic, an ECDSA P-256 signature over SHA-256 of the magic and the payload, and the
//! payload, which is JSON. The signature is checked with Windows' own CNG, so verifying needs no
//! other library. Every reader goes through `open`: nothing in an unverified file is parsed.
//!
//! The public key and the download address are fixed when Rivet is built, from
//! `RIVET_TABLE_PUBLIC_KEY` and `RIVET_TABLE_URL`. A build without a key accepts no file, and then
//! only the table compiled into it applies.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The public key that release builds trust, as hex of X and Y.
pub const RELEASE_KEY: Option<&str> = option_env!("RIVET_TABLE_PUBLIC_KEY");
/// Where release builds download the table.
pub const RELEASE_URL: Option<&str> = option_env!("RIVET_TABLE_URL");
/// The payload format this Rivet reads.
pub const SCHEMA: u64 = 1;

const MAGIC: &[u8] = b"RVTB1\n";
const SIGNATURE: usize = 64;
/// More than any real table, so a wrong answer from a server cannot fill the disk.
const LARGEST: usize = 64 * 1024 * 1024;

/// Decodes hex, or names what it was for.
fn hex_bytes(text: &str, what: &str, length: usize) -> Result<Vec<u8>, String> {
    crate::chrome_build::hex(text.trim())
        .filter(|bytes| bytes.len() == length)
        .ok_or_else(|| format!("the {what} is not {length} bytes of hex"))
}

fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Makes a new key pair: private as hex of X, Y and D, public as hex of X and Y.
pub fn generate_key() -> Result<(String, String), String> {
    let private = cng::generate()?;
    let public = hex_text(&private[..64]);
    Ok((hex_text(&private), public))
}

/// Makes a signed file of a payload.
pub fn sign(private_key: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
    let private = hex_bytes(private_key, "private key", 96)?;
    let signature = cng::sign(&private, &digest(payload)?)?;
    let mut file = Vec::with_capacity(MAGIC.len() + SIGNATURE + payload.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&signature);
    file.extend_from_slice(payload);
    Ok(file)
}

fn digest(payload: &[u8]) -> Result<[u8; 32], String> {
    let mut signed = Vec::with_capacity(MAGIC.len() + payload.len());
    signed.extend_from_slice(MAGIC);
    signed.extend_from_slice(payload);
    cng::sha256(&signed)
}

/// Checks a signed file and returns its payload.
pub fn open<'a>(public_key: &str, file: &'a [u8]) -> Result<&'a [u8], String> {
    let public = hex_bytes(public_key, "public key", 64)?;
    let rest = file
        .strip_prefix(MAGIC)
        .ok_or("the file is not a Rivet build table")?;
    if rest.len() < SIGNATURE {
        return Err("the file is too short to hold a signature".into());
    }
    let (signature, payload) = rest.split_at(SIGNATURE);
    cng::verify(&public, &digest(payload)?, signature)?;
    Ok(payload)
}

/// Reads a verified payload, refusing one this Rivet cannot read.
pub fn parse(payload: &[u8]) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| format!("the payload is not JSON: {error}"))?;
    match value["schema"].as_u64() {
        Some(SCHEMA) => {}
        schema => return Err(format!("the table has schema {schema:?}, not {SCHEMA}")),
    }
    if value["sequence"].as_u64().is_none() || !value["rows"].is_array() {
        return Err("the table has no sequence or no rows".into());
    }
    Ok(value)
}

/// Where the applied table lives, below `%LOCALAPPDATA%`. It is not in the install folder, which
/// the person's account cannot write. Anyone who can write here still cannot forge a signature.
pub fn store_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Rivet")
        .join("builds")
        .join("table.bin")
}

/// The store below the current user's `%LOCALAPPDATA%`.
pub fn default_store() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|local| !local.is_empty())
        .map(|local| store_path(Path::new(&local)))
}

/// Reads the applied table. `Ok(None)` when none has been applied.
pub fn read_store(store: &Path, public_key: &str) -> Result<Option<Value>, String> {
    let file = match std::fs::read(store) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read {}: {error}", store.display())),
    };
    parse(open(public_key, &file)?).map(Some)
}

/// Puts a generated row into a table's rows, replacing one of the same browser and version.
pub fn upsert_row(table: &mut Value, row: Value) -> Result<(), String> {
    let identity = |row: &Value| {
        (
            row["frame"]["browser"].clone(),
            row["frame"]["version"].clone(),
        )
    };
    let new = identity(&row);
    if new.0.is_null() || new.1.is_null() {
        return Err("the row has no browser and version".into());
    }
    let rows = table["rows"]
        .as_array_mut()
        .ok_or("the table has no rows")?;
    rows.retain(|known| identity(known) != new);
    rows.push(row);
    rows.sort_by_key(|row| {
        let (browser, version) = identity(row);
        let parts: Vec<u32> = version
            .as_str()
            .unwrap_or_default()
            .split('.')
            .filter_map(|part| part.parse().ok())
            .collect();
        (browser.as_str().unwrap_or_default().to_owned(), parts)
    });
    Ok(())
}

/// The table to publish: the rows already published, then the rows of `table`, which win. Published
/// rows stay, since another run's row may not have reached the repository yet. Its sequence is
/// later than the published one, so a client takes it, and at least `now`.
pub fn merge_for_signing(
    table: &Value,
    published: Option<&Value>,
    now: u64,
) -> Result<Value, String> {
    let mut merged = published.cloned().unwrap_or_else(|| table.clone());
    for row in table["rows"].as_array().ok_or("the table has no rows")? {
        upsert_row(&mut merged, row.clone())?;
    }
    if let Some(symbols) = table["symbols"].as_object() {
        for (name, key) in symbols {
            merged["symbols"][name] = key.clone();
        }
    }
    let after = published
        .and_then(|table| table["sequence"].as_u64())
        .unwrap_or(0)
        + 1;
    merged["sequence"] = now.max(after).into();
    merged["schema"] = SCHEMA.into();
    Ok(merged)
}

/// Applies a downloaded file when its signature holds and it is not older than the applied one.
/// A person's running browser keeps what it loaded, and the next one reads the new file. The file
/// moves into place whole, so a reader never sees half of one. Returns the applied sequence.
pub fn install(store: &Path, public_key: &str, file: &[u8]) -> Result<u64, String> {
    let table = parse(open(public_key, file)?)?;
    let sequence = table["sequence"].as_u64().unwrap_or_default();
    // A table that no longer verifies, say under a replaced key, must not block a good one.
    if let Ok(Some(applied)) = read_store(store, public_key) {
        let applied = applied["sequence"].as_u64().unwrap_or_default();
        if sequence < applied {
            return Err(format!(
                "the file has sequence {sequence}, older than the applied {applied}"
            ));
        }
    }
    let folder = store.parent().ok_or("the store has no folder")?;
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    let unfinished = store.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&unfinished, file)
        .map_err(|error| format!("could not write {}: {error}", unfinished.display()))?;
    std::fs::rename(&unfinished, store).map_err(|error| {
        let _ = std::fs::remove_file(&unfinished);
        format!("could not replace {}: {error}", store.display())
    })?;
    Ok(sequence)
}

/// Downloads and applies the table when the applied one is older than `max_age`. It is limited by
/// `timeout_ms`, and every failure leaves the applied table in place, so it never holds up a window.
pub fn refresh(
    store: &Path,
    public_key: &str,
    url: &str,
    max_age: std::time::Duration,
    timeout_ms: i32,
) -> Result<Option<u64>, String> {
    let fresh = std::fs::metadata(store)
        .and_then(|data| data.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < max_age);
    if fresh {
        return Ok(None);
    }
    let file = http::get(url, timeout_ms)?;
    install(store, public_key, &file).map(Some)
}

/// Brings the applied table up to date before a Rivet window opens, when this build has a key and
/// an address. It waits a moment at most: a window never stays closed for a table, and a browser
/// updated since the last check is the one case a fresh table can save.
pub fn refresh_before_start() {
    const AT_MOST_MS: i32 = 2_000;
    const CHECKED_RECENTLY: std::time::Duration = std::time::Duration::from_secs(3_600);
    if let (Some(key), Some(url), Some(store)) = (RELEASE_KEY, RELEASE_URL, default_store()) {
        let _ = refresh(&store, key, url, CHECKED_RECENTLY, AT_MOST_MS);
    }
}

#[cfg(windows)]
mod cng {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptDestroyKey, BCryptExportKey, BCryptFinalizeKeyPair, BCryptGenerateKeyPair,
        BCryptHash, BCryptImportKeyPair, BCryptSignHash, BCryptVerifySignature, BCRYPT_ECCKEY_BLOB,
        BCRYPT_ECCPRIVATE_BLOB, BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDSA_P256_ALG_HANDLE,
        BCRYPT_ECDSA_PRIVATE_P256_MAGIC, BCRYPT_ECDSA_PUBLIC_P256_MAGIC, BCRYPT_KEY_HANDLE,
        BCRYPT_SHA256_ALG_HANDLE,
    };

    const HEADER: usize = std::mem::size_of::<BCRYPT_ECCKEY_BLOB>();

    fn check(status: i32, what: &str) -> Result<(), String> {
        if status >= 0 {
            Ok(())
        } else {
            Err(format!("{what} failed with status {:#x}", status as u32))
        }
    }

    struct Key(BCRYPT_KEY_HANDLE);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { BCryptDestroyKey(self.0) };
        }
    }

    fn import(magic: u32, kind: windows_sys::core::PCWSTR, material: &[u8]) -> Result<Key, String> {
        let mut blob = Vec::with_capacity(HEADER + material.len());
        blob.extend_from_slice(&magic.to_le_bytes());
        blob.extend_from_slice(&32u32.to_le_bytes());
        blob.extend_from_slice(material);
        let mut key: BCRYPT_KEY_HANDLE = std::ptr::null_mut();
        check(
            unsafe {
                BCryptImportKeyPair(
                    BCRYPT_ECDSA_P256_ALG_HANDLE,
                    std::ptr::null_mut(),
                    kind,
                    &mut key,
                    blob.as_ptr(),
                    blob.len() as u32,
                    0,
                )
            },
            "importing the key",
        )?;
        Ok(Key(key))
    }

    pub fn sha256(input: &[u8]) -> Result<[u8; 32], String> {
        let mut output = [0u8; 32];
        check(
            unsafe {
                BCryptHash(
                    BCRYPT_SHA256_ALG_HANDLE,
                    std::ptr::null(),
                    0,
                    input.as_ptr(),
                    input.len() as u32,
                    output.as_mut_ptr(),
                    32,
                )
            },
            "hashing",
        )?;
        Ok(output)
    }

    pub fn generate() -> Result<Vec<u8>, String> {
        let mut key: BCRYPT_KEY_HANDLE = std::ptr::null_mut();
        check(
            unsafe { BCryptGenerateKeyPair(BCRYPT_ECDSA_P256_ALG_HANDLE, &mut key, 256, 0) },
            "making a key",
        )?;
        let key = Key(key);
        check(
            unsafe { BCryptFinalizeKeyPair(key.0, 0) },
            "finishing the key",
        )?;
        let mut blob = vec![0u8; HEADER + 96];
        let mut written = 0u32;
        check(
            unsafe {
                BCryptExportKey(
                    key.0,
                    std::ptr::null_mut(),
                    BCRYPT_ECCPRIVATE_BLOB,
                    blob.as_mut_ptr(),
                    blob.len() as u32,
                    &mut written,
                    0,
                )
            },
            "exporting the key",
        )?;
        Ok(blob[HEADER..written as usize].to_vec())
    }

    pub fn sign(private: &[u8], hash: &[u8; 32]) -> Result<[u8; 64], String> {
        let key = import(
            BCRYPT_ECDSA_PRIVATE_P256_MAGIC,
            BCRYPT_ECCPRIVATE_BLOB,
            private,
        )?;
        let mut signature = [0u8; 64];
        let mut written = 0u32;
        check(
            unsafe {
                BCryptSignHash(
                    key.0,
                    std::ptr::null(),
                    hash.as_ptr(),
                    32,
                    signature.as_mut_ptr(),
                    64,
                    &mut written,
                    0,
                )
            },
            "signing",
        )?;
        Ok(signature)
    }

    pub fn verify(public: &[u8], hash: &[u8; 32], signature: &[u8]) -> Result<(), String> {
        let key = import(
            BCRYPT_ECDSA_PUBLIC_P256_MAGIC,
            BCRYPT_ECCPUBLIC_BLOB,
            public,
        )?;
        let status = unsafe {
            BCryptVerifySignature(
                key.0,
                std::ptr::null(),
                hash.as_ptr(),
                32,
                signature.as_ptr(),
                signature.len() as u32,
                0,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err("the signature does not match".into())
        }
    }
}

#[cfg(not(windows))]
mod cng {
    const NEEDS_WINDOWS: &str = "the build table is signed with Windows CNG";

    pub fn sha256(_input: &[u8]) -> Result<[u8; 32], String> {
        Err(NEEDS_WINDOWS.into())
    }
    pub fn generate() -> Result<Vec<u8>, String> {
        Err(NEEDS_WINDOWS.into())
    }
    pub fn sign(_private: &[u8], _hash: &[u8; 32]) -> Result<[u8; 64], String> {
        Err(NEEDS_WINDOWS.into())
    }
    pub fn verify(_public: &[u8], _hash: &[u8; 32], _signature: &[u8]) -> Result<(), String> {
        Err(NEEDS_WINDOWS.into())
    }
}

#[cfg(windows)]
mod http {
    use core::ffi::c_void;

    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
        WINHTTP_QUERY_STATUS_CODE,
    };

    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn opened(handle: *mut c_void, what: &str) -> Result<Handle, String> {
        if handle.is_null() {
            Err(format!("{what} failed"))
        } else {
            Ok(Handle(handle))
        }
    }

    /// Fetches an https address. The signature is what is trusted, but a table has no reason to
    /// travel in the clear.
    pub fn get(url: &str, timeout_ms: i32) -> Result<Vec<u8>, String> {
        let rest = url
            .strip_prefix("https://")
            .ok_or("the table address must be https")?;
        let (host, path) = rest
            .split_once('/')
            .ok_or("the table address has no path")?;
        unsafe {
            let session = opened(
                WinHttpOpen(
                    wide("Rivet").as_ptr(),
                    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                ),
                "starting the download",
            )?;
            WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
            let connection = opened(
                WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0),
                "connecting",
            )?;
            let request = opened(
                WinHttpOpenRequest(
                    connection.0,
                    wide("GET").as_ptr(),
                    wide(&format!("/{path}")).as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    WINHTTP_FLAG_SECURE,
                ),
                "making the request",
            )?;
            if WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) == 0
                || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0
            {
                return Err("the table server did not answer".into());
            }
            let mut status = 0u32;
            let mut size = 4u32;
            let mut index = 0u32;
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                (&raw mut status).cast(),
                &mut size,
                &mut index,
            );
            if status != 200 {
                return Err(format!("the table server answered {status}"));
            }
            let mut body = Vec::new();
            let mut chunk = vec![0u8; 256 * 1024];
            loop {
                let mut read = 0u32;
                if WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut read,
                ) == 0
                {
                    return Err("the download stopped".into());
                }
                if read == 0 {
                    return Ok(body);
                }
                body.extend_from_slice(&chunk[..read as usize]);
                if body.len() > super::LARGEST {
                    return Err("the answer is too large to be a build table".into());
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod http {
    pub fn get(_url: &str, _timeout_ms: i32) -> Result<Vec<u8>, String> {
        Err("the build table is downloaded with Windows WinHTTP".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("rivet-table-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn payload(sequence: u64) -> Vec<u8> {
        format!(r#"{{"schema":1,"sequence":{sequence},"rows":[]}}"#).into_bytes()
    }

    #[test]
    fn nfr6_a_signed_table_opens_with_its_key() {
        let (private, public) = generate_key().unwrap();
        let file = sign(&private, &payload(7)).unwrap();
        assert_eq!(open(&public, &file).unwrap(), payload(7));
        assert_eq!(parse(open(&public, &file).unwrap()).unwrap()["sequence"], 7);
    }

    #[test]
    fn nfr6_a_changed_byte_anywhere_is_refused() {
        let (private, public) = generate_key().unwrap();
        let file = sign(&private, &payload(7)).unwrap();
        for at in [
            0,
            MAGIC.len(),
            MAGIC.len() + 63,
            MAGIC.len() + SIGNATURE,
            file.len() - 1,
        ] {
            let mut changed = file.clone();
            changed[at] ^= 1;
            assert!(open(&public, &changed).is_err(), "byte {at}");
        }
        assert!(open(&public, &file[..MAGIC.len() + 10]).is_err());
    }

    #[test]
    fn nfr6_a_table_signed_with_another_key_is_refused() {
        let (private, _) = generate_key().unwrap();
        let (_, other) = generate_key().unwrap();
        let file = sign(&private, &payload(7)).unwrap();
        assert_eq!(
            open(&other, &file).unwrap_err(),
            "the signature does not match"
        );
    }

    #[test]
    fn nfr6_install_applies_a_good_file_and_refuses_a_bad_one_without_touching_the_applied() {
        let (private, public) = generate_key().unwrap();
        let store = store_path(&folder("install"));
        assert_eq!(read_store(&store, &public).unwrap(), None);
        let good = sign(&private, &payload(7)).unwrap();
        assert_eq!(install(&store, &public, &good), Ok(7));
        let mut forged = sign(&private, &payload(8)).unwrap();
        *forged.last_mut().unwrap() ^= 1;
        assert!(install(&store, &public, &forged).is_err());
        assert_eq!(std::fs::read(&store).unwrap(), good);
        assert_eq!(read_store(&store, &public).unwrap().unwrap()["sequence"], 7);
    }

    #[test]
    fn nfr6_install_refuses_an_older_table() {
        let (private, public) = generate_key().unwrap();
        let store = store_path(&folder("older"));
        install(&store, &public, &sign(&private, &payload(7)).unwrap()).unwrap();
        assert!(
            install(&store, &public, &sign(&private, &payload(6)).unwrap())
                .unwrap_err()
                .contains("older")
        );
        assert_eq!(
            install(&store, &public, &sign(&private, &payload(8)).unwrap()),
            Ok(8)
        );
    }

    #[test]
    fn nfr6_a_tampered_store_reads_as_an_error() {
        let (private, public) = generate_key().unwrap();
        let store = store_path(&folder("tampered"));
        install(&store, &public, &sign(&private, &payload(7)).unwrap()).unwrap();
        let mut file = std::fs::read(&store).unwrap();
        *file.last_mut().unwrap() ^= 1;
        std::fs::write(&store, &file).unwrap();
        assert!(read_store(&store, &public).is_err());
    }

    #[test]
    fn nfr6_publishing_keeps_published_rows_and_moves_the_sequence_forward() {
        let row = |browser: &str, version: &str, mark: u64| serde_json::json!({"frame": {"browser": browser, "version": version}, "build": {"mark": mark}});
        let published = serde_json::json!({"schema": 1, "sequence": 900, "symbols": {"old": "a"},
            "rows": [row("B", "1.0.0.9", 1), row("B", "1.0.0.10", 1)]});
        let table = serde_json::json!({"schema": 1, "sequence": 0, "symbols": {"new": "b"},
            "rows": [row("B", "1.0.0.10", 2), row("A", "2.0.0.1", 2)]});
        let merged = merge_for_signing(&table, Some(&published), 500).unwrap();
        assert_eq!(merged["sequence"], 901);
        assert_eq!(
            merge_for_signing(&table, Some(&published), 5000).unwrap()["sequence"],
            5000
        );
        let rows = merged["rows"].as_array().unwrap();
        let listed: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row["frame"]["version"].as_str().unwrap(),
                    row["build"]["mark"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(listed, [("2.0.0.1", 2), ("1.0.0.9", 1), ("1.0.0.10", 2)]);
        assert_eq!(merged["symbols"]["old"], "a");
        assert_eq!(merged["symbols"]["new"], "b");
    }

    #[test]
    fn nfr6_another_schema_is_refused() {
        assert!(parse(br#"{"schema":2,"sequence":1,"rows":[]}"#).is_err());
        assert!(parse(br#"{"schema":1,"rows":[]}"#).is_err());
    }

    #[test]
    fn nfr6_refresh_leaves_a_fresh_table_alone_and_refuses_plain_http() {
        let (private, public) = generate_key().unwrap();
        let store = store_path(&folder("refresh"));
        let hour = std::time::Duration::from_secs(3600);
        assert!(
            refresh(&store, &public, "http://example.invalid/t", hour, 100)
                .unwrap_err()
                .contains("https")
        );
        install(&store, &public, &sign(&private, &payload(7)).unwrap()).unwrap();
        assert_eq!(
            refresh(&store, &public, "http://example.invalid/t", hour, 100),
            Ok(None)
        );
    }
}
