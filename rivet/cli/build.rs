use std::{env, fs, path::PathBuf};

fn main() {
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../runtime/windows/start/start.html");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    fs::copy(source, profile.join("start.html")).expect("copy start page beside Rivet binaries");
}
