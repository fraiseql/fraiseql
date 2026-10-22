//! #839 gate: every TOML example in the docs parses as the file it says it is.
//!
//! `docs/architecture/overview.md` shipped a production `fraiseql.toml` whose keys sat
//! in `[server]`/`[database]` grouping tables that `ServerConfig` does not have; serde
//! silently discarded every documented key and the example "worked" while configuring
//! nothing. The config structs now deny unknown fields, and this test closes the loop
//! on the docs side.
//!
//! Every toml-fenced block in `README.md` and `docs/` (history excepted) names its file
//! in its first non-empty line, a comment such as `# fraiseql.toml`, and parses as it:
//!
//! * `server.toml` deserializes into the server's [`ServerConfig`];
//! * `fraiseql.toml` deserializes into one of the compiler's two readers of that file:
//!   [`TomlSchema`] (the file as `fraiseql compile`'s input, TOML-first authoring) or
//!   [`TomlProjectConfig`] (the project config beside a `schema.json`). Two structs for one file
//!   name is the dialect split #1641 removes; until then either counts, and both deny unknown keys,
//!   so an invented key still fails;
//! * any other `*.toml` (`Cargo.toml`, `pyproject.toml`, ...) is valid TOML.
//!
//! A block that names no file fails: an unnamed example is one a reader cannot check,
//! and the two config dialects (D-9) make "which file is this?" a real question.
//!
//! Wired in `.dagger/main.go`'s test leg next to `config_coverage_manifest_test`,
//! with the full server feature set so every feature-gated section exists. The
//! `#![cfg]` guard below keeps a bare feature-less `cargo test` from false-failing —
//! the dagger line is the gate of record.

#![cfg(all(feature = "auth", feature = "observers", feature = "rest"))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code
#![allow(missing_docs)] // Reason: test code

use std::path::{Path, PathBuf};

use fraiseql_cli::config::{TomlProjectConfig, TomlSchema};
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

/// The file a block names in its first non-empty line (`# fraiseql.toml — ...`), if any.
fn named_file(block: &str) -> Option<&str> {
    let first = block.lines().find(|l| !l.trim().is_empty())?.trim_start();
    let rest = first.strip_prefix('#')?.trim_start();
    let end = rest.find(".toml")? + ".toml".len();
    let name = &rest[..end];
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
        .then_some(name)
}

/// Markdown files under `dir`, recursively, skipping the directories that record history.
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    const HISTORY: [&str; 3] = ["history", "audits", "migration"];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !HISTORY.iter().any(|h| path.ends_with(h)) {
                markdown_files(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

#[test]
fn every_documented_toml_example_parses_as_the_file_it_names() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let (mut server, mut compiler, mut other) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let shown = path.strip_prefix(&root).unwrap().display();
        for (line, block) in toml_blocks(&content) {
            let result = match named_file(&block) {
                None => Err("names no file: start the block with a comment such as \
                             `# fraiseql.toml` or `# server.toml`"
                    .to_string()),
                Some(name) if name.ends_with("server.toml") => {
                    server += 1;
                    toml::from_str::<ServerConfig>(&block).map(drop).map_err(|e| e.to_string())
                },
                Some(name) if name.ends_with("fraiseql.toml") => {
                    compiler += 1;
                    toml::from_str::<TomlSchema>(&block).map(drop).or_else(|schema| {
                        toml::from_str::<TomlProjectConfig>(&block).map(drop).map_err(|project| {
                            format!("as TomlSchema: {schema}; as TomlProjectConfig: {project}")
                        })
                    })
                },
                Some(_) => {
                    other += 1;
                    toml::from_str::<toml::Value>(&block).map(drop).map_err(|e| e.to_string())
                },
            };
            if let Err(e) = result {
                failures.push(format!("{shown}:{line}: {e}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented TOML examples that do not parse as the file they name \
         (fix the doc or the struct — never let them drift):\n{}",
        failures.join("\n")
    );

    // A silently shrinking corpus is how the gate stops gating: if a floor trips because
    // examples were removed, lower it consciously.
    assert!(server >= 6, "expected at least 6 server.toml examples, found {server}");
    assert!(compiler >= 6, "expected at least 6 fraiseql.toml examples, found {compiler}");
    assert!(other >= 1, "expected at least 1 example of another TOML file, found {other}");
}
