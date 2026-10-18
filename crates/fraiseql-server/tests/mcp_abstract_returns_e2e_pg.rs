//! #1546: MCP calls an operation whose return type is a union or an interface.
//!
//! The tool executor built its document's selection from the return type's own scalar
//! fields, looked up among the schema's object types only. A union or an interface is not
//! there, so the document was `mutation { createUser(…) }`, refused before execution
//! (*Field of composite type `WriteResult` must have a selection set*): every tool for such
//! an operation failed, and under `auto_error_union` that is every mutation tool. Each call
//! here goes through the MCP service, as an agent's would, against PostgreSQL.
//!
//! #1615: the selection is the caller's: a field it may not read is not selected, so the
//! call is not refused for a field the agent never asked for.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p1546_mcp` schema → run `--test-threads=1`.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition,
        McpConfig, MutationDefinition, MutationOperation, QueryDefinition, TypeDefinition,
        UnionDefinition,
    },
};
use fraiseql_server::{mcp::handler::FraiseQLMcpService, routes::graphql::AppState};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1546_mcp";

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
             email text UNIQUE)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_user VALUES ('00000000-0000-0000-0000-000000001546', \
             'ada@example.com')"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_build_object('__typename', 'User', \
             'id', id, 'email', email, 'secret', 's3cret') AS data FROM {SCHEMA}.tb_user"
        ),
        // #1612: a second implementor, and a view holding rows of both, each naming its type.
        format!(
            "CREATE VIEW {SCHEMA}.v_bot AS SELECT '00000000-0000-0000-0000-000000000b07'::uuid AS id, \
             jsonb_build_object('__typename', 'Bot', 'id', '00000000-0000-0000-0000-000000000b07', \
             'email', 'bot@example.com', 'model', 'gpt') AS data"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_account AS SELECT * FROM {SCHEMA}.v_user \
             UNION ALL SELECT * FROM {SCHEMA}.v_bot"
        ),
        // A view whose rows name no type: the query's one implementor over it decides.
        format!(
            "CREATE VIEW {SCHEMA}.v_guest AS SELECT '00000000-0000-0000-0000-00000000c0de'::uuid \
             AS id, jsonb_build_object('id', '00000000-0000-0000-0000-00000000c0de', \
             'email', 'guest@example.com', 'visits', 3) AS data"
        ),
        // A taken email is answered with the mutation's typed error, not an exception.
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_user(p_email text) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; n uuid; BEGIN \
             IF EXISTS (SELECT 1 FROM {SCHEMA}.tb_user WHERE email = p_email) THEN \
               v.succeeded := false; v.state_changed := false; v.error_class := 'conflict'; \
               v.status_detail := 'email_taken'; v.message := 'email taken'; \
               v.http_status := 409; RETURN v; END IF; \
             INSERT INTO {SCHEMA}.tb_user (email) VALUES (p_email) RETURNING id INTO n; \
             v.succeeded := true; v.state_changed := true; v.entity_type := 'User'; \
             v.entity_id := n; v.entity := jsonb_build_object('id', n, 'email', p_email); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

fn user_type() -> TypeDefinition {
    let mut user = TypeDefinition::new("User", format!("{SCHEMA}.v_user"));
    user.implements = vec!["Account".to_string()];
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("email", FieldType::String),
    ];
    user
}

/// [`schema`] with `User.secret`, which requires the `read:secret` scope and refuses a
/// caller without it, `Bot.model`, which requires `read:model` and is masked, and
/// `Guest.visits`, gated by the field authorizer (#423).
fn schema_with_gated_fields() -> CompiledSchema {
    let mut schema = schema();
    let mut secret = FieldDefinition::nullable("secret", FieldType::String);
    secret.requires_scope = Some("read:secret".to_string());
    schema.types.iter_mut().find(|t| t.name == "User").unwrap().fields.push(secret);
    for td in &mut schema.types {
        for f in &mut td.fields {
            if td.name == "Bot" && f.name.as_str() == "model" {
                f.requires_scope = Some("read:model".to_string());
                f.on_deny = fraiseql_core::schema::FieldDenyPolicy::Mask;
            }
            if td.name == "Guest" && f.name.as_str() == "visits" {
                f.authorize = true;
            }
        }
    }
    schema.build_indexes();
    schema
}

