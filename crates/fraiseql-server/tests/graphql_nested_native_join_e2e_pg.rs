//! A GraphQL nested level read through its relationship correlates on the native columns
//! the relationship's `native_join` proves, so the target's index serves the join (#1887).
//!
//! The level is joined to its parent's rows when the target type's row predicate cannot be
//! evaluated over the embedded document: here a policy that does not declare the keys it
//! reads. The statement the executor ran is read back from `pg_stat_statements`, which the
//! pinned `pg_tviews` image preloads, and its plan is asserted with `EXPLAIN`.
//!
//! Requires `TVIEWS_DATABASE_URL` (`make test-pg-tviews`, or the `pg_tviews` CI leg).

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

#[allow(clippy::disallowed_types)]
// Reason: the test builds the policy it passes in, which is the assertion, not a production
// construction
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::Result,
    prelude::UserId,
    runtime::{Executor, RuntimeConfig},
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, NativeJoin, QueryDefinition,
        Relationship, TypeDefinition,
    },
    security::{
        CompiledRLSPolicy, ConstrainedPaths, RLSPolicy, RlsWhereClause, SecurityContext,
        rls_policy::{RLSRule, RlsTarget},
    },
    types::TenantId,
};
use serde_json::{Value, json};

const SCHEMA: &str = "p1887";

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

/// 2,000 organisations, 2,000 groups and 1,000 items of tenant `A`, analysed. The last item's
/// group is tenant `B`'s, which tenant `A` must not see. `v_item_doc` reads the same items
/// with each one's group document embedded, as a view denormalising its to-one relation
/// does.
async fn provision(client: &tokio_postgres::Client) {
    client
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_org (id bigint PRIMARY KEY, name text NOT NULL,
                 tenant_id text NOT NULL);
             CREATE TABLE {SCHEMA}.tb_group (id bigint PRIMARY KEY, name text NOT NULL,
                 tenant_id text NOT NULL, fk_org bigint NOT NULL REFERENCES {SCHEMA}.tb_org (id));
             CREATE TABLE {SCHEMA}.tb_item (id bigint PRIMARY KEY, name text NOT NULL,
                 tenant_id text NOT NULL,
                 fk_group bigint NOT NULL REFERENCES {SCHEMA}.tb_group (id));
             INSERT INTO {SCHEMA}.tb_org SELECT o, 'o' || o, 'A' FROM generate_series(1, 2000) o;
             INSERT INTO {SCHEMA}.tb_group
                 SELECT g, 'g' || g, 'A', g % 2000 + 1 FROM generate_series(1, 2000) g;
             INSERT INTO {SCHEMA}.tb_group VALUES (2001, 'theirs', 'B', 1);
             INSERT INTO {SCHEMA}.tb_item
                 SELECT i, 'i' || i, 'A', (i * 7) % 2000 + 1 FROM generate_series(1, 999) i;
             INSERT INTO {SCHEMA}.tb_item VALUES (1000, 'i1000', 'A', 2001);
             ANALYZE {SCHEMA}.tb_org, {SCHEMA}.tb_group, {SCHEMA}.tb_item;
             CREATE VIEW {SCHEMA}.v_org AS SELECT id, jsonb_build_object('id', id, 'name', name,
                 'tenant_id', tenant_id) AS data FROM {SCHEMA}.tb_org;
             CREATE VIEW {SCHEMA}.v_group AS SELECT id, fk_org, jsonb_build_object('id', id,
                 'name', name, 'tenant_id', tenant_id, 'org_id', fk_org) AS data
                 FROM {SCHEMA}.tb_group;
             CREATE VIEW {SCHEMA}.v_item AS SELECT id, fk_group, jsonb_build_object('id', id,
                 'name', name, 'tenant_id', tenant_id, 'group_id', fk_group) AS data
                 FROM {SCHEMA}.tb_item;
             CREATE VIEW {SCHEMA}.v_item_doc AS SELECT i.id, jsonb_build_object('id', i.id,
                 'tenant_id', i.tenant_id, 'group', jsonb_build_object('id', g.id, 'name',
                 g.name, 'tenant_id', g.tenant_id, 'org_id', g.fk_org)) AS data
                 FROM {SCHEMA}.tb_item i JOIN {SCHEMA}.tb_group g ON g.id = i.fk_group;"
        ))
        .await
        .unwrap();
}

