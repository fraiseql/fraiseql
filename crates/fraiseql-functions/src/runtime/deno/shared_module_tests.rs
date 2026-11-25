//! A function runs as an ES module, importing only the shared modules its load captured.
//!
//! These drive the executor with a shared-module map directly, the map a function's
//! load produces: what runs, what is refused, and that a refusal is permanent.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use super::executor::run_in_dedicated_thread;
use crate::{
    modules::{SharedModules, resolve::resolve_specifier},
    types::{PERMANENT_ERROR_MARKER, ResourceLimits},
};

fn shared(modules: &[(&str, &str)]) -> Option<Arc<SharedModules>> {
    Some(Arc::new(
        modules
            .iter()
            .map(|(specifier, code)| (resolve_specifier(specifier).unwrap(), (*code).to_string()))
            .collect(),
    ))
}

fn run(source: &str, modules: Option<Arc<SharedModules>>) -> Result<Value, String> {
    let source = source.to_string();
    std::thread::spawn(move || {
        run_in_dedicated_thread(
            &source,
            modules,
            &json!({ "n": 21 }),
            &ResourceLimits::default(),
            None,
        )
        .map(|out| out.value)
    })
    .join()
    .unwrap()
}

fn refused(source: &str, modules: Option<Arc<SharedModules>>) -> String {
    let error = run(source, modules).expect_err("this import must be refused");
    assert!(error.contains(PERMANENT_ERROR_MARKER), "a refused import is permanent: {error}");
    error
}

const HELPER: (&str, &str) = ("@shared/math.ts", "export const double = (n) => n * 2;\n");

// ── What runs ────────────────────────────────────────────────────────────────

#[test]
fn a_function_runs_a_shared_helper() {
    let source = "import { double } from \"@shared/math.ts\";\n\
                  export default async (event) => ({ doubled: double(event.n) });\n";
    assert_eq!(run(source, shared(&[HELPER])).unwrap(), json!({ "doubled": 42 }));
}

#[test]
fn two_functions_share_one_helper() {
    let modules = shared(&[HELPER]);
    let first = "import { double } from \"@shared/math.ts\";\nexport default (e) => double(e.n);\n";
    let second =
        "import { double } from \"@shared/math.ts\";\nexport default (e) => double(e.n) + 1;\n";
    assert_eq!(run(first, modules.clone()).unwrap(), json!(42));
    assert_eq!(run(second, modules).unwrap(), json!(43));
}

#[test]
fn a_shared_module_imports_another() {
    let modules = shared(&[
        HELPER,
        (
            "@shared/lib/quad.ts",
            "import { double } from \"@shared/math.ts\";\nexport const quad = (n) => double(double(n));\n",
        ),
    ]);
    let source =
        "import { quad } from \"@shared/lib/quad.ts\";\nexport default (e) => quad(e.n);\n";
    assert_eq!(run(source, modules).unwrap(), json!(84));
}

#[test]
fn an_import_cycle_runs() {
    let modules = shared(&[
        (
            "@shared/a.ts",
            "import { b } from \"@shared/b.ts\";\nexport const a = (n) => n > 0 ? b(n - 1) : \"a\";\n",
        ),
        (
            "@shared/b.ts",
            "import { a } from \"@shared/a.ts\";\nexport const b = (n) => n > 0 ? a(n - 1) : \"b\";\n",
        ),
    ]);
    let source = "import { a } from \"@shared/a.ts\";\nexport default () => [a(0), a(1), a(2)];\n";
    assert_eq!(run(source, modules).unwrap(), json!(["a", "b", "a"]));
}

#[test]
fn a_literal_dynamic_import_of_a_loaded_module_runs() {
    let source = "export default async (e) => (await import(\"@shared/math.ts\")).double(e.n);\n";
    assert_eq!(run(source, shared(&[HELPER])).unwrap(), json!(42));
}

#[test]
fn a_computed_dynamic_import_of_a_loaded_module_runs() {
    let source =
        "export default async (e) => (await import(\"@shared/\" + \"math.ts\")).double(e.n);\n";
    assert_eq!(run(source, shared(&[HELPER])).unwrap(), json!(42));
}

