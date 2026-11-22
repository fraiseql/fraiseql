use serde_json::json;

use super::{annotate, is_refused, mark_refused};

fn sample() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tls": { "anyOf": [ { "$ref": "#/$defs/Tls" }, { "type": "null" } ] },
            "name": { "type": "string" }
        },
        "$defs": {
            "Tls": {
                "type": "object",
                "additionalProperties": false,
                "properties": { "cert_path": { "type": "string" } }
            }
        }
    })
}

#[test]
fn a_refused_key_under_an_optional_section_is_written_into_its_definition() {
    let mut schema = sample();
    mark_refused(&mut schema, "tls", "cert_file", "spelled `cert_path` here");
    let node = &schema["$defs"]["Tls"]["properties"]["cert_file"];
    assert!(is_refused(node), "{node}");
    assert_eq!(node["deprecated"], true);
    assert_eq!(node["description"], "spelled `cert_path` here");
    assert!(!is_refused(&schema["$defs"]["Tls"]["properties"]["cert_path"]));
}

#[test]
fn a_refused_top_level_table_the_type_never_had_is_added() {
    let mut schema = sample();
    mark_refused(&mut schema, "server", "", "the keys are top-level");
    assert!(is_refused(&schema["properties"]["server"]));
    assert!(!is_refused(&schema["properties"]["name"]));
}

#[test]
fn a_refused_table_keeps_the_keys_under_it() {
    let mut schema = json!({
        "type": "object",
        "properties": { "old": { "type": "object", "properties": { "k": { "type": "string" } } } }
    });
    mark_refused(&mut schema, "old", "", "retired");
    assert!(is_refused(&schema["properties"]["old"]));
    assert_eq!(schema["properties"]["old"]["properties"]["k"]["type"], "string");
}

#[test]
fn an_annotation_lands_on_the_key_through_an_optional_section() {
    let mut schema = sample();
    annotate(&mut schema, "tls.cert_path", "x-env", json!("TLS_CERT"));
    annotate(&mut schema, "name", "x-env", json!("NAME"));
    assert_eq!(schema["$defs"]["Tls"]["properties"]["cert_path"]["x-env"], "TLS_CERT");
    assert_eq!(schema["$defs"]["Tls"]["properties"]["cert_path"]["type"], "string");
    assert_eq!(schema["properties"]["name"]["x-env"], "NAME");
}
