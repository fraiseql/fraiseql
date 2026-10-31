//! What a Deno function's runtime provides, pinned: the globals it can rely on, the
//! ones it cannot, and how a library gets in. `docs/architecture/functions.md`
//! ("What a function's runtime provides") states the same list; a change here is a
//! change there.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;

use super::{DenoConfig, DenoRuntime};
use crate::{
    EventPayload, FunctionModule, FunctionRuntime, ResourceLimits, RuntimeType,
    host::NoopHostContext,
};

async fn invoke(source: &str) -> fraiseql_error::Result<crate::FunctionResult> {
    let module =
        FunctionModule::from_source("probe".to_string(), source.to_string(), RuntimeType::Deno);
    let config = DenoConfig {
        enable_typescript: false,
        v8_flags:          vec![],
    };
    let runtime = DenoRuntime::new(&config).expect("Failed to create DenoRuntime");
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Probe".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    Utc::now(),
    };
    runtime
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await
}

async fn run(source: &str) -> fraiseql_error::Result<serde_json::Value> {
    invoke(source).await.map(|r| r.value.unwrap_or(serde_json::Value::Null))
}

/// Each global the issue asked about, with `typeof` as the runtime reports it.
const GLOBALS: &[(&str, &str)] = &[
    ("TextEncoder", "undefined"),
    ("TextDecoder", "undefined"),
    ("atob", "undefined"),
    ("btoa", "undefined"),
    ("URL", "undefined"),
    ("crypto", "undefined"),
    ("setTimeout", "undefined"),
    ("console", "object"),
    ("structuredClone", "undefined"),
    ("queueMicrotask", "function"),
    ("fetch", "undefined"),
    ("WebAssembly", "object"),
    ("Intl", "object"),
];

#[tokio::test]
async fn the_globals_a_function_can_rely_on() {
    let names: Vec<&str> = GLOBALS.iter().map(|(name, _)| *name).collect();
    let source = format!(
        "export default async () => Object.fromEntries({names:?}.map((n) => [n, typeof globalThis[n]]));\n"
    );
    let report = run(&source).await.unwrap();
    for (name, expected) in GLOBALS {
        assert_eq!(report[name], *expected, "typeof {name}: {report}");
    }
}

/// There is no module loader: a function is one file, and a library reaches it bundled.
#[tokio::test]
async fn a_static_import_is_a_syntax_error() {
    let error = run("import { x } from \"./lib.js\";\nexport default async () => x;\n")
        .await
        .expect_err("a static import has nothing to load it");
    assert!(error.to_string().contains("Cannot use import statement"), "{error}");
}

#[tokio::test]
async fn a_dynamic_import_fails() {
    let error = run("export default async () => (await import(\"./lib.js\")).x;\n")
        .await
        .expect_err("a dynamic import has nothing to load it");
    assert!(error.to_string().contains("import"), "{error}");
}
