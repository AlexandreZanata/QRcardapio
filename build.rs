use std::fs;
use std::path::{Path, PathBuf};

const FNV_OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let static_root = manifest_dir.join("static");
    println!("cargo:rerun-if-changed={}", static_root.display());

    let mut files = Vec::new();
    for subdir in ["css", "js", "icons"] {
        let dir = static_root.join(subdir);
        if dir.is_dir() {
            collect_files(&static_root, &dir, &mut files);
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hash = FNV_OFFSET_BASIS;
    for (rel_path, full_path) in &files {
        println!("cargo:rerun-if-changed={}", full_path.display());
        hash_bytes(&mut hash, rel_path.as_bytes());
        hash_bytes(&mut hash, b"\0");
        let bytes = fs::read(full_path).expect("read static asset");
        hash_bytes(&mut hash, &bytes);
        hash_bytes(&mut hash, b"\n");
    }

    let full_hex = format!("{hash:032x}");
    println!(
        "cargo:rustc-env=QRCARDAPIO_ASSET_VERSION={}",
        &full_hex[..12]
    );
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read static directory")
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("relative static path")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn hash_bytes(state: &mut u128, bytes: &[u8]) {
    for &byte in bytes {
        *state ^= u128::from(byte);
        *state = state.wrapping_mul(FNV_PRIME);
    }
}
