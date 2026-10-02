//! `@key` validation of federation entities (#1395).
//!
//! Each refusal has a twin that must stay accepted, so a rule that refused everything
//! would fail as surely as one that refused nothing.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{CompiledSchema, FederationKeyProblem};

/// A schema with `Org` (keyable fields), `Address` (nested object), `Reading` (embedded),
/// `Node` (interface) and the given entities.
fn schema(entities: &str) -> CompiledSchema {
    let json = format!(
        r#"{{
        "types": [
            {{"name":"Org","sql_source":"v_org","fields":[
                {{"name":"id","field_type":"ID"}},
                {{"name":"organizationId","field_type":"UUID"}},
                {{"name":"address","field_type":{{"Object":"Address"}}}},
                {{"name":"tags","field_type":{{"List":"String"}}}},
                {{"name":"owner","field_type":{{"Interface":"Node"}}}},
                {{"name":"status","field_type":{{"Enum":"Status"}}}}
            ]}},
            {{"name":"Address","sql_source":"","embedded":true,"fields":[
                {{"name":"city","field_type":"String"}},
                {{"name":"geo","field_type":{{"Object":"Geo"}}}}
            ]}},
            {{"name":"Geo","sql_source":"","embedded":true,"fields":[
                {{"name":"lat","field_type":"Float"}}
            ]}},
            {{"name":"Reading","sql_source":"","embedded":true,"fields":[
                {{"name":"serial","field_type":"String"}}
            ]}}
        ],
        "interfaces": [{{"name":"Node","fields":[{{"name":"id","field_type":"ID"}}]}}],
        "enums": [{{"name":"Status","values":[{{"name":"ACTIVE"}}]}}],
        "queries": [], "mutations": [], "subscriptions": [],
        "federation": {{"enabled": true, "version": "v2", "entities": [{entities}]}}
    }}"#
    );
    serde_json::from_str(&json).unwrap()
}

fn problems(entities: &str) -> Vec<FederationKeyProblem> {
    schema(entities).federation_key_problems()
}

#[test]
fn the_issue_repro_an_embedded_type_is_refused_as_an_entity() {
    assert_eq!(
        problems(r#"{"name":"Reading","key_fields":["id"]}"#),
        vec![FederationKeyProblem::EmbeddedType {
            entity: "Reading".into(),
        }]
    );
}

#[test]
fn a_key_naming_a_field_the_type_lacks_is_refused() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["organizationId","region"]}"#),
        vec![FederationKeyProblem::MissingField {
            entity: "Org".into(),
            path:   "region".into(),
            owner:  "Org".into(),
            hint:   None,
        }]
    );
}

#[test]
fn a_composite_key_written_as_one_string_is_checked_field_by_field() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["organizationId region"]}"#),
        vec![FederationKeyProblem::MissingField {
            entity: "Org".into(),
            path:   "region".into(),
            owner:  "Org".into(),
            hint:   None,
        }]
    );
}

#[test]
fn a_snake_case_key_against_a_camel_case_field_is_refused_with_the_published_spelling() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["organization_id"]}"#),
        vec![FederationKeyProblem::MissingField {
            entity: "Org".into(),
            path:   "organization_id".into(),
            owner:  "Org".into(),
            hint:   Some("organizationId".into()),
        }]
    );
}

#[test]
fn a_nested_key_naming_a_missing_field_is_refused_with_its_path() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["address { zip }"]}"#),
        vec![FederationKeyProblem::MissingField {
            entity: "Org".into(),
            path:   "address.zip".into(),
            owner:  "Address".into(),
            hint:   None,
        }]
    );
}

#[test]
fn a_selection_on_a_leaf_is_refused() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["id { x }"]}"#),
        vec![FederationKeyProblem::SelectionOnLeaf {
            entity: "Org".into(),
            path:   "id".into(),
        }]
    );
}

#[test]
fn an_object_without_a_selection_is_refused() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["address"]}"#),
        vec![FederationKeyProblem::ObjectWithoutSelection {
            entity: "Org".into(),
            path:   "address".into(),
        }]
    );
}

#[test]
fn list_and_interface_fields_cannot_be_part_of_a_key() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":["tags","owner { id }"]}"#),
        vec![
            FederationKeyProblem::InvalidKeyFieldType {
                entity: "Org".into(),
                path:   "tags".into(),
                kind:   "a list",
            },
            FederationKeyProblem::InvalidKeyFieldType {
                entity: "Org".into(),
                path:   "owner".into(),
                kind:   "an interface",
            },
        ]
    );
}

