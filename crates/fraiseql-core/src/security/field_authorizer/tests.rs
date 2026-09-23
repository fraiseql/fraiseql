//! Unit tests for the [`FieldAuthorizer`](super::FieldAuthorizer) trait surface and
//! the reference implementations used across the field-authz test suites.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;
use serde_json::json;

use super::{
    FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, selection_set_has_nested_gated_field,
    selection_set_selects_gated_field,
};
use crate::{
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, TypeDefinition},
    security::SecurityContext,
    types::UserId,
};

/// Allows every field. Reference impl for the passthrough/no-op case.
struct AllowAll;
impl FieldAuthorizer for AllowAll {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Ok(FieldAuthzDecision::Allow)
    }
}

/// Denies every field with `Reject`. Reference impl for the hard-deny case.
struct DenyAll;
impl FieldAuthorizer for DenyAll {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Ok(FieldAuthzDecision::Deny {
            code:    "denied".to_string(),
            on_deny: FieldDenyPolicy::Reject,
        })
    }
}

/// Always returns `Err`. Reference impl for the fail-closed honesty invariant.
struct RaisingFieldAuthorizer;
impl FieldAuthorizer for RaisingFieldAuthorizer {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Err(FraiseQLError::Validation {
            message: "policy backend unreachable".to_string(),
            path:    None,
        })
    }
}

/// Reveals the field only to the row's owner; masks it otherwise.
struct OwnerOnly;
impl FieldAuthorizer for OwnerOnly {
    fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        let owner = req.parent.and_then(|p| p.get("owner_id")).and_then(|v| v.as_str());
        if owner == Some(req.principal.user_id.as_str()) {
            Ok(FieldAuthzDecision::Allow)
        } else {
            Ok(FieldAuthzDecision::Deny {
                code:    "not_owner".to_string(),
                on_deny: FieldDenyPolicy::Mask,
            })
        }
    }
}

fn ctx(user_id: &str) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new(user_id),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn request<'a>(
    principal: &'a SecurityContext,
    parent: &'a serde_json::Value,
) -> FieldAuthzRequest<'a> {
    FieldAuthzRequest {
        principal,
        type_name: "User",
        field_name: "email",
        parent: Some(parent),
        arguments: None,
    }
}

#[test]
fn allow_all_allows() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u2" });
    let decision = AllowAll.authorize_field(&request(&principal, &parent)).unwrap();
    assert!(matches!(decision, FieldAuthzDecision::Allow));
}

#[test]
fn deny_all_rejects() {
    let principal = ctx("u1");
    let parent = json!({});
    let decision = DenyAll.authorize_field(&request(&principal, &parent)).unwrap();
    match decision {
        FieldAuthzDecision::Deny { code, on_deny } => {
            assert_eq!(code, "denied");
            assert_eq!(on_deny, FieldDenyPolicy::Reject);
        },
        FieldAuthzDecision::Allow => panic!("expected deny"),
    }
}

#[test]
fn raising_authorizer_returns_err() {
    let principal = ctx("u1");
    let parent = json!({});
    let result = RaisingFieldAuthorizer.authorize_field(&request(&principal, &parent));
    assert!(result.is_err(), "raising authorizer must return Err for fail-closed handling");
}

#[test]
fn owner_only_allows_owner() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u1" });
    let decision = OwnerOnly.authorize_field(&request(&principal, &parent)).unwrap();
    assert!(matches!(decision, FieldAuthzDecision::Allow));
}

#[test]
fn owner_only_masks_non_owner() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u2" });
    let decision = OwnerOnly.authorize_field(&request(&principal, &parent)).unwrap();
    match decision {
        FieldAuthzDecision::Deny { code, on_deny } => {
            assert_eq!(code, "not_owner");
            assert_eq!(on_deny, FieldDenyPolicy::Mask);
        },
        FieldAuthzDecision::Allow => panic!("expected deny for non-owner"),
    }
}

// ── inline-fragment gated-field detection (released #423 bypass regression) ───

/// A `User` type whose `ssn` field is policy-gated.
fn schema_with_gated_ssn() -> crate::schema::CompiledSchema {
    use crate::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};
    let mut schema = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("ssn", FieldType::String).with_authorize(true),
    ];
    schema.types.push(user);
    schema.build_indexes();
    schema
}

