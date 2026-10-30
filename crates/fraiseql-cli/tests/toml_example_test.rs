//! `fraiseql.toml.example`, the configuration reference the getting-started guide links,
//! loads as the project file it tells the reader to copy it to (#1830).
//!
//! A setting the template shows but leaves off is written `#key = value` or `#[table]`, with
//! no space after the `#`, so it can be told from prose (`# text`). The template is loaded
//! twice: as shipped, and with every such setting turned on, so a commented example cannot
//! drift from the loader either.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::path::{Path, PathBuf};

use fraiseql_cli::config::TomlProjectConfig;

/// Settings the template leaves off. Fewer means the second load checks less.
const SETTINGS_LEFT_OFF: usize = 28;

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fraiseql.toml.example")
}

/// Loads and validates `path` the way `fraiseql compile` and `fraiseql run` do.
fn load(path: &Path) -> anyhow::Result<TomlProjectConfig> {
    let config = TomlProjectConfig::from_file(path.to_str().unwrap())?;
    config.validate()?;
    Ok(config)
}

/// The template with every `#key = value` and `#[table]` line turned on.
fn with_every_setting_on(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| match line.strip_prefix('#') {
            Some(rest) if rest.starts_with('[') || rest.split_once('=').is_some_and(is_key) => rest,
            _ => line,
        })
        .collect();
    lines.join("\n") + "\n"
}

fn is_key((key, _): (&str, &str)) -> bool {
    let key = key.trim_end();
    key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[test]
fn the_configuration_template_loads_as_the_project_file() {
    load(&template()).unwrap_or_else(|e| panic!("fraiseql.toml.example: {e:#}"));
}

#[test]
fn every_setting_the_template_leaves_off_loads_when_turned_on() {
    let shipped = std::fs::read_to_string(template()).unwrap();
    let text = with_every_setting_on(&shipped);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fraiseql.toml");
    std::fs::write(&path, &text).unwrap();

    load(&path).unwrap_or_else(|e| panic!("with every setting turned on: {e:#}\n\n{text}"));
    // Counted on what was loaded, so a rewrite that turns nothing on cannot pass.
    let turned_on = shipped.lines().zip(text.lines()).filter(|(a, b)| a != b).count();
    assert_eq!(
        turned_on, SETTINGS_LEFT_OFF,
        "fewer means this test checks less; more, re-count"
    );
}
