//! Unit tests for composed direct reads (`query_composed`).
//!
//! What these pin is the plan: which gate each embedded level passes, which predicate it
//! carries, which keys it may return, what the tree is charged — all decided before the
//! adapter is called, so each is asserted on the [`ComposedLevel`] the adapter receives,
//! or on the adapter never being called. Whether PostgreSQL answers the right rows for
//! that level is `rest_embedding_composed_gates_e2e_pg`'s question.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::json;

use crate::{
    backend::{
        ComposedLevel, EmbedShape, JsonbValue, LevelKeys, ScalarFieldType, WhereClause,
        WhereOperator,
    },
    error::{FraiseQLError, Result},
    graphql::{DirectReadProjection, estimate_direct_read_cost},
    runtime::{
        CountSelection, EmbedSelection, Executor, QueryMatch, RuntimeConfig,
        executor::test_support::CapturingMockAdapter,
    },
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        Relationship, TypeDefinition,
    },
    security::{RLSPolicy, RlsWhereClause, SecurityContext, rls_policy::RlsTarget},
};

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

/// A policy that scopes each type by a column only that type has, so which type's
/// predicate a level carries is visible in the level's `WHERE`.
struct PerTypePolicy;

impl RLSPolicy for PerTypePolicy {
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        let column = match target.type_name {
            Some("User") => "team_id",
            Some("Order") => "owner_id",
            other => {
                return Err(FraiseQLError::Validation {
                    message: format!("no policy for {other:?}"),
                    path:    None,
                });
            },
        };
        Ok(Some(RlsWhereClause::new(WhereClause::Field {
            path:     vec![column.to_string()],
            operator: WhereOperator::Eq,
            value:    json!(context.user_id.to_string()),
        })))
    }
}

fn scoped(
    name: &str,
    field_type: FieldType,
    scope: &str,
    on_deny: FieldDenyPolicy,
) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, field_type);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = on_deny;
    field
}

fn list_query(name: &str, return_type: &str, view: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type);
    query.returns_list = true;
    query.sql_source = Some(view.to_string());
    query
}

/// `User` with `orders` (to-many) and `Order` with `user` (to-one), each gated by fields
/// only the other type does not have.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("name", FieldType::String),
        ],
        relationships: vec![Relationship {
            name:           "orders".to_string(),
            target_type:    "Order".to_string(),
            cardinality:    Cardinality::OneToMany,
            foreign_key:    "fk_user".to_string(),
            referenced_key: "id".to_string(),
        }],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("fk_user", FieldType::Int),
            FieldDefinition::new("total", FieldType::Int),
            scoped("margin", FieldType::Int, "read:margin", FieldDenyPolicy::Mask),
            scoped("cost_price", FieldType::Int, "read:cost", FieldDenyPolicy::Reject),
        ],
        relationships: vec![Relationship {
            name:           "user".to_string(),
            target_type:    "User".to_string(),
            cardinality:    Cardinality::ManyToOne,
            foreign_key:    "fk_user".to_string(),
            referenced_key: "id".to_string(),
        }],
        ..TypeDefinition::new("Order", "v_order")
    });
    schema.queries.push(list_query("users", "User", "v_user"));
    schema.queries.push(list_query("orders", "Order", "v_order"));
    // Field-level RBAC is inert without a security section: a fixture lacking one would
    // assert the permissive shape whatever the composition did.
    schema.security = Some(crate::schema::SecurityConfig::default());
    schema.build_indexes();
    schema
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          crate::types::UserId::new("u-7"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-composed".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn users(schema: &CompiledSchema, fields: &[&str], limit: Option<u32>) -> QueryMatch {
    let query = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
    let mut arguments = std::collections::HashMap::new();
    if let Some(limit) = limit {
        arguments.insert("limit".to_string(), json!(limit));
    }
    QueryMatch::from_operation(
        query,
        fields.iter().map(ToString::to_string).collect(),
        arguments,
        schema.find_type("User"),
    )
    .unwrap()
}

