//! Every reader of the `fraiseql.enriched.*` namespace is in
//! [`CompiledSchema::enriched_fields_read`] (#1989).
//!
//! One test per reader kind: each schema reads one field through one reader and nothing
//! else, so dropping a reader from the helper reddens exactly its own test. Each also
//! asserts [`CompiledSchema::declares_enrichment_consumer`], so the predicate the boot
//! check, the startup warning and the engine backstop key on counts the same readers.

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

/// `schema` reads exactly `field`, and so declares an enrichment consumer.
fn assert_reads_only(schema: &CompiledSchema, field: &str) {
    assert_eq!(read(schema), only(field));
    assert!(
        schema.declares_enrichment_consumer(),
        "a schema reading `{field}` declares an enrichment consumer"
    );
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
    assert_reads_only(&schema, "agent_role");
}

#[test]
fn a_query_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut query = QueryDefinition::new("orders", "Order");
    query.inject_params.insert("p_org".to_string(), enrichment("org_id"));
    schema.queries.push(query);
    assert_reads_only(&schema, "org_id");
}

#[test]
fn a_mutation_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut mutation = MutationDefinition::new("approveOrder", "Order");
    mutation.inject_params.insert("p_actor".to_string(), enrichment("actor_id"));
    schema.mutations.push(mutation);
    assert_reads_only(&schema, "actor_id");
}

#[test]
fn a_type_enrichment_inject_is_read() {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.inject_params.insert("tenant_id".to_string(), enrichment("tenant_ref"));
    schema.types.push(order);
    assert_reads_only(&schema, "tenant_ref");
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
    assert_reads_only(&schema, "author_ref");
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
    assert_reads_only(&schema, "preferred_locale");
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
    assert_reads_only(&schema, "user_id");
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
    assert!(!schema.declares_enrichment_consumer());
}

/// The predicate is computed once per schema value, never at a default of `false`: a
/// schema that never ran `build_indexes` still declares its consumer.
#[test]
fn a_schema_that_never_built_its_indexes_still_declares_its_consumer() {
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
    assert!(schema.declares_enrichment_consumer());
}

/// A clone answers for itself: one taken after the original answered, then given a
/// reader, declares it rather than repeating the original's `false`.
#[test]
fn a_clone_given_a_reader_declares_it() {
    let original = CompiledSchema::new();
    assert!(!original.declares_enrichment_consumer());
    let mut clone = original.clone();
    let mut query = QueryDefinition::new("orders", "Order");
    query.inject_params.insert("p_org".to_string(), enrichment("org_id"));
    clone.queries.push(query);
    assert!(clone.declares_enrichment_consumer());
    assert!(
        !original.declares_enrichment_consumer(),
        "the original still answers for itself"
    );
}

/// `build_indexes`, which a direct mutation is documented to call, recomputes it.
#[test]
fn build_indexes_recomputes_the_consumer_flag() {
    let mut schema = CompiledSchema::new();
    assert!(!schema.declares_enrichment_consumer());
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.agent_role".to_string(),
        source: SessionVariableSource::Enrichment {
            field: "agent_role".to_string(),
        },
    });
    schema.build_indexes();
    assert!(schema.declares_enrichment_consumer());
}
