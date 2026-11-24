//! A relay route publishes the page it answers: `meta` with the page info and cursors, and
//! `links` with `next`/`prev`. An offset route keeps its own envelope.

use super::*;

fn relay_schema() -> CompiledSchema {
    let mut schema = rest_schema();
    for q in &mut schema.queries {
        if q.name == "users" {
            q.relay = true;
        }
    }
    schema
}

fn envelope(spec: &serde_json::Value) -> &serde_json::Value {
    &spec["paths"]["/users"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
}

#[test]
fn a_relay_route_publishes_its_page_info_and_links() {
    let spec = generate(&relay_schema());
    let schema = envelope(&spec);
    for key in ["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"] {
        assert!(schema["properties"]["meta"]["properties"][key].is_object(), "`{key}`: {schema}");
    }
    for link in ["next", "prev"] {
        let link = &schema["properties"]["links"]["properties"][link];
        assert_eq!(link["nullable"], true, "{schema}");
    }
    assert!(schema["properties"]["meta"]["properties"]["offset"].is_null(), "{schema}");
}

#[test]
fn an_offset_route_publishes_no_links() {
    let spec = generate(&rest_schema());
    let schema = envelope(&spec);
    assert!(schema["properties"]["meta"]["properties"]["offset"].is_object(), "{schema}");
    assert!(schema["properties"]["links"].is_null(), "{schema}");
}
