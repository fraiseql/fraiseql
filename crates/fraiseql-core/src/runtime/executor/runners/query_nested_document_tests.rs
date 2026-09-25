//! The in-memory row filter over a document a write returned: which predicates it accepts,
//! and that it is never looser than SQL.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{DocumentRowFilter, equalities, meets};
use crate::{
    backend::{WhereClause, WhereOperator},
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition},
};

fn eq(key: &str, value: Value) -> WhereClause {
    WhereClause::Field {
        path: vec![key.to_string()],
        operator: WhereOperator::Eq,
        value,
    }
}

fn select(name: &str, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name: name.to_string(),
        alias: None,
        arguments: Vec::new(),
        nested_fields,
        directives: Vec::new(),
    }
}

#[test]
fn a_conjunction_of_equalities_is_accepted() {
    let mut out = Vec::new();
    assert!(equalities(
        &WhereClause::And(vec![eq("tenant_id", json!("A")), eq("owner", json!("u-alice"))]),
        &mut out
    ));
    assert_eq!(out.len(), 2);
}

#[test]
fn any_other_shape_is_refused() {
    let or = WhereClause::Or(vec![eq("tenant_id", json!("A")), eq("owner", json!("u"))]);
    let not = WhereClause::Not(Box::new(eq("tenant_id", json!("A"))));
    let neq = WhereClause::Field {
        path:     vec!["tenant_id".to_string()],
        operator: WhereOperator::Neq,
        value:    json!("B"),
    };
    for clause in [
        or,
        not,
        neq,
        WhereClause::And(vec![eq("a", json!(1)), WhereClause::Or(vec![])]),
    ] {
        assert!(!equalities(&clause, &mut Vec::new()), "{clause:?}");
    }
}

#[test]
fn a_document_meets_a_condition_only_with_the_same_value() {
    let tenant = vec![(vec!["tenant_id".to_string()], json!("A"))];
    assert!(meets(&json!({ "tenant_id": "A" }), &tenant));
    assert!(!meets(&json!({ "tenant_id": "B" }), &tenant));
    assert!(!meets(&json!({}), &tenant), "an absent key meets nothing");
    // Stricter than SQL's cast, never looser: a numeric string is not the number.
    let id = vec![(vec!["owner".to_string()], json!(7))];
    assert!(!meets(&json!({ "owner": "7" }), &id));
    // `NULL = NULL` is not true in SQL, and not here.
    let null = vec![(vec!["owner".to_string()], Value::Null)];
    assert!(!meets(&json!({ "owner": null }), &null));
    // A path is followed key by key.
    let deep = vec![(vec!["meta".to_string(), "tenant".to_string()], json!("A"))];
    assert!(meets(&json!({ "meta": { "tenant": "A" } }), &deep));
    assert!(!meets(&json!({ "meta": "A" }), &deep));
}

/// `User.orders` (a list) and `Order.user` (a to-one), filtered to tenant `A`.
fn filtered() -> (DocumentRowFilter, CompiledSchema) {
    let mut schema = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new(
            "orders",
            FieldType::List(Box::new(FieldType::Object("Order".to_string()))),
        ),
    ];
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("user", FieldType::Object("User".to_string())),
    ];
    schema.types.push(user);
    schema.types.push(order);
    schema.build_indexes();
    let tenant_a = vec![(vec!["tenant_id".to_string()], json!("A"))];
    let mut by_field = HashMap::new();
    by_field.insert(("User".to_string(), "orders".to_string()), tenant_a.clone());
    by_field.insert(("Order".to_string(), "user".to_string()), tenant_a);
    (DocumentRowFilter { by_field }, schema)
}

#[test]
fn a_list_keeps_the_elements_that_meet_it_and_a_to_one_that_does_not_is_null() {
    let (filter, schema) = filtered();
    let mut document = json!({
        "id": 1,
        "orders": [
            { "id": 10, "tenant_id": "A", "user": { "id": 1, "tenant_id": "A" } },
            { "id": 12, "tenant_id": "B" },
            { "id": 13, "tenant_id": "A", "user": { "id": 2, "tenant_id": "B" } }
        ]
    });
    let selection = [
        select("id", vec![]),
        select("orders", vec![select("user", vec![select("id", vec![])])]),
    ];
    filter.apply(&mut document, "User", &selection, &schema);
    let orders = document["orders"].as_array().unwrap();
    let ids: Vec<i64> = orders.iter().map(|o| o["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, [10, 13]);
    assert_eq!(orders[0]["user"]["id"], 1);
    assert!(orders[1]["user"].is_null(), "{document}");
}

#[test]
fn an_unselected_level_is_left_as_stored() {
    let (filter, schema) = filtered();
    let mut document = json!({ "id": 1, "orders": [{ "id": 12, "tenant_id": "B" }] });
    filter.apply(&mut document, "User", &[select("id", vec![])], &schema);
    assert_eq!(document["orders"].as_array().unwrap().len(), 1, "the projector drops it");
}

/// Records every nested level put to it, as `parent.path`, and denies the one under `Team`.
#[derive(Default)]
struct RecordingAuthorizer {
    asked: std::sync::Mutex<Vec<String>>,
}

impl crate::security::Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        req: &crate::security::AuthzRequest<'_>,
    ) -> crate::error::Result<crate::security::AuthzDecision> {
        let Some(nesting) = req.nesting else {
            return Ok(crate::security::AuthzDecision::Allow);
        };
        self.asked
            .lock()
            .unwrap()
            .push(format!("{}.{}", nesting.parent_type, nesting.path));
        Ok(if nesting.parent_type == "Team" {
            crate::security::AuthzDecision::Deny {
                reason: "not through a team".to_string(),
            }
        } else {
            crate::security::AuthzDecision::Allow
        })
    }
}

/// `User.members` and `Team.members`, both lists of `Member`.
fn two_parents() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    for parent in ["User", "Team"] {
        let mut t = TypeDefinition::new(parent, "");
        t.fields = vec![FieldDefinition::new(
            "members",
            FieldType::List(Box::new(FieldType::Object("Member".to_string()))),
        )];
        schema.types.push(t);
    }
    let mut member = TypeDefinition::new("Member", "");
    member.fields = vec![FieldDefinition::new("id", FieldType::Int)];
    schema.types.push(member);
    schema.build_indexes();
    schema
}

fn principal() -> crate::security::SecurityContext {
    crate::security::SecurityContext {
        user_id:          "u-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-document".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// One classification of several roots — a write's payload — asks the authorizer of each
/// root's nested levels once: the same path under one parent type is one read, under two
/// it is two — and the one under `Team` is denied although the one under `User`, asked
/// first, was allowed.
#[test]
fn the_same_path_under_two_roots_is_asked_of_each() {
    let schema = two_parents();
    let authorizer = RecordingAuthorizer::default();
    let authz = super::LevelAuthz {
        authorizer: &authorizer,
        input:      None,
    };
    let selection = [select("members", vec![select("id", vec![])])];
    let result = super::SelectionAccess::classify_roots(
        &schema,
        &[
            ("User", &selection),
            ("User", &selection),
            ("Team", &selection),
        ],
        Some(&principal()),
        Some(authz),
    );
    assert!(matches!(result, Err(crate::error::FraiseQLError::Authorization { .. })));
    assert_eq!(*authorizer.asked.lock().unwrap(), ["User.members", "Team.members"]);
}