fn guest_type() -> TypeDefinition {
    let mut guest = TypeDefinition::new("Guest", format!("{SCHEMA}.v_guest"));
    guest.implements = vec!["Account".to_string()];
    guest.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("email", FieldType::String),
        FieldDefinition::nullable("visits", FieldType::Int),
    ];
    guest
}

fn bot_type() -> TypeDefinition {
    let mut bot = TypeDefinition::new("Bot", format!("{SCHEMA}.v_bot"));
    bot.implements = vec!["Account".to_string()];
    bot.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("email", FieldType::String),
        FieldDefinition::nullable("model", FieldType::String),
    ];
    bot
}

/// `createUser: WriteResult` (`User | MutationError`), `accounts: [Account]` over the
/// interface `User` implements.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut error = TypeDefinition::new("MutationError", "");
    error.is_error = true;
    error.fields = vec![
        FieldDefinition::new("status", FieldType::String),
        FieldDefinition::nullable("message", FieldType::String),
        FieldDefinition::nullable("httpStatus", FieldType::Int),
    ];
    schema.types.extend([user_type(), bot_type(), guest_type(), error]);
    schema.unions.push(UnionDefinition {
        name:         "WriteResult".to_string(),
        member_types: vec!["User".to_string(), "MutationError".to_string()],
        description:  None,
    });
    schema.interfaces.push(
        InterfaceDefinition::new("Account")
            .with_field(FieldDefinition::nullable("email", FieldType::String)),
    );
    schema.queries.push(
        QueryDefinition::new("accounts", "Account")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_user")),
    );
    schema.queries.push(
        QueryDefinition::new("everyone", "Account")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_account")),
    );
    schema.queries.push(
        QueryDefinition::new("guests", "Account")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_guest")),
    );
    let mut create = MutationDefinition::new("createUser", "WriteResult");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_user"));
    create.operation = MutationOperation::Insert {
        table: "tb_user".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("email", FieldType::String)];
    schema.mutations.push(create);
    schema.build_indexes();
    schema
}

/// The same mutation authored without a union and compiled under `[fraiseql.mutations]
/// auto_error_union = true`, which rewrites its return type to `CreateUserResult`.
fn auto_error_union_schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "User", "sql_source": format!("{SCHEMA}.v_user"),
            "fields": [
                { "name": "id", "type": "ID", "nullable": false },
                { "name": "email", "type": "String", "nullable": true }
            ]
        }],
        "queries": [{ "name": "users", "return_type": "User", "returns_list": true,
                      "nullable": false, "sql_source": format!("{SCHEMA}.v_user") }],
        "mutations": [{ "name": "createUser", "return_type": "User", "nullable": false,
                        "sql_source": format!("{SCHEMA}.fn_create_user"), "operation": "CREATE",
                        "arguments": [{ "name": "email", "type": "String", "nullable": false }] }]
    }))
    .unwrap();
    let options = ConvertOptions {
        auto_error_union: true,
    };
    let mut schema = SchemaConverter::convert_artifact(intermediate, &options).unwrap().schema;
    schema.build_indexes();
    schema
}

/// Call `tool` through MCP: whether it failed, and the GraphQL response it carries.
async fn call(schema: CompiledSchema, url: &str, tool: &str, arguments: &Value) -> (bool, Value) {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema, adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            read_only: false,
            ..McpConfig::default()
        },
    );
    let result = service
        .call_tool_authenticated(
            tool,
            arguments.as_object(),
            None,
            "mcp-1546".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    let failed = result.is_error == Some(true);
    let response = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (failed, response)
}

async fn provisioned() -> Option<String> {
    let url = try_database_url()?;
    provision(&PostgresAdapter::new(&url).await.unwrap()).await;
    Some(url)
}

