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
    let bound = bound
        .serving()
        .expect("no tenancy runtime: the observer must be bound to the serving executor");
    assert!(
        Arc::ptr_eq(bound, &state.executor),
        "the observer must hold the serving state's executor, so dispatched functions query \
         the schema that is being served"
    );
}

/// #1550: under the tenancy runtime the observer is bound to the tenant registry, so a
/// dispatched function's write runs on its tenant's executor rather than the default one.
#[tokio::test]
async fn under_the_tenancy_runtime_the_observer_dispatches_through_the_registry() {
    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let functions: FunctionsConfig = serde_json::from_value(serde_json::json!({
        "module_dir": modules.path(),
        "definitions": [
            { "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }
        ]
    }))
    .unwrap();
    let mut config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    config.tenancy.runtime.enabled = true;
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
    let bound = server
        .after_mutation_observer
        .as_ref()
        .and_then(|observer| observer.bound_executor())
        .expect("the observer must be bound");
    let bound = bound
        .registry()
        .expect("the tenancy runtime is on: the observer must be bound to the registry");
    assert!(Arc::ptr_eq(bound, state.tenant_registry().unwrap()), "the serving registry");

    // A tenant executor's copy dispatches within its tenant, through the same registry.
    let observer = server.after_mutation_observer.as_ref().unwrap();
    assert_eq!(observer.tenant(), None, "the default executor's copy is bound to no tenant");
    let copy = observer.for_tenant("acme");
    assert_eq!(copy.tenant(), Some("acme"));
    let copy_bound = copy.bound_executor().expect("the copy shares the binding");
    assert!(
        Arc::ptr_eq(copy_bound.registry().unwrap(), state.tenant_registry().unwrap()),
        "a tenant's copy shares the observer's executor binding"
    );
}
