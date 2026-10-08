//! `fraiseql federation sdl` prints exactly what `_service { sdl }` serves (#1427).
//!
//! Composing a supergraph needs each subgraph's SDL, and the only source was a running
//! server's `_service { sdl }`. The command reads the compiled artifact instead. Both
//! sides here are the real binary on the same `schema.compiled.json`: `fraiseql query`
//! executes `{ _service { sdl } }` through the executor against PostgreSQL (the arm
//! `/graphql` reaches), and `fraiseql federation sdl` prints the artifact's SDL. They
//! must agree byte for byte.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** safe (temporary files only; `_service` reads no table).
#![cfg(all(feature = "test-postgres", feature = "federation"))]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{path::Path, process::Command};

use tempfile::TempDir;

/// An SDK-shaped `schema.json` with a federation block, as the Python SDK emits it.
const FEDERATED_SCHEMA: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "total", "type": "Float", "nullable": false},
        {"name": "note", "type": "String", "nullable": true}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_orders",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": [],
  "federation": {
    "enabled": true,
    "service_name": "orders",
    "version": "v2",
    "entities": [
      {"name": "Order", "key_fields": ["id"]}
    ]
  }
}
"#;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Compile `schema_json` into `dir`, returning the artifact's path.
fn compile(dir: &TempDir, schema_json: &str) -> std::path::PathBuf {
    let input = dir.path().join("schema.json");
    std::fs::write(&input, schema_json).unwrap();
    let artifact = dir.path().join("schema.compiled.json");
    let out = cli().args(["compile", path(&input), "-o", path(&artifact)]).output().unwrap();
    assert!(
        out.status.success(),
        "compile failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    artifact
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn federation_sdl_prints_what_service_serves() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1427 federation sdl test: DATABASE_URL not set");
        return;
    };
    let dir = TempDir::new().unwrap();
    let artifact = compile(&dir, FEDERATED_SCHEMA);

    let served = cli()
        .args([
            "query",
            "{ _service { sdl } }",
            "-s",
            path(&artifact),
            "--database",
            &url,
        ])
        .output()
        .unwrap();
    assert!(
        served.status.success(),
        "_service query failed:\n{}",
        String::from_utf8_lossy(&served.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&served.stdout).unwrap();
    let served_sdl = response["data"]["_service"]["sdl"].as_str().unwrap().to_string();
    assert!(served_sdl.contains("@key"), "the served SDL is federated: {served_sdl}");

    let printed = cli().args(["federation", "sdl", path(&artifact)]).output().unwrap();
    assert!(
        printed.status.success(),
        "federation sdl failed:\n{}",
        String::from_utf8_lossy(&printed.stderr)
    );
    assert_eq!(
        String::from_utf8(printed.stdout).unwrap(),
        served_sdl,
        "`federation sdl` must print exactly what `_service {{ sdl }}` serves"
    );

    let file = dir.path().join("subgraph.graphql");
    let written = cli()
        .args(["federation", "sdl", path(&artifact), "-o", path(&file)])
        .output()
        .unwrap();
    assert!(written.status.success(), "{}", String::from_utf8_lossy(&written.stderr));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), served_sdl, "`-o` writes the same SDL");
}
