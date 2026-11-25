//! Unit tests for a REST relay route's embeds (`query_relay_embed`).
//!
//! What these pin is the statement: the page as its root, unchanged by the embed, and each
//! embedded level gated as a read of its own target — asserted on the [`ComposedLevel`] the
//! adapter receives, or on the adapter never being called. Whether PostgreSQL answers the
//! same parent rows with and without the embed is `rest_pagination_stability_e2e_pg`'s
//! `relay_embed` question.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::json;

use super::{super::query_composed::PlannedEmbed, relay_page_projection};
use crate::{
    backend::{ComposedLevel, JsonbValue, LevelKeys, WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
    graphql::{DirectReadProjection, estimate_direct_read_cost},
    runtime::{
        EmbedSelection, Executor, QueryMatch, RuntimeConfig,
        executor::test_support::CapturingMockAdapter,
    },
    schema::{
        Cardinality, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        QueryDefinition, Relationship, TypeDefinition,
    },
    security::{
        Authorizer, AuthzDecision, AuthzRequest, RLSPolicy, RlsWhereClause, SecurityContext,
        rls_policy::RlsTarget,
    },
};

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

/// A policy that scopes each type by a column only that type has, so which type's
/// predicate a level carries is visible in its `WHERE`.
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
            other => panic!("no policy for {other:?}"),
        };
        Ok(Some(RlsWhereClause::new(WhereClause::Field {
            path:     vec![column.to_string()],
            operator: WhereOperator::Eq,
            value:    json!(context.user_id.to_string()),
        })))
    }
}

fn scoped(name: &str, scope: &str, on_deny: FieldDenyPolicy) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::Int);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = on_deny;
    field
}

/// `User`, paged by a relay query, with `orders` (to-many) read through `Order`'s list
/// query; `Order` has a masked and a rejected field.
fn schema(edit_orders: impl FnOnce(&mut QueryDefinition)) -> CompiledSchema {
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
            native_join:    None,
        }],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("fk_user", FieldType::Int),
            FieldDefinition::new("total", FieldType::Int),
            scoped("margin", "read:margin", FieldDenyPolicy::Mask),
            scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
        ],
        ..TypeDefinition::new("Order", "v_order")
    });
    let mut users = QueryDefinition::new("users", "User");
    users.returns_list = true;
    users.sql_source = Some("v_user".to_string());
    users.relay = true;
    users.relay_cursor_column = Some("pk_user".to_string());
    users.relay_cursor_type = CursorType::Int64;
    schema.queries.push(users);
    let mut orders = QueryDefinition::new("orders", "Order");
    orders.returns_list = true;
    orders.sql_source = Some("v_order".to_string());
    edit_orders(&mut orders);
    schema.queries.push(orders);
    crate::runtime::executor::test_support::declare_node_types(&mut schema, &["User"]);
    // Field-level RBAC is inert without a security section.
    schema.security = Some(crate::schema::SecurityConfig::default());
    schema.build_indexes();
    schema
}

