//! Build script for fraiseql-server.
//!
//! Runs the Studio SPA build (`TypeScript` → esbuild → `dist/app.js`) before
//! the crate compiles so that `rust-embed` can embed the resulting assets.
//!
//! Requires Node.js and npm on the build machine. The build is skipped
//! gracefully when `node` is not found, allowing `cargo check` to work
//! without a Node environment.

#![allow(clippy::panic)] // Reason: test code, panics acceptable
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let studio_dir = Path::new(&manifest_dir).join("studio");
    let dist_dir = studio_dir.join("dist");

    // Tell cargo to re-run if the studio source changes.
    println!("cargo:rerun-if-changed=studio/src/app.ts");
    println!("cargo:rerun-if-changed=studio/package.json");
    println!("cargo:rerun-if-changed=studio/tsconfig.json");

    // Run `npm install` if node_modules is absent.
    let node_modules = studio_dir.join("node_modules");
    if !node_modules.exists() {
        let status = Command::new("npm")
            .args(["install", "--prefer-offline"])
            .current_dir(&studio_dir)
            .status();

        match status {
            Ok(s) if s.success() => {},
            Ok(s) => panic!("npm install failed with status {s}"),
            Err(e) => {
                // Node not available — skip studio build.
                eprintln!("cargo:warning=npm not found ({e}), skipping Studio SPA build");
                emit_fallback_dist(&dist_dir);
                println!("cargo:rustc-env=FRAISEQL_STUDIO_DIST={}", dist_dir.display());
                return;
            },
        }
    }

    // Run `npm run build`.
    let status = Command::new("npm").args(["run", "build"]).current_dir(&studio_dir).status();

    match status {
        Ok(s) if s.success() => {},
        Ok(s) => panic!("npm run build failed with status {s}"),
        Err(e) => {
            eprintln!("cargo:warning=npm not found ({e}), skipping Studio SPA build");
            emit_fallback_dist(&dist_dir);
        },
    }

    // Export the dist path so rust-embed can locate it.
    println!("cargo:rustc-env=FRAISEQL_STUDIO_DIST={}", dist_dir.display());
}

/// Write a minimal fallback `dist/app.js` when Node.js is unavailable.
///
/// This keeps the Rust crate compilable in environments without Node
/// (e.g. pure-Rust CI jobs or cross-compilation targets).
fn emit_fallback_dist(dist_dir: &PathBuf) {
    fs::create_dir_all(dist_dir).expect("failed to create studio/dist/");
    let js_path = dist_dir.join("app.js");
    if !js_path.exists() {
        fs::write(&js_path, b"// FraiseQL Studio placeholder\n")
            .expect("failed to write fallback app.js");
    }
    let css_path = dist_dir.join("app.css");
    if !css_path.exists() {
        fs::write(&css_path, b"/* FraiseQL Studio placeholder */\n")
            .expect("failed to write fallback app.css");
    }
}
