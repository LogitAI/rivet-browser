use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    for (source, name) in [
        ("../runtime/windows/start/start.html", "start.html"),
        ("../runtime/windows/fonts/Manrope[wght].ttf", "Manrope.ttf"),
    ] {
        let source = manifest.join(source);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(&source, profile.join(name)).expect("copy start page files beside Rivet binaries");
    }
}
