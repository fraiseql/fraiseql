#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_mutation_executor_creation() {
    // Test that executor can be created (mock adapter would be used)
    // Actual mutation tests are in integration tests
}

// M-fed-mut-executor: an unrecognised operation name must fail loud rather than
// silently default to UPDATE (which would issue an `UPDATE` for a typo'd or
// unsupported mutation).

#[test]
fn determine_mutation_type_recognises_known_verbs() {
    assert_eq!(determine_mutation_type("createUser").unwrap(), MutationType::Create);
    assert_eq!(determine_mutation_type("addUser").unwrap(), MutationType::Create);
    assert_eq!(determine_mutation_type("updateUser").unwrap(), MutationType::Update);
    assert_eq!(determine_mutation_type("modifyUser").unwrap(), MutationType::Update);
    assert_eq!(determine_mutation_type("deleteUser").unwrap(), MutationType::Delete);
    assert_eq!(determine_mutation_type("removeUser").unwrap(), MutationType::Delete);
}

#[test]
fn determine_mutation_type_rejects_unknown_verb() {
    let result = determine_mutation_type("frobnicateUser");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Validation { .. })),
        "an unrecognised operation name must error, not default to UPDATE: {result:?}"
    );
}

// #400: under a camelCase GraphQL surface, mutation input keys must be reversed to
// the entity table's canonical snake_case column names before the query builders
// turn them into SQL identifiers — or the write targets a column that does not
// exist. Federation mutations are scalar-only, so only top-level keys are recased.

fn make_metadata(typename: &str, key_field: &str) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                typename.to_string(),
            keys:                vec![crate::types::KeyDirective {
                fields:     vec![key_field.to_string()],
                resolvable: true,
            }],
            is_extends:          false,
            external_fields:     Vec::new(),
            shareable_fields:    Vec::new(),
            inaccessible_fields: Vec::new(),
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

#[test]
fn canonicalize_input_keys_recases_camel_and_acronyms() {
    let vars = serde_json::json!({
        "name": "web-1",
        "dns1Id": "d-1",
        "s3Key": "k-2",
        "ipv4Cidr": "10.0.0.0/8",
        "oauth2Token": "t-3"
    });
    let out = canonicalize_input_keys(&vars, true);
    let obj = out.as_object().unwrap();
    assert!(obj.contains_key("name"), "single-word key unchanged: {obj:?}");
    assert_eq!(obj["dns_1_id"], "d-1", "digit-boundary key must recase: {obj:?}");
    assert_eq!(obj["s3_key"], "k-2", "acronym key must recase: {obj:?}");
    assert_eq!(obj["ipv4_cidr"], "10.0.0.0/8", "acronym key must recase: {obj:?}");
    assert_eq!(obj["oauth2_token"], "t-3", "acronym key must recase: {obj:?}");
    for stale in ["dns1Id", "s3Key", "ipv4Cidr", "oauth2Token"] {
        assert!(!obj.contains_key(stale), "verbatim '{stale}' must not survive: {obj:?}");
    }
}

#[test]
fn canonicalize_input_keys_noop_when_disabled() {
    // Preserve convention (recase = false): keys pass through verbatim.
    let vars = serde_json::json!({ "s3Key": "k" });
    assert_eq!(canonicalize_input_keys(&vars, false), vars);
}

#[test]
fn canonicalize_input_keys_idempotent_on_snake() {
    let vars = serde_json::json!({ "s3_key": "k", "dns_1_id": "d" });
    assert_eq!(
        canonicalize_input_keys(&vars, true),
        vars,
        "already-snake keys must be unchanged"
    );
}

#[test]
fn canonicalize_input_keys_leaves_values_untouched() {
    // Only keys are recased; a value that happens to look camelCase stays verbatim.
    let vars = serde_json::json!({ "s3Key": "myBucketName" });
    let out = canonicalize_input_keys(&vars, true);
    assert_eq!(out["s3_key"], "myBucketName", "value must not be recased: {out:?}");
}

#[test]
fn recased_keys_produce_snake_case_insert_columns() {
    // End-to-end: recased keys reach the builder as the table's snake_case columns.
    let meta = make_metadata("Server", "id");
    let vars = canonicalize_input_keys(
        &serde_json::json!({ "id": "s1", "s3Key": "b", "dns1Id": "d" }),
        true,
    );
    let sql = build_insert_query("Server", &vars, &meta).unwrap();
    assert!(sql.contains("\"s3_key\""), "insert column must be snake_case: {sql}");
    assert!(sql.contains("\"dns_1_id\""), "insert column must be snake_case: {sql}");
    assert!(!sql.contains("\"s3Key\""), "camelCase column must not survive: {sql}");
    assert!(!sql.contains("\"dns1Id\""), "camelCase column must not survive: {sql}");
}

#[test]
fn recased_keys_fix_update_set_and_key_lookup() {
    // The @key field is canonical (`dns_1_id`); the client sends the camelCase
    // surface (`dns1Id`). Without recasing, `vars.get("dns_1_id")` would miss and
    // the build would error "Key field 'dns_1_id' missing"; recasing fixes both
    // the SET column casing and the WHERE-key lookup.
    let meta = make_metadata("Server", "dns_1_id");
    let vars = canonicalize_input_keys(&serde_json::json!({ "dns1Id": "k1", "s3Key": "b" }), true);
    let sql = build_update_query("Server", &vars, &meta).unwrap();
    assert!(sql.contains("\"s3_key\""), "SET column must be snake_case: {sql}");
    assert!(sql.contains("WHERE \"dns_1_id\""), "WHERE key column must be snake_case: {sql}");
    assert!(!sql.contains("\"dns1Id\""), "camelCase must not survive: {sql}");
}