#[test]
fn a_default_re_exported_from_a_shared_module_is_the_entry_point() {
    let modules = shared(&[("@shared/handler.ts", "export default (e) => ({ handled: e.n });\n")]);
    let source = "export { default } from \"@shared/handler.ts\";\n";
    assert_eq!(run(source, modules).unwrap(), json!({ "handled": 21 }));
}

/// Each invocation builds a fresh isolate and evaluates every module again: a shared
/// module's top-level state starts over, so one invocation cannot see another's.
#[test]
fn module_state_does_not_survive_between_invocations() {
    let modules =
        shared(&[("@shared/counter.ts", "let count = 0;\nexport const next = () => ++count;\n")]);
    let source =
        "import { next } from \"@shared/counter.ts\";\nexport default () => [next(), next()];\n";
    assert_eq!(run(source, modules.clone()).unwrap(), json!([1, 2]));
    assert_eq!(run(source, modules).unwrap(), json!([1, 2]));
}

/// A module is strict: assigning an undeclared name throws instead of creating a global.
#[test]
fn a_function_runs_in_strict_mode() {
    let error = run("export default () => { undeclared = 1; return 1; };\n", None).unwrap_err();
    assert!(error.contains("ReferenceError"), "{error}");
}

// ── What is refused, permanently ─────────────────────────────────────────────

#[test]
fn an_import_without_a_shared_dir_is_refused_naming_the_key() {
    let source =
        "import { double } from \"@shared/math.ts\";\nexport default (e) => double(e.n);\n";
    assert!(refused(source, None).contains("`[functions] shared_dir`"));
}

#[test]
fn a_module_the_load_did_not_capture_is_refused() {
    let source = "import { x } from \"@shared/other.ts\";\nexport default () => x;\n";
    assert!(refused(source, shared(&[HELPER])).contains("not loaded with the function"));
}

/// A computed `import()` names, at run time, a module the function never imports
/// literally: the load did not read it, so it is not there to run.
#[test]
fn a_computed_dynamic_import_of_an_unloaded_module_is_refused() {
    let source = "export default async () => { \
                    try { await import(\"@shared/\" + \"other.ts\"); return \"loaded\"; } \
                    catch (e) { throw e; } };\n";
    assert!(refused(source, shared(&[HELPER])).contains("not loaded with the function"));
}

#[test]
fn each_import_rule_applies_in_the_isolate() {
    for (specifier, reason) in [
        ("./math.ts", "relative path"),
        ("../shared/math.ts", "relative path"),
        ("https://example.com/math.ts", "`https:` scheme"),
        ("npm:zod", "`npm:` scheme"),
        ("node:fs", "`node:` scheme"),
        ("/etc/passwd.ts", "absolute path"),
        ("math", "bare specifier"),
        ("@shared/../math.ts", "`..` segment"),
    ] {
        let source = format!(
            "import {{ double }} from {specifier:?};\nexport default (e) => double(e.n);\n"
        );
        let error = refused(&source, shared(&[HELPER]));
        assert!(error.contains(reason), "{specifier}: {error}");
    }
}

/// The function's own module and the driver are reachable only from where the executor
/// put them: a guest naming either by URL is refused like any other scheme.
#[test]
fn neither_the_function_module_nor_the_driver_is_importable_by_url() {
    for url in ["file:///fraiseql-function.ts", "file:///fraiseql/main.js"] {
        let source = format!("import * as m from {url:?};\nexport default () => 1;\n");
        assert!(refused(&source, shared(&[HELPER])).contains("`file:` scheme"), "{url}");
    }
}

#[test]
fn an_import_with_attributes_is_refused() {
    let source =
        "import m from \"@shared/math.ts\" with { type: \"json\" };\nexport default () => m;\n";
    assert!(refused(source, shared(&[HELPER])).contains("import attributes"));
}

/// A shared module whose top level throws fails the invocation with that error, before
/// the function is called.
#[test]
fn a_shared_module_throwing_at_its_top_level_fails_the_invocation() {
    let modules = shared(&[(
        "@shared/broken.ts",
        "throw new Error(\"broken at load\");\nexport const x = 1;\n",
    )]);
    let source = "import { x } from \"@shared/broken.ts\";\nexport default () => x;\n";
    let error = run(source, modules).unwrap_err();
    assert!(error.contains("broken at load"), "{error}");
}
