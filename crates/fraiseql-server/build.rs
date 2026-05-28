//! Build script for fraiseql-server.
//!
//! Runs the Studio SPA build (`TypeScript` → esbuild → `dist/app.js`) before
//! the crate compiles so that `rust-embed` can embed the resulting assets.
//!
//! All build artifacts (npm `node_modules`, esbuild output) are written to
//! `$OUT_DIR/studio/` so the source tree is never modified — this keeps
//! `cargo publish`'s source-modification check happy.
//!
//! Requires Node.js and npm on the build machine. The build is skipped
//! gracefully when `node` is not found, allowing `cargo check` to work
//! without a Node environment.

#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)] // Reason: build script
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let studio_src = Path::new(&manifest_dir).join("studio");
    let build_dir = Path::new(&out_dir).join("studio");
    let dist_dir = build_dir.join("dist");

    // Tell cargo to re-run if the studio source changes.
    println!("cargo:rerun-if-changed=studio/src/app.ts");
    println!("cargo:rerun-if-changed=studio/package.json");
    println!("cargo:rerun-if-changed=studio/package-lock.json");
    println!("cargo:rerun-if-changed=studio/tsconfig.json");

    // Stage the studio package into OUT_DIR so npm/esbuild only touch the
    // build directory, not the source tree.
    if let Err(e) = stage_studio(&studio_src, &build_dir) {
        eprintln!("cargo:warning=failed to stage studio sources: {e}");
        emit_fallback_dist(&dist_dir);
        println!("cargo:rustc-env=FRAISEQL_STUDIO_DIST={}", dist_dir.display());
        return;
    }

    // Run `npm install` if node_modules is absent in the staging dir.
    let node_modules = build_dir.join("node_modules");
    if !node_modules.exists() {
        let status = Command::new("npm")
            .args(["install", "--prefer-offline", "--no-audit", "--no-fund"])
            .current_dir(&build_dir)
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

    // Run `npm run build` in the staging dir.
    let status = Command::new("npm").args(["run", "build"]).current_dir(&build_dir).status();

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

/// Copy `studio/{package.json, package-lock.json, tsconfig.json, src/}` from
/// the source tree into the staging build directory.
fn stage_studio(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for file in ["package.json", "package-lock.json", "tsconfig.json"] {
        let from = src.join(file);
        if from.exists() {
            fs::copy(&from, dst.join(file))?;
        }
    }
    let src_dir = src.join("src");
    if src_dir.exists() {
        copy_dir_all(&src_dir, &dst.join("src"))?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
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
