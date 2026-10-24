//! What `prepare_functions_runtime` resolves from the deployment: where modules load from,
//! and the hosts guests may call.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::path::PathBuf;

use super::resolve_module_dir;

#[test]
fn the_environment_overrides_the_compiled_module_dir() {
    let resolved = resolve_module_dir(PathBuf::from("functions"), |key| {
        (key == "FRAISEQL_FUNCTIONS_MODULE_DIR").then(|| "/srv/modules".to_owned())
    });
    assert_eq!(resolved, PathBuf::from("/srv/modules"));
}

#[test]
fn the_compiled_module_dir_stands_when_the_environment_is_silent() {
    for env in [None, Some(String::new()), Some("  ".to_owned())] {
        let resolved = resolve_module_dir(PathBuf::from("functions"), |_| env.clone());
        assert_eq!(resolved, PathBuf::from("functions"), "{env:?}");
    }
}

/// The boot resolves `[egress]` once and hands it to the hooks every function and source
/// connector host is built from.
#[cfg(feature = "functions-runtime")]
#[tokio::test]
async fn the_functions_runtime_carries_the_egress_list() {
    use std::sync::Arc;

    use fraiseql_core::schema::CompiledSchema;
    use fraiseql_test_utils::failing_adapter::FailingAdapter;

    use crate::{
        Server,
        server_config::{EgressConfig, ServerConfig},
    };

    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let functions: crate::schema::loader::FunctionsConfig =
        serde_json::from_value(serde_json::json!({
            "module_dir": modules.path(),
            "definitions": [
                { "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }
            ]
        }))
        .unwrap();
    let config = ServerConfig {
        cors_enabled: false,
        egress: EgressConfig {
            allowed_domains: vec!["api.example.com".to_owned()],
        },
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
    assert_eq!(&*hooks.egress, ["api.example.com".to_owned()]);
}
