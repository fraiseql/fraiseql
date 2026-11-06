//! A token claim with no value is treated as absent, on every path that turns a claim into a
//! value the database sees.
//!
//! Each case sends the same request three times, under a token that carries the claim, one
//! that omits it, and one that carries it with a JSON `null` value. The omitted claim is the
//! reference: whatever the server does for it (refuse the request, leave a session variable
//! unset, deny enrichment), it must do for the claim with no value. The claim that is present
//! is the control that the fixture works at all.
//!
//! The paths, each driven through a real server with HS256 authentication:
//!
//! - a function root's argument injected from a claim (`jwt:tenant_id`);
//! - a list root's row filter injected from the same claim;
//! - a mutation's argument injected from the claim, recorded by its function;
//! - a session variable sourced from the claim, read back through a view;
//! - an identity-enrichment query binding `$region` from a claim.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `claim_presence` schema and sets a
//! process-global env var for the HS256 secret → run `--test-threads=1`.
#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InjectedParamSource,
        MutationDefinition, MutationOperation, QueryDefinition, SessionVariableMapping,
        SessionVariableSource, SessionVariablesConfig, TypeDefinition,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::{issuer::TestIssuer, try_database_url};
use serde_json::{Value, json};

/// The schema every fixture object lives in.
const SCHEMA: &str = "claim_presence";

/// The three shapes a token can give the claim `name`.
#[derive(Clone, Copy, Debug)]
enum Claim {
    Present(&'static str),
    Absent,
    NoValue,
}

impl Claim {
    /// The claims a token of this shape adds for `name`.
    fn extra(self, name: &str) -> Value {
        match self {
            Self::Present(value) => json!({ name: value }),
            Self::Absent => json!({}),
            Self::NoValue => json!({ name: null }),
        }
    }
}

fn jwt(claim: &str) -> InjectedParamSource {
    InjectedParamSource::Jwt(claim.to_string())
}

/// The suite's schema. With `enrichment`, its one session variable is the enriched `region`
/// (the server resolves it for every request); without, it is the `tenant_id` claim.
fn schema(enrichment: bool) -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    // A function root: its function echoes the tenant argument it was called with.
    let mut echo = TypeDefinition::new("TenantEcho", "");
    echo.fields = vec![FieldDefinition::new("seen", FieldType::String)];
    schema.types.push(echo);
    let mut echoes = QueryDefinition::new("tenantEchoes", "TenantEcho")
        .returning_list()
        .with_sql_source_function(format!("{SCHEMA}.fn_echo_tenant"));
    echoes.inject_params.insert("tenant_id".to_string(), jwt("tenant_id"));
    schema.queries.push(echoes);

    // A list root over a view, filtered by the tenant claim.
    let mut row = TypeDefinition::new("TenantRow", format!("{SCHEMA}.v_row"));
    row.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(row);
    let mut rows = QueryDefinition::new("tenantRows", "TenantRow")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_row"));
    rows.inject_params.insert("tenant_id".to_string(), jwt("tenant_id"));
    schema.queries.push(rows);

    // The session variable, read back through a view.
    let mut reader = TypeDefinition::new("SettingReader", format!("{SCHEMA}.v_setting_reader"));
    reader.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::nullable("setting", FieldType::String),
    ];
    schema.types.push(reader);
    schema.queries.push(
        QueryDefinition::new("settingReaders", "SettingReader")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_setting_reader")),
    );

    // Two mutations whose function records the injected argument it received: one from the
    // tenant claim, one from an optional claim.
    let mut note = TypeDefinition::new("SeenNote", format!("{SCHEMA}.v_note"));
    note.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(note);
    schema.queries.push(
        QueryDefinition::new("seenNotes", "SeenNote")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_note")),
    );
    for (name, source) in [
        ("recordTenant", jwt("tenant_id")),
        ("recordActor", InjectedParamSource::JwtOptional("actor".to_string())),
    ] {
        let mut record = MutationDefinition::new(name, "SeenNote");
        record.sql_source = Some(format!("{SCHEMA}.fn_record"));
        record.operation = MutationOperation::Insert {
            table: "tb_note".to_string(),
        };
        record.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
        record.inject_params.insert("injected".to_string(), source);
        schema.mutations.push(record);
    }

    let (variable, source) = if enrichment {
        (
            "app.region",
            SessionVariableSource::Enrichment {
                field: "region".to_string(),
            },
        )
    } else {
        (
            "app.tenant",
            SessionVariableSource::Jwt {
                claim: "tenant_id".to_string(),
            },
        )
    };
    schema.session_variables = SessionVariablesConfig {
        variables:         vec![SessionVariableMapping {
            name: variable.to_string(),
            source,
        }],
        inject_started_at: false,
    };
    schema.build_indexes();
    schema
}