fn orders(fields: &[&str]) -> EmbedSelection {
    EmbedSelection {
        relationship: "orders".to_string(),
        output_key: "orders".to_string(),
        fields: fields.iter().map(ToString::to_string).collect(),
        limit: Some(100),
        ..EmbedSelection::default()
    }
}

struct Run {
    adapter: Arc<CapturingMockAdapter>,
    result:  Result<serde_json::Value>,
}

async fn run(
    schema: CompiledSchema,
    config: RuntimeConfig,
    query_match: &QueryMatch,
    embeds: &[EmbedSelection],
    counts: &[CountSelection],
    rows: Vec<serde_json::Value>,
) -> Run {
    let adapter =
        Arc::new(CapturingMockAdapter::new(rows.into_iter().map(JsonbValue::new).collect()));
    let executor = Executor::read_only_with_config(schema, adapter.clone(), config);
    let ctx = principal();
    let result = executor
        .execute_query_composed(query_match, embeds, counts, None, Some(&ctx), None)
        .await;
    Run { adapter, result }
}

fn only_embed(level: &ComposedLevel) -> &crate::backend::ComposedEmbed {
    assert_eq!(level.embeds.len(), 1, "one embed: {level:?}");
    &level.embeds[0]
}

/// Every `path` the clause constrains, depth-first.
fn constrained_paths(clause: Option<&WhereClause>) -> Vec<String> {
    fn walk(clause: &WhereClause, out: &mut Vec<String>) {
        match clause {
            WhereClause::Field { path, .. } => out.push(path.join(".")),
            WhereClause::NativeField { column, .. } => out.push(column.clone()),
            WhereClause::And(cs) | WhereClause::Or(cs) => cs.iter().for_each(|c| walk(c, out)),
            WhereClause::Not(c) => walk(c, out),
            WhereClause::Typed { inner, .. } => walk(inner, out),
            other => panic!("a clause this walk does not know: {other:?}"),
        }
    }
    let mut out = Vec::new();
    if let Some(clause) = clause {
        walk(clause, &mut out);
    }
    out
}

// ---------------------------------------------------------------------------
// Each level is gated as a read of its own target
// ---------------------------------------------------------------------------

// Why this is a unit test and not only an e2e one: the real-database fixture
// (`rest_embedding_composed_gates_e2e_pg`) scopes `User` and `Order` by the same `owner`
// predicate, so a lateral that inherited its parent's predicate renders the same SQL and the
// "lateral takes the parent's predicate" mutation survives the whole e2e suite. Only a policy
// that scopes each type by a column the other lacks, as `PerTypePolicy` does, tells them apart.
/// The ruling's first clause: the target's RLS predicate goes in the lateral's `WHERE`.
///
/// The parent's predicate says nothing about the tenant of the rows a view embeds, so a
/// level carrying the parent's — or none — serves another principal's orders under this
/// principal's users.
#[tokio::test]
async fn an_embedded_level_carries_its_targets_rls_predicate_not_its_parents() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(PerTypePolicy));

    let run = run(schema, config, &qm, &[orders(&["id"])], &[], vec![]).await;
    run.result.unwrap();

    let read = run.adapter.captured_composed().expect("the statement is sent");
    assert_eq!(constrained_paths(read.where_clause.as_ref()), ["team_id"], "the root: {read:?}");
    let nested = &only_embed(&read).level;
    assert_eq!(
        constrained_paths(nested.where_clause.as_ref()),
        ["owner_id"],
        "the embedded level is scoped by Order's policy and by nothing of User's: {nested:?}"
    );
}