fn config(schema: &CompiledSchema) -> RuntimeConfig {
    RuntimeConfig::from_compiled_schema(schema)
        .unwrap()
        .with_rls_policy(Arc::new(PerTypePolicy))
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          crate::types::UserId::new("u-7"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-relay-embed".to_string(),
        ip_address:       None,
        authenticated_at: fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_time::Timestamp::now()
            + fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The match a REST relay route hands the engine: `id, name`, and its window as arguments.
fn page(schema: &CompiledSchema, window: &[(&str, serde_json::Value)]) -> QueryMatch {
    let query = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
    QueryMatch::from_operation(
        query,
        vec!["id".to_string(), "name".to_string()],
        window.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
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

/// The page of `query_match` with `embeds`, through the executor's REST relay entry; the
/// flat entry when `embeds` is empty, as the REST handler dispatches.
async fn run(
    schema: CompiledSchema,
    config: RuntimeConfig,
    query_match: &QueryMatch,
    embeds: &[EmbedSelection],
    rows: Vec<serde_json::Value>,
) -> Run {
    let adapter =
        Arc::new(CapturingMockAdapter::new(rows.into_iter().map(JsonbValue::new).collect()));
    let executor = Executor::with_config_and_relay(schema, adapter.clone(), config);
    let ctx = principal();
    let result = if embeds.is_empty() {
        executor.execute_relay_direct(query_match, Some(&ctx), None).await
    } else {
        executor
            .execute_relay_composed(query_match, embeds, &[], Some(&ctx), None)
            .await
    };
    Run { adapter, result }
}

fn statement(run: &Run) -> ComposedLevel {
    assert!(run.result.is_ok(), "{:?}", run.result);
    run.adapter
        .captured_composed()
        .expect("the page was read as a composed statement")
}

/// Every `path` the clause constrains, depth-first.
fn constrained_paths(clause: Option<&WhereClause>) -> Vec<String> {
    fn walk(clause: &WhereClause, out: &mut Vec<String>) {
        match clause {
            WhereClause::Field { path, .. } => out.push(path.join(".")),
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
// The page is the statement's root, as it is without the embed
// ---------------------------------------------------------------------------

/// The page — scoped by the parent's predicate, `first` + 1 rows from the start, forward —
/// is the root the embed is joined to.
#[tokio::test]
async fn the_page_is_the_root_the_embed_is_joined_to() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;
    let root = statement(&run);

    let keyset = root.keyset.as_ref().expect("the root is paged by keyset");
    assert!(keyset.forward && keyset.cursor.is_none(), "{keyset:?}");
    assert_eq!(keyset.cursor_column, "pk_user");
    assert_eq!(root.limit, Some(3), "first + 1 rows, to answer hasNextPage");
    assert_eq!(root.offset, None);
    assert_eq!(root.view, "v_user");
    assert_eq!(constrained_paths(root.where_clause.as_ref()), ["team_id"]);
    assert_eq!(root.embeds.len(), 1, "{root:?}");
    assert_eq!(root.embeds[0].output_key, "orders");
}

/// The page with the embed is scoped exactly as the page without it, which the relay
/// adapter reads.
#[tokio::test]
async fn the_page_is_scoped_as_the_page_without_the_embed() {
    let schema = schema(|_| {});
    let qm = page(&schema, &[("first", json!(2))]);
    let flat = run(schema.clone(), config(&schema), &qm, &[], Vec::new()).await;
    assert!(flat.result.is_ok(), "{:?}", flat.result);
    let embedded = run(schema.clone(), config(&schema), &qm, &[orders(&["id"])], Vec::new()).await;

    assert_eq!(
        statement(&embedded).where_clause,
        flat.adapter.captured_where(),
        "the embed changes nothing about which rows the page holds"
    );
}

/// With no `first` or `last`, the page with an embed is the default page, as the flat
/// page is: an embed does not make the page unbounded.
#[tokio::test]
async fn with_no_window_the_page_with_an_embed_is_the_default_size() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[]);
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;
    assert_eq!(statement(&run).limit, Some(21), "the default page of 20, + 1");
}

/// A window over `max_page_size` is refused with an embed exactly as without one, before
/// any statement is sent.
#[tokio::test]
async fn a_window_over_the_ceiling_is_refused_with_an_embed_as_without_one() {
    let schema = schema(|_| {});
    let mut config = config(&schema);
    config.max_page_size = Some(5);
    let qm = page(&schema, &[("first", json!(50))]);
    let flat = run(schema.clone(), config.clone(), &qm, &[], Vec::new()).await;
    // The embed's own page is within the ceiling, so what is refused is the page's window.
    let embed = EmbedSelection {
        limit: Some(1),
        ..orders(&["id"])
    };
    let embedded = run(schema, config, &qm, &[embed], Vec::new()).await;

    let flat = flat.result.expect_err("the flat page is refused");
    let refused = embedded.result.as_ref().expect_err("the page with the embed is refused");
    assert_eq!(refused.to_string(), flat.to_string());
    assert!(embedded.adapter.captured_composed().is_none(), "refused before the statement");
}

// ---------------------------------------------------------------------------
// Each embedded level is gated as a read of its own target
// ---------------------------------------------------------------------------

/// The embedded level carries the target's row predicate, not the page's.
#[tokio::test]
async fn an_embedded_level_carries_its_targets_predicate_not_the_pages() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;
    let root = statement(&run);

    assert_eq!(constrained_paths(root.embeds[0].level.where_clause.as_ref()), ["owner_id"]);
}

/// A masked field of the target is written `NULL` by the statement, so its value never
/// leaves the database.
#[tokio::test]
async fn a_masked_field_of_the_target_is_null_in_the_statement() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let run = run(schema, config, &qm, &[orders(&["id", "margin"])], Vec::new()).await;
    let root = statement(&run);

    match &root.embeds[0].level.keys {
        LevelKeys::Only { kept, masked } => {
            assert!(masked.iter().any(|k| k == "margin"), "masked: {masked:?}");
            assert!(!kept.iter().any(|k| k == "margin"), "kept: {kept:?}");
        },
        other => panic!("an embedded level reads only the keys its access allows: {other:?}"),
    }
}

/// A rejected field of the target refuses the request before the statement is sent.
#[tokio::test]
async fn a_rejected_field_of_the_target_refuses_before_the_statement() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let run = run(schema, config, &qm, &[orders(&["id", "cost_price"])], Vec::new()).await;

    assert!(
        matches!(run.result, Err(FraiseQLError::Authorization { .. })),
        "{:?}",
        run.result
    );
    assert!(run.adapter.captured_composed().is_none());
}

/// The target query's role gate applies to its level.
#[tokio::test]
async fn the_targets_role_gate_applies_to_its_level() {
    let schema = schema(|orders| orders.requires_role = Some("auditor".to_string()));
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let flat = run(schema.clone(), config.clone(), &qm, &[], Vec::new()).await;
    assert!(flat.result.is_ok(), "the page alone is served: {:?}", flat.result);
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;

    assert!(run.result.is_err(), "{:?}", run.result);
    assert!(run.adapter.captured_composed().is_none());
}

/// A policy-gated field of the target selected in the embed is refused, as on the offset
/// route: no REST read adjudicates such a field per row. The same embed without it is served.
#[tokio::test]
async fn a_policy_gated_field_of_the_target_is_refused_in_the_embed() {
    let mut schema = schema(|_| {});
    let order = schema.types.iter_mut().find(|t| t.name == "Order").unwrap();
    order.fields.iter_mut().find(|f| f.name == "total").unwrap().authorize = true;
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let served = run(schema.clone(), config.clone(), &qm, &[orders(&["id"])], Vec::new()).await;
    assert!(served.result.is_ok(), "the embed without the gated field: {:?}", served.result);
    let run = run(schema, config, &qm, &[orders(&["id", "total"])], Vec::new()).await;

    assert!(
        matches!(run.result, Err(FraiseQLError::Authorization { .. })),
        "{:?}",
        run.result
    );
    assert!(run.adapter.captured_composed().is_none());
}

/// A selected nested object whose type scopes its rows beside an embed is refused, as on
/// the offset route; the page selecting it without the embed reads it as a gated level.
#[tokio::test]
async fn a_row_scoped_nested_object_beside_an_embed_is_refused() {
    let mut schema = schema(|_| {});
    // The whole object is selected: no field of it may be one the principal is refused.
    let order = schema.types.iter_mut().find(|t| t.name == "Order").unwrap();
    order.fields.retain(|f| f.name != "cost_price");
    let user = schema.types.iter_mut().find(|t| t.name == "User").unwrap();
    user.fields
        .push(FieldDefinition::new("latest_order", FieldType::Object("Order".to_string())));
    // Joined through its relationship, where the level's own predicate is evaluated.
    user.relationships.push(Relationship {
        name:           "latest_order".to_string(),
        target_type:    "Order".to_string(),
        cardinality:    Cardinality::OneToOne,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
        native_join:    None,
    });
    schema.build_indexes();
    let config = config(&schema);
    let query = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
    let qm = QueryMatch::from_operation(
        query,
        vec!["id".to_string(), "latest_order".to_string()],
        std::iter::once(("first".to_string(), json!(2))).collect(),
        schema.find_type("User"),
    )
    .unwrap();
    let qm = super::super::query_nested::expand_leaf_objects(&schema, &qm)
        .unwrap()
        .into_owned();
    let alone = run(schema.clone(), config.clone(), &qm, &[], Vec::new()).await;
    assert!(alone.result.is_ok(), "the nested object alone: {:?}", alone.result);
    assert_eq!(statement(&alone).embeds.len(), 1, "read as a gated level");
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;

    assert!(matches!(run.result, Err(FraiseQLError::Unsupported { .. })), "{:?}", run.result);
    assert!(run.adapter.captured_composed().is_none());
}

/// Denies a read of `Order` asked as a nested level; allows everything else.
struct DenyNestedOrders;

impl Authorizer for DenyNestedOrders {
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        Ok(if req.target_type == Some("Order") && req.nesting.is_some() {
            AuthzDecision::Deny {
                reason: "no embedded orders".to_string(),
            }
        } else {
            AuthzDecision::Allow
        })
    }
}

