#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! How a schema document's `security` block merges with the TOML compiled beside it.

use serde_json::json;

use super::*;

fn declared(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| (*k).to_owned()).collect()
}

fn lowered() -> Value {
    json!({"rate_limiting": {"enabled": true}, "tenancy": {"mode": "none"}})
}

#[test]
fn without_a_document_block_the_lowered_section_is_returned_unchanged() {
    let merged = reconcile("fraiseql.toml", lowered(), &declared(&["tenancy"]), None).unwrap();
    assert_eq!(merged, lowered());
}

#[test]
fn a_key_the_toml_does_not_set_is_taken_from_the_document() {
    let document = json!({"cost_budget": {"per_request_max": 100}, "tenancy": {"mode": "row"}});
    let merged = reconcile("fraiseql.toml", lowered(), &declared(&[]), Some(document)).unwrap();
    assert_eq!(merged["cost_budget"], json!({"per_request_max": 100}));
    assert_eq!(merged["tenancy"], json!({"mode": "row"}));
    assert_eq!(merged["rate_limiting"], json!({"enabled": true}));
}

#[test]
fn a_key_both_set_to_the_same_compiled_value_is_accepted() {
    // `{"mode": "none"}` and `{}` read as the same (default) tenancy section.
    let document = json!({"tenancy": {}});
    let merged =
        reconcile("fraiseql.toml", lowered(), &declared(&["tenancy"]), Some(document)).unwrap();
    assert_eq!(merged["tenancy"], json!({"mode": "none"}));
}

#[test]
fn a_key_both_set_to_different_values_is_refused_naming_it() {
    let document = json!({"tenancy": {"mode": "row"}});
    let err = reconcile("fraiseql.toml", lowered(), &declared(&["tenancy"]), Some(document))
        .unwrap_err()
        .to_string();
    assert!(err.contains("security `tenancy`"), "{err}");
}

#[test]
fn a_document_block_that_is_not_an_object_is_refused() {
    let err = reconcile("fraiseql.toml", lowered(), &declared(&[]), Some(json!([])))
        .unwrap_err()
        .to_string();
    assert!(err.contains("must be an object"), "{err}");
}

#[test]
fn the_project_config_sets_enterprise_through_the_tables_it_is_derived_from() {
    for source in ENTERPRISE_SOURCES {
        let table: toml::Table =
            format!("[fraiseql.security.{source}]\nenabled = true\n").parse().unwrap();
        assert!(declared_in_project_config(&table).contains("enterprise"), "{source}");
    }
    let table: toml::Table = "[project]\nname = \"app\"\n".parse().unwrap();
    assert!(declared_in_project_config(&table).is_empty());
}

#[test]
fn the_project_config_sets_tenancy_and_named_security_keys() {
    let table: toml::Table =
        "[fraiseql]\n[fraiseql.tenancy]\nmode = \"row\"\n[fraiseql.security]\nmulti_tenant = \
         true\n[fraiseql.security.rls]\nenabled = true\n"
            .parse()
            .unwrap();
    assert_eq!(
        declared_in_project_config(&table),
        declared(&["multi_tenant", "rls", "tenancy"])
    );
}

#[test]
fn a_toml_schema_sets_tenancy_and_named_security_keys() {
    let table: toml::Table =
        "[tenancy]\nmode = \"row\"\n[security.cost_budget]\nper_request_max = 5\n"
            .parse()
            .unwrap();
    assert_eq!(declared_in_toml_schema(&table), declared(&["cost_budget", "tenancy"]));
    let table: toml::Table = "[schema]\nname = \"app\"\n".parse().unwrap();
    assert!(declared_in_toml_schema(&table).is_empty());
}
