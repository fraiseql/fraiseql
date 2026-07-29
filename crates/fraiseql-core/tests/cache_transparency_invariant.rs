//! **The cache-correctness invariant, against real PostgreSQL.**
//!
//! For a randomised sequence of reads and writes, every read served from the
//! cache must equal the read the same sequence serves with caching switched off.
//! That one property subsumes the whole staleness class this phase closed:
//!
//! - #740 — a re-cached entry detached from its invalidation indexes,
//! - #741 — a CREATE stamping `entity_id` routed away from view invalidation,
//! - #742 — 0-row and 1-row results classified as "not a list" and never evicted,
//! - #763 — an UPDATE that moves a row *into* a cached result set.
//!
//! Each of those shows up here as one read whose answer differs between the two
//! runs. Nothing in this file names them, and that is the point: it does not test
//! four fixes, it tests the property they were violating.
//!
//! The op script is generated once from a fixed seed and replayed identically in
//! both runs, so a failure is reproducible and the two runs are comparable.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops the `p12_cache` schema → run
//! `--test-threads=1`.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: house SKIP pattern

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig},
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, TypeDefinition,
    },
};
use fraiseql_test_support::try_database_url;

const SCHEMA: &str = "p12_cache";

// ---------------------------------------------------------------------------
// The op script
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Op {
    /// `stamps_entity_id` chooses between the two configurations a hand-written
    /// PostgreSQL create function can have. Both must invalidate identically:
    /// stamping the id is what #741 mistook for "this is an UPDATE", and *not*
    /// stamping it is what routed #742's 0-row and 1-row results to a list
    /// classification they never satisfied.
    Create {
        id:               String,
        role:             String,
        stamps_entity_id: bool,
    },
    Update {
        id:   String,
        role: String,
    },
    Delete {
        id: String,
    },
    Read,
    /// Several identical reads issued at once.
    ///
    /// A purely sequential script can never re-cache a live entry — a `put`
    /// only follows a miss, and a miss only follows an eviction — so it cannot
    /// reach #740, where the entry displaced by a concurrent second `put` takes
    /// the live entry's invalidation-index registrations with it. Concurrent
    /// misses of one hot query are how that happens in production, and they are
    /// what makes the detached entry reachable here.
    ConcurrentRead,
}

/// Deterministic LCG — a fixed seed so a failing sequence is reproducible, and
/// no dependency on the ambient RNG.
struct Lcg(u64);

impl Lcg {
    #[allow(clippy::cast_possible_truncation)]
    // Reason: the high bits are the usable ones in an LCG; the result is reduced modulo a
    // small test-fixture bound immediately.
    fn next(&mut self, modulo: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % modulo
    }
}

fn uuid_for(n: usize) -> String {
    format!("{n:08x}-0000-4000-8000-000000000000")
}

/// A sequence that reaches every shape the invalidation logic distinguished:
/// reads over an empty table, over one row, and over several; creates that grow
/// the set; updates that change a row already in the set; deletes that shrink it.
///
/// A `Read` follows every write, because a stale entry is only observable on the
/// read *after* the write that should have evicted it.
fn script(steps: usize) -> Vec<Op> {
    let mut rng = Lcg(0x0C12_CACE_5EED_0001);
    let roles = ["admin", "member", "guest"];
    let mut live: Vec<String> = Vec::new();
    let mut ops = Vec::with_capacity(steps * 2);

    // Read the empty table first: the 0-row result is exactly the entry #742
    // classified as "not a list" and never evicted.
    ops.push(Op::Read);

    for i in 0..steps {
        let choice = rng.next(10);
        let role = roles[rng.next(roles.len())].to_string();
        let op = if live.is_empty() || choice < 4 {
            let id = uuid_for(i);
            live.push(id.clone());
            Op::Create {
                id,
                role,
                stamps_entity_id: i % 2 == 0,
            }
        } else if choice < 8 {
            let id = live[rng.next(live.len())].clone();
            Op::Update { id, role }
        } else {
            let idx = rng.next(live.len());
            Op::Delete {
                id: live.remove(idx),
            }
        };
        ops.push(op);
        // Warm the entry from several requests at once, then observe it: the
        // concurrent pair is what can produce a detached entry, and the read
        // after the *next* write is what exposes one.
        ops.push(Op::ConcurrentRead);
        ops.push(Op::Read);
    }
    ops
}