/// The authorizer is asked about the embed as a nested read of its type, under the page's
/// type and the relationship's name.
#[tokio::test]
async fn the_embed_is_put_to_the_authorizer_as_a_nested_read() {
    let schema = schema(|_| {});
    let config = config(&schema).with_authorizer(Arc::new(DenyNestedOrders));
    let qm = page(&schema, &[("first", json!(2))]);
    let flat = run(schema.clone(), config.clone(), &qm, &[], Vec::new()).await;
    assert!(flat.result.is_ok(), "the page alone is served: {:?}", flat.result);
    let run = run(schema, config, &qm, &[orders(&["id"])], Vec::new()).await;

    assert!(
        matches!(run.result, Err(FraiseQLError::Authorization { .. })),
        "{:?}",
        run.result
    );
    assert!(run.adapter.captured_composed().is_none());
}

// ---------------------------------------------------------------------------
// Cost
// ---------------------------------------------------------------------------

fn cost(projection: &DirectReadProjection) -> u64 {
    estimate_direct_read_cost("users", &std::collections::HashMap::new(), projection) as u64
}

/// An embed only ever adds to the page's charge: the tree never scores below the flat
/// page it is rooted at, for any page, any field count and any level beneath it.
#[test]
fn a_page_with_embeds_never_scores_below_the_flat_page() {
    let levels = [
        DirectReadProjection::flat(0, None),
        DirectReadProjection::flat(1, Some(1)),
        DirectReadProjection::flat(3, Some(100)),
        DirectReadProjection {
            nested: vec![DirectReadProjection::flat(2, Some(10))],
            ..DirectReadProjection::flat(1, Some(5))
        },
    ];
    for fields in [0, 1, 4] {
        for page in [None, Some(1), Some(20), Some(500)] {
            let flat = relay_page_projection(fields, page, &[]);
            assert_eq!(flat, DirectReadProjection::flat(fields, page), "no embed: the flat page");
            for level in &levels {
                let tree = DirectReadProjection {
                    nested: vec![level.clone()],
                    ..flat.clone()
                };
                assert!(
                    cost(&tree) >= cost(&flat),
                    "{fields} fields over {page:?} with {level:?}: {} < {}",
                    cost(&tree),
                    cost(&flat)
                );
            }
        }
    }
    // The embedded levels the tree is built from are the plan's.
    let none: &[PlannedEmbed] = &[];
    assert!(relay_page_projection(2, Some(2), none).nested.is_empty());
}

