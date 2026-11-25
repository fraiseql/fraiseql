//! The shared-module directory, checked at compile time.
//!
//! Two facts are checked when `module_dir` exists, so the compiler is looking at the
//! real layout (the trade [`super::functions`] makes for module presence):
//!
//! - a declared `shared_dir` passes the engine's own layout rule, [`check_shared_dir`]: a
//!   directory, and not one holding function files;
//! - a `deno.json` in `module_dir`, there for editors and `deno check`, maps `@shared/` to that
//!   same directory. The engine resolves `@shared/` from `shared_dir` and never reads `deno.json`,
//!   so the two can drift apart silently: an editor would then type-check imports against one
//!   directory while the server loads another.

use std::path::{Path, PathBuf};

use fraiseql_functions::{FunctionsConfig, modules::check_shared_dir};
use serde_json::Value;

/// The editor configuration file the compiler cross-checks.
const DENO_JSON: &str = "deno.json";

/// The import-map prefix shared modules are imported under.
const SHARED_PREFIX: &str = "@shared/";

/// One line per problem with the shared directory or its `deno.json` mapping.
pub(super) fn failures(functions: &FunctionsConfig) -> Vec<String> {
    if !functions.module_dir.is_dir() {
        return Vec::new();
    }
    let shared = match functions
        .shared_dir
        .as_deref()
        .map(|shared_dir| check_shared_dir(&functions.module_dir, shared_dir))
    {
        Some(Err(error)) => return vec![format!("  {error}")],
        Some(Ok(canonical)) => Some(canonical),
        None => None,
    };
    deno_json_drift(&functions.module_dir, functions.shared_dir.as_deref(), shared.as_deref())
        .into_iter()
        .collect()
}

/// A `deno.json` in `module_dir` whose `@shared/` entry disagrees with `shared_dir`.
fn deno_json_drift(
    module_dir: &Path,
    shared_dir: Option<&Path>,
    canonical_shared: Option<&Path>,
) -> Option<String> {
    let path = module_dir.join(DENO_JSON);
    let text = std::fs::read_to_string(&path).ok()?;
    let config: Value = match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(error) => {
            return Some(format!(
                "  {}: not valid JSON ({error}). The compiler reads its `imports` entry for \
                 `{SHARED_PREFIX}` to check it names `[functions] shared_dir`",
                path.display()
            ));
        },
    };
    let entry = config.get("imports").and_then(|imports| imports.get(SHARED_PREFIX));
    match (shared_dir, canonical_shared, entry) {
        (None, _, None) => None,
        (None, _, Some(entry)) => Some(format!(
            "  {}: maps `{SHARED_PREFIX}` to {entry}, but `[functions] shared_dir` is not set, so \
             the server refuses every import an editor resolves through it. Set \
             `[functions] shared_dir` to that directory, or remove the entry",
            path.display()
        )),
        (Some(shared_dir), _, None) => Some(format!(
            "  {}: has no `imports` entry for `{SHARED_PREFIX}`, so editors and `deno check` \
             cannot resolve the imports the server loads from `[functions] shared_dir` ({}). \
             Add `\"imports\": {{ \"{SHARED_PREFIX}\": \"<path to {}>/\" }}`",
            path.display(),
            shared_dir.display(),
            shared_dir.display()
        )),
        (Some(shared_dir), Some(canonical_shared), Some(entry)) => {
            let target = entry_target(module_dir, entry);
            if target.as_deref() == Some(canonical_shared) {
                None
            } else {
                Some(format!(
                    "  {}: maps `{SHARED_PREFIX}` to {entry}{}, but `[functions] shared_dir` is {}. \
                     The server resolves `{SHARED_PREFIX}` from shared_dir and never reads {DENO_JSON}, \
                     so both must name the same directory",
                    path.display(),
                    target.map_or_else(
                        || " (not a directory relative to module_dir)".to_string(),
                        |target| format!(" ({})", target.display())
                    ),
                    shared_dir.display()
                ))
            }
        },
        // `shared_dir` is set but failed its layout check, which is reported instead.
        (Some(_), None, Some(_)) => None,
    }
}

/// The directory an `@shared/` entry names, canonical, relative to `module_dir`.
///
/// An import-map prefix entry maps to a value ending in `/`, as for Deno; a value
/// without it, a non-string, or a URL (which is no directory under `module_dir`) names
/// no directory here.
fn entry_target(module_dir: &Path, entry: &Value) -> Option<PathBuf> {
    let entry = entry.as_str()?;
    if !entry.ends_with('/') {
        return None;
    }
    let target = module_dir.join(entry).canonicalize().ok()?;
    target.is_dir().then_some(target)
}
