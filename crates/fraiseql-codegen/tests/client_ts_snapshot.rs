//! Snapshot test: the `TypeScript` client generated from the reference fixture
//! must match the committed `tests/fixtures/tutorial-expected/` tree byte-for-byte.
//!
//! The expected tree is real, `tsc --strict`-checked `TypeScript` (see
//! `tests/fixtures/tsconfig.json`). To regenerate it after an intentional change:
//!
//! ```sh
//! FRAISEQL_BLESS=1 cargo test -p fraiseql-codegen --test client_ts_snapshot
//! ```
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fraiseql_codegen::client::typescript;
use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

fn expected_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tutorial-expected")
}

#[test]
fn tutorial_schema_matches_reference() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();
    let generated = typescript::generate(&schema).unwrap();
    let dir = expected_dir();

    if std::env::var_os("FRAISEQL_BLESS").is_some() {
        bless(&dir, &generated);
        return;
    }

    let mut problems = Vec::new();
    for (rel, content) in &generated {
        match std::fs::read_to_string(dir.join(rel)) {
            Ok(expected) if &expected == content => {},
            Ok(_) => problems.push(format!("  {} differs", rel.display())),
            Err(_) => problems.push(format!("  {} missing from expected tree", rel.display())),
        }
    }

    // Catch expected files the generator no longer emits.
    let emitted: BTreeSet<_> = generated.keys().map(|p| p.to_string_lossy().into_owned()).collect();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_ts = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ts"));
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_ts && !emitted.contains(&name) {
                problems.push(format!("  {name} is stale (no longer generated)"));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "generated client diverged from tests/fixtures/tutorial-expected\n\
         (run `FRAISEQL_BLESS=1 cargo test -p fraiseql-codegen --test client_ts_snapshot` to update):\n{}",
        problems.join("\n"),
    );
}

fn bless(dir: &Path, generated: &fraiseql_codegen::Generated) {
    if dir.exists() {
        std::fs::remove_dir_all(dir).unwrap();
    }
    for (rel, content) in generated {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }
}