/// A page whose embed puts it over `per_request_max` is refused before the statement is
/// sent, while the same page without the embed, at the same ceiling, is served.
#[tokio::test]
async fn an_embed_that_puts_the_page_over_the_ceiling_is_refused_before_the_statement() {
    let schema = schema(|_| {});
    let mut config = config(&schema);
    // The flat page's own score: `id, name` over `first: 2`.
    config.max_operation_cost = Some(cost(&DirectReadProjection::flat(2, Some(2))));
    let qm = page(&schema, &[("first", json!(2))]);
    let flat = run(schema.clone(), config.clone(), &qm, &[], Vec::new()).await;
    assert!(flat.result.is_ok(), "the page alone is served: {:?}", flat.result);
    let run = run(schema, config, &qm, &[orders(&["id", "total"])], Vec::new()).await;

    assert!(run.result.is_err(), "{:?}", run.result);
    assert!(run.adapter.captured_composed().is_none());
}

// ---------------------------------------------------------------------------
// Each node carries its embedded rows
// ---------------------------------------------------------------------------

/// Each node is projected as on the flat page, its cursor taken from the page's row, and
/// carries the rows embedded for it, projected by their own level.
#[tokio::test]
async fn each_node_carries_the_rows_embedded_for_it() {
    let schema = schema(|_| {});
    let config = config(&schema);
    let qm = page(&schema, &[("first", json!(2))]);
    let rows = vec![
        json!({
            "d": {"id": "u1", "name": "Ada", "pk_user": 1, "team_id": "x"},
            "e": {"orders": [{"d": {"id": 10, "total": 5, "fk_user": 1}, "e": {}}]}
        }),
        json!({
            "d": {"id": "u2", "name": "Bo", "pk_user": 2, "team_id": "x"},
            "e": {"orders": []}
        }),
    ];
    let run = run(schema, config, &qm, &[orders(&["id", "total"])], rows).await;
    let response = run.result.unwrap();
    let edges = response["data"]["users"]["edges"].as_array().unwrap();

    assert_eq!(edges.len(), 2, "{response}");
    assert_eq!(
        edges[0]["node"],
        json!({"id": "u1", "name": "Ada", "orders": [{"id": 10, "total": 5}]})
    );
    assert_eq!(edges[1]["node"], json!({"id": "u2", "name": "Bo", "orders": []}));
    assert_eq!(response["data"]["users"]["pageInfo"]["hasNextPage"], json!(false));
}

