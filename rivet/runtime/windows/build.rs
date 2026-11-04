use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    // The start page loads Manrope from beside itself, so it matches the frame
    // without relying on an installed copy.
    for (source, name) in [
        ("start/start.html", "start.html"),
        ("fonts/Manrope[wght].ttf", "Manrope.ttf"),
    ] {
        let source = manifest.join(source);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(&source, profile.join(name)).expect("copy start page files beside Rivet binaries");
    }
}
