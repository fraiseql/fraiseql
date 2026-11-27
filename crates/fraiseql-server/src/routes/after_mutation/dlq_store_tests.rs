//! An unknown dead-letter store refuses boot, naming where the value came from, rather
//! than degrading a durability setting to the in-memory store.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::DlqStoreKind;

const VAR: &str = "FRAISEQL_FUNCTIONS_DLQ_STORE";

fn env(value: &'static str) -> impl Fn(&str) -> Option<String> {
    move |key| (key == VAR).then(|| value.to_string())
}

#[test]
fn an_unknown_compiled_store_is_refused_naming_the_key() {
    for value in ["postgress", "redis", ""] {
        let refusal = DlqStoreKind::resolve(Some(value), |_| None).unwrap_err();
        assert!(refusal.contains("[functions] dlq_store"), "{value:?}: {refusal}");
        assert!(refusal.contains(&format!("{value:?}")), "{value:?}: {refusal}");
    }
}

#[test]
fn an_unknown_store_from_the_environment_is_refused_naming_the_variable() {
    for value in ["postgress", "redis", ""] {
        let refusal = DlqStoreKind::resolve(Some("postgres"), env(value)).unwrap_err();
        assert!(refusal.contains(VAR), "{value:?}: {refusal}");
    }
}

#[test]
fn the_known_stores_resolve() {
    assert_eq!(DlqStoreKind::resolve(None, |_| None).unwrap(), DlqStoreKind::Memory);
    assert_eq!(DlqStoreKind::resolve(Some(" PG "), |_| None).unwrap(), DlqStoreKind::Postgres);
    assert_eq!(DlqStoreKind::resolve(None, env("memory")).unwrap(), DlqStoreKind::Memory);
}