fn unique(name: &str) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::Int);
    field.unique = true;
    field
}

/// A `ManyToOne` relationship `name` from `foreign_key` to `target`'s `id`, its native join
/// (`native_column`, child `child`, parent `parent`) declared when `native` is.
fn to_one(
    name: &str,
    target: &str,
    foreign_key: &str,
    native: Option<(&str, &str, &str)>,
) -> Relationship {
    Relationship {
        name:           name.to_string(),
        target_type:    target.to_string(),
        cardinality:    Cardinality::ManyToOne,
        foreign_key:    foreign_key.to_string(),
        referenced_key: "id".to_string(),
        native_join:    native.map(|(column, child, parent)| NativeJoin {
            foreign_key:    column.to_string(),
            referenced_key: "id".to_string(),
            child_view:     format!("{SCHEMA}.{child}"),
            parent_view:    format!("{SCHEMA}.{parent}"),
        }),
    }
}

/// `Item.group` and `Group.org`, their native joins declared when `native` is. `Org`
/// publishes no `tenant_id`, so a predicate on it is never evaluated over a document.
/// `ItemDoc` is an item whose group is read from its own document.
fn schema(native: bool) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut item = TypeDefinition::new("Item", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("group_id", FieldType::Int),
        FieldDefinition::nullable("group", FieldType::Object("Group".to_string())),
    ];
    item.relationships.push(to_one(
        "group",
        "Group",
        "group_id",
        native.then_some(("fk_group", "v_item", "v_group")),
    ));
    schema.types.push(item);

    let mut item_doc = TypeDefinition::new("ItemDoc", format!("{SCHEMA}.v_item_doc"));
    item_doc.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::nullable("group", FieldType::Object("Group".to_string())),
    ];
    schema.types.push(item_doc);

    let mut group = TypeDefinition::new("Group", format!("{SCHEMA}.v_group"));
    group.fields = vec![
        unique("id"),
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("org_id", FieldType::Int),
        FieldDefinition::nullable("org", FieldType::Object("Org".to_string())),
    ];
    group.relationships.push(to_one(
        "org",
        "Org",
        "org_id",
        native.then_some(("fk_org", "v_group", "v_org")),
    ));
    schema.types.push(group);

    let mut org = TypeDefinition::new("Org", format!("{SCHEMA}.v_org"));
    org.fields = vec![
        unique("id"),
        FieldDefinition::new("name", FieldType::String),
    ];
    schema.types.push(org);

    for (name, return_type, view) in [
        ("items", "Item", "v_item"),
        ("item_docs", "ItemDoc", "v_item_doc"),
        ("groups", "Group", "v_group"),
        ("orgs", "Org", "v_org"),
    ] {
        schema.queries.push(
            QueryDefinition::new(name, return_type)
                .returning_list()
                .with_sql_source(format!("{SCHEMA}.{view}")),
        );
    }
    schema.build_indexes();
    schema
}

/// Tenant isolation on every type, as a schema author declares it.
#[allow(clippy::disallowed_types)] // Reason: the test builds the policy it passes in, which is the assertion, not a production construction
fn tenant_rule() -> CompiledRLSPolicy {
    CompiledRLSPolicy::new(
        HashMap::new(),
        Some(RLSRule {
            name:              "tenant".to_string(),
            expression:        "user.tenant_id == object.tenant_id".to_string(),
            cacheable:         false,
            cache_ttl_seconds: None,
        }),
    )
}

/// [`tenant_rule`], not declaring the key it reads for the types named: a policy written
/// before `constrained_paths` existed, as far as those types are concerned.
struct TenantPolicy(&'static [&'static str]);

