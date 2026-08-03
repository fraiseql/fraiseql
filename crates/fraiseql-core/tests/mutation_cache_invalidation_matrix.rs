#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Post-mutation cache invalidation, asserted on the ANSWER (#741, #742, #763).
//!
//! Every case here warms the cache with a real query, changes what the database
//! would return, runs a real mutation through the executor, and re-runs the same
//! query. The assertion is on the rows served — not on an invalidation counter
//! and not on "no error". Three of the defects this suite pins are silent
//! wrong-answer bugs: the query succeeds and returns the pre-mutation row set.
//!
//! The matrix is {CREATE, UPDATE, DELETE} × {empty list, single-row list,
//! multi-row list} plus the `entity_id`-carrying CREATE that #741 misroutes.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{
        traits::{DatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, MutationDefinition, MutationOperation, SqlProjectionHint},
};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// A mock whose read result can be swapped between calls
// ---------------------------------------------------------------------------

/// Reads return whatever `rows` currently holds; the mutation function returns
/// `mutation_row`. Swapping `rows` between the mutation and the re-read is what
/// makes a stale cache hit visible as a wrong answer.
struct SwappableAdapter {
    rows:         Mutex<Vec<JsonbValue>>,
    mutation_row: HashMap<String, serde_json::Value>,
}

impl SwappableAdapter {
    const fn new(rows: Vec<JsonbValue>, mutation_row: HashMap<String, serde_json::Value>) -> Self {
        Self {
            rows: Mutex::new(rows),
            mutation_row,
        }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; implementations must match
// its transformed method signatures.
#[async_trait]
impl DatabaseAdapter for SwappableAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.rows.lock().expect("rows lock").clone())
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.rows.lock().expect("rows lock").clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            active_connections: 0,
            idle_connections:   1,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![self.mutation_row.clone()])
    }
}

impl SupportsMutations for SwappableAdapter {}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// `mutation_response.entity_id` is parsed as a UUID, so the row ids have to be
// real UUIDs for the entity-aware path to be exercised at all.
const ALICE: &str = "11111111-1111-4111-8111-111111111111";
const BOB: &str = "22222222-2222-4222-8222-222222222222";
const NEW: &str = "33333333-3333-4333-8333-333333333333";

fn user_row(id: &str, role: &str) -> JsonbValue {
    JsonbValue::new(json!({"id": id, "name": id, "role": role}))
}

/// A successful `app.mutation_response` row.
fn success_row(entity_id: Option<&str>) -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("ok"));
    row.insert("entity".to_string(), json!({"id": NEW, "name": NEW, "role": "admin"}));
    row.insert("entity_type".to_string(), json!("User"));
    row.insert(
        "entity_id".to_string(),
        entity_id.map_or(serde_json::Value::Null, |id| json!(id)),
    );
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert("metadata".to_string(), serde_json::Value::Null);
    row
}

fn mutation(name: &str, operation: MutationOperation) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "User");
    m.sql_source = Some(format!("fn_{name}"));
    m.operation = operation;
    m
}

/// One `User` type on `v_user`, a list query and a point-lookup query over it,
/// and one mutation of each kind. No mutation declares `invalidates_views` —
/// that is the configuration these three issues are about.
fn schema() -> CompiledSchema {
    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::new("role", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .with_sql_source("v_user")
                .build(),
        )
        .with_query(TestQueryBuilder::new("user", "User").with_sql_source("v_user").build())
        .with_mutation(mutation(
            "createUser",
            MutationOperation::Insert {
                table: "tb_user".to_string(),
            },
        ))
        .with_mutation(mutation(
            "updateUser",
            MutationOperation::Update {
                table: "tb_user".to_string(),
            },
        ))
        .with_mutation(mutation(
            "deleteUser",
            MutationOperation::Delete {
                table: "tb_user".to_string(),
            },
        ))
        .build()
}

