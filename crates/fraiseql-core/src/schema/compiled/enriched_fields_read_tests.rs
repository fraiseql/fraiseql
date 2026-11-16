//! Every reader of the `fraiseql.enriched.*` namespace is in
//! [`CompiledSchema::enriched_fields_read`] (#1989).
//!
//! One test per reader kind: each schema reads one field through one reader and nothing
//! else, so dropping a reader from the helper reddens exactly its own test.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeSet;

use super::{CompiledSchema, mutation::MutationDefinition, query::QueryDefinition};
use crate::schema::{
    InjectedParamSource, LocaleConfig, LocaleSource, SessionVariableMapping, SessionVariableSource,
    SubscriptionPolicy, TypeDefinition,
};

fn read(schema: &CompiledSchema) -> BTreeSet<&str> {
    schema.enriched_fields_read()
}

fn only(field: &str) -> BTreeSet<&str> {
    BTreeSet::from([field])
}

fn enrichment(field: &str) -> InjectedParamSource {
    InjectedParamSource::Enrichment(field.to_string())
}

#[test]
fn an_enrichment_session_variable_is_read() {
    let mut schema = CompiledSchema::new();
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.agent_role".to_string(),
        source: SessionVariableSource::Enrichment {
            field: "agent_role".to_string(),
        },
    });
    assert_eq!(read(&schema), only("agent_role"), "the field, never the setting's name");
}

#[test]
fn a_query_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut query = QueryDefinition::new("orders", "Order");
    query.inject_params.insert("p_org".to_string(), enrichment("org_id"));
    schema.queries.push(query);
    assert_eq!(read(&schema), only("org_id"), "the field, never the parameter's name");
}

#[test]
fn a_mutation_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut mutation = MutationDefinition::new("approveOrder", "Order");
    mutation.inject_params.insert("p_actor".to_string(), enrichment("actor_id"));
    schema.mutations.push(mutation);
    assert_eq!(read(&schema), only("actor_id"));
}

#[test]
fn a_type_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.inject_params.insert("tenant_id".to_string(), enrichment("tenant_ref"));
    schema.types.push(order);
    assert_eq!(read(&schema), only("tenant_ref"));
}

#[test]
fn a_fact_table_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let table = serde_json::from_value(serde_json::json!({
        "table_name": "tf_sales",
        "measures": [{"name": "revenue", "sql_type": "Decimal", "nullable": false}],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [{"name": "author_id", "sql_type": "Text", "indexed": true}],
        "inject_params": {"author_id": {"source": "enrichment", "claim": "author_ref"}}
    }))
    .unwrap();
    schema.fact_tables.insert("tf_sales".to_string(), table);
    assert_eq!(read(&schema), only("author_ref"));
}

#[test]
fn a_locale_enrichment_source_is_read() {
    let schema = CompiledSchema {
        locale: Some(
            LocaleConfig::new(
                "en-US",
                vec!["en-US".to_string()],
                std::collections::BTreeMap::new(),
                vec![LocaleSource::Enrichment {
                    enrichment: "preferred_locale".to_string(),
                }],
            )
            .unwrap(),
        ),
        ..CompiledSchema::new()
    };
    assert_eq!(read(&schema), only("preferred_locale"));
}

#[test]
fn a_subscription_policy_identity_field_is_read() {
    let mut schema = CompiledSchema::new();
    schema
        .types
        .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
            SubscriptionPolicy {
                owner_path:     "$.owner_id".to_string(),
                identity_field: "user_id".to_string(),
                bypass_roles:   vec![],
            },
        ));
    assert_eq!(read(&schema), only("user_id"));
}

/// Only `enrichment` sources read the namespace: a claim of the same name is not one.
#[test]
fn a_jwt_source_of_the_same_name_is_not_read() {
    let mut schema = CompiledSchema::new();
    let mut query = QueryDefinition::new("orders", "Order");
    query
        .inject_params
        .insert("p_org".to_string(), InjectedParamSource::Jwt("org_id".to_string()));
    schema.queries.push(query);
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.org_id".to_string(),
        source: SessionVariableSource::Jwt {
            claim: "org_id".to_string(),
        },
    });
    assert!(read(&schema).is_empty(), "{:?}", read(&schema));
}
