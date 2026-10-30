//! A field the caller's scopes mask is written by the projection as `null`, and the
//! projection a document keeps is keyed by those masks.
//!
//! The kept projection answers every later read of the same document under an equal
//! `ProjectionKey`. Were a caller's masks read by the projection but not keyed, the first
//! caller to read a document would fix its SQL for every caller after: an unmasked
//! projection would serve the masked field to a caller who may not read it. So every read
//! here is checked against the projection a fresh executor builds for the same caller.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use proptest::prelude::*;

use crate::{
    backend::types::JsonbValue,
    runtime::executor::{Executor, test_support::CapturingMockAdapter},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};

/// The scopes the schema's masked fields require, one per level.
const SCOPES: [&str; 3] = ["read:salary", "read:email", "read:zip"];

/// The roles that grant them, in the same order.
const ROLES: [&str; 3] = ["payroll", "contact", "postal"];

/// A field that reads as `null` to a caller without `scope`.
fn masked(name: &str, scope: &str) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::String);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = FieldDenyPolicy::Mask;
    field
}

fn object(name: &str, view: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
    let mut type_def = TypeDefinition::new(name, view);
    type_def.fields = fields;
    type_def
}

/// `users` over `v_user`: a masked field at the root, in a nested object and in an object
/// below that.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema
        .queries
        .push(QueryDefinition::new("users", "User").returning_list().with_sql_source("v_user"));
    schema.types.push(object(
        "User",
        "v_user",
        vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("name", FieldType::String),
            masked("salary", SCOPES[0]),
            FieldDefinition::nullable("profile", FieldType::Object("Profile".to_string())),
        ],
    ));
    schema.types.push(object(
        "Profile",
        "v_profile",
        vec![
            FieldDefinition::new("tier", FieldType::String),
            masked("email", SCOPES[1]),
            FieldDefinition::nullable("address", FieldType::Object("Address".to_string())),
        ],
    ));
    schema.types.push(object(
        "Address",
        "v_address",
        vec![
            FieldDefinition::new("city", FieldType::String),
            masked("zip", SCOPES[2]),
        ],
    ));
    let mut security = SecurityConfig::new();
    for (role, scope) in ROLES.iter().zip(SCOPES) {
        security.add_role(RoleDefinition::new(*role, vec![scope.to_string()]));
    }
    schema.security = Some(security);
    schema
}

fn rows() -> Vec<JsonbValue> {
    vec![JsonbValue::new(serde_json::json!({
        "id": 1, "name": "Ada", "salary": "S",
        "profile": {"tier": "gold", "email": "E", "address": {"city": "Paris", "zip": "Z"}}
    }))]
}