impl RLSPolicy for TenantPolicy {
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        tenant_rule().evaluate(context, target)
    }

    fn constrained_paths(&self, target: &RlsTarget<'_>) -> ConstrainedPaths {
        if target.type_name.is_some_and(|t| self.0.contains(&t)) {
            ConstrainedPaths::Opaque
        } else {
            tenant_rule().constrained_paths(target)
        }
    }
}

fn tenant_a() -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from("u-1887"),
        roles:            vec![],
        tenant_id:        Some(TenantId::from("A")),
        scopes:           vec![],
        attributes:       HashMap::from([("tenant_id".to_string(), json!("A"))]),
        request_id:       "req-1887".to_string(),
        ip_address:       None,
        authenticated_at: fraiseql_core::fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// What a scan node of a plan reads: its node type and its index, if any.
#[derive(Debug, PartialEq, Eq)]
struct Scan {
    node:  String,
    index: Option<String>,
}

/// Every scan of `relation` in `plan`.
fn scans_of(plan: &Value, relation: &str, out: &mut Vec<Scan>) {
    if plan["Relation Name"] == relation {
        out.push(Scan {
            node:  plan["Node Type"].as_str().unwrap().to_string(),
            index: plan["Index Name"].as_str().map(str::to_string),
        });
    }
    for child in plan["Plans"].as_array().into_iter().flatten() {
        scans_of(child, relation, out);
    }
}

/// The response to `query` over `schema(native)` for tenant `A` with `policy`, and the
/// plan of the one statement the executor ran over `view`.
///
/// `pg_stat_statements` records a statement with its constants as parameters, so the
/// plan is the generic one of that text. The correlation it is asked about holds no
/// constant: its join is the one the executor's statement was planned with.
async fn run(
    url: &str,
    observer: &tokio_postgres::Client,
    native: bool,
    policy: TenantPolicy,
    (view, query): (&str, &str),
) -> (Value, Value) {
    let schema = schema(native);
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let config = RuntimeConfig::from_compiled_schema(&schema)
        .unwrap()
        .with_rls_policy(Arc::new(policy));
    let executor = Executor::with_config(schema, adapter, config);

    observer.batch_execute("SELECT pg_stat_statements_reset()").await.unwrap();
    let response = executor.execute_with_security(query, None, &tenant_a()).await.unwrap();

    let statements = observer
        .query(
            &format!(
                "SELECT query FROM pg_stat_statements WHERE query LIKE '%\"{SCHEMA}\".\"{view}\"%' \
                 AND query NOT LIKE '%pg_stat_statements%'"
            ),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(statements.len(), 1, "one statement read `{view}`: {statements:?}");
    let statement: String = statements[0].get(0);
    // The simple protocol: over the extended one, the statement's `$n` would be parameters
    // of the `EXPLAIN` itself, which `GENERIC_PLAN` leaves untyped.
    let explained = observer
        .simple_query(&format!("EXPLAIN (GENERIC_PLAN, FORMAT JSON) {statement}"))
        .await
        .unwrap();
    let text = explained
        .iter()
        .find_map(|message| match message {
            tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_string),
            _ => None,
        })
        .expect("a plan");
    let plan: Value = serde_json::from_str(&text).unwrap();
    (response, plan[0]["Plan"].clone())
}

fn scans(plan: &Value, relation: &str) -> Vec<Scan> {
    let mut out = Vec::new();
    scans_of(plan, relation, &mut out);
    out
}

fn index_scan(index: &str) -> Scan {
    Scan {
        node:  "Index Scan".to_string(),
        index: Some(index.to_string()),
    }
}

macro_rules! url_or_skip {
    () => {
        match tviews_database_url() {
            Some(url) => url,
            None => {
                eprintln!("skipping: TVIEWS_DATABASE_URL not set");
                return;
            },
        }
    };
}

