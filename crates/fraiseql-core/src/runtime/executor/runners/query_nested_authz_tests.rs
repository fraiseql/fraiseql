//! The #422 operation authorizer at every level of a read.
//!
//! REST has put each embedded level to the authorizer since embeds were composed: each
//! level resolves through `resolve_direct_read` against its target's list query. A GraphQL
//! nested level never reached it, so an authorizer's rule for reading `Order` held for
//! `{ orders }` and not for `{ users { orders } }`. Now every level on either transport is
//! asked once per request per path, never per row, with `target_type` on every call and
//! `nesting` saying where a nested level sits. `name` keeps REST's spelling: the root field
//! at the root, the type's canonical list query (its first declared SQL-backed list query)
//! below it.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::{
    backend::JsonbValue,
    error::{FraiseQLError, Result},
    runtime::{
        EmbedSelection, Executor, QueryMatch, RuntimeConfig,
        executor::test_support::CapturingMockAdapter,
    },
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship,
        SecurityConfig, TypeDefinition,
    },
    security::{Authorizer, AuthzDecision, AuthzRequest, SecurityContext},
};

/// What the authorizer was asked: `(name, target_type, (parent_type, path))`.
type Asked = (String, Option<String>, Option<(String, String)>);

/// Records every request and denies the ones `deny` names.
struct Recording {
    asked: Mutex<Vec<Asked>>,
    deny:  fn(&AuthzRequest<'_>) -> bool,
}

impl Recording {
    fn new(deny: fn(&AuthzRequest<'_>) -> bool) -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Vec::new()),
            deny,
        })
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl Authorizer for Recording {
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        self.asked.lock().unwrap().push((
            req.name.to_string(),
            req.target_type.map(str::to_string),
            req.nesting.map(|n| (n.parent_type.clone(), n.path.clone())),
        ));
        Ok(if (self.deny)(req) {
            AuthzDecision::Deny {
                reason: "denied".to_string(),
            }
        } else {
            AuthzDecision::Allow
        })
    }
}

fn list_query(name: &str, return_type: &str, view: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type);
    query.returns_list = true;
    query.sql_source = Some(view.to_string());
    query
}

/// `User.orders`, embedded and declared as a relationship. `Order` has two list queries:
/// `orders`, declared first and so canonical, and `archivedOrders`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    let mut user = TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::new(
                "orders",
                FieldType::List(Box::new(FieldType::Object("Order".to_string()))),
            ),
        ],
        ..TypeDefinition::new("User", "v_user")
    };
    user.relationships.push(Relationship {
        name:           "orders".to_string(),
        target_type:    "Order".to_string(),
        cardinality:    Cardinality::OneToMany,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
    });
    schema.types.push(user);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("total", FieldType::Int),
        ],
        ..TypeDefinition::new("Order", "v_order")
    });
    schema.queries.push(list_query("users", "User", "v_user"));
    schema.queries.push(list_query("orders", "Order", "v_order"));
    schema.queries.push(list_query("archivedOrders", "Order", "v_order_archive"));
    schema.security = Some(SecurityConfig::default());
    schema.build_indexes();
    schema
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          crate::types::UserId::new("u-alice"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-nested-authz".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn rows() -> Vec<JsonbValue> {
    vec![JsonbValue::new(json!({
        "id": 1, "name": "alice", "orders": [{"id": 10, "total": 5}, {"id": 11, "total": 6}]
    }))]
}

fn executor(authorizer: Arc<Recording>) -> Executor {
    let schema = schema();
    let config = RuntimeConfig::from_compiled_schema(&schema)
        .unwrap()
        .with_authorizer(authorizer);
    Executor::read_only_with_config(schema, Arc::new(CapturingMockAdapter::new(rows())), config)
}

async fn graphql(authorizer: &Arc<Recording>, query: &str) -> Result<Value> {
    executor(authorizer.clone())
        .execute_with_security(query, None, &principal())
        .await
}

/// `users?select=id,orders(id)`, as REST resolves it.
async fn rest_embed(authorizer: &Arc<Recording>) -> Result<Value> {
    let executor = executor(authorizer.clone());
    let schema = executor.schema();
    let users = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
    let query_match = QueryMatch::from_operation(
        users,
        vec!["id".to_string()],
        std::collections::HashMap::new(),
        schema.find_type("User"),
    )
    .unwrap();
    let embed = EmbedSelection {
        relationship: "orders".to_string(),
        output_key: "orders".to_string(),
        fields: vec!["id".to_string()],
        ..EmbedSelection::default()
    };
    executor
        .execute_query_composed(&query_match, &[embed], &[], None, Some(&principal()), None)
        .await
}

