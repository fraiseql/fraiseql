//! The server boot loads a Deno function with its shared modules, from the directories
//! the environment names, and computes the hash `fraiseql functions invoke` prints.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::{collections::BTreeMap, path::Path, sync::Arc};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_functions::modules::{hash::function_hash, resolve::resolve_specifier};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{Server, server_config::ServerConfig};

const FUNCTION: &str = "import { tag } from \"@shared/tag.ts\";\nexport default () => tag;\n";

fn tag_module(tag: &str) -> String {
    format!("export const tag = {tag:?};\n")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `<dir>/functions/marker.ts` and `<dir>/functions/shared/tag.ts`.
fn modules(dir: &Path, tag: &str) -> std::path::PathBuf {
    let module_dir = dir.join("functions");
    write(&module_dir.join("marker.ts"), FUNCTION);
    write(&module_dir.join("shared/tag.ts"), &tag_module(tag));
    module_dir
}

/// The hash of `marker` over `tag`'s helper, computed here, not by the loader.
fn expected_hash(tag: &str) -> String {
    let modules = BTreeMap::from([(resolve_specifier("@shared/tag.ts").unwrap(), tag_module(tag))]);
    function_hash(FUNCTION.as_bytes(), &modules)
}

/// Boot the functions runtime for `marker` compiled against `module_dir`, with `env`
/// as the only functions directory variables set, and return the hash it loaded.
#[allow(clippy::future_not_send)] // Reason: `temp_env`'s lock guard is not `Send`.
async fn loaded_hash(module_dir: &Path, env: &[(&str, &Path)]) -> String {
    let functions: crate::schema::loader::FunctionsConfig =
        serde_json::from_value(serde_json::json!({
            "module_dir": module_dir,
            "shared_dir": module_dir.join("shared"),
            "definitions": [
                { "name": "marker", "trigger": "after:mutation:Order", "runtime": "Deno" }
            ]
        }))
        .unwrap();
    let vars: Vec<(&str, Option<String>)> = fraiseql_functions::modules::dirs::DIR_ENV_VARS
        .into_iter()
        .map(|key| {
            let value = env.iter().find(|(k, _)| *k == key).map(|(_, v)| v.display().to_string());
            (key, value)
        })
        .collect();
    Box::pin(temp_env::async_with_vars(vars, async move {
        let config = ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        };
        let mut server = Box::pin(Server::new(
            config,
            CompiledSchema::new(),
            Arc::new(FailingAdapter::new()),
            None,
        ))
        .await
        .unwrap()
        .with_functions_config(Some(functions));
        server.prepare_functions_runtime().await.unwrap();
        let hooks = server.functions_hooks.as_ref().expect("the runtime was prepared");
        hooks.module_registry["marker"].source_hash.clone()
    }))
    .await
}

#[tokio::test]
async fn the_server_loads_a_function_with_its_shared_modules() {
    let dir = tempfile::tempdir().unwrap();
    let module_dir = modules(dir.path(), "compiled");
    assert_eq!(loaded_hash(&module_dir, &[]).await, expected_hash("compiled"));
}

#[tokio::test]
async fn the_server_loads_from_the_module_dir_the_environment_names() {
    let dir = tempfile::tempdir().unwrap();
    let module_dir = modules(dir.path(), "compiled");
    let deployed = modules(&dir.path().join("deployed"), "deployed");
    let hash = loaded_hash(
        &module_dir,
        &[
            ("FRAISEQL_FUNCTIONS_MODULE_DIR", &deployed),
            ("FRAISEQL_FUNCTIONS_SHARED_DIR", &deployed.join("shared")),
        ],
    )
    .await;
    assert_eq!(hash, expected_hash("deployed"));
}

#[tokio::test]
async fn the_server_loads_from_the_shared_dir_the_environment_names() {
    let dir = tempfile::tempdir().unwrap();
    let module_dir = modules(dir.path(), "compiled");
    let shared = dir.path().join("elsewhere");
    write(&shared.join("tag.ts"), &tag_module("elsewhere"));
    let hash = loaded_hash(&module_dir, &[("FRAISEQL_FUNCTIONS_SHARED_DIR", &shared)]).await;
    assert_eq!(hash, expected_hash("elsewhere"));
}

// ── Acceptance: two functions sharing a helper, run by the server ────────────

