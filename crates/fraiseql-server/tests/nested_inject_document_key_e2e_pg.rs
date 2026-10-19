//! #1579: a type scoped by an `inject_params` column that is also a **native** column of its
//! view is embeddable, when the type publishes that key as a field.
//!
//! The convention, checked at `compile --database`: a native column and the document key of
//! the same name hold the same value. So a root read of `Customer` filters on the column
//! (indexed), and a `Customer` document another view embeds is filtered on its key, in the
//! composed read (`{ orders { customer { … } } }`), over a write's returned document
//! (`touchOrder`), and inside a relation filter (`orders(where: {customer: …})`). Before the
//! fix every one of them was refused (`row_gate` treated a native column as never in a
//! document). A type that does **not** publish the key is still refused: the rule fails
//! closed.
//!
//! Tenants `A` and `B`, one customer each. Order 1 (tenant `A`) embeds customer `A`; order 2
//! (tenant `A`) embeds customer `B`, the discriminating row: its customer is another tenant's,
//! so tenant `A` reads it as `null`. Order 3 is tenant `B`'s. Every order also embeds a
//! `Vault` document, a type scoped the same way that does not publish `tenant_id`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p1579_nested_inject` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    prelude::{DatabaseAdapter as _, UserId},
    runtime::{Executor, RuntimeConfig},
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InjectedParamSource,
        MutationDefinition, MutationOperation, NativeColumn, QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
    types::TenantId,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1579_nested_inject";
const TENANT_A: &str = "00000000-0000-0000-0000-00000000000a";
const TENANT_B: &str = "00000000-0000-0000-0000-00000000000b";

async fn provision(adapter: &PostgresAdapter) {
    let stmts = [
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
            "CREATE TABLE {SCHEMA}.tb_customer (id int PRIMARY KEY, tenant_id uuid NOT NULL, \
             name text NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_customer VALUES (1, '{TENANT_A}', 'ada'), (2, \
             '{TENANT_B}', 'bob')"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_vault (id int PRIMARY KEY, tenant_id uuid NOT NULL, \
             label text NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_vault VALUES (1, '{TENANT_A}', 'a-vault'), (2, \
             '{TENANT_B}', 'b-vault')"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id int PRIMARY KEY, tenant_id uuid NOT NULL, \
             fk_customer int NOT NULL, fk_vault int NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (1, '{TENANT_A}', 1, 1), (2, '{TENANT_A}', 2, \
             2), (3, '{TENANT_B}', 2, 2)"
        ),
        // `tenant_id` is a column next to `data`, and `Customer` publishes it in `data` too.
        format!(
            "CREATE VIEW {SCHEMA}.v_customer AS SELECT id, tenant_id, jsonb_build_object('id', \
             id, 'tenant_id', tenant_id, 'name', name) AS data FROM {SCHEMA}.tb_customer"
        ),
        // `Vault` keeps `tenant_id` as a column only.
        format!(
            "CREATE VIEW {SCHEMA}.v_vault AS SELECT id, tenant_id, jsonb_build_object('id', id, \
             'label', label) AS data FROM {SCHEMA}.tb_vault"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT o.id, o.tenant_id, jsonb_build_object('id', \
             o.id, 'customer', c.data, 'vault', v.data) AS data FROM {SCHEMA}.tb_order o JOIN \
             {SCHEMA}.v_customer c ON c.id = o.fk_customer JOIN {SCHEMA}.v_vault v ON v.id = \
             o.fk_vault"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_write (id int)"),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_touch_order(p_id int) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN INSERT INTO \
             {SCHEMA}.tb_write VALUES (p_id); v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'Order'; v.entity := (SELECT data FROM {SCHEMA}.v_order WHERE id \
             = p_id); RETURN v; END $$"
        ),
    ];
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// A list query over `view`, scoped by `tenant_id` from the token, read from the native
/// `uuid` column as `compile --database` records it.
fn scoped(name: &str, return_type: &str, view: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type)
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{view}"));
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    query.native_columns.insert(
        "tenant_id".to_string(),
        NativeColumn {
            pg_type:  "uuid".to_string(),
            not_null: false,
        },
    );
    query.auto_params.has_where = true;
    query
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut customer = TypeDefinition::new("Customer", format!("{SCHEMA}.v_customer"));
    customer.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenantId", FieldType::Uuid),
        FieldDefinition::new("name", FieldType::String),
    ];
    let mut vault = TypeDefinition::new("Vault", format!("{SCHEMA}.v_vault"));
    vault.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("label", FieldType::String),
    ];
    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::nullable("customer", FieldType::Object("Customer".to_string())),
        FieldDefinition::nullable("vault", FieldType::Object("Vault".to_string())),
    ];
    schema.types.extend([customer, vault, order]);
    schema.queries.extend([
        scoped("customers", "Customer", "v_customer"),
        scoped("vaults", "Vault", "v_vault"),
        scoped("orders", "Order", "v_order"),
    ]);
    let mut touch = MutationDefinition::new("touchOrder", "Order");
    touch.sql_source = Some(format!("{SCHEMA}.fn_touch_order"));
    touch.operation = MutationOperation::Update {
        table: "tb_order".to_string(),
    };
    touch.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
    schema.mutations.push(touch);
    schema.build_indexes();
    schema
}

