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
    let vars: Vec<(&str, Option<String>)> = [
        "FRAISEQL_FUNCTIONS_MODULE_DIR",
        "FRAISEQL_FUNCTIONS_SHARED_DIR",
    ]
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
