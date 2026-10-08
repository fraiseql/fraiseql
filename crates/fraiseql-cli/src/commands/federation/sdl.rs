//! `fraiseql federation sdl`: a subgraph's federation SDL, from its compiled schema (#1427).
//!
//! Composing a supergraph needs each subgraph's SDL. A running server serves it as
//! `_service { sdl }`; this prints the same text from `schema.compiled.json`, so a
//! composition gate needs neither a booted server nor a principal allowed to call it.

use std::path::Path;

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;

/// The SDL the server would serve as `_service { sdl }` from the compiled schema at
/// `schema_path`: the same function renders both.
///
/// # Errors
///
/// The file cannot be read or is not a compiled schema, or its `[federation]` section is
/// absent or disabled.
pub fn render(schema_path: &Path) -> Result<String> {
    let json = std::fs::read_to_string(schema_path)
        .with_context(|| format!("failed to read {}", schema_path.display()))?;
    let schema = CompiledSchema::from_json(&json, false)
        .with_context(|| format!("{} is not a compiled schema", schema_path.display()))?;
    schema
        .federation_service_sdl()
        .with_context(|| format!("{} is not a federated subgraph", schema_path.display()))
}

/// Print the SDL to stdout, exactly as served, or write it to `output`.
///
/// # Errors
///
/// As [`render`], or `output` cannot be written.
pub fn run(schema_path: &Path, output: Option<&Path>) -> Result<()> {
    let sdl = render(schema_path)?;
    match output {
        Some(path) => {
            std::fs::write(path, sdl).with_context(|| format!("failed to write {}", path.display()))
        },
        None => {
            use std::io::Write as _;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(sdl.as_bytes()).context("failed to write the SDL to stdout")?;
            stdout.flush().context("failed to write the SDL to stdout")
        },
    }
}