/// A bare field selection (no args / nesting / directives).
fn field(name: &str) -> crate::graphql::FieldSelection {
    crate::graphql::FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

/// Regression for a released-version (#423, since v2.5.0) query-path authorization
/// bypass: a policy-gated field wrapped in a same-type inline fragment
/// (`{ users { ... on User { ssn } } }`) was invisible to gated-field detection —
/// the detector matched selection names literally, so the authorizer was skipped
/// while the projector (which resolves fragments) emitted the field. The detector
/// now resolves inline fragments before matching.
#[test]
fn gated_field_inside_inline_fragment_is_detected() {
    let schema = schema_with_gated_ssn();
    let fragment = crate::graphql::FieldSelection {
        name:          "...on User".to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![field("id"), field("ssn")],
        directives:    vec![],
    };
    let selections = vec![fragment];

    assert!(
        super::selection_set_selects_gated_field(&schema, "User", &selections),
        "a gated field inside a same-type inline fragment must be detected"
    );
    let gated = super::collect_top_level_gated_fields(
        &schema,
        "User",
        &selections,
        &std::collections::HashMap::new(),
    )
    .expect("arguments are well-formed");
    assert_eq!(gated.len(), 1, "the fragment-wrapped gated field must be collected");
    assert_eq!(gated[0].field_name, "ssn");
}

// ── #903: a policy must see the argument the client sent ──────────────────────

/// A gated field carrying `mask: $level` — the shape every generated client
/// produces, since clients pass arguments as variables rather than inlining them.
fn gated_field_with_variable_argument() -> Vec<crate::graphql::FieldSelection> {
    let mut ssn = field("ssn");
    ssn.arguments = vec![crate::graphql::GraphQLArgument {
        name:       "mask".to_string(),
        value_type: "variable".to_string(),
        value_json: crate::graphql::value_json::variable_ref("level").to_string(),
    }];
    vec![field("id"), ssn]
}

#[test]
fn a_gated_field_argument_reaches_the_authorizer_as_its_value() {
    let schema = schema_with_gated_ssn();
    let variables = std::collections::HashMap::from([("level".to_string(), json!("full"))]);

    let gated = super::collect_top_level_gated_fields(
        &schema,
        "User",
        &gated_field_with_variable_argument(),
        &variables,
    )
    .expect("arguments are well-formed");

    assert_eq!(gated.len(), 1);
    assert_eq!(
        gated[0].arguments.as_ref().expect("the gated field carries one argument"),
        &json!({"mask": "full"}),
        "#903: a policy written as `arguments.mask == \"full\"` must compare against the \
         value the client sent. Delivering the variable reference marker instead makes \
         every such policy silently take its `otherwise` branch, on data nobody sent"
    );
}

#[test]
fn an_omitted_variable_reaches_the_authorizer_as_null_not_as_a_marker() {
    let schema = schema_with_gated_ssn();
    let gated = super::collect_top_level_gated_fields(
        &schema,
        "User",
        &gated_field_with_variable_argument(),
        &std::collections::HashMap::new(),
    )
    .expect("arguments are well-formed");

    assert_eq!(
        gated[0].arguments.as_ref().unwrap(),
        &json!({"mask": null}),
        "an undefined variable is GraphQL null — the marker must never survive into a \
         policy input, whether or not the variable was supplied"
    );
}

#[test]
fn an_inline_argument_is_unaffected_by_resolution() {
    let schema = schema_with_gated_ssn();
    let mut ssn = field("ssn");
    ssn.arguments = vec![crate::graphql::GraphQLArgument {
        name:       "mask".to_string(),
        value_type: "string".to_string(),
        value_json: json!("full").to_string(),
    }];

    let gated = super::collect_top_level_gated_fields(
        &schema,
        "User",
        &[field("id"), ssn],
        &std::collections::HashMap::from([("level".to_string(), json!("ignored"))]),
    )
    .expect("arguments are well-formed");

    assert_eq!(
        gated[0].arguments.as_ref().unwrap(),
        &json!({"mask": "full"}),
        "an inline literal must pass through untouched"
    );
}

// ── The nested-gate detector (#423) ────────────────────────────────────────────
//
// `selection_set_has_nested_gated_field` is the guard three read paths fail closed
// on — `apply_dynamic_field_authorizer` (query_regular.rs:855), `row_field_gate`
// (query_regular.rs:1412) and the mutation projection (mutation/mod.rs:78). Per-row
// enforcement covers the top-level entity row only, so a gated field reached through
// a materialised nested object is refused rather than adjudicated, and the refusal is
// only as good as this detector's ability to *see* the field.
//
// It had no tests. The tests below pin the three ways it resolves a nested type,
// because each is a way it could quietly answer `false` — which is not a refusal that
// fails, it is a gated field served without adjudication, under a 200.

/// A plain, ungated field.
fn plain(name: &str, field_type: FieldType) -> FieldDefinition {
    FieldDefinition::new(name, field_type)
}

/// A field carrying the dynamic `authorize` gate.
fn gated(name: &str, field_type: FieldType) -> FieldDefinition {
    FieldDefinition {
        authorize: true,
        ..FieldDefinition::new(name, field_type)
    }
}

/// An object type with the given fields. `sql_source` is irrelevant here — the
/// detector reads `fields` and `field_type` only.
fn object(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
    TypeDefinition {
        fields,
        ..TypeDefinition::new(name, "v_unused")
    }
}

/// A selection, with or without a sub-selection.
fn select(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     Vec::new(),
        nested_fields: nested,
        directives:    Vec::new(),
    }
}