async fn provision(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_echo_tenant(p_tenant text) RETURNS TABLE(data jsonb) \
             LANGUAGE sql STABLE AS $$ SELECT jsonb_build_object('seen', \
             coalesce(p_tenant, '<NULL>')) $$"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_row (id int PRIMARY KEY, data jsonb NOT NULL)"),
        format!(
            "INSERT INTO {SCHEMA}.tb_row VALUES \
             (1, '{{\"id\": 1, \"tenant_id\": \"t1\", \"label\": \"t1-row\"}}'), \
             (2, '{{\"id\": 2, \"tenant_id\": \"t2\", \"label\": \"t2-row\"}}')"
        ),
        format!("CREATE VIEW {SCHEMA}.v_row AS SELECT id, data FROM {SCHEMA}.tb_row"),
        format!(
            "CREATE VIEW {SCHEMA}.v_setting_reader AS SELECT 1 AS id, jsonb_build_object('id', 1, \
             'setting', coalesce(current_setting('app.tenant', true), '') || \
             coalesce(current_setting('app.region', true), '')) AS data"
        ),
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("CREATE TABLE {SCHEMA}.tb_seen (label text NOT NULL, injected text)"),
        format!("CREATE TABLE {SCHEMA}.tb_note (id uuid PRIMARY KEY, label text NOT NULL)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_note AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_note"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_record(p_label text, p_injected text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             INSERT INTO {SCHEMA}.tb_seen VALUES (p_label, p_injected); \
             n := gen_random_uuid(); \
             INSERT INTO {SCHEMA}.tb_note (id, label) VALUES (n, p_label); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'SeenNote'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// What the mutation function recorded for each write labelled `label`: one entry per call,
/// `None` where the injected argument was SQL `NULL`.
async fn seen(url: &str, label: &str) -> Vec<Option<String>> {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "SELECT injected FROM {SCHEMA}.tb_seen WHERE label = '{label}'"
        ))
        .await
        .unwrap()
        .iter()
        .map(|row| row["injected"].as_str().map(str::to_string))
        .collect()
}

/// A running server and the issuer its tokens come from; dropping it shuts both down.
struct Running {
    base:      String,
    issuer:    TestIssuer,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl Running {
    /// A token from the issuer, its default claims extended by `extra`.
    fn token(&self, extra: &Value) -> String {
        let mut claims = self.issuer.claims().set("sub", "claim-presence");
        for (name, value) in extra.as_object().into_iter().flatten() {
            claims = claims.set(name, value.clone());
        }
        self.issuer.mint(&claims)
    }

    /// A token whose `tenant_id` claim has the shape `tenant`.
    fn tenant_token(&self, tenant: Claim) -> String {
        self.token(&tenant.extra("tenant_id"))
    }
}

/// A server against `url` trusting a test issuer, and identity enrichment binding
/// `$region` when `enrichment` is set.
async fn serve(url: &str, enrichment: bool) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    provision(&adapter).await;
    let issuer = TestIssuer::start().await;
    let mut config = ServerConfig {
        database_url: url.to_string(),
        auth: Some(fraiseql_core::security::OidcConfig {
            issuer: Some(issuer.issuer().to_string()),
            audience: Some(issuer.audience().to_string()),
            required: true,
            allowed_algorithms: vec!["ES256".to_string()],
            ..fraiseql_core::security::OidcConfig::default()
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    if enrichment {
        config.identity = serde_json::from_value(json!({
            "enrichment": {
                "enabled": true,
                "query": "SELECT coalesce($region::text, '<NULL>') AS region",
                "map": {"region": "region"},
            }
        }))
        .unwrap();
    }
    // The pool is what makes identity enrichment live; inert otherwise.
    let pool = Some(sqlx::PgPool::connect(url).await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(enrichment), adapter, pool)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Running {
        base: format!("http://127.0.0.1:{port}"),
        issuer,
        _shutdown: tx,
    }
}

/// The `DATABASE_URL` the suite runs against. Without one the suite is skipped, loudly: the
/// `integration (server)` leg sets it.
fn database_url() -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP claim_presence_e2e_pg: DATABASE_URL not set");
    }
    url
}

/// A GraphQL `POST` of `query` under `token`: the status and the body.
async fn graphql(server: &Running, query: &str, token: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .bearer_auth(token)
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or_else(|_| json!({ "body": text })))
}

