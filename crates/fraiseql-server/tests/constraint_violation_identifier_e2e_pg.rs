//! #1531: a constraint a mutation's function violates is served as the mutation's typed
//! error **naming the constraint**, never a row value.
//!
//! Since #1424 a class-23 SQLSTATE is answered by the mutation's single error member, but with
//! a generic message and nothing else, so a client could not tell which rule failed. It now
//! carries one `errors[]` entry, the shape a function's own `mutation_err_entries` gives:
//! `identifier` is the constraint's name, `code` the HTTP status, `details.sqlstate` the
//! SQLSTATE; under `mutation_constraint_metadata = "full"` also `details.table` and
//! `details.columns`, resolved from the catalog (`pg_constraint`, or `pg_index` for a unique
//! index). Under `"none"`, no entry.
//!
//! What PostgreSQL 18 reports decides what can be said (verified on 18.6): a partial unique
//! index is named as the violated "constraint"; a not-null violation names only its column,
//! and its catalogued not-null constraint is resolved from it; a foreign-key violation names
//! the **referencing** table in both directions, so its direction is not recoverable and both
//! stay `conflict` / 409 (pinned here, so splitting them later is a visible change).
//!
//! #1618: every field selected on the error member is served, in selection order, `null`
//! where the server has no value for it; and a REST read completes a field its document
//! lacks, through the same projectors.
//!
//! The typed error exists only for a mutation returning a union or interface with one error
//! member. `/graphql` serves it, and MCP since #1546 (`mcp_abstract_returns_e2e_pg`); REST
//! mounts no route for such a mutation (`rest/resource/mod.rs`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p1531_identifier` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::ConstraintMetadata,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, RestConfig, TypeDefinition, UnionDefinition,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1531_identifier";
const TAKEN_EMAIL: &str = "taken@example.com";
const TEAM: &str = "00000000-0000-0000-0000-0000000000a1";
const NO_TEAM: &str = "00000000-0000-0000-0000-0000000000ff";

/// A function `fn_<name>(args) RETURNS app.mutation_response` whose body is `dml`.
fn function(name: &str, args: &str, dml: &str) -> String {
    format!(
        "CREATE FUNCTION {SCHEMA}.fn_{name}({args}) RETURNS app.mutation_response \
         LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN {dml}; \
         v.succeeded := true; v.state_changed := true; RETURN v; END; $$"
    )
}