const GROUPS: (&str, &str) = ("v_item", "{ items { id group { id name } } }");
const ORGS: (&str, &str) = ("v_item", "{ items { id group { id org { id } } } }");
const EMBEDDED_ORGS: (&str, &str) = ("v_item_doc", "{ item_docs { id group { id org { id } } } }");

/// The level joined to the items reads `tb_group` by its primary key, on `v_item`'s
/// `fk_group`, and never scans it.
#[tokio::test]
async fn a_joined_level_reads_its_target_by_index_on_the_native_join() {
    let url = url_or_skip!();
    let observer = client(&url).await;
    provision(&observer).await;

    let (_, plan) = run(&url, &observer, true, TenantPolicy(&["Group"]), GROUPS).await;
    assert_eq!(scans(&plan, "tb_group"), [index_scan("tb_group_pkey")], "{plan:#}");
}

/// The control: without the native join the level correlates on the stored keys, which
/// no index serves. The assertion above can see a scan.
#[tokio::test]
async fn control_without_a_native_join_the_joined_level_scans_its_target() {
    let url = url_or_skip!();
    let observer = client(&url).await;
    provision(&observer).await;

    let (_, plan) = run(&url, &observer, false, TenantPolicy(&["Group"]), GROUPS).await;
    let groups = scans(&plan, "tb_group");
    assert!(groups.iter().any(|s| s.node == "Seq Scan"), "{groups:?}");
}

/// The native join changes how the rows are found, not which: the same response either
/// way, and another tenant's group answers `null`.
#[tokio::test]
async fn the_native_join_serves_the_same_rows() {
    let url = url_or_skip!();
    let observer = client(&url).await;
    provision(&observer).await;

    let (native, _) = run(&url, &observer, true, TenantPolicy(&["Group"]), GROUPS).await;
    let (stored, _) = run(&url, &observer, false, TenantPolicy(&["Group"]), GROUPS).await;
    assert_eq!(native, stored);

    let items = native["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1000);
    assert_eq!(items[0]["group"], json!({"id": 8, "name": "g8"}));
    let theirs = items.iter().find(|i| i["id"] == 1000).unwrap();
    assert_eq!(theirs["group"], Value::Null, "tenant B's group is not tenant A's");
}

/// A level joined below a joined level correlates on that level's view's columns: both
/// targets are read by index.
#[tokio::test]
async fn a_level_joined_below_a_joined_level_reads_its_target_by_index() {
    let url = url_or_skip!();
    let observer = client(&url).await;
    provision(&observer).await;

    let (response, plan) = run(&url, &observer, true, TenantPolicy(&["Group", "Org"]), ORGS).await;
    assert_eq!(scans(&plan, "tb_group"), [index_scan("tb_group_pkey")], "{plan:#}");
    assert_eq!(scans(&plan, "tb_org"), [index_scan("tb_org_pkey")], "{plan:#}");
    assert_eq!(response["data"]["items"][0]["group"], json!({"id": 8, "org": {"id": 9}}));
}

/// A level joined below a level read from `v_item_doc`'s embedded group documents: those
/// rows carry no `v_group` column, so it correlates on the stored keys, and answers the
/// same.
#[tokio::test]
async fn a_level_joined_below_an_embedded_level_answers_on_the_stored_keys() {
    let url = url_or_skip!();
    let observer = client(&url).await;
    provision(&observer).await;

    let (native, _) = run(&url, &observer, true, TenantPolicy(&[]), EMBEDDED_ORGS).await;
    let (stored, _) = run(&url, &observer, false, TenantPolicy(&[]), EMBEDDED_ORGS).await;
    assert_eq!(native, stored);
    let items = native["data"]["item_docs"].as_array().unwrap();
    assert_eq!(items.len(), 1000);
    assert_eq!(items[0]["group"], json!({"id": 8, "org": {"id": 9}}));
    let theirs = items.iter().find(|i| i["id"] == 1000).unwrap();
    assert_eq!(theirs["group"], Value::Null, "tenant B's group is not tenant A's");
}
