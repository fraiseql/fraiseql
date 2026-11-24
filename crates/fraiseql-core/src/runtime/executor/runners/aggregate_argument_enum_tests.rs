//! A value at an enum position of a fact-table root is refused by the enum the root
//! publishes, on every entry, as a list root's is: naming the path and the enum, never the
//! value.

use serde_json::json;

use super::{Root, every_entry};

#[tokio::test]
async fn a_value_no_member_names_is_refused_by_the_published_enum() {
    let mut failures = Vec::new();
    for (root, argument, literal, value, path, enum_name) in [
        (
            Root::Aggregate,
            "orderBy",
            r#"[{field: "bogus"}]"#,
            json!([{"field": "bogus"}]),
            "orderBy[0].field",
            "SalesAggregateOrderByField",
        ),
        (
            Root::Aggregate,
            "orderBy",
            r#"[{field: "tenant_id", direction: "UP"}]"#,
            json!([{"field": "tenant_id", "direction": "UP"}]),
            "orderBy[0].direction",
            "SortDirection",
        ),
        (
            Root::Window,
            "orderBy",
            r#"[{field: "tenant_id", direction: "UP"}]"#,
            json!([{"field": "tenant_id", "direction": "UP"}]),
            "orderBy[0].direction",
            "SortDirection",
        ),
        (
            Root::Window,
            "windows",
            r#"[{function: {type: "bogus"}, alias: "rank"}]"#,
            json!([{"function": {"type": "bogus"}, "alias": "rank"}]),
            "windows[0].function.type",
            "WindowFunctionKind",
        ),
    ] {
        let refusal = format!("at `{path}` has enum type `{enum_name}`");
        let written =
            value.pointer(&format!("/0/{}", path.split_once('.').unwrap().1.replace('.', "/")));
        for (case, (res, sql)) in every_entry(root, argument, literal, &value).await {
            match (res, sql) {
                (Err(e), None) => {
                    let message = e.to_string();
                    if !message.contains(&refusal) {
                        failures.push(format!("{case}: not the enum's refusal: {message}"));
                    }
                    if let Some(Some(written)) = written.map(serde_json::Value::as_str) {
                        if message.contains(&format!("`{written}`")) {
                            failures.push(format!("{case}: quotes the value: {message}"));
                        }
                    }
                },
                (res, sql) => failures.push(format!("{case}: not refused: {res:?} {sql:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