fn schema_of(types: Vec<TypeDefinition>) -> CompiledSchema {
    CompiledSchema {
        types,
        ..CompiledSchema::default()
    }
}

#[test]
fn a_gated_field_on_a_nested_object_is_seen() {
    let schema = schema_of(vec![
        object(
            "Post",
            vec![
                plain("title", FieldType::String),
                plain("author", FieldType::Object("Author".to_string())),
            ],
        ),
        object(
            "Author",
            vec![
                plain("name", FieldType::String),
                gated("salary", FieldType::Int),
            ],
        ),
    ]);
    let selection = vec![select("author", vec![select("salary", vec![])])];

    assert!(
        selection_set_has_nested_gated_field(&schema, "Post", &selection),
        "`author {{ salary }}` gates on Author, not on Post: the detector must resolve the \
         nested field's own type to find it"
    );
}

#[test]
fn a_gated_field_inside_a_list_of_nested_objects_is_seen() {
    let schema = schema_of(vec![
        object(
            "Post",
            vec![
                plain("title", FieldType::String),
                plain(
                    "comments",
                    FieldType::List(Box::new(FieldType::Object("Comment".to_string()))),
                ),
            ],
        ),
        object(
            "Comment",
            vec![
                plain("body", FieldType::String),
                gated("authorIp", FieldType::String),
            ],
        ),
    ]);
    let selection = vec![select("comments", vec![select("authorIp", vec![])])];

    // The list wrapper, specifically. A `List(Object(_))` whose element type is not
    // unwrapped names no type the schema knows, and the detector then reports "nothing
    // gated below here" for every to-many nested selection in the schema.
    assert!(
        selection_set_has_nested_gated_field(&schema, "Post", &selection),
        "a gated field on the element type of a list-valued nested selection must be seen"
    );
}

#[test]
fn a_gated_field_two_levels_down_is_seen() {
    let schema = schema_of(vec![
        object("Post", vec![plain("author", FieldType::Object("Author".to_string()))]),
        object("Author", vec![plain("org", FieldType::Object("Org".to_string()))]),
        object(
            "Org",
            vec![
                plain("name", FieldType::String),
                gated("revenue", FieldType::Int),
            ],
        ),
    ]);
    let selection = vec![select(
        "author",
        vec![select("org", vec![select("revenue", vec![])])],
    )];

    // Depth two, so the detector has to recurse rather than look one level down. A
    // one-level check passes the previous two tests and admits this one.
    assert!(
        selection_set_has_nested_gated_field(&schema, "Post", &selection),
        "the search must recurse to arbitrary depth, not inspect only the first level"
    );
}

#[test]
fn a_nested_selection_with_nothing_gated_is_not_flagged() {
    let schema = schema_of(vec![
        object("Post", vec![plain("author", FieldType::Object("Author".to_string()))]),
        object(
            "Author",
            vec![
                plain("name", FieldType::String),
                gated("salary", FieldType::Int),
            ],
        ),
    ]);
    // `salary` is gated on Author but is *not selected*.
    let selection = vec![select("author", vec![select("name", vec![])])];

    // The negative control. Without it a detector stuck at `true` passes every test
    // above, and every nested selection in every schema is refused — the guard would
    // read as airtight while making the feature unusable.
    assert!(
        !selection_set_has_nested_gated_field(&schema, "Post", &selection),
        "a gated field that exists on the nested type but is not selected gates nothing"
    );
}

#[test]
fn a_gated_field_at_the_top_level_is_not_reported_as_nested() {
    let schema = schema_of(vec![object(
        "Post",
        vec![
            plain("title", FieldType::String),
            gated("draftNotes", FieldType::String),
        ],
    )]);
    let selection = vec![select("title", vec![]), select("draftNotes", vec![])];

    // The two detectors must not collapse into one. `selects_gated` decides whether the
    // per-row authorizer runs at all; `has_nested` decides whether the request is
    // refused instead. Reporting a top-level gated field as nested would refuse every
    // gated read outright — the #423 behaviour `apply_dynamic_field_authorizer` exists
    // to replace.
    assert!(
        selection_set_selects_gated_field(&schema, "Post", &selection),
        "a top-level gated field is selected"
    );
    assert!(
        !selection_set_has_nested_gated_field(&schema, "Post", &selection),
        "a top-level gated field is adjudicated per row, not refused as nested"
    );
}
