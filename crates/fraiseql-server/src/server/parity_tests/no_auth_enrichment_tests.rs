//! A build without the `auth` feature refuses a schema that reads the enriched identity,
//! at every door a schema comes in through: each public constructor, a hot reload and a
//! tenant registration. Nothing in such a build resolves the enriched identity.

use std::sync::Arc;

use axum::extract::{Path, State};
use fraiseql_core::schema::{CompiledSchema, SessionVariableMapping, SessionVariableSource};

use super::{
    NoopRelayAdapter, assert_every_constructor_refuses, fully_configured_schema,
    fully_configured_server_config, write_schema,
};
use crate::{
    Server,
    extractors::OptionalSecurityContext,
    routes::{
        api::tenant_admin::{TenantRegistrationRequest, upsert_tenant_handler},
        graphql::TenantExecutorRegistry,
    },
};

const REFUSAL: &str = "built without the `auth` feature";

/// The fixture schema, with a session variable sourced from the enriched identity.
fn consumer_schema() -> CompiledSchema {
    let mut schema = fully_configured_schema();
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.actor".to_string(),
        source: SessionVariableSource::Enrichment {
            field: "actor_id".to_string(),
        },
    });
    assert!(schema.declares_enrichment_consumer(), "the schema reads the enriched identity");
    schema
}

async fn served() -> Server {
    Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter::default()),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema")
}

#[tokio::test]
async fn every_constructor_refuses_a_schema_reading_the_enriched_identity() {
    assert_every_constructor_refuses(
        &consumer_schema(),
        &fully_configured_server_config(),
        &NoopRelayAdapter::default(),
        "enrichment_readers_need_auth_check",
        REFUSAL,
    )
    .await;
}

#[tokio::test]
async fn a_reload_to_a_schema_reading_the_enriched_identity_is_refused() {
    let state = served().await.build_app_state();
    let before = state.executor.load().schema().content_hash();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &consumer_schema()).await;
    let error = state.reload_schema(&path).await.expect_err("the reload is refused");

    assert!(error.contains(REFUSAL), "{error}");
    assert!(error.contains("`actor_id`"), "names the field read: {error}");
    assert_eq!(state.executor.load().schema().content_hash(), before, "the served schema stays");
}

#[tokio::test]
async fn a_tenant_registration_with_a_schema_reading_the_enriched_identity_is_refused() {
    let state = served().await.build_app_state();
    let registry = TenantExecutorRegistry::new(state.executor.clone());
    let state =
        state
            .with_tenant_registry(Arc::new(registry))
            .with_tenant_executor_factory(Arc::new(|_, _, _, _| {
                panic!("a refused schema builds no executor")
            }));
    let request: TenantRegistrationRequest = serde_json::from_value(serde_json::json!({
        "schema": consumer_schema(),
        "connection": { "connection_string": "postgresql://unused" },
    }))
    .expect("a registration request");

    let refused = upsert_tenant_handler(
        State(state.clone()),
        Path("noauth".to_string()),
        OptionalSecurityContext(None),
        axum::Json(request),
    )
    .await
    .expect_err("the registration is refused");

    assert_eq!(refused.code, "VALIDATION_ERROR", "{refused:?}");
    assert!(refused.error.contains(REFUSAL), "{}", refused.error);
    assert!(
        state.tenant_registry().expect("multi-tenant").tenant_keys().is_empty(),
        "nothing is installed"
    );
}