/// A policy that cannot answer for the target refuses the request — it does not serve the
/// embed unscoped. Fail closed at the level, as a flat read of the target does (#784).
#[tokio::test]
async fn a_policy_that_refuses_the_target_refuses_the_request_before_it_is_sent() {
    struct UserOnly;
    impl RLSPolicy for UserOnly {
        fn evaluate(
            &self,
            ctx: &SecurityContext,
            target: &RlsTarget<'_>,
        ) -> Result<Option<RlsWhereClause>> {
            if target.type_name == Some("User") {
                PerTypePolicy.evaluate(ctx, target)
            } else {
                Err(FraiseQLError::Authorization {
                    message:  "no".to_string(),
                    action:   None,
                    resource: None,
                })
            }
        }
    }
    let schema = schema();
    let qm = users(&schema, &["id"], None);
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(UserOnly));

    let run = run(schema, config, &qm, &[orders(&["id"])], &[], vec![]).await;

    assert!(
        matches!(run.result, Err(FraiseQLError::Authorization { .. })),
        "{:?}",
        run.result
    );
    assert!(run.adapter.captured_composed().is_none(), "nothing is sent");
}

/// The ruling's second clause: `Mask` becomes `NULL` in the level's document.
///
/// `margin` is `Order`'s, not `User`'s. Classified against the **parent** type — one
/// classification for the request, the mistake genuinely available here — it is a field
/// `User` does not gate, and it is read out of the database in full.
#[tokio::test]
async fn a_masked_field_of_the_target_is_null_in_the_statement() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);

    let run = run(
        schema,
        RuntimeConfig::default(),
        &qm,
        &[orders(&["id", "total", "margin"])],
        &[],
        vec![],
    )
    .await;
    run.result.unwrap();

    let read = run.adapter.captured_composed().unwrap();
    assert_eq!(read.keys, LevelKeys::Whole, "the root is projected in Rust, as a flat read is");
    assert_eq!(
        only_embed(&read).level.keys,
        LevelKeys::Only {
            kept:   vec!["id".to_string(), "total".to_string()],
            masked: vec!["margin".to_string()],
        },
    );
}

/// The ruling's third clause: `Reject` is refused before sending.
#[tokio::test]
async fn a_rejected_field_of_the_target_refuses_the_request_before_it_is_sent() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);

    let run = run(
        schema,
        RuntimeConfig::default(),
        &qm,
        &[orders(&["id", "cost_price"])],
        &[],
        vec![],
    )
    .await;

    match run.result {
        Err(FraiseQLError::Authorization { resource, .. }) => {
            assert_eq!(resource.as_deref(), Some("Order.cost_price"));
        },
        other => panic!("expected the target's Reject: {other:?}"),
    }
    assert!(run.adapter.captured_composed().is_none(), "nothing is sent");
}

/// The target **query's** role gate applies to its level, as it did to the sub-read.
#[tokio::test]
async fn the_target_querys_role_gate_applies_to_its_level() {
    let mut schema = schema();
    schema.queries.iter_mut().find(|q| q.name == "orders").unwrap().requires_role =
        Some("auditor".to_string());
    let qm = users(&schema, &["id"], None);

    let run = run(schema, RuntimeConfig::default(), &qm, &[orders(&["id"])], &[], vec![]).await;

    assert!(
        run.result.is_err(),
        "a caller without the role reads no orders: {:?}",
        run.result
    );
    assert!(run.adapter.captured_composed().is_none(), "nothing is sent");
}

/// A count is a read of the target too, and is gated as one.
#[tokio::test]
async fn a_count_carries_its_targets_predicate() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(PerTypePolicy));
    let count = CountSelection {
        relationship: "orders".to_string(),
        output_key:   "orders_count".to_string(),
        filter:       None,
    };

    let run = run(schema, config, &qm, &[], &[count], vec![]).await;
    run.result.unwrap();

    let embed = only_embed(&run.adapter.captured_composed().unwrap()).clone();
    assert_eq!(embed.shape, EmbedShape::Count);
    assert_eq!(constrained_paths(embed.level.where_clause.as_ref()), ["owner_id"]);
}