#[tokio::test]
async fn a_mutation_returning_a_union_is_called_and_says_which_member_it_is() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1546: DATABASE_URL not set");
        return;
    };

    let (failed, created) =
        call(schema(), &url, "createUser", &json!({ "email": "grace@example.com" })).await;
    let (_, refused) =
        call(schema(), &url, "createUser", &json!({ "email": "ada@example.com" })).await;

    assert!(!failed, "{created}");
    assert_eq!(created["data"]["createUser"]["__typename"], json!("User"), "{created}");
    assert_eq!(created["data"]["createUser"]["email"], json!("grace@example.com"));
    assert_eq!(refused["data"]["createUser"]["__typename"], json!("MutationError"), "{refused}");
    assert_eq!(refused["data"]["createUser"]["httpStatus"], json!(409), "{refused}");
}

#[tokio::test]
async fn a_query_returning_an_interface_is_called_with_its_fields() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1546: DATABASE_URL not set");
        return;
    };

    let (failed, response) = call(schema(), &url, "accounts", &json!({})).await;

    // The interface's fields, which implementor each row is, and that implementor's own
    // fields (#1612), as for a union.
    assert!(!failed, "{response}");
    assert_eq!(
        response["data"]["accounts"],
        json!([{
            "__typename": "User",
            "email": "ada@example.com",
            "id": "00000000-0000-0000-0000-000000001546"
        }]),
        "{response}"
    );
}

/// The GraphQL response of `query` against `schema` through the executor, anonymously.
async fn read_in(schema: CompiledSchema, url: &str, query: &str) -> Result<Value, String> {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    Executor::new(schema, adapter)
        .execute(query, None)
        .await
        .map_err(|e| e.to_string())
}

async fn read(url: &str, query: &str) -> Result<Value, String> {
    read_in(schema(), url, query).await
}

/// #1612: a read typed as an interface answers `__typename` with the object type each row
/// is, and applies the inline fragments it satisfies: from the one implementor whose view
/// the query reads, or from the row's own `__typename` when the view holds several.
#[tokio::test]
async fn a_read_typed_as_an_interface_resolves_each_rows_type() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1612: DATABASE_URL not set");
        return;
    };

    let one = read(&url, "{ accounts { __typename email ... on User { id } } }")
        .await
        .unwrap();
    let by_view = read(&url, "{ guests { __typename ... on Guest { visits } } }").await.unwrap();
    let several = read(
        &url,
        "{ everyone { __typename email ... on User { id } ... on Bot { model } } }",
    )
    .await
    .unwrap();

    assert_eq!(
        one["data"]["accounts"],
        json!([{
            "__typename": "User",
            "email": "ada@example.com",
            "id": "00000000-0000-0000-0000-000000001546"
        }]),
        "{one}"
    );
    assert_eq!(
        by_view["data"]["guests"],
        json!([{ "__typename": "Guest", "visits": 3 }]),
        "a row naming no type is of the one implementor over its view: {by_view}"
    );
    let mut rows: Vec<Value> = several["data"]["everyone"]
        .as_array()
        .unwrap_or_else(|| panic!("{several}"))
        .clone();
    rows.sort_by_key(|r| r["email"].to_string());
    assert_eq!(
        rows,
        [
            json!({ "__typename": "User", "email": "ada@example.com",
                    "id": "00000000-0000-0000-0000-000000001546" }),
            json!({ "__typename": "Bot", "email": "bot@example.com", "model": "gpt" }),
        ],
        "{several}"
    );
}

/// An implementor's scoped field, selected through an inline fragment on the interface, is
/// gated as the same field selected on the implementor is: refused to a caller without the
/// scope, never served.
#[tokio::test]
async fn a_scoped_field_in_a_fragment_on_an_interface_is_gated() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1612: DATABASE_URL not set");
        return;
    };

    let through_interface = read_in(
        schema_with_gated_fields(),
        &url,
        "{ accounts { email ... on User { secret } } }",
    )
    .await;

    let served = through_interface.as_ref().map(ToString::to_string).unwrap_or_default();
    assert!(!served.contains("s3cret"), "the scoped value is never served: {served}");
    assert!(
        through_interface
            .as_ref()
            .is_err_and(|e| e.contains("'secret' on type 'User'") && e.contains("scope")),
        "refused, naming the field and its type: {through_interface:?}"
    );
}