/// Ids returned by `{ users { id } }`, in order.
fn read_user_ids(response: &serde_json::Value) -> Vec<String> {
    response
        .get("data")
        .and_then(|d| d.get("users"))
        .and_then(|u| u.as_array())
        .map(|rows| {
            rows.iter()
                .map(|r| r.get("id").and_then(serde_json::Value::as_str).unwrap_or("?").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Read `{ users { id } }`, run `mutation_query`, swap the database to `after`,
/// and return the ids the second read serves — with the cache on or off.
async fn ids_after_mutation(
    before: Vec<JsonbValue>,
    after: Vec<JsonbValue>,
    mutation_row: HashMap<String, serde_json::Value>,
    mutation_query: &str,
    cache_enabled: bool,
) -> Vec<String> {
    let config = if cache_enabled {
        CacheConfig::enabled()
    } else {
        CacheConfig::disabled()
    };
    let inner = SwappableAdapter::new(before, mutation_row);
    let cached = Arc::new(CachedDatabaseAdapter::new(
        inner,
        QueryResultCache::new(config),
        "test-v1".to_string(),
    ));
    let executor = Executor::new(schema(), Arc::clone(&cached));

    // 1. Warm the cache.
    executor.execute("query { users { id } }", None).await.expect("warm read");

    // 2. The database now holds the post-mutation truth.
    *cached.inner().rows.lock().expect("rows lock") = after;

    // 3. Run the mutation through the real executor path.
    executor.execute(mutation_query, None).await.expect("mutation must succeed");

    // 4. Re-read. A correct cache serves the new rows; a stale one serves the old.
    let again = executor.execute("query { users { id } }", None).await.expect("re-read");
    read_user_ids(&again)
}

/// The invariant, per case: the answer a cache-backed read serves after a
/// mutation is the answer the same sequence serves with caching switched off.
///
/// `expected` is asserted against the cache-off run first, so a fixture that
/// cannot distinguish the two states fails as a fixture bug rather than
/// silently passing.
async fn assert_cache_is_transparent(
    before: Vec<JsonbValue>,
    after: Vec<JsonbValue>,
    mutation_row: HashMap<String, serde_json::Value>,
    mutation_query: &str,
    expected: &[&str],
    issue: &str,
) {
    let uncached = ids_after_mutation(
        before.clone(),
        after.clone(),
        mutation_row.clone(),
        mutation_query,
        false,
    )
    .await;
    assert_eq!(uncached, expected, "fixture check: the cache-OFF answer must be {expected:?}");

    let cached = ids_after_mutation(before, after, mutation_row, mutation_query, true).await;
    assert_eq!(
        cached, uncached,
        "{issue}: after `{mutation_query}` the cached read served {cached:?}, but the same \
         sequence with caching disabled serves {uncached:?}"
    );
}

// ---------------------------------------------------------------------------
// CREATE — #742 (empty and single-row lists) and #741 (entity_id present)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_invalidates_an_empty_list_result() {
    // The query a CREATE most obviously affects: one that currently matches nothing.
    assert_cache_is_transparent(
        vec![],
        vec![user_row(NEW, "admin")],
        success_row(None),
        "mutation { createUser { id } }",
        &[NEW],
        "#742",
    )
    .await;
}

#[tokio::test]
async fn create_invalidates_a_single_row_list_result() {
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin")],
        vec![user_row(ALICE, "admin"), user_row(NEW, "admin")],
        success_row(None),
        "mutation { createUser { id } }",
        &[ALICE, NEW],
        "#742",
    )
    .await;
}

#[tokio::test]
async fn create_invalidates_a_multi_row_list_result() {
    // The one shape that already worked — kept as the control.
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        vec![
            user_row(ALICE, "admin"),
            user_row(BOB, "admin"),
            user_row(NEW, "admin"),
        ],
        success_row(None),
        "mutation { createUser { id } }",
        &[ALICE, BOB, NEW],
        "control",
    )
    .await;
}

#[tokio::test]
async fn create_that_returns_entity_id_still_invalidates_the_list() {
    // #741: stamping the new row's id on `mutation_response` routed the whole
    // mutation into entity-aware eviction, which can match nothing by construction —
    // no cached entry can contain an id that did not exist when it was cached.
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        vec![
            user_row(ALICE, "admin"),
            user_row(BOB, "admin"),
            user_row(NEW, "admin"),
        ],
        success_row(Some(NEW)),
        "mutation { createUser { id } }",
        &[ALICE, BOB, NEW],
        "#741",
    )
    .await;
}

