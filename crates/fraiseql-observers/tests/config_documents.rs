//! Every configuration document this crate ships parses into the struct it documents.
//!
//! `examples/*.toml` and the TOML blocks of `docs/configuration-examples.md` are what a
//! reader copies. #1451 made `[performance]` refuse keys it no longer reads, and the shipped
//! examples went on setting four of them, so the files the README tells a reader to start
//! from failed to load. Nothing parsed them. This does, for every file and every block, so a
//! document can no longer drift from `ObserverRuntimeConfig` without a red test.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::path::{Path, PathBuf};

use fraiseql_observers::config::{ObserverRuntimeConfig, RetryConfig};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Parse `text` as a whole runtime config and run every validator it carries.
fn load(text: &str, what: &str) {
    let config: ObserverRuntimeConfig =
        toml::from_str(text).unwrap_or_else(|e| panic!("{what} does not parse: {e}"));
    config.validate().unwrap_or_else(|e| panic!("{what} does not validate: {e}"));
    config.transport.validate().unwrap_or_else(|e| panic!("{what}: transport: {e}"));
    if let Some(redis) = &config.redis {
        redis.validate().unwrap_or_else(|e| panic!("{what}: redis: {e}"));
    }
}

/// The `toml` fenced blocks of a Markdown file, with the line each starts on.
fn toml_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in markdown.lines().enumerate() {
        match (&mut current, line.trim_start()) {
            (None, l) if l.starts_with("```toml") => current = Some((index + 1, String::new())),
            (Some(_), l) if l.starts_with("```") => blocks.push(current.take().unwrap()),
            (Some((_, body)), _) => {
                body.push_str(line);
                body.push('\n');
            },
            (None, _) => {},
        }
    }
    assert!(current.is_none(), "unterminated ```toml block");
    blocks
}

#[test]
fn every_shipped_example_config_loads() {
    let dir = crate_dir().join("examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no example configs under {}", dir.display());
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        load(&std::fs::read_to_string(file).unwrap(), &name);
    }
}

#[test]
fn every_toml_block_in_the_configuration_guide_loads() {
    let path: &Path = &crate_dir().join("docs/configuration-examples.md");
    let blocks = toml_blocks(&std::fs::read_to_string(path).unwrap());
    let mut checked = 0;
    for (line, body) in &blocks {
        let what = format!("configuration-examples.md block at line {line}");
        let value: toml::Table = toml::from_str(body).unwrap_or_else(|e| panic!("{what}: {e}"));
        // A Cargo.toml snippet, not a runtime config.
        if value.contains_key("dependencies") {
            continue;
        }
        // A fragment showing one observer's `retry` table on its own.
        let retry_only = value.keys().all(|k| k == "observers")
            && value.get("observers").and_then(toml::Value::as_table).is_some_and(|observers| {
                observers
                    .values()
                    .all(|o| o.as_table().is_some_and(|t| t.keys().all(|k| k == "retry")))
            });
        if retry_only {
            for observer in value["observers"].as_table().unwrap().values() {
                let retry = observer["retry"].clone();
                retry.try_into::<RetryConfig>().unwrap_or_else(|e| panic!("{what}: retry: {e}"));
            }
        } else {
            load(body, &what);
        }
        checked += 1;
    }
    assert!(checked >= 5, "only {checked} runtime-config blocks found; the guide has more");
}

/// The four runtime keys had no reader anywhere (the server has its own
/// `[observers.runtime]`); they are refused, like #1451's `[performance]` keys, so a config
/// that sets one learns at load time that nothing honours it.
#[test]
fn the_unread_runtime_keys_are_refused() {
    for key in [
        "channel_capacity = 1000",
        "max_concurrency = 50",
        "backlog_alert_threshold = 500",
        "shutdown_timeout = \"30s\"",
    ] {
        let err = toml::from_str::<ObserverRuntimeConfig>(key).unwrap_err().to_string();
        assert!(err.contains("unknown field"), "{key} must be refused: {err}");
    }
}
