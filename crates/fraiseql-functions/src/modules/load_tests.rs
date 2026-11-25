#![cfg(feature = "runtime-deno")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{collections::BTreeMap, path::Path};

use tempfile::TempDir;

use super::{LoadError, load_function_module};
use crate::{
    modules::{hash::function_hash, resolve::resolve_specifier},
    types::{FunctionsConfig, RuntimeType},
};

const HELPER: &str = "export const double = (n: number): number => n * 2;\n";
const FUNCTION: &str = "import { double } from \"@shared/math.ts\";\n\
                        export default (e: { n: number }) => double(e.n);\n";

/// `<root>/functions/` with `files` written under it, and the config pointing there.
fn project(files: &[(&str, &str)], shared_dir: Option<&str>) -> (TempDir, FunctionsConfig) {
    let root = TempDir::new().unwrap();
    let module_dir = root.path().join("functions");
    for (path, text) in files {
        let path = module_dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(&module_dir).unwrap();
    let config = FunctionsConfig {
        shared_dir: shared_dir.map(|dir| module_dir.join(dir)),
        module_dir,
        definitions: Vec::new(),
        dlq_store: None,
    };
    (root, config)
}

fn load(config: &FunctionsConfig, name: &str) -> Result<crate::FunctionModule, LoadError> {
    load_function_module(config, name, RuntimeType::Deno)
}

fn hash_of(entry: &str, modules: &[(&str, &str)]) -> String {
    let modules: BTreeMap<_, _> = modules
        .iter()
        .map(|(key, text)| (resolve_specifier(key).unwrap(), *text))
        .collect();
    function_hash(entry.as_bytes(), &modules)
}

// ── What a load captures ─────────────────────────────────────────────────────

#[test]
fn a_function_loads_with_its_shared_modules_transpiled() {
    let (_root, config) =
        project(&[("f.ts", FUNCTION), ("shared/math.ts", HELPER)], Some("shared"));
    let module = load(&config, "f").unwrap();
    assert_eq!(&module.bytecode[..], FUNCTION.as_bytes(), "the function's source is kept");
    let shared = module.shared.unwrap();
    let code = &shared[&resolve_specifier("@shared/math.ts").unwrap()];
    assert!(code.contains("n * 2") && !code.contains(": number"), "types stripped: {code}");
}

#[test]
fn the_hash_covers_the_function_and_the_sources_of_its_shared_modules() {
    let (_root, config) =
        project(&[("f.ts", FUNCTION), ("shared/math.ts", HELPER)], Some("shared"));
    let module = load(&config, "f").unwrap();
    assert_eq!(module.source_hash, hash_of(FUNCTION, &[("@shared/math.ts", HELPER)]));
}

#[test]
fn editing_a_shared_module_changes_the_hash() {
    let (root, config) = project(&[("f.ts", FUNCTION), ("shared/math.ts", HELPER)], Some("shared"));
    let before = load(&config, "f").unwrap().source_hash;
    std::fs::write(
        root.path().join("functions/shared/math.ts"),
        "export const double = (n) => n + n;\n",
    )
    .unwrap();
    assert_ne!(load(&config, "f").unwrap().source_hash, before);
}

#[test]
fn a_function_without_imports_hashes_only_itself() {
    let source = "export default () => 1;\n";
    for shared_dir in [None, Some("shared")] {
        let (_root, config) = project(&[("f.ts", source), ("shared/.keep", "")], shared_dir);
        let module = load(&config, "f").unwrap();
        assert_eq!(module.source_hash, hash_of(source, &[]), "{shared_dir:?}");
        assert_eq!(module.shared.is_some(), shared_dir.is_some(), "{shared_dir:?}");
    }
}

/// The walk reads the code the isolate runs. An import erased by type-stripping never
/// reaches the isolate, so it is not loaded: the file it names need not exist.
#[test]
fn an_import_the_transpiler_erases_is_not_loaded() {
    for import in [
        "import type { Shape } from \"@shared/types.ts\";\n",
        "import { Shape } from \"@shared/types.ts\";\n",
    ] {
        let source = format!("{import}export default (e: Shape) => e;\n");
        let (_root, config) = project(&[("f.ts", &source), ("shared/.keep", "")], Some("shared"));
        let module = load(&config, "f").unwrap();
        assert!(module.shared.unwrap().is_empty(), "{import}");
        assert_eq!(module.source_hash, hash_of(&source, &[]), "{import}");
    }
}

/// A function whose code does not parse still loads, so its invocation reports the
/// syntax error as it always has; nothing is read for it.
#[test]
fn a_function_that_does_not_parse_loads_and_fails_when_invoked() {
    let source = "export default ( => ;\n";
    let (_root, config) = project(&[("f.ts", source)], None);
    let module = load(&config, "f").unwrap();
    assert_eq!(module.source_hash, hash_of(source, &[]));
}

#[test]
fn a_wasm_module_loads_as_bytes() {
    let (_root, config) = project(&[("w.wasm", "\0asm")], None);
    let module = load_function_module(&config, "w", RuntimeType::Wasm).unwrap();
    assert_eq!(&module.bytecode[..], b"\0asm");
    assert!(module.shared.is_none());
}

// ── What a load refuses ──────────────────────────────────────────────────────

#[test]
fn a_missing_module_names_the_paths_tried() {
    let (_root, config) = project(&[], None);
    let error = load(&config, "ghost").unwrap_err();
    assert!(matches!(error, LoadError::NoModule { .. }), "{error:?}");
    assert!(error.to_string().contains("ghost.{"), "{error}");
}

#[test]
fn an_import_without_a_shared_dir_is_refused_at_load_naming_the_key() {
    let (_root, config) = project(&[("f.ts", FUNCTION)], None);
    let error = load(&config, "f").unwrap_err();
    assert!(matches!(error, LoadError::Graph { .. }), "{error:?}");
    let message = error.to_string();
    assert!(
        message.contains("\"f\"") && message.contains("`[functions] shared_dir`"),
        "{message}"
    );
}

#[test]
fn a_missing_shared_module_is_refused_at_load() {
    let (_root, config) = project(&[("f.ts", FUNCTION), ("shared/.keep", "")], Some("shared"));
    assert!(matches!(load(&config, "f").unwrap_err(), LoadError::Graph { .. }));
}

#[test]
fn a_shared_module_that_does_not_parse_is_refused_at_load() {
    let (_root, config) =
        project(&[("f.ts", FUNCTION), ("shared/math.ts", "export const = ;\n")], Some("shared"));
    let message = load(&config, "f").unwrap_err().to_string();
    assert!(message.contains("@shared/math.ts"), "{message}");
}

/// The layout check runs at load too, not only at compile: an environment override can
/// point the shared directory somewhere the compiler never saw.
#[test]
fn a_shared_dir_holding_function_files_is_refused_at_load() {
    let (_root, mut config) = project(&[("f.ts", "export default () => 1;\n")], None);
    config.shared_dir = Some(config.module_dir.clone());
    assert!(matches!(load(&config, "f").unwrap_err(), LoadError::SharedDir(_)));
}

#[test]
fn a_shared_dir_that_does_not_exist_is_refused_at_load() {
    let (_root, mut config) = project(&[("f.ts", "export default () => 1;\n")], None);
    config.shared_dir = Some(Path::new("/nonexistent/fraiseql-shared").to_path_buf());
    assert!(matches!(load(&config, "f").unwrap_err(), LoadError::SharedDir(_)));
}