#[tokio::test]
async fn a_relationship_the_parent_does_not_declare_is_refused() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);
    let mut invoices = orders(&["id"]);
    invoices.relationship = "invoices".to_string();

    let run = run(schema, RuntimeConfig::default(), &qm, &[invoices], &[], vec![]).await;

    match run.result {
        Err(FraiseQLError::Validation { message, .. }) => {
            assert!(message.contains("has no relationship 'invoices'"), "{message}");
        },
        other => panic!("expected a validation error: {other:?}"),
    }
    assert!(run.adapter.captured_composed().is_none());
}

// ---------------------------------------------------------------------------
// The statement's shape
// ---------------------------------------------------------------------------

/// Each side of the join key in the spelling and type the flat sub-read compared.
#[tokio::test]
async fn the_correlation_joins_the_declared_keys_as_their_declared_type() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);

    let run = run(schema, RuntimeConfig::default(), &qm, &[orders(&["id"])], &[], vec![]).await;
    run.result.unwrap();

    let read = run.adapter.captured_composed().unwrap();
    let embed = only_embed(&read);
    assert_eq!(
        embed.source,
        crate::backend::EmbedSource::Correlated {
            target_key: vec!["fk_user".to_string()],
            parent_key: vec!["id".to_string()],
            key_type:   ScalarFieldType::Integer,
        },
        "Order.fk_user is an Int"
    );
    assert_eq!(embed.shape, EmbedShape::Many);
    assert_eq!(embed.level.limit, Some(100), "the embed's own page");
    assert!(read.where_clause.is_none(), "no correlation leaks into the root");
}

/// #1329: a function-backed list query has no relation to embed from. The relationship
/// is answered empty without a read, as it was, and the rest of the statement is sent.
#[tokio::test]
async fn a_target_with_only_a_function_backed_list_query_embeds_nothing() {
    let mut schema = schema();
    let query = schema.queries.iter_mut().find(|q| q.name == "orders").unwrap();
    query.function = Some("fn_orders".to_string());
    let qm = users(&schema, &["id"], None);

    let run = run(
        schema,
        RuntimeConfig::default(),
        &qm,
        &[orders(&["id"])],
        &[],
        vec![json!({"d": {"id": "u1"}, "e": {}})],
    )
    .await;

    let body = run.result.unwrap();
    assert!(run.adapter.captured_composed().unwrap().embeds.is_empty(), "no lateral for it");
    assert_eq!(body["data"]["users"], json!([{"id": "u1", "orders": []}]));
}

/// #1329, the other half: a type with a function-backed list query **and** a SQL-backed
/// one embeds from the SQL-backed one, whichever the schema declares first.
#[tokio::test]
async fn a_function_backed_sibling_is_passed_over_for_the_sql_backed_list_query() {
    let mut schema = schema();
    let mut via_function = list_query("orders_by_function", "Order", "v_unused");
    via_function.function = Some("fn_orders".to_string());
    schema.queries.insert(0, via_function);
    let qm = users(&schema, &["id"], None);

    let run = run(schema, RuntimeConfig::default(), &qm, &[orders(&["id"])], &[], vec![]).await;
    run.result.unwrap();

    assert_eq!(only_embed(&run.adapter.captured_composed().unwrap()).level.view, "v_order");
}

/// The rows the statement returns come back as a flat read and its sub-reads served
/// them: each level projected by its own query, masked keys `null`, counts as numbers,
/// an absent to-one `null`.
#[tokio::test]
async fn composed_rows_are_projected_level_by_level() {
    let schema = schema();
    let qm = users(&schema, &["id", "name"], None);
    let embeds = [EmbedSelection {
        embeds: vec![EmbedSelection {
            relationship: "user".to_string(),
            output_key: "buyer".to_string(),
            fields: vec!["name".to_string()],
            ..EmbedSelection::default()
        }],
        ..orders(&["id", "margin"])
    }];
    let counts = [CountSelection {
        relationship: "orders".to_string(),
        output_key:   "orders_count".to_string(),
        filter:       None,
    }];
    let rows = vec![json!({
        "d": {"id": "u1", "name": "alice", "team_id": "t"},
        "e": {
            "orders": [
                {"d": {"id": 10, "margin": null}, "e": {"buyer": {"d": {"name": "alice"}, "e": {}}}},
                {"d": {"id": 11, "margin": null}, "e": {"buyer": null}},
            ],
            "orders_count": 2,
        },
    })];

    let run = run(schema, RuntimeConfig::default(), &qm, &embeds, &counts, rows).await;

    assert_eq!(
        run.result.unwrap()["data"]["users"],
        json!([{
            "id": "u1",
            "name": "alice",
            "orders": [
                {"id": 10, "margin": null, "buyer": {"name": "alice"}},
                {"id": 11, "margin": null, "buyer": null},
            ],
            "orders_count": 2,
        }])
    );
}