/// The answer to `query` under each shape of the `tenant_id` claim.
async fn per_shape(server: &Running, query: &str) -> [(u16, Value); 3] {
    [
        graphql(server, query, &server.tenant_token(Claim::Present("t1"))).await,
        graphql(server, query, &server.tenant_token(Claim::Absent)).await,
        graphql(server, query, &server.tenant_token(Claim::NoValue)).await,
    ]
}

/// The request was refused: an error, and no data.
fn refused((_, body): &(u16, Value)) -> bool {
    body.get("errors").is_some_and(|e| e.as_array().is_some_and(|e| !e.is_empty()))
        && body.get("data").is_none_or(|data| {
            data.as_object().is_none_or(|fields| fields.values().all(Value::is_null))
        })
}

/// A setting read back through the reader view is unset: a pooled connection that once had it
/// applied transaction-locally reads `''`, not `NULL`.
fn setting(body: &Value) -> Option<String> {
    body["data"]["settingReaders"][0]["setting"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A function root's injected argument: a claim with no value refuses the call, as an absent
/// claim does, rather than calling the function with SQL `NULL`.
#[tokio::test]
async fn a_function_root_refuses_a_claim_with_no_value_as_it_refuses_an_absent_one() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let [present, absent, no_value] = per_shape(&server, "{ tenantEchoes { seen } }").await;

    assert_eq!(
        present.1["data"]["tenantEchoes"],
        json!([{ "seen": "t1" }]),
        "control: {present:?}"
    );
    assert!(refused(&absent), "control: an absent claim is refused: {absent:?}");
    assert!(
        refused(&no_value),
        "a claim with no value must be refused like an absent one, not passed to the \
         function: {no_value:?}"
    );
    assert_eq!(no_value.1["errors"], absent.1["errors"], "the same refusal");
}

/// A list root's injected row filter: a claim with no value refuses the read, as an absent
/// claim does, rather than filtering on SQL `NULL`.
#[tokio::test]
async fn a_list_root_refuses_a_claim_with_no_value_as_it_refuses_an_absent_one() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let [present, absent, no_value] = per_shape(&server, "{ tenantRows { label } }").await;

    assert_eq!(
        present.1["data"]["tenantRows"],
        json!([{ "label": "t1-row" }]),
        "control: {present:?}"
    );
    assert!(refused(&absent), "control: an absent claim is refused: {absent:?}");
    assert!(
        refused(&no_value),
        "a claim with no value must be refused like an absent one: {no_value:?}"
    );
    assert_eq!(no_value.1["errors"], absent.1["errors"], "the same refusal");
}

/// A mutation's argument injected from the tenant claim: a claim with no value refuses the
/// write, as an absent claim does, and the function never runs.
#[tokio::test]
async fn a_mutation_refuses_a_tenant_claim_with_no_value_and_its_function_never_runs() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let write = |label: &str| format!(r#"mutation {{ recordTenant(label: "{label}") {{ id }} }}"#);
    let present =
        graphql(&server, &write("tenant-present"), &server.tenant_token(Claim::Present("t1")))
            .await;
    let absent =
        graphql(&server, &write("tenant-absent"), &server.tenant_token(Claim::Absent)).await;
    let no_value =
        graphql(&server, &write("tenant-no-value"), &server.tenant_token(Claim::NoValue)).await;

    assert_eq!(
        seen(&url, "tenant-present").await,
        vec![Some("t1".to_string())],
        "control: {present:?}"
    );
    assert!(refused(&absent), "control: an absent claim is refused: {absent:?}");
    assert_eq!(seen(&url, "tenant-absent").await, Vec::<Option<String>>::new(), "control");
    assert_eq!(
        seen(&url, "tenant-no-value").await,
        Vec::<Option<String>>::new(),
        "the function must not run for a claim with no value: {no_value:?}"
    );
    assert!(refused(&no_value), "refused like an absent claim: {no_value:?}");
    assert_eq!(no_value.1["errors"], absent.1["errors"], "the same refusal");
}

