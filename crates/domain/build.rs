use sha2::{Digest, Sha256};
use std::{env, fs, path::{Path, PathBuf}};

fn visit(root: &Path, relative: &str, files: &mut Vec<String>) {
    if relative.split('/').any(|part| part == "__pycache__") || relative.ends_with(".pyc") || relative.ends_with(".pyo") { return; }
    let absolute = root.join(relative);
    assert!(!fs::symlink_metadata(&absolute).expect("release input metadata").file_type().is_symlink(), "release input must not be a symlink");
    println!("cargo:rerun-if-changed={}", absolute.display());
    if absolute.is_dir() {
        for item in fs::read_dir(&absolute).expect("release input directory") {
            let item = item.expect("release input");
            visit(root, &format!("{relative}/{}", item.file_name().to_str().unwrap()), files);
        }
    } else {
        assert!(absolute.is_file(), "missing release input: {relative}");
        files.push(relative.into());
    }
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(root.join("release.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"].as_str().unwrap(), env::var("CARGO_PKG_VERSION").unwrap());
    let inputs: Vec<String> = serde_json::from_slice(&fs::read(root.join("release-inputs.json")).unwrap()).unwrap();
    let mut files = Vec::new();
    for input in inputs { visit(&root, &input, &mut files); }
    files.sort(); files.dedup();
    let mut hash = Sha256::new();
    hash.update(b"braintrade-source-v1\0");
    for file in files {
        // Match source archives and Git checkouts with LF or CRLF.
        let text = fs::read_to_string(root.join(&file)).expect("UTF-8 release input").replace("\r\n", "\n");
        hash.update((file.len() as u64).to_be_bytes()); hash.update(file.as_bytes());
        hash.update((text.len() as u64).to_be_bytes()); hash.update(text.as_bytes());
    }
    println!("cargo:rustc-env=BRAINTRADE_BUILD_ID={:x}", hash.finalize());
    for (key, field) in [("BRAINTRADE_RELEASE_NAME", "name"), ("BRAINTRADE_RELEASE_MARKER", "marker"),
                         ("BRAINTRADE_RELEASE_CHANNEL", "channel")] {
        println!("cargo:rustc-env={key}={}", manifest[field].as_str().unwrap());
    }
    println!("cargo:rustc-env=BRAINTRADE_CORE_PROTOCOL={}", manifest["core_protocol"]);
}
