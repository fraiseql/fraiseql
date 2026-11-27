//! What `prepare_functions_runtime` resolves from the deployment: where modules load from,
//! and the hosts guests may call.

#![allow(clippy::unwrap_used)] // Reason: test code

/// The boot resolves `[egress]` once and hands it to the hooks every function and source
/// connector host is built from.
#[cfg(feature = "functions-runtime")]
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: `temp_env`'s lock guard is not `Send`.
async fn the_functions_runtime_carries_the_egress_list() {
    // Booting reads the functions directories from the environment, which a sibling
    // test sets inside a `temp_env` window: hold its lock with them unset.
    let unset = fraiseql_functions::modules::dirs::DIR_ENV_VARS.map(|var| (var, None::<&str>));
    Box::pin(temp_env::async_with_vars(unset, async {
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
    }))
    .await;
}

/// #1989: a function's `run_as` presenting an enriched field the served schema never reads
/// is refused before any module loads.
#[cfg(feature = "functions-runtime")]
#[tokio::test]
async fn the_functions_runtime_refuses_an_unproduced_static_enriched_field() {
    use std::sync::Arc;

    use fraiseql_core::schema::CompiledSchema;
    use fraiseql_test_utils::failing_adapter::FailingAdapter;

    use crate::{Server, server_config::ServerConfig};

    let functions: crate::schema::loader::FunctionsConfig =
        serde_json::from_value(serde_json::json!({
            "module_dir": "functions",
            "definitions": [{
                "name": "expireSessions", "trigger": "cron:*/5 * * * *", "runtime": "Deno",
                "run_as": { "roles": ["maintenance"], "static_enriched": { "agent_role": "manager" } }
            }]
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

    let error = server.prepare_functions_runtime().await.expect_err("refused").to_string();
    assert!(
        error.contains("function `expireSessions` declares `agent_role`"),
        "refused by the static_enriched check, got: {error}"
    );
}

/// `FRAISEQL_SOURCES_ENABLED` decides which source connectors load with the functions, so
/// a value that is neither true nor false refuses boot here rather than switching the
/// scheduler on.
#[cfg(all(feature = "functions-runtime", feature = "sources"))]
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: `temp_env`'s lock guard is not `Send`.
async fn a_sources_switch_that_is_not_a_boolean_refuses_boot() {
    let mut vars: Vec<(&str, Option<&str>)> =
        fraiseql_functions::modules::dirs::DIR_ENV_VARS.map(|var| (var, None)).to_vec();
    vars.push(("FRAISEQL_SOURCES_ENABLED", Some("disabled")));
    Box::pin(temp_env::async_with_vars(vars, async {
        use std::sync::Arc;

        use fraiseql_core::schema::CompiledSchema;
        use fraiseql_test_utils::failing_adapter::FailingAdapter;

        use crate::{Server, server_config::ServerConfig};

        let modules = tempfile::tempdir().unwrap();
        let functions: crate::schema::loader::FunctionsConfig =
            serde_json::from_value(serde_json::json!({ "module_dir": modules.path() })).unwrap();
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

        let refusal = server.prepare_functions_runtime().await.unwrap_err().to_string();
        assert!(refusal.contains("FRAISEQL_SOURCES_ENABLED"), "{refusal}");
    }))
    .await;
}