/// Control: an optional claim already passes SQL `NULL` both when it is absent and when it
/// has no value.
#[tokio::test]
async fn an_optional_claim_with_no_value_is_null_as_an_absent_one_is() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let write = |label: &str| format!(r#"mutation {{ recordActor(label: "{label}") {{ id }} }}"#);
    for (label, actor) in [
        ("actor-present", Claim::Present("a1")),
        ("actor-absent", Claim::Absent),
        ("actor-no-value", Claim::NoValue),
    ] {
        let mut extra = actor.extra("actor");
        extra["tenant_id"] = json!("t1");
        let answer = graphql(&server, &write(label), &server.token(&extra)).await;
        assert!(answer.1.get("errors").is_none(), "{label}: {answer:?}");
    }
    assert_eq!(seen(&url, "actor-present").await, vec![Some("a1".to_string())]);
    assert_eq!(seen(&url, "actor-absent").await, vec![None]);
    assert_eq!(seen(&url, "actor-no-value").await, vec![None]);
}

/// Control: a numeric tenant claim is a value, and still reaches the function.
#[tokio::test]
async fn a_numeric_tenant_claim_is_a_value() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let answer =
        graphql(&server, "{ tenantEchoes { seen } }", &server.token(&json!({ "tenant_id": 42 })))
            .await;
    assert_eq!(answer.1["data"]["tenantEchoes"], json!([{ "seen": "42" }]), "{answer:?}");
}

/// A session variable sourced from a claim: a claim with no value leaves it unset, as an
/// absent claim does, rather than setting it to the text `null`.
#[tokio::test]
async fn a_session_variable_from_a_claim_with_no_value_is_unset_as_for_an_absent_one() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let [present, absent, no_value] = per_shape(&server, "{ settingReaders { setting } }").await;

    assert_eq!(setting(&present.1).as_deref(), Some("t1"), "control: {present:?}");
    assert_eq!(setting(&absent.1), None, "control: an absent claim sets nothing: {absent:?}");
    assert_eq!(
        setting(&no_value.1),
        None,
        "a claim with no value must leave the variable unset: {no_value:?}"
    );
}

/// Identity enrichment binds `$region` from the token's claims: a claim with no value is not
/// bound, as an absent one is not, so enrichment is denied for both.
#[tokio::test]
async fn enrichment_does_not_bind_a_claim_with_no_value() {
    let Some(url) = database_url() else {
        return;
    };
    let server = serve(&url, true).await;
    let reader = "{ settingReaders { setting } }";
    let present = graphql(&server, reader, &server.token(&json!({ "region": "eu" }))).await;
    let absent = graphql(&server, reader, &server.token(&json!({}))).await;
    let no_value = graphql(&server, reader, &server.token(&json!({ "region": null }))).await;

    assert_eq!(
        setting(&present.1).as_deref(),
        Some("eu"),
        "control: enrichment is live: {present:?}"
    );
    assert!(
        absent.0 >= 400 || refused(&absent),
        "control: enrichment is denied for an absent claim: {absent:?}"
    );
    assert_eq!(
        (no_value.0, &no_value.1),
        (absent.0, &absent.1),
        "a claim with no value must be denied as an absent one is"
    );
}

/// The suite's schemas load with no database.
#[test]
fn the_document_loads_without_a_database() {
    for enrichment in [false, true] {
        let schema = schema(enrichment);
        CompiledSchema::from_json(&serde_json::to_string(&schema).unwrap(), false)
            .unwrap_or_else(|e| panic!("the claim-presence suite's schema must load: {e}"));
    }
}
