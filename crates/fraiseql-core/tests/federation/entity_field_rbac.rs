//! `_entities` field-level RBAC against **real PostgreSQL** (#1030).
//!
//! The in-crate `entities_authz` unit tests pin this over a mock adapter. This suite is
//! the operator's version of the same question, and it exists because the defect was
//! precisely that one entry point's guarantees did not hold at another: a schema whose
//! `Employee.salary` carries `requires_scope` answers 403 on
//! `query { employees { salary } }` and returned the value in full through
//! `_entities` — over a real database, real SQL, real rows.
//!
//! The `Mask` case is the one worth having a live row for. `on_deny = Mask` is not a
//! refusal, so nothing about the response *looks* wrong; the only way to tell the fix
//! from the defect is that a value the database really does hold comes back `null`.
//! A mock returning a canned row cannot distinguish "masked" from "never stored".
//!
//! Provisions `v_employee` as the repo's standard `data jsonb` shape, so the resolver
//! runs its jsonb projection path (`"data"->'salary' AS "salary"`) rather than the flat
//! columnar one.
//!
//! Skips cleanly when no Postgres is configured (the non-DB preflight leg); runs for
//! real on the Dagger `integration --suite=postgres` leg, where the `federation` test
//! target is invoked with a bound `DATABASE_URL`.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip notes are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    runtime::Executor,
    schema::{
        CompiledSchema, FederationConfig, FederationEntity, FieldDefinition, FieldDenyPolicy,
        FieldType, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};
use serde_json::json;

use super::common;

const EMPLOYEE_RELATION: &str = "v_employee";

/// `Employee`, federated by `id`, with one `Reject`-gated and one `Mask`-gated field.
///
/// The type carries no backing query on purpose: `entity_sources` then falls back to the
/// type-level `sql_source` (#507's owner-split shape), which is the arrangement in which
/// the field gates had no other enforcement site at all.
fn employee_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name: "Employee".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    });
    schema.types.push({
        let mut t = TypeDefinition::new("Employee", EMPLOYEE_RELATION);
        t.fields = vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::new("salary", FieldType::Int)
                .with_requires_scope("read:Employee.salary")
                .with_on_deny(FieldDenyPolicy::Reject),
            FieldDefinition::new("email", FieldType::String)
                .with_requires_scope("read:Employee.email")
                .with_on_deny(FieldDenyPolicy::Mask),
        ];
        t
    });
    schema.security = Some(SecurityConfig {
        role_definitions: vec![
            RoleDefinition {
                name:        "viewer".into(),
                description: None,
                scopes:      vec!["read:Employee".into()],
            },
            RoleDefinition {
                name:        "auditor".into(),
                description: None,
                scopes:      vec!["read:Employee.salary".into(), "read:Employee.email".into()],
            },
        ],
        ..Default::default()
    });
    schema
}

fn ctx(role: &str) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![role.to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-1030".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn entities_query(fields: &str) -> String {
    format!(
        r#"{{ _entities(representations: [{{ __typename: "Employee", id: "e-1" }}]) {{ ... on Employee {{ {fields} }} }} }}"#
    )
}

fn representations() -> serde_json::Value {
    json!({ "representations": [{ "__typename": "Employee", "id": "e-1" }] })
}

/// Seed one real employee row and return an executor over the live adapter.
async fn fixture() -> Option<(fraiseql_test_support::Service, Executor<PostgresAdapter>)> {
    let row: HashMap<String, serde_json::Value> = std::iter::once((
        "data".to_string(),
        json!({ "id": "e-1", "name": "Ada", "salary": 120_000, "email": "ada@example.com" }),
    ))
    .collect();
    let (pg, adapter) =
        common::pg_entity_fixture(EMPLOYEE_RELATION, &["data jsonb"], &[row]).await?;
    Some((pg, Executor::new(employee_schema(), Arc::clone(&adapter))))
}

#[tokio::test]
async fn reject_gated_field_is_refused_over_real_postgres() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP reject_gated_field_is_refused_over_real_postgres: no postgres");
        return;
    };

    let err = executor
        .execute_with_security(
            &entities_query("id salary"),
            Some(&representations()),
            &ctx("viewer"),
        )
        .await
        .expect_err("a requires_scope Reject field must refuse through _entities");
    assert!(
        matches!(err, FraiseQLError::Authorization { .. }),
        "must be an authorization refusal, not some other failure; got: {err}"
    );
}

/// The quiet limb: the row genuinely holds `ada@example.com`, and the response must not.
#[tokio::test]
async fn masked_gated_field_returns_null_while_the_row_holds_a_value() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP masked_gated_field_returns_null_while_the_row_holds_a_value: no postgres");
        return;
    };

    let result = executor
        .execute_with_security(
            &entities_query("id name email"),
            Some(&representations()),
            &ctx("viewer"),
        )
        .await
        .unwrap();

    let entity = &result["data"]["_entities"][0];
    assert_eq!(entity["name"], json!("Ada"), "an ungated field must still be served");
    assert_eq!(
        entity["email"],
        serde_json::Value::Null,
        "the Mask field must come back null; got: {entity}"
    );
}

/// Positive control: the same query, by a caller holding both scopes, reads the real
/// stored values — so the two tests above cannot be green because nothing resolves.
#[tokio::test]
async fn a_scoped_caller_reads_the_real_values() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP a_scoped_caller_reads_the_real_values: no postgres");
        return;
    };

    let result = executor
        .execute_with_security(
            &entities_query("id salary email"),
            Some(&representations()),
            &ctx("auditor"),
        )
        .await
        .unwrap();

    let entity = &result["data"]["_entities"][0];
    assert_eq!(entity["salary"], json!(120_000), "the stored salary must be readable");
    assert_eq!(entity["email"], json!("ada@example.com"));
}