// ---------------------------------------------------------------------------
// UPDATE — #763 (the row newly matches a cached list it was absent from)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_invalidates_a_list_the_row_newly_matches() {
    // `users(where: {role: "admin"})` held [alice]; bob is promoted to admin.
    // No cached entry mentions bob, so entity-aware eviction matches nothing.
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin")],
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        success_row(Some(BOB)),
        "mutation { updateUser { id } }",
        &[ALICE, BOB],
        "#763",
    )
    .await;
}

#[tokio::test]
async fn update_invalidates_a_list_the_row_no_longer_matches() {
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        vec![user_row(ALICE, "admin")],
        success_row(Some(BOB)),
        "mutation { updateUser { id } }",
        &[ALICE],
        "#763",
    )
    .await;
}

#[tokio::test]
async fn update_invalidates_an_entry_whose_rows_contain_the_entity() {
    // The case entity-aware eviction was built for — kept as the control.
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin"), user_row(BOB, "member")],
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        success_row(Some(BOB)),
        "mutation { updateUser { id } }",
        &[ALICE, BOB],
        "control",
    )
    .await;
}

// ---------------------------------------------------------------------------
// DELETE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_invalidates_a_list_containing_the_row() {
    assert_cache_is_transparent(
        vec![user_row(ALICE, "admin"), user_row(BOB, "admin")],
        vec![user_row(ALICE, "admin")],
        success_row(Some(BOB)),
        "mutation { deleteUser { id } }",
        &[ALICE],
        "delete",
    )
    .await;
}

#[tokio::test]
async fn delete_invalidates_a_single_row_list_result() {
    assert_cache_is_transparent(
        vec![user_row(BOB, "admin")],
        vec![],
        success_row(Some(BOB)),
        "mutation { deleteUser { id } }",
        &[],
        "delete",
    )
    .await;
}

// ---------------------------------------------------------------------------
// #761 — a query's declared secondary views
// ---------------------------------------------------------------------------

/// `report` reads `v_report`, which joins user data, so the query declares
/// `additional_views = ["v_user"]`. A `User` mutation invalidates `v_user`;
/// the cached report must go with it.
fn report_schema() -> CompiledSchema {
    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::new("role", FieldType::String).build())
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("Report", "v_report")
                .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("reports", "Report")
                .returns_list(true)
                .with_sql_source("v_report")
                // TTL 0 = "mutation-invalidated only"; also opts the view into
                // caching, without which this case would pass vacuously.
                .with_cache_ttl(0)
                .with_additional_views(vec!["v_user".to_string()])
                .build(),
        )
        .with_mutation(mutation(
            "updateUser",
            MutationOperation::Update {
                table: "tb_user".to_string(),
            },
        ))
        .build()
}