/// How many requests a `ConcurrentRead` issues at once.
const CONCURRENT_READERS: usize = 8;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// `app.mutation_response` plus an isolated `p12_cache` schema holding
/// `tb_item`, the `v_item` read view, and one function per mutation kind.
///
/// Every function stamps `entity_id` — the configuration #741 and #763 are
/// about, and the one a hand-written PostgreSQL mutation naturally produces.
async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE SCHEMA IF NOT EXISTS app").await.unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();

    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_item (id UUID PRIMARY KEY, role TEXT NOT NULL)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, \
             jsonb_build_object('id', id, 'role', role) AS data \
             FROM {SCHEMA}.tb_item ORDER BY id"
        ))
        .await
        .unwrap();

    for stmt in [
        format!(
            "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_create_item(p_id uuid, p_role text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             INSERT INTO {SCHEMA}.tb_item (id, role) VALUES (p_id, p_role); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'Item'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id, 'role', p_role); \
             RETURN v; END; $$"
        ),
        // The same INSERT without the `entity_id` stamp.
        format!(
            "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_create_item_anon(p_id uuid, p_role text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             INSERT INTO {SCHEMA}.tb_item (id, role) VALUES (p_id, p_role); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'Item'; \
             v.entity := jsonb_build_object('id', p_id, 'role', p_role); \
             RETURN v; END; $$"
        ),
        format!(
            "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_update_item(p_id uuid, p_role text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             UPDATE {SCHEMA}.tb_item SET role = p_role WHERE id = p_id; \
             v.succeeded := true; v.state_changed := true; v.message := 'updated'; \
             v.entity_type := 'Item'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id, 'role', p_role); \
             v.updated_fields := ARRAY['role']; \
             RETURN v; END; $$"
        ),
        format!(
            "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_delete_item(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             DELETE FROM {SCHEMA}.tb_item WHERE id = p_id; \
             v.succeeded := true; v.state_changed := true; v.message := 'deleted'; \
             v.entity_type := 'Item'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id); \
             RETURN v; END; $$"
        ),
    ] {
        adapter.execute_raw_query(&stmt).await.unwrap();
    }
}

fn item_mutation(
    name: &str,
    function: &str,
    op: MutationOperation,
    args: &[&str],
) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "Item");
    m.sql_source = Some(format!("{SCHEMA}.{function}"));
    m.operation = op;
    // Positional function arguments, in declaration order.
    m.arguments = args
        .iter()
        .map(|name| ArgumentDefinition::new(*name, FieldType::String))
        .collect();
    m
}

/// `cache_ttl_seconds = 0` — "no TTL, mutation-invalidated only", the annotation
/// under which a missed invalidation is permanent rather than merely long-lived.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut item = TypeDefinition::new("Item", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("role", FieldType::String),
    ];
    schema.types.push(item);

    let mut items = QueryDefinition::new("items", "Item")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_item"));
    items.cache_ttl_seconds = Some(0);
    schema.queries.push(items);

    schema.mutations.push(item_mutation(
        "createItem",
        "fn_create_item",
        MutationOperation::Insert {
            table: "tb_item".to_string(),
        },
        &["p_id", "p_role"],
    ));
    schema.mutations.push(item_mutation(
        "createItemAnon",
        "fn_create_item_anon",
        MutationOperation::Insert {
            table: "tb_item".to_string(),
        },
        &["p_id", "p_role"],
    ));
    schema.mutations.push(item_mutation(
        "updateItem",
        "fn_update_item",
        MutationOperation::Update {
            table: "tb_item".to_string(),
        },
        &["p_id", "p_role"],
    ));
    schema.mutations.push(item_mutation(
        "deleteItem",
        "fn_delete_item",
        MutationOperation::Delete {
            table: "tb_item".to_string(),
        },
        &["p_id"],
    ));
    schema
}

