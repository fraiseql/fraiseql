//! Tests for the `fraiseql sources` status view.
//!
//! The merge (`build_status`) and rendering are pure, so they are covered here
//! without a database. The [`reader`](super::reader) is a thin PostgreSQL
//! projection exercised by the CLI integration leg.
#![allow(clippy::unwrap_used)] // Reason: test module

use std::collections::HashMap;

use fraiseql_core::schema::{RunAs, SourceDefinition};
use serde_json::json;

use super::{build_status, reader::CursorRow, render_text};

fn cursor_row(source: &str, version: i64, age: f64) -> CursorRow {
    CursorRow {
        source_name: source.to_string(),
        value: Some(json!({ "page": version })),
        version,
        updated_at: "2026-07-15T11:32:04+00:00".to_string(),
        age_seconds: age,
    }
}

#[test]
fn build_status_marks_cursor_unknown_without_a_database() {
    let sources = vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")];
    let statuses = build_status(&sources, None);

    assert_eq!(statuses.len(), 1);
    let status = &statuses[0];
    assert_eq!(status.name, "orders");
    assert_eq!(status.schedule, "*/5 * * * *");
    assert_eq!(status.cursor.state, "unknown");
    assert!(status.cursor.version.is_none());
}

#[test]
fn build_status_reports_never_advanced_when_no_row_exists() {
    let sources = vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")];
    // A connected-but-empty database: the map has no row for this source.
    let cursors: HashMap<String, CursorRow> = HashMap::new();

    let statuses = build_status(&sources, Some(&cursors));

    assert_eq!(statuses[0].cursor.state, "never_advanced");
}

#[test]
fn build_status_reports_the_advanced_watermark_and_lag() {
    let sources = vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")];
    let mut cursors = HashMap::new();
    cursors.insert("orders".to_string(), cursor_row("orders", 4, 37.2));

    let statuses = build_status(&sources, Some(&cursors));

    let cursor = &statuses[0].cursor;
    assert_eq!(cursor.state, "advanced");
    assert_eq!(cursor.version, Some(4));
    assert_eq!(cursor.value, Some(json!({ "page": 4 })));
    assert_eq!(cursor.age_seconds, Some(37.2));
}

#[test]
fn build_status_keys_cursors_on_the_declared_cursor_the_runtime_advances_under() {
    // The runtime advances the *declared* cursor key (`SourceDefinition::cursor_name()`), so
    // the status lookup must use the same key.
    //
    // This test previously asserted the opposite, on the grounds that "the runtime poller
    // advances under the source *name*". That was accurate and it was the bug (#868 item 4):
    // the poller ignored the declared override entirely, so `fraiseql sources` printed a
    // `cursor_name` next to a watermark read from a *different* key. Both halves are fixed —
    // the poller advances the override, and this lookup follows it.
    let sources =
        vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_cursor("shared")];
    let mut cursors = HashMap::new();
    cursors.insert("shared".to_string(), cursor_row("shared", 2, 5.0));

    let statuses = build_status(&sources, Some(&cursors));

    assert_eq!(statuses[0].cursor_name, "shared", "the declared cursor is surfaced");
    assert_eq!(
        statuses[0].cursor.state, "advanced",
        "and the lookup keys on it, so the reported watermark is the one the runtime advances"
    );
}

#[test]
fn build_status_reports_never_advanced_when_only_the_source_name_has_a_row() {
    // The negative direction: a row under the source *name* must not be reported as this
    // source's watermark when an override is declared. Without this, keying on either name
    // would satisfy the test above whenever both rows happened to exist.
    let sources =
        vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_cursor("shared")];
    let mut cursors = HashMap::new();
    cursors.insert("orders".to_string(), cursor_row("orders", 2, 5.0));

    let statuses = build_status(&sources, Some(&cursors));

    assert_eq!(
        statuses[0].cursor.state, "never_advanced",
        "a row under the source name is not this source's watermark once `cursor` is declared"
    );
}

#[test]
fn build_status_flags_an_unconfigured_run_as_as_fail_closed() {
    let configured =
        SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_run_as(RunAs {
            roles:  vec!["order:write".to_string()],
            scopes: vec![],
            tenant: None,
        });
    let unconfigured = SourceDefinition::new("invoices", "0 * * * *", "pollInvoices");

    let statuses = build_status(&[configured, unconfigured], None);

    assert!(statuses[0].run_as.configured);
    assert_eq!(statuses[0].run_as.roles, vec!["order:write".to_string()]);
    assert!(!statuses[1].run_as.configured, "no run_as ⇒ fail-closed");
}

#[test]
fn render_text_shows_fail_closed_and_disabled_and_lag() {
    let sources = vec![
        SourceDefinition::new("orders", "*/5 * * * *", "pollOrders"),
        SourceDefinition::new("invoices", "0 * * * *", "pollInvoices").disabled(),
    ];
    let mut cursors = HashMap::new();
    cursors.insert("orders".to_string(), cursor_row("orders", 4, 37.0));
    let statuses = build_status(&sources, Some(&cursors));

    let text = render_text(&statuses, true);

    assert!(text.contains("Sources (2)"), "header with the count");
    assert!(text.contains("fail-closed"), "the unconfigured run_as is called out");
    assert!(text.contains("DISABLED"), "the disabled source is marked");
    assert!(text.contains("advanced 37s ago"), "the cursor lag is shown");
    assert!(text.contains("never advanced"), "the un-advanced source is shown");
}

#[test]
fn render_text_notes_a_missing_database_connection() {
    let sources = vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")];
    let statuses = build_status(&sources, None);

    let text = render_text(&statuses, false);

    assert!(
        text.contains("no database connection"),
        "the operator is told cursor state is unread"
    );
}

#[test]
fn render_text_handles_no_sources() {
    let text = render_text(&[], true);
    assert!(text.contains("No sources declared"));
}
