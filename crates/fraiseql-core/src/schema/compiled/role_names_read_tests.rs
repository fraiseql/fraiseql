//! Every reader of a principal's role names is in [`CompiledSchema::role_names_read`].
//!
//! One test per reader kind: each schema names one role through one reader and nothing
//! else, so dropping a reader from the helper reddens exactly its own test.

use std::collections::BTreeSet;

use super::{CompiledSchema, mutation::MutationDefinition, query::QueryDefinition};
use crate::schema::{RoleDefinition, SecurityConfig, SubscriptionPolicy, TypeDefinition};

fn read(schema: &CompiledSchema) -> BTreeSet<&str> {
    schema.role_names_read()
}

fn only(role: &str) -> BTreeSet<&str> {
    BTreeSet::from([role])
}

#[test]
fn a_role_definition_is_read() {
    let mut schema = CompiledSchema::new();
    let mut security = SecurityConfig::new();
    security.add_role(RoleDefinition::new("cost_reader", vec!["read:cost".to_string()]));
    schema.security = Some(security);
    assert_eq!(read(&schema), only("cost_reader"), "the role's name, never its scopes");
}

#[test]
fn the_default_role_is_read() {
    let mut schema = CompiledSchema::new();
    let mut security = SecurityConfig::new();
    security.default_role = Some("viewer".to_string());
    schema.security = Some(security);
    assert_eq!(read(&schema), only("viewer"));
}

#[test]
fn a_query_requires_role_is_read() {
    let mut schema = CompiledSchema::new();
    let mut query = QueryDefinition::new("invoicePrices", "InvoicePrice");
    query.requires_role = Some("pricing".to_string());
    schema.queries.push(query);
    assert_eq!(read(&schema), only("pricing"));
}

#[test]
fn a_mutation_requires_role_is_read() {
    let mut schema = CompiledSchema::new();
    let mut mutation = MutationDefinition::new("approveInvoice", "Invoice");
    mutation.requires_role = Some("approver".to_string());
    schema.mutations.push(mutation);
    assert_eq!(read(&schema), only("approver"));
}

#[test]
fn a_type_requires_role_is_read() {
    let mut schema = CompiledSchema::new();
    let mut ledger = TypeDefinition::new("Ledger", "v_ledger");
    ledger.requires_role = Some("accountant".to_string());
    schema.types.push(ledger);
    assert_eq!(read(&schema), only("accountant"));
}

#[test]
fn a_subscription_bypass_role_is_read() {
    let mut schema = CompiledSchema::new();
    schema
        .types
        .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
            SubscriptionPolicy {
                owner_path:     "$.owner_id".to_string(),
                identity_field: "user_id".to_string(),
                bypass_roles:   vec!["dispatcher".to_string()],
            },
        ));
    assert_eq!(read(&schema), only("dispatcher"));
}

/// `admin` carries no meaning of its own here: a schema that names no `admin` does not
/// read it, so an application table cannot grant it.
#[test]
fn a_schema_naming_no_role_reads_none_not_even_admin() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(QueryDefinition::new("orders", "Order"));
    schema.types.push(TypeDefinition::new("Order", "v_order"));
    assert!(read(&schema).is_empty());
}
