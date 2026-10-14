#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::schema::{CompiledSchema, LocaleConfig, LocaleSource};

use super::session_variables_supported_check;

fn with_locale() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".to_string()],
            std::collections::BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

/// #1115: the wire adapter cannot apply session variables, so a schema with `[locale]` is
/// refused when a server is built over it; the same schema boots on an adapter that applies
/// them, and a schema without them boots on the wire adapter. The server binary no longer
/// boots on it (#1566), but the adapter is still a library an embedder can pass to
/// `Server::new_read_only`.
#[test]
fn a_schema_needing_session_variables_is_refused_on_the_wire_adapter() {
    let wire = fraiseql_core::db::FraiseWireAdapter::new("postgres://localhost/unused");
    let err = session_variables_supported_check(&with_locale(), &wire).unwrap_err();
    assert!(err.to_string().contains("[locale]"), "{err}");
    session_variables_supported_check(&CompiledSchema::new(), &wire)
        .expect("nothing to apply: the wire adapter boots");

    let double = fraiseql_test_utils::failing_adapter::FailingAdapter::new();
    session_variables_supported_check(&with_locale(), &double)
        .expect("an adapter that applies them boots");
}

/// The same refusal, through the constructor an embedder serving it would use.
#[tokio::test]
async fn a_server_over_the_wire_adapter_refuses_to_boot_with_locale() {
    let wire = std::sync::Arc::new(fraiseql_core::db::FraiseWireAdapter::new(
        "postgres://localhost/unused",
    ));
    let config = crate::server_config::ServerConfig {
        database_url: "postgres://localhost/unused".to_string(),
        ..crate::server_config::ServerConfig::default()
    };
    let err = Box::pin(crate::Server::new_read_only(config, with_locale(), wire, None))
        .await
        .err()
        .expect("a server over the wire adapter with [locale] must not boot");
    assert!(err.to_string().contains("cannot apply session variables"), "{err}");
}

// ── #1555: the row cache is opt-in, and a deployment that turned it on is told so ──

/// The JSON log events `warn_when_nothing_is_cacheable` emits for `schema`.
fn cache_opt_in_events(schema: &CompiledSchema, cache_enabled: bool) -> Vec<serde_json::Value> {
    log_events(|| super::warn_when_nothing_is_cacheable(schema, cache_enabled))
}

/// The JSON log events `emit` writes.
fn log_events(emit: impl FnOnce()) -> Vec<serde_json::Value> {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt().json().with_writer(move || writer.clone()).finish();
    tracing::subscriber::with_default(subscriber, emit);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    log.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn schema_with_users(cache_ttl_seconds: Option<u64>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut users = fraiseql_core::schema::QueryDefinition::new("users", "User").returning_list();
    users.sql_source = Some("v_user".to_string());
    users.cache_ttl_seconds = cache_ttl_seconds;
    schema.queries.push(users);
    schema
}

#[test]
fn a_cache_no_query_opts_into_is_reported_at_boot() {
    let events = cache_opt_in_events(&schema_with_users(None), true);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    assert_eq!(events[0]["fields"]["cacheable_queries"], 0);
}

#[test]
fn a_cache_a_query_opts_into_says_nothing() {
    assert!(cache_opt_in_events(&schema_with_users(Some(60)), true).is_empty());
}

#[test]
fn a_disabled_cache_says_nothing_here() {
    assert!(cache_opt_in_events(&schema_with_users(None), false).is_empty());
}

// ── #1577: the cache is per process; an entry that never expires is stale on every other ──

/// The JSON log events `warn_on_entries_no_other_server_evicts` emits for `schema`.
fn per_process_events(schema: &CompiledSchema, cache_enabled: bool) -> Vec<serde_json::Value> {
    use crate::server_config::CacheCrossInstance;
    log_events(|| {
        super::warn_on_entries_no_other_server_evicts(
            schema,
            cache_enabled,
            CacheCrossInstance::Off,
        );
    })
}

#[test]
fn a_query_cached_without_expiry_is_named_at_boot() {
    let events = per_process_events(&schema_with_users(Some(0)), true);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    assert_eq!(events[0]["fields"]["queries"], "[\"users\"]");
}

#[test]
fn a_query_cached_with_a_ttl_says_nothing() {
    assert!(per_process_events(&schema_with_users(Some(60)), true).is_empty());
}

#[test]
fn a_query_reading_no_view_is_not_named() {
    let mut schema = schema_with_users(Some(0));
    schema.queries[0].sql_source = None;
    assert!(per_process_events(&schema, true).is_empty(), "the cache never stores it");
}

#[test]
fn a_server_hearing_the_others_names_no_query_cached_without_expiry() {
    use crate::server_config::CacheCrossInstance;
    let events = log_events(|| {
        super::warn_on_entries_no_other_server_evicts(
            &schema_with_users(Some(0)),
            true,
            CacheCrossInstance::Notify,
        );
    });
    assert!(events.is_empty(), "another server's write evicts here too: {events:?}");
}

#[test]
fn a_disabled_cache_names_no_query_cached_without_expiry() {
    assert!(per_process_events(&schema_with_users(Some(0)), false).is_empty());
}
