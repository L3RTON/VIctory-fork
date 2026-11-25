//! The fingerprint of the shader translator for the shader cache
//! (`src/shader_cache.rs`): a hash of every source file of `victory-shader`
//! and of the cache's own encoding, so that a rebuild that leaves the
//! translator alone keeps its translations (a hash of the whole executable
//! threw them away at every rebuild: some 26,000 shaders, minutes of
//! translating again before the title could run).

use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "toml") {
            out.push(path);
        }
    }
}

fn main() {
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let shader = here.join("../victory-shader");
    let mut sources = Vec::new();
    files(&shader.join("src"), &mut sources);
    sources.push(shader.join("Cargo.toml"));
    sources.push(here.join("src/shader_cache.rs"));
    // In a fixed order, by path relative to the crates folder.
    let base = here.join("..");
    let mut named: Vec<(String, PathBuf)> = sources
        .into_iter()
        .map(|p| {
            let name = p
                .strip_prefix(&base)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (name, p)
        })
        .collect();
    named.sort();
    let mut hasher = blake3::Hasher::new();
    for (name, path) in &named {
        let bytes = std::fs::read(path).unwrap_or_default();
        hasher.update(name.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    // The compiler too: another one may compile the same code otherwise.
    if let Ok(rustc) = std::env::var("RUSTC")
        && let Ok(out) = std::process::Command::new(rustc).arg("-V").output()
    {
        hasher.update(&out.stdout);
    }
    // Not the profile: a release and a diagnostics build translate alike
    // (the one debugging switch of the translator, the loop limit, is in
    // the key at run time), so they share the cache.
    println!(
        "cargo:rustc-env=VICTORY_TRANSLATOR_SOURCES={}",
        hasher.finalize().to_hex()
    );
    println!("cargo:rerun-if-changed=../victory-shader/src");
    println!("cargo:rerun-if-changed=../victory-shader/Cargo.toml");
    println!("cargo:rerun-if-changed=src/shader_cache.rs");
}
