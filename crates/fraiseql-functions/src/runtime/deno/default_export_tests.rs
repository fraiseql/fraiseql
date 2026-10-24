//! Every documented default-export form runs, with or without a trailing `;`, whether or
//! not the source is type-stripped first; and only the module's real default export is
//! rewritten, not the same words in a comment or a string.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;

use super::{DenoConfig, DenoRuntime};
use crate::{
    EventPayload, FunctionModule, FunctionRuntime, ResourceLimits, RuntimeType,
    host::NoopHostContext,
};

async fn run(source: &str, enable_typescript: bool) -> fraiseql_error::Result<serde_json::Value> {
    let module =
        FunctionModule::from_source("notify".to_string(), source.to_string(), RuntimeType::Deno);
    let config = DenoConfig {
        enable_typescript,
        v8_flags: vec![],
    };
    let runtime = DenoRuntime::new(&config).expect("Failed to create DenoRuntime");
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Order".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": "o-1"}),
        timestamp:    Utc::now(),
    };
    runtime
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await
        .map(|r| r.value.unwrap_or(serde_json::Value::Null))
}

/// The forms `wrap_source` documents, each written with and without its trailing `;`.
const FORMS: &[&str] = &[
    "export default async function notify(event) {\n  return { id: event.id };\n}\n",
    "export default async function notify(event) {\n  return { id: event.id };\n};\n",
    "export default async function (event) {\n  return { id: event.id };\n}\n",
    "export default function notify(event) {\n  return { id: event.id };\n}\n",
    "export default async (event) => {\n  return { id: event.id };\n}\n",
    "export default async (event) => {\n  return { id: event.id };\n};\n",
    "export default (event) => ({ id: event.id })\n",
];

async fn every_form_runs(enable_typescript: bool) {
    for source in FORMS {
        let value = run(source, enable_typescript)
            .await
            .unwrap_or_else(|e| panic!("`{source}` failed (typescript={enable_typescript}): {e}"));
        assert_eq!(value["id"], "o-1", "`{source}` returned {value}");
    }
}

#[tokio::test]
async fn every_default_export_form_runs_type_stripped() {
    every_form_runs(true).await;
}

#[tokio::test]
async fn every_default_export_form_runs_verbatim() {
    every_form_runs(false).await;
}

#[tokio::test]
async fn export_default_in_a_comment_or_a_string_is_not_rewritten() {
    let source = "// export default is how a module names its entry point.\n\
                  const label = \"export default\";\n\
                  export default async (event) => ({ id: event.id, label });\n";
    let value = run(source, false).await.unwrap();
    assert_eq!(value["label"], "export default", "the string literal was rewritten: {value}");
}

#[tokio::test]
async fn a_module_with_no_default_export_is_a_syntax_error() {
    let error = run("async function notify(event) { return event; }\n", false)
        .await
        .expect_err("a module with no entry point must not run");
    assert!(error.to_string().contains("no default export"), "{error}");
}
