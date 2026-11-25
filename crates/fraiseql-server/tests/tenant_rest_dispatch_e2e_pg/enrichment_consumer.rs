//! A tenant's schema meets the enrichment rule boot and reload apply: a schema that reads
//! the enriched identity is refused when nothing resolves one. A tenant registered with
//! such a schema on a server whose `[identity.enrichment]` is off would serve an
//! authenticated caller's request with no identity resolved.
//!
//! **Parallelism:** creates and drops `tenant_p2521` and `public.v_p2521_gadget`.

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
};
use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy},
    runtime::Executor,
    schema::{CompiledSchema, InjectedParamSource},
    security::AuthenticatedUser,
    types::UserId,
};
use fraiseql_server::{
    extractors::OptionalSecurityContext,
    middleware::{AuthUser, TenantClaim},
    routes::{
        api::tenant_admin::upsert_tenant_handler,
        graphql::{AppState, tenant_registry::TenantExecutorRegistry},
        rest::{RestMountConfig, rest_router},
    },
    tenancy::make_executor_factory,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

use super::{exec, registration, send};

const TENANT: &str = "p2521";

/// A relay route over `v_p2521_gadget`, and a list route over `v_p2521_widget`.
fn schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Gadget", "sql_source": "v_p2521_gadget", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }, {
            "name": "Widget", "sql_source": "v_p2521_widget", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }],
        "queries": [{
            "name": "gadgets", "return_type": "Gadget", "returns_list": true, "nullable": false,
            "sql_source": "v_p2521_gadget", "arguments": [], "relay": true
        }, {
            "name": "widgets", "return_type": "Widget", "returns_list": true,
            "nullable": false, "sql_source": "v_p2521_widget", "arguments": []
        }],
        "security": { "tenancy": { "mode": "schema", "tenant_claim": "tenant_id" } }
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.rest_config = Some(fraiseql_core::schema::RestConfig {
        enabled: true,
        ..fraiseql_core::schema::RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// [`schema`], with `widgets` filtered on the caller's enriched `org`: a reader of the
/// enriched identity. The relay route reads nothing enriched.
fn consumer_schema() -> CompiledSchema {
    let mut schema = schema();
    let own = schema.queries.iter_mut().find(|q| q.name == "widgets").unwrap();
    own.inject_params
        .insert("label".to_string(), InjectedParamSource::Enrichment("org".to_string()));
    schema.build_indexes();
    assert!(schema.declares_enrichment_consumer(), "the schema reads the enriched identity");
    schema
}

/// An authenticated caller of the tenant.
fn caller(request: &mut Request<Body>) {
    let user = AuthenticatedUser {
        user_id:      UserId::new("caller-p2521"),
        scopes:       vec![],
        expires_at:   fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::from([("tenant_id".to_string(), json!(TENANT))]),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    };
    request.extensions_mut().insert(AuthUser(user));
    request.extensions_mut().insert(TenantClaim(Arc::from("tenant_id")));
}

fn router(state: &AppState) -> axum::Router {
    rest_router(
        state,
        &RestMountConfig {
            compression_enabled: false,
            auth_layer_attached: false,
            export:              Arc::default(),
        },
    )
    .expect("the REST router mounts")
}

/// A multi-tenant server serving [`schema`], with no identity resolver configured.
async fn server(url: &str) -> (PostgresAdapter, AppState) {
    let admin = PostgresAdapter::new(url).await.expect("connect to the test database");
    exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT} CASCADE")).await;
    exec(&admin, "DROP VIEW IF EXISTS public.v_p2521_gadget").await;
    exec(
        &admin,
        "CREATE VIEW public.v_p2521_gadget AS SELECT 1::bigint AS pk_gadget, \
         jsonb_build_object('id', '1', 'label', 'PUBLIC') AS data WHERE false",
    )
    .await;
    let default_executor = Arc::new(Executor::new(schema(), Arc::new(admin.clone())));
    let registry = Arc::new(TenantExecutorRegistry::new(Arc::new(arc_swap::ArcSwap::from(
        Arc::clone(&default_executor),
    ))));
    let state = AppState::new(default_executor)
        .with_tenant_registry(registry)
        .with_tenant_executor_factory(make_executor_factory::<PostgresAdapter>(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            fraiseql_core::db::postgres::VectorScanConfig::default(),
            fraiseql_core::db::postgres::TransactionRetry::off(),
        ));
    (admin, state)
}

/// A tenant whose schema reads the enriched identity is refused on a server that resolves
/// none, with boot's and reload's message. A tenant executor answers a relay route with
/// `400` (it is built without relay support), so what the refusal closes is the schema
/// being installed: every other entry of such a tenant refuses an unresolved caller.
#[tokio::test]
async fn a_tenant_schema_reading_the_enriched_identity_is_refused_without_a_resolver() {
    let Some(url) = try_database_url() else {
        return;
    };
    let (_admin, state) = server(&url).await;
    let mut request = registration(&url);
    request.schema = serde_json::to_value(consumer_schema()).unwrap();

    let registered = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT.to_string()),
        OptionalSecurityContext(None),
        axum::Json(request),
    )
    .await;

    let Err(refused) = registered else {
        let (status, body) = relay(&state).await;
        assert_eq!(
            (status, body),
            relay_unsupported(),
            "what a registered tenant's relay route answers"
        );
        panic!("a tenant reading the enriched identity was registered with no resolver");
    };
    assert_eq!(refused.code, "VALIDATION_ERROR", "{refused:?}");
    let message = &refused.error;
    assert!(message.contains("`[identity.enrichment]` is not enabled"), "{message}");
    assert!(message.contains("`org`"), "names the field read: {message}");
    let installed = state.tenant_registry().unwrap().tenant_keys();
    assert!(!installed.contains(&TENANT.to_string()), "nothing is installed: {installed:?}");

    // The same server registers the same tenant without the reader.
    let mut request = registration(&url);
    request.schema = serde_json::to_value(schema()).unwrap();
    let _registered = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT.to_string()),
        OptionalSecurityContext(None),
        axum::Json(request),
    )
    .await
    .expect("a schema reading no enriched field registers");
    // A tenant executor does not serve a relay route. Were it to, the tenant path would
    // reach the relay entry with an unresolved caller, and this suite would need to say so.
    assert_eq!(relay(&state).await, relay_unsupported());
}

/// An authenticated caller's relay request to the tenant.
async fn relay(state: &AppState) -> (StatusCode, Value) {
    let mut request = Request::get("/rest/v1/gadgets?first=5").body(Body::empty()).unwrap();
    caller(&mut request);
    send(router(state), request).await
}

/// What a tenant executor answers a relay route.
fn relay_unsupported() -> (StatusCode, Value) {
    (
        StatusCode::BAD_REQUEST,
        json!({ "error": { "code": "BAD_REQUEST", "message": "Validation error: Relay \
            pagination is not supported by the postgresql adapter. Use a relay-capable adapter \
            (e.g. PostgreSQL) and construct the executor with `Executor::new_with_relay`." } }),
    )
}
