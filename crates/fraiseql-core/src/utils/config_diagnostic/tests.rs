#![allow(clippy::unwrap_used)] // Reason: test code

use super::*;

/// A code serializes as the string tools match on: the serde name and `as_str` are one
/// spelling, for every code.
#[test]
fn every_code_serializes_as_its_string() {
    for code in DiagnosticCode::ALL {
        assert_eq!(serde_json::to_value(code).unwrap(), code.as_str(), "{code:?}");
    }
}

#[test]
fn a_diagnostic_serializes_without_its_message_and_with_a_null_key_for_a_table() {
    let diagnostic = ConfigDiagnostic::new(
        DiagnosticCode::Retired,
        "schema",
        "",
        "`[schema]` is no longer accepted".to_owned(),
    )
    .with_replacement("`[project]`");
    assert_eq!(
        serde_json::to_value(&diagnostic).unwrap(),
        serde_json::json!({
            "code": "E-CONFIG-RETIRED",
            "file": null,
            "table": "schema",
            "key": null,
            "replacement": "`[project]`",
            "belongs_in": null,
            "also_in": null
        })
    );
    assert_eq!(diagnostic.to_string(), "`[schema]` is no longer accepted");
}

#[test]
fn a_setting_made_twice_serializes_the_other_file_and_its_key() {
    let diagnostic = ConfigDiagnostic::new(
        DiagnosticCode::SetTwice,
        "server",
        "port",
        "made in both".to_owned(),
    )
    .with_also_in("server.toml", "bind_addr");
    assert_eq!(
        serde_json::to_value(&diagnostic).unwrap(),
        serde_json::json!({
            "code": "E-CONFIG-SET-TWICE",
            "file": null,
            "table": "server",
            "key": "port",
            "replacement": null,
            "belongs_in": null,
            "also_in": {"file": "server.toml", "key": "bind_addr"}
        })
    );
}