async fn provision(adapter: &PostgresAdapter) {
    let mut stmts = vec![
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
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_user (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
             email text, handle text CONSTRAINT tb_user_handle_key UNIQUE, deleted_at \
             timestamptz, name text NOT NULL, age int CONSTRAINT tb_user_age_check CHECK (age \
             >= 0))"
        ),
        format!(
            "CREATE UNIQUE INDEX tb_user_email_live_key ON {SCHEMA}.tb_user (email) WHERE \
             deleted_at IS NULL"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_user (email, handle, name) VALUES ('{TAKEN_EMAIL}', \
             'taken', 'n')"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_team (id uuid PRIMARY KEY)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_member (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
             fk_team uuid CONSTRAINT tb_member_fk_team REFERENCES {SCHEMA}.tb_team)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_team VALUES ('{TEAM}')"),
        format!("INSERT INTO {SCHEMA}.tb_member (fk_team) VALUES ('{TEAM}')"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_booking (during tsrange, CONSTRAINT tb_booking_no_overlap \
             EXCLUDE USING gist (during WITH &&))"
        ),
        format!("INSERT INTO {SCHEMA}.tb_booking VALUES (tsrange('2026-01-01', '2026-01-05'))"),
        format!(
            // A user with no email has no `email` key (#1618's REST read).
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_strip_nulls(jsonb_build_object('id', \
             id, 'email', email)) AS data FROM {SCHEMA}.tb_user"
        ),
        function(
            "create_user",
            "p_email text, p_handle text, p_name text, p_age int",
            &format!(
                "INSERT INTO {SCHEMA}.tb_user (email, handle, name, age) VALUES (p_email, \
                 p_handle, p_name, p_age)"
            ),
        ),
        function(
            "add_member",
            "p_team uuid",
            &format!("INSERT INTO {SCHEMA}.tb_member (fk_team) VALUES (p_team)"),
        ),
        function(
            "delete_team",
            "p_team uuid",
            &format!("DELETE FROM {SCHEMA}.tb_team WHERE id = p_team"),
        ),
        // A failure the function reports itself, with no `metadata` (#1618).
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_refuse() RETURNS app.mutation_response LANGUAGE plpgsql \
             AS $$ DECLARE v app.mutation_response; BEGIN v.succeeded := false; \
             v.state_changed := false; v.error_class := 'conflict'; v.http_status := 409; \
             v.message := 'refused'; RETURN v; END; $$"
        ),
        function(
            "book",
            "p_from text, p_to text",
            &format!(
                "INSERT INTO {SCHEMA}.tb_booking VALUES (tsrange(p_from::timestamp, \
                 p_to::timestamp))"
            ),
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// `User`, the error member `MutationError` (serving its `errors` entries), and one mutation
/// per function, each returning `WriteResult = User | MutationError`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", format!("{SCHEMA}.v_user"));
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("email", FieldType::String),
    ];
    let mut error = TypeDefinition::new("MutationError", "");
    error.is_error = true;
    error.fields = vec![
        FieldDefinition::new("status", FieldType::String),
        FieldDefinition::nullable("message", FieldType::String),
        FieldDefinition::nullable("httpStatus", FieldType::Int),
        FieldDefinition::nullable("errors", FieldType::Json),
        FieldDefinition::nullable("errorClass", FieldType::String),
        FieldDefinition::nullable("metadata", FieldType::Json),
    ];
    schema.types.extend([user, error]);
    schema.unions.push(UnionDefinition {
        name:         "WriteResult".to_string(),
        member_types: vec!["User".to_string(), "MutationError".to_string()],
        description:  None,
    });
    schema.queries.push(
        QueryDefinition::new("users", "User")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_user")),
    );
    let mutation = |name: &str, function: &str, args: Vec<ArgumentDefinition>| {
        let mut m = MutationDefinition::new(name, "WriteResult");
        m.sql_source = Some(format!("{SCHEMA}.fn_{function}"));
        m.operation = MutationOperation::Insert {
            table: "tb_user".to_string(),
        };
        m.arguments = args;
        m
    };
    let create_user = mutation(
        "createUser",
        "create_user",
        vec![
            ArgumentDefinition::optional("email", FieldType::String),
            ArgumentDefinition::optional("handle", FieldType::String),
            ArgumentDefinition::optional("name", FieldType::String),
            ArgumentDefinition::optional("age", FieldType::Int),
        ],
    );
    schema.mutations.extend([
        create_user,
        mutation(
            "addMember",
            "add_member",
            vec![ArgumentDefinition::new("team", FieldType::Uuid)],
        ),
        mutation(
            "deleteTeam",
            "delete_team",
            vec![ArgumentDefinition::new("team", FieldType::Uuid)],
        ),
        mutation(
            "book",
            "book",
            vec![
                ArgumentDefinition::new("from", FieldType::String),
                ArgumentDefinition::new("to", FieldType::String),
            ],
        ),
    ]);
    schema.mutations.push(mutation("refuse", "refuse", Vec::new()));
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve(metadata: ConstraintMetadata) -> Option<Running> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        mutation_constraint_metadata: metadata,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .unwrap()
        .with_rest_write_surface();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some(Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    })
}

const SELECTION: &str = "{ __typename ... on MutationError { status message httpStatus errors } }";

/// The mutation's typed error, with the whole response body checked for row values.
async fn typed_error(server: &Running, call: &str) -> Value {
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": format!("mutation {{ {call} {SELECTION} }}") }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let text = body.to_string();
    for value in [TAKEN_EMAIL, "Failing row", "Key (", "2026-01"] {
        assert!(
            !text.contains(value),
            "no row value or DETAIL in the response ({value}): {body}"
        );
    }
    let root = call.split('(').next().unwrap();
    let result = body["data"][root].clone();
    assert_eq!(
        result["__typename"], "MutationError",
        "{call}: served as the error member: {body}"
    );
    result
}

/// The one entry the typed error carries.
fn entry(result: &Value) -> &Value {
    let errors = result["errors"].as_array().unwrap_or_else(|| panic!("no errors[]: {result}"));
    assert_eq!(errors.len(), 1, "{result}");
    &errors[0]
}

fn assert_identifies(result: &Value, status: &str, code: u16, identifier: &str, sqlstate: &str) {
    assert_eq!(result["status"], status, "{result}");
    assert_eq!(result["httpStatus"], code, "{result}");
    let entry = entry(result);
    assert_eq!(entry["identifier"], identifier, "{result}");
    assert_eq!(entry["code"], code, "{result}");
    assert_eq!(entry["details"]["sqlstate"], sqlstate, "{result}");
}

