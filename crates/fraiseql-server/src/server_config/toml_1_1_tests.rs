//! The server's configuration file is parsed as TOML 1.1. These pin the syntax TOML 1.0
//! refused, so the difference from a TOML 1.0 reader, such as the Python SDK's loader, is
//! deliberate.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::ServerConfig;

fn loaded(toml: &str) -> ServerConfig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fraiseql-server.toml");
    std::fs::write(&path, toml).unwrap();
    ServerConfig::load_file(&path).unwrap()
}

#[test]
fn an_inline_table_may_end_with_a_comma() {
    let config = loaded("storage = { media = { backend = \"local\", path = \"/var/media\", }, }\n");
    assert_eq!(config.storage["media"].path.as_deref(), Some("/var/media"));
}

#[test]
fn an_inline_table_may_span_lines() {
    let config = loaded(
        "storage = {\n  media = {\n    backend = \"local\",\n    path = \"/var/media\"\n  }\n}\n",
    );
    assert_eq!(config.storage["media"].backend, "local");
}

#[test]
fn a_basic_string_may_hold_a_hex_escape() {
    let config =
        loaded("storage = { media = { backend = \"local\", path = \"/var/\\x6dedia\" } }\n");
    assert_eq!(config.storage["media"].path.as_deref(), Some("/var/media"));
}
