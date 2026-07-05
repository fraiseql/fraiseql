//! Tests for the function-module loader.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_functions::{RuntimeType, types::FunctionDefinition};
use tempfile::tempdir;

use super::{build_functions_subsystem, load_one_module};
use crate::schema::loader::FunctionsConfig;

// WASM is always compiled in under `functions-runtime`, so the loader tests use it
// (the module bytes are not validated at load time — the runtime instantiates them
// lazily). Deno loading is exercised only when the Deno runtime is compiled in.
fn wasm_def(name: &str) -> FunctionDefinition {
    FunctionDefinition::new(name, &format!("after:mutation:Deal:update@{name}"), RuntimeType::Wasm)
}

#[test]
fn loads_a_wasm_module_from_bytecode() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("scoreDeal.wasm"), b"\0asm\x01\0\0\0").unwrap();

    let module = load_one_module(dir.path(), &wasm_def("scoreDeal")).unwrap();
    assert_eq!(module.name, "scoreDeal");
    assert_eq!(module.runtime, RuntimeType::Wasm);
    assert_eq!(&module.bytecode[..], b"\0asm\x01\0\0\0");
}

#[test]
fn a_missing_module_file_fails_loud() {
    let dir = tempdir().unwrap();
    // No file written → the declared function has no loadable module.
    let error = load_one_module(dir.path(), &wasm_def("ghost")).unwrap_err();
    assert!(error.to_string().contains("ghost"), "the error names the function");
}

#[test]
fn build_subsystem_loads_all_declared_modules() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("scoreDeal.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(dir.path().join("chargeCard.wasm"), b"\0asm\x01\0\0\0").unwrap();

    let config = FunctionsConfig {
        module_dir:  dir.path().to_path_buf(),
        definitions: vec![wasm_def("scoreDeal"), wasm_def("chargeCard")],
    };

    let subsystem = build_functions_subsystem(config).unwrap();
    assert_eq!(subsystem.module_registry.len(), 2, "both modules loaded");
    assert!(subsystem.module_registry.contains_key("scoreDeal"));
    assert!(subsystem.module_registry.contains_key("chargeCard"));
}

#[test]
fn build_subsystem_fails_loud_on_a_missing_module() {
    let dir = tempdir().unwrap();
    // Declared but no file on disk.
    let config = FunctionsConfig {
        module_dir:  dir.path().to_path_buf(),
        definitions: vec![wasm_def("missing")],
    };
    assert!(build_functions_subsystem(config).is_err());
}

/// A function targeting a runtime that is not compiled into this build fails loud.
#[cfg(not(feature = "functions-runtime-deno"))]
#[test]
fn a_deno_function_without_the_deno_runtime_fails_loud() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("followUp.ts"), "export default async () => ({});").unwrap();
    let def = FunctionDefinition::new(
        "followUp",
        "after:mutation:Deal:update@followUp",
        RuntimeType::Deno,
    );
    let error = load_one_module(dir.path(), &def).unwrap_err();
    assert!(error.to_string().contains("not compiled into this build"));
}

/// When the Deno runtime is compiled in, a `.ts`/`.js` module loads from source.
#[cfg(feature = "functions-runtime-deno")]
#[test]
fn loads_a_deno_module_from_source() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("followUp.ts"), "export default async () => ({ ok: true });")
        .unwrap();
    let def = FunctionDefinition::new(
        "followUp",
        "after:mutation:Deal:update@followUp",
        RuntimeType::Deno,
    );
    let module = load_one_module(dir.path(), &def).unwrap();
    assert_eq!(module.runtime, RuntimeType::Deno);
    assert!(String::from_utf8_lossy(&module.bytecode).contains("export default"));
}