// ---------------------------------------------------------------------------
// Scored once, as the tree it is
// ---------------------------------------------------------------------------

/// `users?select=id,orders(id,total)&limit=2`, the composed shape `4369e56e2` scored
/// against the fan-out it replaces.
fn two_users_with_orders() -> DirectReadProjection {
    DirectReadProjection {
        leaf_fields: 1,
        limit:       Some(2),
        nested:      vec![DirectReadProjection::flat(2, Some(100))],
    }
}

/// The ceiling bounds the tree: served at exactly its score, refused one below —
/// before anything is sent. Charged for the root's own columns alone, it would be
/// served at 3.
#[tokio::test]
async fn the_statement_is_charged_once_for_the_tree_before_it_is_sent() {
    let predicted = estimate_direct_read_cost(
        "users",
        &std::collections::HashMap::<String, usize>::new(),
        &two_users_with_orders(),
    ) as u64;
    assert_eq!(predicted, 405);

    for (ceiling, served) in [(predicted, true), (predicted - 1, false)] {
        let schema = schema();
        let qm = users(&schema, &["id"], Some(2));
        let config = RuntimeConfig {
            max_operation_cost: Some(ceiling),
            ..RuntimeConfig::default()
        };

        let run = run(schema, config, &qm, &[orders(&["id", "total"])], &[], vec![]).await;

        assert_eq!(run.result.is_ok(), served, "at {ceiling}: {:?}", run.result);
        assert_eq!(
            run.adapter.captured_composed().is_some(),
            served,
            "a refused statement is not sent"
        );
    }
}

// ---------------------------------------------------------------------------
// An adapter that cannot compose
// ---------------------------------------------------------------------------

/// The `501` comes from `supports_composed_reads()`, the flag the REST mount warns from,
/// and nothing is sent. The double *implements* the composed read, so only the flag can
/// refuse here: an engine that asked the adapter instead would be served.
#[tokio::test]
async fn an_adapter_without_the_capability_is_refused_from_the_flag_before_sending() {
    let schema = schema();
    let qm = users(&schema, &["id"], None);
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]).without_composed_reads());
    let executor =
        Executor::read_only_with_config(schema, adapter.clone(), RuntimeConfig::default());

    assert!(!executor.supports_composed_reads(), "the flag the mount reads");
    let result = executor
        .execute_query_composed(&qm, &[orders(&["id"])], &[], None, Some(&principal()), None)
        .await;

    assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })), "{result:?}");
    assert!(adapter.captured_composed().is_none(), "nothing is sent");
}

/// The server wraps every adapter in the cache, so the wrapper answers for the capability
/// of what it wraps — not its own default, which would refuse every embed.
#[test]
fn the_composed_read_capability_is_the_wrapped_adapters() {
    use crate::{
        backend::DatabaseAdapter as _,
        cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    };

    let cached = |inner: CapturingMockAdapter| {
        CachedDatabaseAdapter::new(inner, QueryResultCache::new(CacheConfig::enabled()), "1".into())
    };
    assert!(cached(CapturingMockAdapter::new(vec![])).supports_composed_reads());
    assert!(
        !cached(CapturingMockAdapter::new(vec![]).without_composed_reads())
            .supports_composed_reads()
    );
}