/// The fixture `fraiseql functions invoke`'s acceptance test runs: `greet` and `shout`,
/// both importing `@shared/text.ts`.
fn shared_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fraiseql-cli/tests/fixtures/functions_shared/functions")
}

fn copy_shared_fixture(dir: &Path) -> std::path::PathBuf {
    let module_dir = dir.join("functions");
    for relative in [
        "greet.ts",
        "shout.ts",
        "deno.json",
        "shared/text.ts",
        "shared/types.ts",
    ] {
        write(
            &module_dir.join(relative),
            &std::fs::read_to_string(shared_fixture().join(relative)).unwrap(),
        );
    }
    module_dir
}

/// The hash `invoke`'s acceptance test expects, computed the same way from the files.
fn fixture_hash(module_dir: &Path, name: &str) -> String {
    let entry = std::fs::read(module_dir.join(format!("{name}.ts"))).unwrap();
    let text = std::fs::read(module_dir.join("shared/text.ts")).unwrap();
    function_hash(&entry, &BTreeMap::from([(resolve_specifier("@shared/text.ts").unwrap(), text)]))
}

/// Boot the functions runtime over `module_dir`, run `greet` and `shout` through the
/// server's observer, and return each one's result and loaded hash.
#[allow(clippy::future_not_send)] // Reason: `temp_env`'s lock guard is not `Send`.
async fn serve_both(module_dir: &Path) -> Vec<(serde_json::Value, String)> {
    use fraiseql_functions::{EventPayload, ResourceLimits, host::NoopHostContext};

    let functions: crate::schema::loader::FunctionsConfig =
        serde_json::from_value(serde_json::json!({
            "module_dir": module_dir,
            "shared_dir": module_dir.join("shared"),
            "definitions": [
                { "name": "greet", "trigger": "after:mutation:Order", "runtime": "Deno" },
                { "name": "shout", "trigger": "after:mutation:Order", "runtime": "Deno" }
            ]
        }))
        .unwrap();
    let vars = fraiseql_functions::modules::dirs::DIR_ENV_VARS.map(|var| (var, None::<&str>));
    Box::pin(temp_env::async_with_vars(vars, async move {
        let config = ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        };
        let mut server = Box::pin(Server::new(
            config,
            CompiledSchema::new(),
            Arc::new(FailingAdapter::new()),
            None,
        ))
        .await
        .unwrap()
        .with_functions_config(Some(functions));
        server.prepare_functions_runtime().await.unwrap();
        let hooks = server.functions_hooks.as_ref().expect("the runtime was prepared");
        let mut ran = Vec::new();
        for name in ["greet", "shout"] {
            let module = &hooks.module_registry[name];
            let event = EventPayload {
                trigger_type: "mutation".to_owned(),
                entity:       "Order".to_owned(),
                event_kind:   "updated".to_owned(),
                data:         serde_json::json!({}),
                timestamp:    fraiseql_time::Timestamp::now(),
            };
            let host = NoopHostContext::new(event.clone());
            let result = hooks
                .observer
                .invoke(module, event, &host, ResourceLimits::default())
                .await
                .unwrap();
            ran.push((result.value.unwrap(), module.source_hash.clone()));
        }
        ran
    }))
    .await
}

#[tokio::test]
async fn the_server_runs_two_functions_sharing_a_helper_and_an_edit_to_it_changes_both() {
    let dir = tempfile::tempdir().unwrap();
    let module_dir = copy_shared_fixture(dir.path());

    let before = serve_both(&module_dir).await;
    assert_eq!(
        before[0],
        (serde_json::json!("hello, fraiseql"), fixture_hash(&module_dir, "greet"))
    );
    assert_eq!(
        before[1],
        (serde_json::json!("HELLO, FRAISEQL!"), fixture_hash(&module_dir, "shout"))
    );

    let text = module_dir.join("shared/text.ts");
    let edited = std::fs::read_to_string(&text).unwrap().replace("hello, ", "hi, ");
    std::fs::write(&text, edited).unwrap();

    let after = serve_both(&module_dir).await;
    assert_eq!(
        after[0],
        (serde_json::json!("hi, fraiseql"), fixture_hash(&module_dir, "greet"))
    );
    assert_eq!(
        after[1],
        (serde_json::json!("HI, FRAISEQL!"), fixture_hash(&module_dir, "shout"))
    );
    assert_ne!(after[0].1, before[0].1);
    assert_ne!(after[1].1, before[1].1);
}