fn tenant_a() -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from("u-a"),
        roles:            vec![],
        tenant_id:        Some(TenantId::from(TENANT_A)),
        scopes:           vec![],
        attributes:       HashMap::from([("tenant_id".to_string(), json!(TENANT_A))]),
        request_id:       "req-1579".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn rig() -> Option<(Executor, Arc<PostgresAdapter>)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let schema = schema();
    let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
    Some((Executor::with_config(schema, adapter.clone(), config), adapter))
}

async fn as_tenant_a(executor: &Executor, document: &str) -> Result<Value, FraiseQLError> {
    executor.execute_with_security(document, None, &tenant_a()).await
}

/// How many writes `touchOrder` performed.
async fn writes(adapter: &PostgresAdapter) -> usize {
    adapter
        .execute_raw_query(&format!("SELECT id FROM {SCHEMA}.tb_write"))
        .await
        .unwrap()
        .len()
}

macro_rules! rig_or_skip {
    () => {
        match rig().await {
            Some(rig) => rig,
            None => {
                eprintln!("skipping #1579: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// Control: the root read filters on the native column — tenant `A` reads its two orders and
/// its one customer, never tenant `B`'s.
#[tokio::test]
async fn control_root_reads_filter_on_the_native_column() {
    let (executor, _) = rig_or_skip!();
    let orders = as_tenant_a(&executor, "{ orders { id } }").await.unwrap();
    assert_eq!(orders["data"]["orders"], json!([{ "id": 1 }, { "id": 2 }]), "{orders}");
    let customers = as_tenant_a(&executor, "{ customers { name } }").await.unwrap();
    assert_eq!(customers["data"]["customers"], json!([{ "name": "ada" }]), "{customers}");
}

/// An embedded `Customer` is filtered on its document key: order 2's customer is tenant `B`'s
/// and reads as `null`.
#[tokio::test]
async fn an_embedded_document_is_filtered_on_the_key_it_publishes() {
    let (executor, _) = rig_or_skip!();
    let out = as_tenant_a(&executor, "{ orders { id customer { name } } }")
        .await
        .unwrap_or_else(|e| panic!("refused: {e}"));
    assert_eq!(
        out["data"]["orders"],
        json!([
            { "id": 1, "customer": { "name": "ada" } },
            { "id": 2, "customer": null }
        ]),
        "{out}"
    );
}

/// A write's returned document is filtered the same way, after the write.
#[tokio::test]
async fn a_mutation_result_embeds_the_document_filtered_on_its_key() {
    let (executor, adapter) = rig_or_skip!();
    for (id, customer) in [(1, json!({ "name": "ada" })), (2, Value::Null)] {
        let out = as_tenant_a(
            &executor,
            &format!("mutation {{ touchOrder(id: {id}) {{ id customer {{ name }} }} }}"),
        )
        .await
        .unwrap_or_else(|e| panic!("order {id} refused: {e}"));
        assert_eq!(out["data"]["touchOrder"], json!({ "id": id, "customer": customer }), "{out}");
    }
    assert_eq!(writes(&adapter).await, 2);
}

/// A filter through the relation reads the customer only where tenant `A` may read it, so
/// order 2 (whose customer is `bob`, tenant `B`'s) does not match `name = "bob"`.
#[tokio::test]
async fn a_relation_filter_reads_the_embedded_document_through_its_key() {
    let (executor, _) = rig_or_skip!();
    for (name, expected) in [("ada", json!([{ "id": 1 }])), ("bob", json!([]))] {
        let out = as_tenant_a(
            &executor,
            &format!(r#"{{ orders(where: {{customer: {{name: {{eq: "{name}"}}}}}}) {{ id }} }}"#),
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: refused: {e}"));
        assert_eq!(out["data"]["orders"], expected, "{name}: {out}");
    }
}

/// Fail closed: `Vault` is scoped by the same native column but does not publish it, so
/// nothing in its document can be filtered on. Reading it embedded is refused, by a read,
/// by a write before it runs, and by a relation filter.
#[tokio::test]
async fn a_type_that_does_not_publish_the_key_is_still_refused() {
    let (executor, adapter) = rig_or_skip!();
    let read = as_tenant_a(&executor, "{ orders { id vault { label } } }").await;
    assert!(matches!(read, Err(FraiseQLError::Authorization { .. })), "{read:?}");
    let write =
        as_tenant_a(&executor, "mutation { touchOrder(id: 1) { id vault { label } } }").await;
    assert!(matches!(write, Err(FraiseQLError::Authorization { .. })), "{write:?}");
    assert_eq!(writes(&adapter).await, 0, "refused before the write");
    let filter =
        as_tenant_a(&executor, r#"{ orders(where: {vault: {label: {eq: "a-vault"}}}) { id } }"#)
            .await;
    assert!(matches!(filter, Err(FraiseQLError::Authorization { .. })), "{filter:?}");
}
