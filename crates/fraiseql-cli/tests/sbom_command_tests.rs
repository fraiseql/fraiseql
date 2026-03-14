#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for `fraiseql sbom`.
//!
//! The SBOM command reads Cargo.lock and produces a Software Bill of Materials.
//! No database or network required.
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none (reads Cargo.lock from workspace)
//! **Parallelism:** safe

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

// ── SBOM basic ────────────────────────────────────────────────────

/// `fraiseql sbom` exits 0 and produces non-empty output.
#[test]
fn sbom_exits_zero() {
    let out = cli().arg("sbom").output().unwrap();
    assert!(out.status.success(), "sbom must exit 0; status: {:?}", out.status);
    assert!(!out.stdout.is_empty(), "sbom output must not be empty");
}

/// Default output format is `CycloneDX` (JSON with `bomFormat: CycloneDX`).
#[test]
fn sbom_default_format_is_cyclonedx() {
    let out = cli().arg("sbom").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("sbom default output must be valid JSON: {e}\ngot: {stdout}"));
    assert_eq!(
        parsed["bomFormat"].as_str().unwrap_or(""),
        "CycloneDX",
        "default sbom format must be CycloneDX"
    );
}

/// `CycloneDX` output includes a `components` array.
#[test]
fn sbom_cyclonedx_has_components() {
    let out = cli().arg("sbom").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["components"].is_array(), "sbom output must contain a `components` array");
    assert!(
        !parsed["components"].as_array().unwrap().is_empty(),
        "components array must not be empty"
    );
}

/// `CycloneDX` output lists `fraiseql-core` as a component.
#[test]
fn sbom_includes_fraiseql_core() {
    let out = cli().arg("sbom").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fraiseql-core"), "sbom output must mention fraiseql-core");
}

/// `--format cyclonedx` is identical to the default.
#[test]
fn sbom_explicit_cyclonedx_format_works() {
    let out = cli().args(["sbom", "--format", "cyclonedx"]).output().unwrap();
    assert!(out.status.success(), "sbom --format cyclonedx must exit 0");
}

/// `--format spdx` exits 0 and produces SPDX output.
#[test]
fn sbom_spdx_format_exits_zero() {
    let out = cli().args(["sbom", "--format", "spdx"]).output().unwrap();
    assert!(out.status.success(), "sbom --format spdx must exit 0");
    // SPDX output contains the SPDXVersion header or spdxVersion JSON key
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("SPDX") || stdout.contains("spdx"),
        "SPDX output must contain SPDX identifiers; got: {stdout}"
    );
}

/// `--output <file>` writes to the given file rather than stdout.
#[test]
fn sbom_output_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let outfile = dir.path().join("sbom.json").to_string_lossy().to_string();

    let out = cli().args(["sbom", "--output", &outfile]).output().unwrap();
    assert!(out.status.success(), "sbom --output must exit 0");

    let contents = std::fs::read_to_string(&outfile)
        .unwrap_or_else(|e| panic!("sbom --output must write to file {outfile}: {e}"));
    assert!(!contents.is_empty(), "sbom output file must not be empty");
}

/// Unknown format string exits non-zero with a helpful error.
#[test]
fn sbom_unknown_format_exits_nonzero() {
    let out = cli().args(["sbom", "--format", "unknown_format"]).output().unwrap();
    assert!(!out.status.success(), "sbom --format unknown_format must exit non-zero");
}