#[tokio::test]
async fn an_auto_error_union_mutation_is_called() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1546: DATABASE_URL not set");
        return;
    };

    let (failed, created) = call(
        auto_error_union_schema(),
        &url,
        "createUser",
        &json!({ "email": "hopper@example.com" }),
    )
    .await;

    assert!(!failed, "{created}");
    assert_eq!(created["data"]["createUser"]["__typename"], json!("User"), "{created}");
    assert_eq!(created["data"]["createUser"]["email"], json!("hopper@example.com"));
}

/// A masked field reached through a fragment is masked on the rows of its type, and a
/// policy-gated one (#423) refuses the read, as both do when selected on their type.
#[tokio::test]
async fn masked_and_policy_gated_fields_in_fragments_are_gated() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1612: DATABASE_URL not set");
        return;
    };

    let masked =
        read_in(schema_with_gated_fields(), &url, "{ everyone { email ... on Bot { model } } }")
            .await
            .unwrap();
    let policy =
        read_in(schema_with_gated_fields(), &url, "{ guests { ... on Guest { visits } } }").await;

    let bot = masked["data"]["everyone"]
        .as_array()
        .unwrap_or_else(|| panic!("{masked}"))
        .iter()
        .find(|r| r["email"] == "bot@example.com")
        .cloned()
        .unwrap_or_default();
    assert_eq!(bot, json!({ "email": "bot@example.com", "model": null }), "{masked}");
    assert!(
        policy.as_ref().is_err_and(|e| e.contains("Guest")),
        "a policy-gated field through an interface is refused: {policy:?}"
    );
}

// ── #1615: a tool selects what its caller may read ──────────────────────────────────

/// [`schema_with_gated_fields`] with `users: [User]`, and a `reader` role granting
/// `read:secret` and `read:model`; `default_role`, when given, is the role of an
/// authenticated principal that carries none (#894).
fn scoped_schema(default_role: Option<&str>) -> CompiledSchema {
    let mut schema = schema_with_gated_fields();
    schema.queries.push(
        QueryDefinition::new("users", "User")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_user")),
    );
    schema.security = Some(fraiseql_core::schema::SecurityConfig {
        role_definitions: vec![fraiseql_core::schema::RoleDefinition::new(
            "reader",
            vec!["read:secret".to_string(), "read:model".to_string()],
        )],
        default_role: default_role.map(ToString::to_string),
        ..fraiseql_core::schema::SecurityConfig::default()
    });
    schema.build_indexes();
    schema
}