fn nested_orders() -> Asked {
    (
        "orders".to_string(),
        Some("Order".to_string()),
        Some(("User".to_string(), "orders".to_string())),
    )
}

fn denied(result: &Result<Value>) -> bool {
    matches!(result, Err(FraiseQLError::Authorization { .. }))
}

/// A GraphQL nested level is asked as a read of its type, once per path: two selections
/// of `orders` are one read. The root carries its return type too.
#[tokio::test]
async fn a_graphql_nested_level_is_asked_once_with_its_type_and_path() {
    let authorizer = Recording::new(|_| false);
    graphql(&authorizer, "{ users { id a: orders { id } b: orders { total } } }")
        .await
        .unwrap();
    assert_eq!(
        authorizer.asked(),
        [
            ("users".to_string(), Some("User".to_string()), None),
            nested_orders()
        ],
    );
}

/// REST's embedded level is asked the same way, `nesting` now included.
#[tokio::test]
async fn a_rest_embedded_level_is_asked_with_its_type_and_path() {
    let authorizer = Recording::new(|_| false);
    rest_embed(&authorizer).await.unwrap();
    assert_eq!(
        authorizer.asked(),
        [
            ("users".to_string(), Some("User".to_string()), None),
            nested_orders()
        ],
    );
}

/// A rule on `target_type` holds every read of `Order`: at the root through either of
/// its list queries, and nested on either transport.
#[tokio::test]
async fn a_rule_on_target_type_denies_every_read_of_the_type() {
    let authorizer = Recording::new(|req| req.target_type == Some("Order"));
    assert!(denied(&graphql(&authorizer, "{ orders { id } }").await));
    assert!(denied(&graphql(&authorizer, "{ archivedOrders { id } }").await));
    assert!(denied(&graphql(&authorizer, "{ users { id orders { id } } }").await));
    assert!(denied(&rest_embed(&authorizer).await));
    // …and nothing else.
    graphql(&authorizer, "{ users { id name } }").await.unwrap();
}

/// A rule on `name` catches a nested read only through the canonical list query — the
/// difference written down: `archivedOrders` is not the name a nested `Order` is read by.
#[tokio::test]
async fn a_rule_on_name_catches_a_nested_read_only_through_the_canonical_query() {
    let archived = Recording::new(|req| req.name == "archivedOrders");
    assert!(denied(&graphql(&archived, "{ archivedOrders { id } }").await));
    graphql(&archived, "{ users { id orders { id } } }").await.unwrap();
    rest_embed(&archived).await.unwrap();

    let canonical = Recording::new(|req| req.name == "orders");
    assert!(denied(&graphql(&canonical, "{ users { id orders { id } } }").await));
    assert!(denied(&rest_embed(&canonical).await));
}

/// The opt-out: an authorizer that means to gate operations only allows every request
/// whose `nesting` is set, and gets today's operation-only behaviour back.
#[tokio::test]
async fn an_operation_only_authorizer_allows_the_nested_levels() {
    let operations_only = Recording::new(|req| req.nesting.is_none() && req.name == "orders");
    assert!(denied(&graphql(&operations_only, "{ orders { id } }").await));
    graphql(&operations_only, "{ users { id orders { id } } }").await.unwrap();
    rest_embed(&operations_only).await.unwrap();
}

/// An introspection document is put to the authorizer root by root (#1445). Each
/// root is answered, so each is asked: a `__schema` beside `__type` must not be
/// answered on the strength of the first root's verdict.
#[tokio::test]
async fn every_introspection_root_is_asked() {
    let authorizer = Recording::new(|req| req.name == "__schema");
    let result = graphql(
        &authorizer,
        r#"{ t: __type(name: "User") { name } s: __schema { queryType { name } } }"#,
    )
    .await;
    assert!(denied(&result), "a denied `__schema` root must refuse the document: {result:?}");
    let names: Vec<String> = authorizer.asked().into_iter().map(|(name, ..)| name).collect();
    assert_eq!(names, ["__type", "__schema"]);
}