#[tokio::test]
async fn a_mutation_on_a_declared_secondary_view_invalidates_the_joined_query() {
    let inner = SwappableAdapter::new(
        vec![JsonbValue::new(json!({"id": ALICE, "name": "before"}))],
        success_row(Some(BOB)),
    );
    let cached = Arc::new(CachedDatabaseAdapter::new(
        inner,
        QueryResultCache::new(CacheConfig::enabled()),
        "test-v1".to_string(),
    ));
    // The runtime seam every server constructor goes through: without it, the
    // schema's `additional_views` never reach the cache.
    let cached = Arc::new(
        Arc::into_inner(cached)
            .expect("sole owner")
            .with_cache_metadata_from_schema(&report_schema()),
    );
    let executor = Executor::new(report_schema(), Arc::clone(&cached));

    let warm = executor
        .execute("query { reports { id name } }", None)
        .await
        .expect("warm read");
    assert_eq!(warm["data"]["reports"][0]["name"], json!("before"));

    *cached.inner().rows.lock().expect("rows lock") =
        vec![JsonbValue::new(json!({"id": ALICE, "name": "after"}))];

    executor
        .execute("mutation { updateUser { id } }", None)
        .await
        .expect("mutation");

    let again = executor.execute("query { reports { id name } }", None).await.expect("re-read");
    assert_eq!(
        again["data"]["reports"][0]["name"],
        json!("after"),
        "#761: a query declaring additional_views=[v_user] must be invalidated by a User mutation"
    );
}

// ---------------------------------------------------------------------------
// #623 — the exact shape a [[caching.rules]] entry lowers to: a TTL'd query
// plus a cross-view invalidation trigger declared on an unrelated mutation
// ---------------------------------------------------------------------------

/// The compiled output of:
/// `[[caching.rules]] query = "reports", ttl_seconds = 300,
///  invalidation_triggers = ["updateUser"]`
/// — a TTL on the query, and the query's view in the trigger mutation's
/// `invalidates_views`. The mutation's own return type is `User`/`v_user`, so
/// without the declared edge nothing would connect it to `v_report`.
fn caching_rule_schema() -> CompiledSchema {
    let mut trigger = mutation(
        "updateUser",
        MutationOperation::Update {
            table: "tb_user".to_string(),
        },
    );
    trigger.invalidates_views = vec!["v_report".to_string()];

    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::new("role", FieldType::String).build())
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("Report", "v_report")
                .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("reports", "Report")
                .returns_list(true)
                .with_sql_source("v_report")
                .with_cache_ttl(300)
                .build(),
        )
        .with_mutation(trigger)
        .build()
}

/// #623: a declared invalidation trigger — the compiled form of a
/// `[[caching.rules]] invalidation_triggers` entry — must serve fresh rows for
/// the TTL-cached query after the trigger mutation, even though the mutation's
/// own views share nothing with the query's.
#[tokio::test]
async fn a_caching_rule_trigger_invalidates_the_ttl_cached_query() {
    let inner = SwappableAdapter::new(
        vec![JsonbValue::new(json!({"id": ALICE, "name": "before"}))],
        success_row(Some(BOB)),
    );
    let cached = Arc::new(CachedDatabaseAdapter::new(
        inner,
        QueryResultCache::new(CacheConfig::enabled()),
        "test-v1".to_string(),
    ));
    // The runtime seam every server constructor goes through: the rule's TTL
    // reaches the cache only via the schema metadata (opt-in mode).
    let cached = Arc::new(
        Arc::into_inner(cached)
            .expect("sole owner")
            .with_cache_metadata_from_schema(&caching_rule_schema()),
    );
    let executor = Executor::new(caching_rule_schema(), Arc::clone(&cached));

    let warm = executor
        .execute("query { reports { id name } }", None)
        .await
        .expect("warm read");
    assert_eq!(warm["data"]["reports"][0]["name"], json!("before"));

    *cached.inner().rows.lock().expect("rows lock") =
        vec![JsonbValue::new(json!({"id": ALICE, "name": "after"}))];

    executor
        .execute("mutation { updateUser { id } }", None)
        .await
        .expect("mutation");

    let again = executor.execute("query { reports { id name } }", None).await.expect("re-read");
    assert_eq!(
        again["data"]["reports"][0]["name"],
        json!("after"),
        "#623: the declared invalidation trigger must evict the TTL-cached query"
    );
}
