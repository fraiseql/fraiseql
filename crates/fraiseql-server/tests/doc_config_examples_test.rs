//! #839 gate: the operator docs' server-config TOML examples must parse.
//!
//! `docs/architecture/overview.md` shipped a production `fraiseql.toml` whose keys sat
//! in `[server]`/`[database]` grouping tables that `ServerConfig` does not have; serde
//! silently discarded every documented key and the example "worked" while configuring
//! nothing. `ServerConfig` now denies unknown fields, and this test closes the loop on
//! the docs side: every toml-fenced block in the operator-facing docs whose first
//! comment line names `server.toml` is deserialized into the real `ServerConfig`. A
//! doc example that drifts from the struct fails CI instead of lying to operators.
//!
//! Wired in `.dagger/main.go`'s test leg next to `config_coverage_manifest_test`,
//! with the full server feature set so every feature-gated section exists. The
//! `#![cfg]` guard below keeps a bare feature-less `cargo test` from false-failing —
//! the dagger line is the gate of record.

#![cfg(all(feature = "auth", feature = "observers", feature = "rest"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code
#![allow(missing_docs)] // Reason: test code

use std::path::{Path, PathBuf};

use fraiseql_server::ServerConfig;

/// Repo root, resolved from this crate's manifest dir.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Extract every toml-fenced code block from a markdown file, with its starting line.
fn toml_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;
    for (idx, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        match &mut current {
            None if trimmed.starts_with("```toml") => current = Some((idx + 1, Vec::new())),
            None => {},
            Some(_) if trimmed.starts_with("```") => {
                let (start, lines) = current.take().unwrap();
                blocks.push((start, lines.join("\n")));
            },
            Some((_, lines)) => lines.push(line),
        }
    }
    blocks
}

/// A block is a server-config example when its first non-empty line is a comment
/// naming `server.toml`.
fn is_server_config_block(block: &str) -> bool {
    block
        .lines()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.trim_start().starts_with('#') && l.contains("server.toml"))
}

#[test]
fn every_documented_server_config_example_parses_as_server_config() {
    let root = repo_root();
    let doc_dirs = [
        "docs/architecture",
        "docs/runbooks",
        "docs/operations",
        "docs/guides",
        "docs/adr",
    ];

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for dir in doc_dirs {
        let dir_path = root.join(dir);
        let entries = std::fs::read_dir(&dir_path);
        assert!(entries.is_ok(), "cannot read {}: {entries:?}", dir_path.display());
        let entries = entries.unwrap();
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap();
            for (line, block) in toml_blocks(&content) {
                if !is_server_config_block(&block) {
                    continue;
                }
                checked += 1;
                if let Err(e) = toml::from_str::<ServerConfig>(&block) {
                    failures.push(format!("{}:{line}: {e}", path.display()));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented server.toml examples that do not parse as ServerConfig \
         (fix the doc or the struct — never let them drift):\n{}",
        failures.join("\n")
    );

    // If this trips because marked examples were removed, either re-mark them
    // (`# server.toml` as the first comment line) or lower the floor consciously —
    // a silently shrinking corpus is how the gate stops gating.
    assert!(
        checked >= 6,
        "expected at least 6 marked server.toml examples across the docs, found {checked} — \
         did a doc lose its `# server.toml` marker?"
    );
}