/// `(id, role)` pairs as served by `{ items { id role } }`.
fn rows_of(response: &serde_json::Value) -> Vec<(String, String)> {
    response["data"]["items"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r["id"].as_str().unwrap_or("?").to_string(),
                        r["role"].as_str().unwrap_or("?").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Replay `ops` against a freshly provisioned database, returning every read's answer.
async fn replay(url: &str, ops: &[Op], cache_enabled: bool) -> Vec<Vec<(String, String)>> {
    let adapter = PostgresAdapter::new(url).await.expect("connect");
    provision(&adapter).await;

    let config = if cache_enabled {
        CacheConfig::enabled()
    } else {
        CacheConfig::disabled()
    };
    let cached = CachedDatabaseAdapter::new(
        adapter,
        QueryResultCache::new(config),
        "p12-invariant".to_string(),
    )
    .with_cache_metadata_from_schema(&schema());

    // The change-log outbox needs `core.tb_entity_change_log`, which this
    // fixture does not install and which is not what is under test.
    let runtime = RuntimeConfig {
        changelog_enabled: false,
        ..RuntimeConfig::default()
    };
    let executor = Arc::new(Executor::with_config(schema(), Arc::new(cached), runtime));

    let mut answers = Vec::new();
    for op in ops {
        match op {
            Op::Read => {
                let response = executor
                    .execute("query { items { id role } }", None)
                    .await
                    .expect("read must succeed");
                answers.push(rows_of(&response));
            },
            Op::ConcurrentRead => {
                let mut set = tokio::task::JoinSet::new();
                for _ in 0..CONCURRENT_READERS {
                    let ex = Arc::clone(&executor);
                    set.spawn(async move {
                        ex.execute("query { items { id role } }", None)
                            .await
                            .expect("concurrent read must succeed")
                    });
                }
                // Every concurrent reader must answer identically; record one.
                let mut seen: Option<Vec<(String, String)>> = None;
                while let Some(joined) = set.join_next().await {
                    let rows = rows_of(&joined.expect("reader task must not panic"));
                    match &seen {
                        None => seen = Some(rows),
                        Some(first) => {
                            assert_eq!(&rows, first, "concurrent readers of one query disagreed");
                        },
                    }
                }
                answers.push(seen.expect("at least one concurrent reader"));
            },
            Op::Create {
                id,
                role,
                stamps_entity_id,
            } => {
                let vars = serde_json::json!({"p_id": id, "p_role": role});
                let doc = if *stamps_entity_id {
                    "mutation { createItem { id } }"
                } else {
                    "mutation { createItemAnon { id } }"
                };
                executor.execute(doc, Some(&vars)).await.expect("create must succeed");
            },
            Op::Update { id, role } => {
                let vars = serde_json::json!({"p_id": id, "p_role": role});
                executor
                    .execute("mutation { updateItem { id } }", Some(&vars))
                    .await
                    .expect("update must succeed");
            },
            Op::Delete { id } => {
                let vars = serde_json::json!({"p_id": id});
                executor
                    .execute("mutation { deleteItem { id } }", Some(&vars))
                    .await
                    .expect("delete must succeed");
            },
        }
    }
    answers
}

#[tokio::test]
async fn a_cached_read_always_equals_the_uncached_read() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: a_cached_read_always_equals_the_uncached_read — DATABASE_URL not set");
        return;
    };

    let ops = script(24);
    let uncached = replay(&url, &ops, false).await;
    let cached = replay(&url, &ops, true).await;

    assert_eq!(uncached.len(), cached.len(), "both runs execute the same script");

    // The fixture must actually vary, or "identical" proves nothing.
    let distinct: std::collections::HashSet<_> = uncached.iter().collect();
    assert!(
        distinct.len() > 3,
        "fixture check: the uncached run must produce varied results, saw {} distinct",
        distinct.len()
    );
    assert!(
        uncached.iter().any(std::vec::Vec::is_empty),
        "fixture check: the script must include a read over an empty result set"
    );
    assert!(
        uncached.iter().any(|rows| rows.len() == 1),
        "fixture check: the script must include a read over a single-row result set"
    );

    for (step, (want, got)) in uncached.iter().zip(cached.iter()).enumerate() {
        assert_eq!(
            got,
            want,
            "read #{step} differs with the cache enabled.\n  with cache: {got:?}\n  without:    \
             {want:?}\n  preceding ops: {:?}",
            &ops[..ops.iter().filter(|o| matches!(o, Op::Read)).count().min(step + 1)]
        );
    }
}
