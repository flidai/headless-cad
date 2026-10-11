//! Fingerprint kernel implementation and declared compilation inputs without
//! timestamps, checkout paths, Git state, or unrelated process environment.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn field(hash: &mut blake3::Hasher, name: &str, value: &[u8]) {
    hash.update(&(name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
    hash.update(&(value.len() as u64).to_le_bytes());
    hash.update(value);
}

fn rust_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read kernel source directory") {
        let entry = entry.expect("read kernel source entry");
        let path = entry.path();
        if entry.file_type().expect("read source file type").is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let mut hash = blake3::Hasher::new();
    field(&mut hash, "domain", b"headless-cad/kernel-build/v1");

    let mut sources = vec![root.join("Cargo.toml"), root.join("build.rs")];
    rust_sources(&root.join("src"), &mut sources);
    sources.sort();
    // Directory tracking also catches added or removed Rust source files.
    println!("cargo:rerun-if-changed=src");
    for path in sources {
        let relative = path.strip_prefix(&root).expect("kernel-relative path");
        let label = relative.to_string_lossy().replace('\\', "/");
        println!("cargo:rerun-if-changed={label}");
        field(
            &mut hash,
            &label,
            &fs::read(&path).expect("read kernel implementation"),
        );
    }

    // Workspace dependency declarations and resolved versions may change without
    // touching this crate. Labels are stable across checkout locations.
    for ancestor in root.ancestors().skip(1) {
        let manifest = ancestor.join("Cargo.toml");
        let Ok(contents) = fs::read_to_string(&manifest) else {
            continue;
        };
        if !contents.lines().any(|line| line.trim() == "[workspace]") {
            continue;
        }
        for filename in ["Cargo.toml", "Cargo.lock"] {
            let path = ancestor.join(filename);
            println!("cargo:rerun-if-changed={}", path.display());
            if let Ok(contents) = fs::read(&path) {
                field(&mut hash, &format!("workspace/{filename}"), &contents);
            }
        }
        break;
    }

    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var_os("RUSTC").expect("Cargo compiler executable");
    let version = Command::new(rustc)
        .arg("-vV")
        .output()
        .expect("read rustc version");
    assert!(version.status.success(), "rustc -vV failed");
    field(&mut hash, "rustc-version", &version.stdout);

    // Only Cargo's declared compile configuration is read; no arbitrary user
    // environment values, flags containing local paths, or secrets are hashed.
    for name in [
        "TARGET",
        "HOST",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ENV",
        "CARGO_CFG_TARGET_ABI",
        "CARGO_CFG_TARGET_VENDOR",
        "CARGO_CFG_TARGET_ENDIAN",
        "CARGO_CFG_TARGET_POINTER_WIDTH",
        "CARGO_CFG_TARGET_FAMILY",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_CFG_PANIC",
        "CARGO_CFG_DEBUG_ASSERTIONS",
        "CARGO_CFG_OVERFLOW_CHECKS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        // Flag cfgs have an empty value when present; distinguish them from
        // absence (for example cfg(debug_assertions)).
        let mut value = vec![0];
        if let Ok(configured) = env::var(name) {
            value[0] = 1;
            value.extend_from_slice(configured.as_bytes());
        }
        field(&mut hash, name, &value);
    }
    let mut features: Vec<_> = env::vars()
        .filter(|(name, _)| name.starts_with("CARGO_FEATURE_"))
        .collect();
    features.sort();
    for (name, value) in features {
        println!("cargo:rerun-if-env-changed={name}");
        field(&mut hash, &name, value.as_bytes());
    }
    println!(
        "cargo:rustc-env=CAD_KERNEL_BUILD_ID={}",
        hash.finalize().to_hex()
    );
}