#[tokio::test]
async fn every_class_23_violation_names_its_constraint() {
    let Some(server) = serve(ConstraintMetadata::Identifier).await else {
        eprintln!("skipping #1531: DATABASE_URL not set");
        return;
    };
    let cases = [
        // The issue's partial unique index.
        (
            format!("createUser(email: \"{TAKEN_EMAIL}\", name: \"x\")"),
            "conflict",
            409,
            "tb_user_email_live_key",
            "23505",
        ),
        // A plain UNIQUE constraint.
        (
            "createUser(handle: \"taken\", name: \"x\")".to_string(),
            "conflict",
            409,
            "tb_user_handle_key",
            "23505",
        ),
        // Not-null: PostgreSQL names the column; the catalogued constraint is resolved.
        (
            "createUser(email: \"a@b.c\")".to_string(),
            "validation",
            422,
            "tb_user_name_not_null",
            "23502",
        ),
        (
            "createUser(name: \"x\", age: -1)".to_string(),
            "validation",
            422,
            "tb_user_age_check",
            "23514",
        ),
        // Foreign key, both directions: the same conflict (direction is not recoverable).
        (
            format!("addMember(team: \"{NO_TEAM}\")"),
            "conflict",
            409,
            "tb_member_fk_team",
            "23503",
        ),
        (
            format!("deleteTeam(team: \"{TEAM}\")"),
            "conflict",
            409,
            "tb_member_fk_team",
            "23503",
        ),
        (
            "book(from: \"2026-01-03\", to: \"2026-01-07\")".to_string(),
            "conflict",
            409,
            "tb_booking_no_overlap",
            "23P01",
        ),
    ];
    for (call, status, code, identifier, sqlstate) in cases {
        let result = typed_error(&server, &call).await;
        assert_identifies(&result, status, code, identifier, sqlstate);
        assert!(entry(&result)["details"].get("table").is_none(), "identifier mode: {result}");
    }
}

#[tokio::test]
async fn full_metadata_adds_the_table_and_the_constraints_columns() {
    let Some(server) = serve(ConstraintMetadata::Full).await else {
        return;
    };
    for (call, table, columns) in [
        (
            format!("createUser(email: \"{TAKEN_EMAIL}\", name: \"x\")"),
            "tb_user",
            json!(["email"]),
        ),
        (
            "createUser(handle: \"taken\", name: \"x\")".to_string(),
            "tb_user",
            json!(["handle"]),
        ),
        ("createUser(email: \"a@b.c\")".to_string(), "tb_user", json!(["name"])),
        (format!("addMember(team: \"{NO_TEAM}\")"), "tb_member", json!(["fk_team"])),
    ] {
        let result = typed_error(&server, &call).await;
        let details = &entry(&result)["details"];
        assert_eq!(details["table"], table, "{call}: {result}");
        assert_eq!(details["columns"], columns, "{call}: {result}");
    }
}

#[tokio::test]
async fn no_metadata_keeps_the_status_and_says_nothing_more() {
    let Some(server) = serve(ConstraintMetadata::None).await else {
        return;
    };
    let result =
        typed_error(&server, &format!("createUser(email: \"{TAKEN_EMAIL}\", name: \"x\")")).await;
    assert_eq!(result["status"], "conflict", "{result}");
    assert_eq!(result["httpStatus"], 409, "{result}");
    assert!(result["errors"].is_null(), "no entry: {result}");
}

/// The common path: `auto_error_union` synthesizes `MutationError` and wraps an
/// object-returning mutation in `<Mutation>Result`. The synthesized member serves the entry
/// (it had no field to: a function's own `errors[]` was invisible there too).
#[tokio::test]
async fn the_synthesized_error_member_serves_the_entry() {
    use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};

    let Some(url) = try_database_url() else {
        return;
    };
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "User", "sql_source": format!("{SCHEMA}.v_user"), "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "email", "type": "String", "nullable": true }]
        }],
        "queries": [{
            "name": "users", "return_type": "User", "returns_list": true, "nullable": false,
            "sql_source": format!("{SCHEMA}.v_user"), "arguments": []
        }],
        "mutations": [{
            "name": "createUser", "return_type": "User", "operation": "insert",
            "sql_source": format!("{SCHEMA}.fn_create_user"),
            "arguments": [
                { "name": "email", "type": "String", "nullable": true },
                { "name": "handle", "type": "String", "nullable": true },
                { "name": "name", "type": "String", "nullable": true },
                { "name": "age", "type": "Int", "nullable": true }
            ]
        }]
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(
        intermediate,
        &ConvertOptions {
            auto_error_union: true,
        },
    )
    .unwrap()
    .schema;
    schema.build_indexes();

    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, None)).await.unwrap();
    let (shutdown, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let running = Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: shutdown,
    };
    let result =
        typed_error(&running, &format!("createUser(email: \"{TAKEN_EMAIL}\", name: \"x\")")).await;
    assert_identifies(&result, "conflict", 409, "tb_user_email_live_key", "23505");
}