fn principal(roles: &[&str]) -> fraiseql_core::security::SecurityContext {
    fraiseql_core::security::SecurityContext {
        user_id:          "agent".into(),
        roles:            roles.iter().map(|r| (*r).to_string()).collect(),
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-1615".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// Call `tool` as `caller` (anonymous when `None`): whether it failed, and its response.
async fn call_as(
    schema: CompiledSchema,
    url: &str,
    tool: &str,
    arguments: &Value,
    caller: Option<&fraiseql_core::security::SecurityContext>,
) -> (bool, Value) {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let executor = Executor::new(schema.clone(), adapter);
    let config = McpConfig {
        enabled: true,
        require_auth: false,
        read_only: false,
        ..McpConfig::default()
    };
    let sanitizer = fraiseql_server::config::error_sanitization::ErrorSanitizer::new(
        fraiseql_server::config::error_sanitization::ErrorSanitizationConfig::default(),
    );
    let result = fraiseql_server::mcp::executor::call_tool(
        tool,
        arguments.as_object(),
        &fraiseql_server::mcp::executor::McpCallContext {
            schema:           &schema,
            executor:         &executor,
            config:           &config,
            security_context: caller,
            error_sanitizer:  &sanitizer,
        },
    )
    .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    let response = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (result.is_error == Some(true), response)
}

/// The first row of `field` in a successful response.
fn first_row(failed: bool, response: &Value, field: &str) -> Value {
    assert!(!failed, "the call must succeed: {response}");
    response["data"][field][0].clone()
}

/// A field whose `requires_scope` the caller does not hold is not selected, so the call
/// answers the rest; a caller holding the scope, through a role or through the default role,
/// gets it. The tool used to select it for every caller, and the call was refused for a field
/// the agent never asked for.
#[tokio::test]
async fn a_tool_selects_the_scoped_fields_its_caller_may_read() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1615: DATABASE_URL not set");
        return;
    };
    let call = |schema, caller| {
        let url = url.clone();
        async move { call_as(schema, &url, "users", &json!({}), caller).await }
    };

    let (failed, anonymous) = call(scoped_schema(None), None).await;
    let anonymous = first_row(failed, &anonymous, "users");
    let unscoped_principal = principal(&[]);
    let (failed, unscoped) = call(scoped_schema(None), Some(&unscoped_principal)).await;
    let unscoped = first_row(failed, &unscoped, "users");
    let reader = principal(&["reader"]);
    let (failed, by_role) = call(scoped_schema(None), Some(&reader)).await;
    let by_role = first_row(failed, &by_role, "users");
    let (failed, by_default) = call(scoped_schema(Some("reader")), Some(&unscoped_principal)).await;
    let by_default = first_row(failed, &by_default, "users");

    for row in [&anonymous, &unscoped] {
        assert_eq!(row["email"], json!("ada@example.com"), "{row}");
        assert!(row.get("secret").is_none(), "an unreadable field is not selected: {row}");
    }
    assert_eq!(by_role["secret"], json!("s3cret"), "{by_role}");
    assert_eq!(by_default["secret"], json!("s3cret"), "{by_default}");
}

/// The same rule under a union member (a mutation's result) and an interface's
/// implementors, a masked field included: it is left out rather than served as `null`.
#[tokio::test]
async fn abstract_returns_select_what_their_caller_may_read() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1615: DATABASE_URL not set");
        return;
    };
    let unscoped = principal(&[]);
    let reader = principal(&["reader"]);

    let (failed, created) = call_as(
        scoped_schema(None),
        &url,
        "createUser",
        &json!({ "email": "lovelace@example.com" }),
        Some(&unscoped),
    )
    .await;
    let (failed_everyone, everyone) =
        call_as(scoped_schema(None), &url, "everyone", &json!({}), Some(&unscoped)).await;
    let (_, everyone_read) =
        call_as(scoped_schema(None), &url, "everyone", &json!({}), Some(&reader)).await;

    assert!(!failed, "{created}");
    assert_eq!(created["data"]["createUser"]["email"], json!("lovelace@example.com"));
    assert!(created["data"]["createUser"].get("secret").is_none(), "{created}");
    assert!(!failed_everyone, "{everyone}");
    let bot = |response: &Value| {
        response["data"]["everyone"]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .find(|r| r["__typename"] == "Bot")
            .cloned()
            .unwrap_or_default()
    };
    assert!(bot(&everyone).get("model").is_none(), "{everyone}");
    assert_eq!(bot(&everyone_read)["model"], json!("gpt"), "{everyone_read}");
    let user = everyone["data"]["everyone"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["__typename"] == "User")
        .cloned()
        .unwrap_or_default();
    assert!(user.get("secret").is_none(), "{everyone}");
}

/// A field the #423 authorizer decides cannot be read by a caller the authorizer cannot be
/// asked about (anonymous, or no authorizer configured): the engine refuses the whole read for
/// it, so the tool does not select it.
#[tokio::test]
async fn an_authorized_field_is_not_selected_when_it_cannot_be_authorized() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1615: DATABASE_URL not set");
        return;
    };

    let (failed, guests) = call_as(scoped_schema(None), &url, "guests", &json!({}), None).await;

    let guest = first_row(failed, &guests, "guests");
    assert_eq!(guest["email"], json!("guest@example.com"), "{guest}");
    assert!(guest.get("visits").is_none(), "{guest}");
}