// ---------------------------------------------------------------------------
// The enrichment backstop at the entry
// ---------------------------------------------------------------------------

mod enrichment_entry_point_tests {
    use super::*;
    use crate::{
        schema::{SessionVariableMapping, SessionVariableSource},
        security::EnrichmentMark,
    };

    /// The rig's schema, reading enriched identity through a session variable.
    fn enriched_schema() -> CompiledSchema {
        let mut schema = schema(|_| {});
        schema.session_variables.variables.push(SessionVariableMapping {
            name:   "app.actor_id".to_string(),
            source: SessionVariableSource::Enrichment {
                field: "actor_id".to_string(),
            },
        });
        schema
    }

    /// A principal no transport resolved enriched identity for is refused at the entry,
    /// before anything is planned or read.
    #[tokio::test]
    async fn the_relay_composed_entry_refuses_an_unresolved_principal() {
        let schema = enriched_schema();
        let config = config(&schema);
        let qm = page(&schema, &[("first", json!(2))]);
        let adapter = Arc::new(CapturingMockAdapter::new(Vec::new()));
        let executor = Executor::with_config_and_relay(schema, adapter.clone(), config);

        let err = executor
            .execute_relay_composed(&qm, &[orders(&["id"])], &[], Some(&principal()), None)
            .await
            .expect_err("an unresolved principal must not read");
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
        assert!(adapter.captured_composed().is_none());

        let mut resolved = principal();
        resolved.mark_enrichment(EnrichmentMark::Resolved);
        let served = executor
            .execute_relay_composed(&qm, &[orders(&["id"])], &[], Some(&resolved), None)
            .await;
        // Past the backstop, a resolved principal is read on: here the identity it was
        // resolved to lacks the enriched field, which the session variable refuses.
        assert!(
            !matches!(served, Err(FraiseQLError::Authorization { .. })),
            "a resolved principal passes the backstop: {served:?}"
        );
    }

    /// The flat relay entry refuses a principal no transport resolved, on a schema that
    /// reads the enriched identity elsewhere: the page itself reads nothing enriched, so
    /// without the backstop it would be served.
    #[tokio::test]
    async fn the_relay_direct_entry_refuses_an_unresolved_principal() {
        let schema = schema(|orders| {
            orders.inject_params.insert(
                "owner_id".to_string(),
                crate::schema::InjectedParamSource::Enrichment("org".to_string()),
            );
        });
        assert!(schema.declares_enrichment_consumer(), "the schema reads the enriched identity");
        let config = config(&schema);
        let qm = page(&schema, &[("first", json!(2))]);
        let adapter = Arc::new(CapturingMockAdapter::new(Vec::new()));
        let executor = Executor::with_config_and_relay(schema, adapter.clone(), config);

        let err = executor
            .execute_relay_direct(&qm, Some(&principal()), None)
            .await
            .expect_err("an unresolved principal must not read");
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
        assert!(adapter.captured_where().is_none(), "nothing is read");

        let mut resolved = principal();
        resolved.mark_enrichment(EnrichmentMark::Resolved);
        executor
            .execute_relay_direct(&qm, Some(&resolved), None)
            .await
            .expect("a resolved principal is served");
        assert!(adapter.captured_where().is_some(), "the page is read");
    }
}
