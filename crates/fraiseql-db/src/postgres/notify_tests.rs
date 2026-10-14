#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

fn connector(url: &str) -> NotifyConnector {
    NotifyConnector::new(url, PostgresTlsConfig::default()).unwrap()
}

fn name_of(connector: &NotifyConnector) -> Option<&str> {
    connector.config.get_application_name()
}

#[test]
fn a_session_for_a_purpose_says_so_after_the_default_name() {
    let connector = connector("postgres://localhost/app");
    assert_eq!(name_of(&connector), Some(NOTIFY_APPLICATION_NAME));
    assert_eq!(
        name_of(&connector.serving("tviews_changes")),
        Some("fraiseql-notify/tviews_changes")
    );
}

#[test]
fn a_session_for_a_purpose_keeps_the_url_s_name_first() {
    let connector = connector("postgres://localhost/app?application_name=ops");
    assert_eq!(name_of(&connector.serving("tviews_changes")), Some("ops/tviews_changes"));
}

#[test]
fn a_long_name_is_cut_to_what_postgresql_keeps() {
    let connector =
        connector(&format!("postgres://localhost/app?application_name={}", "é".repeat(40)));
    let name = name_of(&connector.serving("tviews_changes")).unwrap().to_string();
    assert!(name.len() <= 63, "{} bytes", name.len());
    assert!(name.starts_with('é'));
}
