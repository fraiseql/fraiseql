//! Build script for CPU feature detection and optimization
//!
//! This script detects available CPU features and sets up build configuration
//! for maximum performance with SIMD optimizations.
//!
//! NOTE: `cargo test` does not work directly for this crate because it uses
//! PyO3 with the `extension-module` feature, which prevents linking to libpython.
//! Tests should be run via pytest after `maturin develop`.

use std::process::Command;

fn main() {
    // Detect target architecture
    let target = std::env::var("TARGET").unwrap();

    println!("cargo:rerun-if-env-changed=TARGET");

    if target.contains("x86_64") {
        // Enable SIMD feature flag
        println!("cargo:rustc-cfg=feature=\"simd\"");

        // Detect CPU features for informational purposes
        if let Ok(cpuinfo) = Command::new("cat").arg("/proc/cpuinfo").output() {
            let cpuinfo_str = String::from_utf8_lossy(&cpuinfo.stdout);

            // Set custom cfgs for conditional compilation
            if cpuinfo_str.contains("avx2") {
                println!("cargo:rustc-cfg=has_avx2");
            }
            if cpuinfo_str.contains("avx512") {
                println!("cargo:rustc-cfg=has_avx512");
            }
            if cpuinfo_str.contains("sse4_2") {
                println!("cargo:rustc-cfg=has_sse42");
            }
        } else {
            // Fallback: assume modern x86_64 has AVX2
            println!("cargo:rustc-cfg=has_avx2");
            println!("cargo:rustc-cfg=has_sse42");
        }
    } else if target.contains("aarch64") {
        // ARM64 SIMD (NEON) support
        println!("cargo:rustc-cfg=feature=\"simd\"");
        println!("cargo:rustc-cfg=has_neon");
    }

    // Print optimization info
    println!("cargo:warning=FraiseQL: Building with SIMD optimizations enabled");
    println!("cargo:warning=FraiseQL: Target architecture: {}", target);

    // Rebuild if CPU features change (though this is rare)
    println!("cargo:rerun-if-changed=/proc/cpuinfo");
}