/// The keys of `object`, in the order the response carries them.
fn keys(object: &Value) -> Vec<&str> {
    object
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {object}"))
        .keys()
        .map(String::as_str)
        .collect()
}

/// #1618: every field a client selects on the error member is in the response, in the order
/// it was selected, `null` where the server has no value for it — whether the database
/// raised the failure (a constraint) or the function returned it (with no `metadata`).
#[tokio::test]
async fn every_selected_field_of_the_error_member_completes_in_selection_order() {
    let Some(server) = serve(ConstraintMetadata::None).await else {
        eprintln!("skipping #1618: DATABASE_URL not set");
        return;
    };
    let selection = "{ __typename ... on MutationError { metadata errorClass status message \
                     errors httpStatus } }";
    for (call, status, message) in [
        (
            "createUser(email: \"a@b.c\")",
            "validation",
            "The request contains an invalid value",
        ),
        ("refuse", "conflict", "refused"),
    ] {
        let body: Value = reqwest::Client::new()
            .post(format!("{}/graphql", server.base))
            .json(&json!({ "query": format!("mutation {{ {call} {selection} }}") }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let root = call.split('(').next().unwrap();
        let result = &body["data"][root];
        assert_eq!(
            keys(result),
            [
                "__typename",
                "metadata",
                "errorClass",
                "status",
                "message",
                "errors",
                "httpStatus"
            ],
            "{call}: every selected field, in selection order: {body}"
        );
        assert_eq!(result["metadata"], Value::Null, "{call}: {body}");
        assert_eq!(result["errors"], Value::Null, "{call}: {body}");
        assert_eq!(result["errorClass"], status, "{call}: {body}");
        assert_eq!(result["status"], status, "{call}: {body}");
        assert_eq!(result["message"], message, "{call}: {body}");
    }
}

/// The generated clients declare `status` on every error type, whether or not the type
/// does, and read it to discriminate the failure: one the selection does not place is still
/// served, after the selected fields.
#[tokio::test]
async fn an_unselected_status_is_still_served_after_the_selection() {
    let Some(server) = serve(ConstraintMetadata::None).await else {
        return;
    };
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(
            &json!({ "query": "mutation { refuse { __typename ... on MutationError { message \
                                  metadata } } }" }),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let result = &body["data"]["refuse"];
    assert_eq!(keys(result), ["__typename", "message", "metadata", "status"], "{body}");
    assert_eq!(result["status"], "conflict", "{body}");
}

/// #1618 through MCP: the tool's selection takes every leaf of the error member, and every
/// one of them comes back, the ones the server has no value for as `null`.
#[tokio::test]
async fn an_mcp_tool_receives_every_field_of_the_error_member() {
    use fraiseql_core::{runtime::Executor, schema::McpConfig};
    use fraiseql_server::{mcp::handler::FraiseQLMcpService, routes::graphql::AppState};

    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema(), adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            read_only: false,
            ..McpConfig::default()
        },
    );
    for (tool, arguments) in [
        ("createUser", json!({ "email": "a@b.c" })),
        ("refuse", json!({})),
    ] {
        let result = service
            .call_tool_authenticated(
                tool,
                arguments.as_object(),
                None,
                "mcp-1618".to_string(),
                &axum::http::HeaderMap::new(),
            )
            .await;
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        let error = &body["data"][tool];
        assert_eq!(error["__typename"], "MutationError", "{tool}: {body}");
        for field in [
            "status",
            "message",
            "httpStatus",
            "errors",
            "errorClass",
            "metadata",
        ] {
            assert!(
                error.as_object().is_some_and(|o| o.contains_key(field)),
                "{tool}: `{field}` is selected and must be present: {body}"
            );
        }
        assert_eq!(error["metadata"], Value::Null, "{tool}: {body}");
    }
}

/// #1618 on the read side of the same projectors: a REST read selecting a field its document
/// does not carry gets it as `null`, in the order `select` names it, as `/graphql` does.
#[tokio::test]
async fn a_rest_read_completes_a_field_the_document_lacks() {
    let Some(server) = serve(ConstraintMetadata::None).await else {
        return;
    };
    let url = try_database_url().unwrap();
    PostgresAdapter::new(&url)
        .await
        .unwrap()
        .execute_raw_query(&format!("INSERT INTO {SCHEMA}.tb_user (name) VALUES ('no email')"))
        .await
        .unwrap();
    let body: Value = reqwest::Client::new()
        .get(format!("{}/rest/v1/users?select=email,id", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rows = body["data"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert_eq!(rows.len(), 2, "{body}");
    for row in rows {
        assert_eq!(keys(row), ["email", "id"], "{body}");
    }
    assert!(rows.iter().any(|row| row["email"].is_null()), "{body}");
}