#[test]
fn an_undeclared_entity_type_is_refused() {
    assert_eq!(
        problems(r#"{"name":"Ghost","key_fields":["id"]}"#),
        vec![FederationKeyProblem::UnknownType {
            entity:      "Ghost".into(),
            suggestions: vec![],
        }]
    );
}

#[test]
fn an_entity_type_typo_suggests_the_declared_spelling() {
    let found = problems(r#"{"name":"Orgg","key_fields":["id"]}"#);
    assert_eq!(found.len(), 1);
    assert!(found[0].to_string().contains("did you mean: Org?"), "got: {}", found[0]);
}

#[test]
fn an_empty_key_is_refused() {
    assert_eq!(
        problems(r#"{"name":"Org","key_fields":[]}"#),
        vec![FederationKeyProblem::EmptyKey {
            entity: "Org".into(),
        }]
    );
}

#[test]
fn an_entity_declared_twice_is_refused() {
    assert_eq!(
        problems(
            r#"{"name":"Org","key_fields":["id"]},{"name":"Org","key_fields":["organizationId"]}"#
        ),
        vec![FederationKeyProblem::DuplicateEntity {
            entity: "Org".into(),
        }]
    );
}

#[test]
fn malformed_field_sets_are_refused() {
    for key in [
        "alias: id",
        "id(arg: 1)",
        "id @skip",
        "... on Org { id }",
        "address { city",
        "id }",
        "address { }",
        "{ id }",
    ] {
        let found = problems(&format!(r#"{{"name":"Org","key_fields":["{key}"]}}"#));
        assert!(
            matches!(found.as_slice(), [FederationKeyProblem::MalformedFieldSet { .. }]),
            "`{key}` must be refused as malformed, got {found:?}"
        );
    }
}

// ── twins: shapes that must stay accepted ────────────────────────────────────────────

#[test]
fn valid_single_composite_and_comma_separated_keys_are_accepted() {
    for key in [
        r#"["id"]"#,
        r#"["id","organizationId"]"#,
        r#"["id organizationId"]"#,
        r#"["id, status"]"#,
    ] {
        let found = problems(&format!(r#"{{"name":"Org","key_fields":{key}}}"#));
        assert!(found.is_empty(), "{key} is a valid key, got {found:?}");
    }
}

#[test]
fn a_valid_nested_key_is_accepted() {
    let found = problems(r#"{"name":"Org","key_fields":["id address { city geo { lat } }"]}"#);
    assert!(found.is_empty(), "got {found:?}");
}

#[test]
fn an_interface_entity_is_checked_against_the_interface_fields() {
    assert!(problems(r#"{"name":"Node","key_fields":["id"]}"#).is_empty());
    assert_eq!(
        problems(r#"{"name":"Node","key_fields":["uuid"]}"#),
        vec![FederationKeyProblem::MissingField {
            entity: "Node".into(),
            path:   "uuid".into(),
            owner:  "Node".into(),
            hint:   None,
        }]
    );
}

#[test]
fn an_extended_entity_declaring_its_external_key_field_is_accepted() {
    let found = problems(
        r#"{"name":"Org","key_fields":["organizationId"],"extends":true,"external_fields":["organizationId"]}"#,
    );
    assert!(found.is_empty(), "got {found:?}");
}

#[test]
fn a_schema_without_federation_has_no_key_problems() {
    let schema: CompiledSchema =
        serde_json::from_str(r#"{"types":[],"queries":[],"mutations":[],"subscriptions":[]}"#)
            .unwrap();
    assert!(schema.federation_key_problems().is_empty());
}

// ── the load path: every entry point refuses the artifact ────────────────────────────

#[test]
fn a_compiled_schema_with_a_bad_key_is_refused_at_load() {
    let json = serde_json::to_string(&schema(
        r#"{"name":"Org","key_fields":["organizationId","region"]}"#,
    ))
    .unwrap();
    let msg = CompiledSchema::from_json(&json, false).unwrap_err().to_string();
    assert!(msg.contains("federation entities cannot be served"), "got: {msg}");
    assert!(
        msg.contains("'Org'") && msg.contains("'region'"),
        "must name type and field: {msg}"
    );
}

#[test]
fn a_compiled_schema_with_valid_keys_loads() {
    let json = serde_json::to_string(&schema(r#"{"name":"Org","key_fields":["organizationId"]}"#))
        .unwrap();
    CompiledSchema::from_json(&json, false).unwrap();
}
