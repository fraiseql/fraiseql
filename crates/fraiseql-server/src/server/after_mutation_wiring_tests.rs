//! #1340 / #1440 — the serving executor carries the after-mutation observer, bound to the
//! serving state's executor.
//!
//! Two halves have to meet for a dispatched `after:mutation` function to work. The
//! observer must be on the executor that serves writes, or nothing dispatches. And it must
//! hold that same state's executor handle, or the functions run without their
//! `fraiseql_query` bridge. Neither half is visible in a response, so this pins both on a
//! real provisioned `Server`.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{Server, schema::loader::FunctionsConfig, server_config::ServerConfig};

#[tokio::test]
async fn the_serving_executor_dispatches_through_a_bound_observer() {
    // The loader reads module bytes without validating them; the runtime would, lazily.
    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let functions: FunctionsConfig = serde_json::from_value(serde_json::json!({
        "module_dir": modules.path(),
        "definitions": [
            { "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }
        ]
    }))
    .unwrap();
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

    let state = server.provisioned_app_state().await.unwrap();

    assert!(
        state.executor.load().config().after_mutation_observer.is_some(),
        "the executor serving writes must carry the after-mutation observer"
    );
    let bound = server
        .after_mutation_observer
        .as_ref()
        .and_then(|observer| observer.bound_executor())
        .expect("the observer must be bound to an executor");
    assert!(
        Arc::ptr_eq(&bound, &state.executor),
        "the observer must hold the serving state's executor, so dispatched functions query \
         the schema that is being served"
    );
}
