#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #1398 — `node(id: x.id)` returns `x`.
//!
//! An object's `id` is its bare UUID (the Relay global id, ADR-0017), but `node(id:)`
//! accepted only `base64("Type:uuid")`, which nothing in production produces: a Relay
//! client could never refetch an object. Driven through the real executor against
//! PostgreSQL: the round trip, an unknown id, an id two Node types share, an id the
//! caller may not read, and the typed form.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, relay::encode_node_id},
    schema::CompiledSchema,
    security::SecurityContext,
};
use serde_json::{Value, json};

const SCHEMA: &str = "issue_1398";
const ALICE: &str = "00000000-0000-0000-0000-0000000000a1";
const ACME: &str = "00000000-0000-0000-0000-0000000000c1";

async fn provision(adapter: &PostgresAdapter) {
    for sql in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id uuid PRIMARY KEY, name text)"),
        format!("INSERT INTO {SCHEMA}.tb_user VALUES ('{ALICE}', 'Alice')"),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, \
             jsonb_build_object('id', id, 'name', name) AS data FROM {SCHEMA}.tb_user"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_user_card AS SELECT id, \
             jsonb_build_object('id', id, 'name', name) AS data FROM {SCHEMA}.tb_user"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_account (id uuid PRIMARY KEY, tenant text, name text)"),
        format!("INSERT INTO {SCHEMA}.tb_account VALUES ('{ACME}', 't1', 'Acme')"),
        format!(
            "CREATE VIEW {SCHEMA}.v_account AS SELECT id, \
             jsonb_build_object('id', id, 'name', name, 'tenant_id', tenant) AS data \
             FROM {SCHEMA}.tb_account"
        ),
    ] {
        adapter.execute_raw_query(&sql).await.unwrap();
    }
}

fn node_type(name: &str, view: &str) -> Value {
    json!({
        "name": name,
        "sql_source": format!("{SCHEMA}.{view}"),
        "relay": true,
        "fields": [
            { "name": "id", "field_type": "ID" },
            { "name": "name", "field_type": "String" }
        ]
    })
}

fn list_query(name: &str, return_type: &str, view: &str) -> Value {
    json!({
        "name": name,
        "return_type": return_type,
        "returns_list": true,
        "nullable": false,
        "sql_source": format!("{SCHEMA}.{view}")
    })
}

/// `User` and a tenant-scoped `Account`; with `with_card`, also `UserCard` and an unscoped
/// `AccountCard` over the same rows — Node types sharing an id space.
fn schema(with_card: bool) -> CompiledSchema {
    let mut types = vec![
        node_type("User", "v_user"),
        node_type("Account", "v_account"),
    ];
    let mut account = list_query("accounts", "Account", "v_account");
    account["inject_params"] = json!({ "tenant_id": { "source": "jwt", "claim": "tenant_id" } });
    let mut queries = vec![list_query("users", "User", "v_user"), account];
    if with_card {
        types.push(node_type("UserCard", "v_user_card"));
        queries.push(list_query("userCards", "UserCard", "v_user_card"));
        // Unscoped, over the same accounts the tenant-scoped `Account` reads.
        types.push(node_type("AccountCard", "v_account"));
        queries.push(list_query("accountCards", "AccountCard", "v_account"));
    }
    serde_json::from_value(json!({ "types": types, "queries": queries })).unwrap()
}

fn tenant(tenant: &str) -> SecurityContext {
    use fraiseql_core::types::{TenantId, UserId};
    SecurityContext {
        user_id:          UserId::new("u-1398"),
        roles:            vec![],
        tenant_id:        Some(TenantId::new(tenant)),
        scopes:           vec![],
        attributes:       std::collections::HashMap::from([(
            "tenant_id".to_string(),
            json!(tenant),
        )]),
        request_id:       "req-1398".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn executor(with_card: bool) -> Executor {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    Executor::new(schema(with_card), adapter)
}

fn node(id: &str) -> String {
    format!("{{ node(id: \"{id}\") {{ ... on User {{ id name }} ... on Account {{ id name }} }} }}")
}

#[tokio::test]
async fn an_objects_own_id_refetches_it_through_node() {
    let executor = executor(false).await;

    let listed = executor.execute("{ users { id name } }", None).await.unwrap();
    let id = listed["data"]["users"][0]["id"].as_str().unwrap().to_string();
    let refetched = executor.execute(&node(&id), None).await.unwrap();
    assert_eq!(refetched["data"]["node"], listed["data"]["users"][0], "{refetched}");

    // The typed form still names the type outright.
    let typed = executor.execute(&node(&encode_node_id("User", &id)), None).await.unwrap();
    assert_eq!(typed["data"]["node"]["name"], "Alice", "{typed}");

    // An id no Node type holds is `null`, not an error.
    let unknown = "00000000-0000-0000-0000-00000000ffff";
    let missing = executor.execute(&node(unknown), None).await.unwrap();
    assert_eq!(missing["data"]["node"], Value::Null, "{missing}");
}

/// The probe applies each type's own scoping: a row the caller may not read is not
/// found, so the id resolves to nothing rather than revealing where it lives.
#[tokio::test]
async fn a_bare_id_resolves_only_where_the_caller_may_read() {
    let executor = executor(false).await;

    let anonymous = executor.execute(&node(ACME), None).await.unwrap();
    assert_eq!(anonymous["data"]["node"], Value::Null, "anonymous: {anonymous}");

    let other = executor.execute_with_security(&node(ACME), None, &tenant("t2")).await.unwrap();
    assert_eq!(other["data"]["node"], Value::Null, "another tenant: {other}");

    let own = executor.execute_with_security(&node(ACME), None, &tenant("t1")).await.unwrap();
    assert_eq!(own["data"]["node"]["name"], "Acme", "its tenant: {own}");
}

/// Two Node types over one table share every id; the lookup refuses to guess.
#[tokio::test]
async fn an_id_two_node_types_share_is_refused_naming_both() {
    let executor = executor(true).await;

    let message = executor.execute(&node(ALICE), None).await.unwrap_err().to_string();
    assert!(
        message.contains("User") && message.contains("UserCard") && message.contains("base64"),
        "{message}"
    );

    let typed = executor.execute(&node(&encode_node_id("UserCard", ALICE)), None).await;
    assert!(typed.is_ok(), "the typed form resolves the ambiguity: {typed:?}");
}

/// The ambiguity check counts only the types the caller may read. A caller outside the
/// tenant sees `AccountCard` alone and gets it; reporting both would reveal that a
/// tenant-scoped `Account` holds the id.
#[tokio::test]
async fn ambiguity_is_judged_among_the_types_the_caller_may_read() {
    let executor = executor(true).await;
    let query = format!("{{ node(id: \"{ACME}\") {{ ... on AccountCard {{ name }} }} }}");

    let outsider = executor.execute_with_security(&query, None, &tenant("t2")).await.unwrap();
    assert_eq!(outsider["data"]["node"]["name"], "Acme", "{outsider}");

    let member = executor.execute_with_security(&query, None, &tenant("t1")).await;
    let message = member.unwrap_err().to_string();
    assert!(
        message.contains("Account,") || message.contains("Account, AccountCard"),
        "{message}"
    );
}
