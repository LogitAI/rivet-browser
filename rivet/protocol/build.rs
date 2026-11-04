use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn main() {
    let schema = Path::new("control.proto");
    println!("cargo:rerun-if-changed={}", schema.display());

    let bytes = fs::read(schema).expect("read control protocol definition");
    let hash = format!("{:x}", Sha256::digest(bytes));
    println!("cargo:rustc-env=RIVET_CONTROL_SCHEMA_HASH={hash}");

    let protoc = protoc_bin_vendored::protoc_bin_path().expect("find vendored protoc");
    std::env::set_var("PROTOC", protoc);
    prost_build::Config::new()
        .compile_protos(&[schema], &[Path::new(".")])
        .expect("generate Rivet protocol Rust types");
}