/// A caller holding `roles`, its other claims drawn from `noise` so that two callers
/// masked alike can differ in everything else: who it is, its tenant, a role that grants
/// nothing, and token scopes, which grant no field (a field's scope is granted by a role).
fn caller(roles: &[&str], noise: u8) -> SecurityContext {
    let mut held: Vec<String> = roles.iter().map(ToString::to_string).collect();
    if noise & 4 != 0 {
        held.push("viewer".to_string());
    }
    SecurityContext {
        user_id:          format!("user-{}", noise % 3).into(),
        roles:            held,
        tenant_id:        (noise & 8 != 0).then(|| "tenant-a".into()),
        scopes:           if noise & 16 == 0 {
            vec![]
        } else {
            SCOPES.iter().map(ToString::to_string).collect()
        },
        attributes:       HashMap::new(),
        request_id:       "req-mask".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The projection SQL `executor` read `document` through for `caller`.
async fn projection_for(
    executor: &Executor,
    adapter: &CapturingMockAdapter,
    document: &str,
    caller: &SecurityContext,
) -> String {
    executor
        .execute_with_security(document, None, caller)
        .await
        .unwrap_or_else(|e| panic!("{document}: {e}"));
    adapter
        .captured_projection
        .lock()
        .unwrap()
        .take()
        .expect("a projected read")
        .projection_template
}

/// The projection a fresh executor, with nothing kept, builds for `caller`.
async fn fresh_projection_for(document: &str, caller: &SecurityContext) -> String {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    let executor = Executor::new(schema(), adapter.clone());
    projection_for(&executor, &adapter, document, caller).await
}

#[tokio::test]
async fn a_masked_field_is_written_as_null_where_it_was_selected_under_its_alias() {
    let document =
        "{ users { name pay: salary profile { tier email address { city code: zip } } } }";
    let sql = fresh_projection_for(document, &caller(&[], 0)).await;

    for written in [
        "'pay' VALUE (NULL::json)",
        "'email' VALUE (NULL::json)",
        "'code' VALUE (NULL::json)",
    ] {
        assert!(sql.contains(written), "{written} missing from {sql}");
    }
    for stored in ["'salary'", "'email' VALUE \"data\"", "->>'zip'"] {
        assert!(!sql.contains(stored), "a masked field's stored key read ({stored}): {sql}");
    }

    let allowed = fresh_projection_for(document, &caller(&ROLES, 0)).await;
    assert!(!allowed.contains("NULL::json"), "a caller holding every role: {allowed}");
    assert!(allowed.contains("->>'salary'"), "{allowed}");
}

/// The RED for a key without the caller's masks, in both orders: a masked caller first
/// would null the field for the caller allowed to read it, an allowed caller first would
/// serve it to the caller who may not.
#[tokio::test]
async fn a_projection_kept_for_one_caller_never_answers_a_caller_masked_otherwise() {
    let document = "{ users { name salary profile { email } } }";
    let allowed = caller(&ROLES, 0);
    let masked = caller(&[], 0);
    for order in [[&allowed, &masked], [&masked, &allowed]] {
        let adapter = Arc::new(CapturingMockAdapter::new(rows()));
        let executor = Executor::new(schema(), adapter.clone());
        for who in order {
            let kept = projection_for(&executor, &adapter, document, who).await;
            assert_eq!(kept, fresh_projection_for(document, who).await, "roles {:?}", who.roles);
        }
    }
}

/// One selection: each field of each level kept or not, and kept under an alias or not.
#[derive(Debug, Clone)]
struct Selection {
    user:    [(bool, bool); 4],
    profile: [(bool, bool); 3],
    address: [(bool, bool); 2],
}

impl Selection {
    /// The document, or `None` when it selects nothing at the root.
    fn document(&self) -> Option<String> {
        let level = |names: &[&str], picks: &[(bool, bool)], nested: Option<String>| {
            let fields: Vec<String> = names
                .iter()
                .zip(picks)
                .filter(|(_, (kept, _))| *kept)
                .filter_map(|(name, (_, aliased))| {
                    let head = if *aliased {
                        format!("a_{name}: {name}")
                    } else {
                        (*name).to_string()
                    };
                    match (*name, &nested) {
                        ("profile" | "address", Some(inner)) => Some(format!("{head} {inner}")),
                        ("profile" | "address", None) => None,
                        _ => Some(head),
                    }
                })
                .collect();
            (!fields.is_empty()).then(|| format!("{{ {} }}", fields.join(" ")))
        };
        let address = level(&["city", "zip"], &self.address, None);
        let profile = level(&["tier", "email", "address"], &self.profile, address);
        let user = level(&["id", "name", "salary", "profile"], &self.user, profile)?;
        Some(format!("{{ users {user} }}"))
    }
}

fn picks<const N: usize>() -> impl Strategy<Value = [(bool, bool); N]> {
    prop::array::uniform::<_, N>((any::<bool>(), any::<bool>()))
}

fn selection() -> impl Strategy<Value = Selection> {
    (picks::<4>(), picks::<3>(), picks::<2>()).prop_map(|(user, profile, address)| Selection {
        user,
        profile,
        address,
    })
}

/// A caller: which of [`ROLES`] it holds, and its other claims.
fn callers() -> impl Strategy<Value = Vec<(u8, u8)>> {
    prop::collection::vec((0u8..8, any::<u8>()), 2..6)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever the selection, its aliases and depths, and whichever callers read it in
    /// whichever order: each read's projection is the one a fresh executor builds for the
    /// same caller. Equal keys therefore mean equal projections, and an input that varies
    /// the projection by caller without varying the key fails here.
    #[test]
    fn a_kept_projection_is_the_one_its_caller_would_build(
        selection in selection(),
        callers in callers(),
    ) {
        let Some(document) = selection.document() else {
            return Ok(());
        };
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let adapter = Arc::new(CapturingMockAdapter::new(rows()));
            let executor = Executor::new(schema(), adapter.clone());
            for (held, noise) in callers {
                let roles: Vec<&str> = ROLES
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| held & (1 << i) != 0)
                    .map(|(_, role)| *role)
                    .collect();
                let who = caller(&roles, noise);
                let kept = projection_for(&executor, &adapter, &document, &who).await;
                let fresh = fresh_projection_for(&document, &who).await;
                assert_eq!(kept, fresh, "{document} for {roles:?}");
                // Not vacuous: the projection does vary by caller, by its masks.
                let masks = ["salary", "email", "zip"]
                    .iter()
                    .zip(ROLES)
                    .any(|(field, role)| document.contains(field) && !roles.contains(&role));
                assert_eq!(kept.contains("NULL::json"), masks, "{document} for {roles:?}: {kept}");
            }
        });
    }
}
