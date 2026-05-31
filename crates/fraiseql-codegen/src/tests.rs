//! Tests for the top-level `fraiseql-codegen` crate.

#![allow(clippy::panic)] // Reason: tests follow workspace convention.

use std::path::PathBuf;

use crate::Generated;

#[test]
fn generated_is_an_ordered_path_to_content_map() {
    let mut generated = Generated::new();
    generated.insert(PathBuf::from("b.ts"), "second".to_string());
    generated.insert(PathBuf::from("a.ts"), "first".to_string());

    // BTreeMap iteration order is sorted by key, giving deterministic output.
    let paths: Vec<_> = generated.keys().map(|p| p.to_string_lossy().into_owned()).collect();
    assert_eq!(paths, vec!["a.ts", "b.ts"]);
}
