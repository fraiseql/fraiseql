//! `fraiseql validate facts` against a real database, from an authoring `schema.json`.
//!
//! An authoring schema declares `fact_tables` as an array. The command used to read it
//! as a map, find none, print "No fact tables declared" and exit 0. A declared fact
//! table that the database does not have must fail the command, naming the table.
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL` (self-skips without it; the integration leg sets it)

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, process::Command};

/// A fact table name no fixture creates.
const ABSENT: &str = "tf_validate_facts_absent";

#[test]
fn a_declared_fact_table_absent_from_the_database_fails() {
    // The workspace sweep compiles this with `--all-features` and no database; the
    // integration leg that wires it sets DATABASE_URL.
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let schema = format!(
        r#"{{"fact_tables": [{{"table_name": "{ABSENT}", "measures": [],
            "dimensions": {{"name": "data", "paths": []}}, "denormalized_filters": []}}]}}"#
    );
    let mut file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(schema.as_bytes()).unwrap();
    file.flush().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "validate",
            "facts",
            "--schema",
            file.path().to_str().unwrap(),
            "--database",
            &url,
        ])
        .output()
        .expect("spawn fraiseql-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!output.status.success(), "a missing declared fact table must fail:\n{text}");
    assert!(
        text.contains(ABSENT) && text.contains("does not exist in database"),
        "the failure must name the missing table, not a connection problem:\n{text}"
    );
}
