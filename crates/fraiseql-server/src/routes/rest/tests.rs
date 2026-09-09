//! Tests for top-level REST module utilities.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::json;

use super::{
    cache_control::{CacheContext, apply_cache_headers},
    sse::{
        accepts_sse, event_kind_to_sse_type, extract_last_event_id, extract_stream_resource,
        format_heartbeat, format_sse_event, is_stream_path, observers_not_available,
    },
};

// ---------------------------------------------------------------------------
// cache_control tests
// ---------------------------------------------------------------------------

#[test]
fn get_public_default_ttl() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
    assert_eq!(headers.get("vary").unwrap().to_str().unwrap(), "Authorization, Accept, Prefer");
}

#[test]
fn get_private_with_auth() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
}

#[test]
fn get_custom_ttl_from_query() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   Some(120),
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=120");
}

#[test]
fn mutation_no_store() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
    assert!(headers.get("vary").is_none());
}

#[test]
fn mutation_no_store_with_auth() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
}

#[test]
fn zero_ttl_disables_caching() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   Some(0),
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=0");
}

#[test]
fn s_maxage_on_public_get() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(
        headers.get("cache-control").unwrap().to_str().unwrap(),
        "public, max-age=60, s-maxage=300"
    );
}

#[test]
fn no_s_maxage_on_private_get() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
}

#[test]
fn no_s_maxage_when_none() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
}

#[test]
fn no_s_maxage_on_mutations() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
}

// ---------------------------------------------------------------------------
// sse tests
// ---------------------------------------------------------------------------

#[test]
fn accepts_sse_true_for_exact_match() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    assert!(accepts_sse(&headers));
}

#[test]
fn accepts_sse_true_in_list() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json, text/event-stream"));
    assert!(accepts_sse(&headers));
}

#[test]
fn accepts_sse_false_for_json() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!accepts_sse(&headers));
}

#[test]
fn accepts_sse_false_when_missing() {
    let headers = HeaderMap::new();
    assert!(!accepts_sse(&headers));
}

#[test]
fn is_stream_path_true() {
    assert!(is_stream_path("/users/stream"));
}

#[test]
fn is_stream_path_false_collection() {
    assert!(!is_stream_path("/users"));
}

#[test]
fn is_stream_path_false_single() {
    assert!(!is_stream_path("/users/123"));
}

#[test]
fn is_stream_path_false_nested() {
    assert!(!is_stream_path("/users/123/stream/extra"));
}

#[test]
fn extract_stream_resource_users() {
    assert_eq!(extract_stream_resource("/users/stream"), Some("users"));
}

#[test]
fn extract_stream_resource_orders() {
    assert_eq!(extract_stream_resource("/orders/stream"), Some("orders"));
}

#[test]
fn extract_stream_resource_none_for_collection() {
    assert_eq!(extract_stream_resource("/users"), None);
}

#[test]
fn extract_stream_resource_none_for_single() {
    assert_eq!(extract_stream_resource("/users/123"), None);
}

#[test]
fn extract_last_event_id_present() {
    let mut headers = HeaderMap::new();
    headers.insert("last-event-id", HeaderValue::from_static("evt-42"));
    assert_eq!(extract_last_event_id(&headers), Some("evt-42".to_string()));
}

#[test]
fn extract_last_event_id_missing() {
    let headers = HeaderMap::new();
    assert_eq!(extract_last_event_id(&headers), None);
}

#[test]
fn format_sse_insert_event() {
    let data = json!({"id": 1, "name": "Alice"});
    let output = format_sse_event("insert", "evt-1", &data);
    assert!(output.starts_with("event: insert\n"));
    assert!(output.contains("id: evt-1\n"));
    assert!(output.contains("data: "));
    assert!(output.ends_with("\n\n"));
    // Data line should be valid JSON
    let data_line = output.lines().find(|l| l.starts_with("data: ")).unwrap();
    let json_str = data_line.strip_prefix("data: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["name"], "Alice");
}

#[test]
fn format_sse_update_event() {
    let data = json!({"id": 1, "name": "Alice Updated"});
    let output = format_sse_event("update", "evt-2", &data);
    assert!(output.starts_with("event: update\n"));
}

#[test]
fn format_sse_delete_event() {
    let data = json!({"entity_id": "abc-123"});
    let output = format_sse_event("delete", "evt-3", &data);
    assert!(output.starts_with("event: delete\n"));
    assert!(output.contains("\"entity_id\""));
}

#[test]
fn format_heartbeat_event() {
    let output = format_heartbeat();
    assert!(output.starts_with("event: ping\n"));
    assert!(output.contains("data: \n"));
    assert!(output.ends_with("\n\n"));
}

#[test]
fn event_kind_insert() {
    assert_eq!(event_kind_to_sse_type("INSERT"), "insert");
}

#[test]
fn event_kind_update() {
    assert_eq!(event_kind_to_sse_type("UPDATE"), "update");
}

#[test]
fn event_kind_delete() {
    assert_eq!(event_kind_to_sse_type("DELETE"), "delete");
}

#[test]
fn event_kind_custom() {
    assert_eq!(event_kind_to_sse_type("CUSTOM"), "custom");
}

#[test]
fn event_kind_unknown() {
    assert_eq!(event_kind_to_sse_type("SOMETHING"), "unknown");
}

#[test]
fn observers_not_available_returns_501() {
    let err = observers_not_available();
    assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(err.code, "NOT_IMPLEMENTED");
}

// ---------------------------------------------------------------------------
// export_config tests (B1 Phase 1 Cycle 1)
// ---------------------------------------------------------------------------

mod export_config {
    use super::super::export_config::{ExportConfig, ExportFormat};

    #[test]
    fn default_values_match_spec() {
        let cfg = ExportConfig::default();
        assert_eq!(cfg.csv_delimiter, ',');
        assert!(cfg.csv_include_bom);
        assert_eq!(cfg.xlsx_max_rows, 100_000);
        assert_eq!(cfg.parquet_max_rows, 1_000_000);
        assert!(cfg.xlsx_temp_dir.is_none());
        assert_eq!(cfg.max_concurrent_xlsx, 10);
        // #917: the default is *all* formats, not the empty vector. Empty is documented
        // as "disables all exports", so defaulting to it — while nothing read the field
        // — meant that giving the kill-switch a consumer would have turned every export
        // off in every deployment that had not written the key.
        assert_eq!(
            cfg.export_formats,
            vec![ExportFormat::Csv, ExportFormat::Xlsx, ExportFormat::Parquet],
            "an unconfigured server must serve every export format"
        );
    }

    /// The kill-switch: an *explicitly* empty list disables everything.
    #[test]
    fn an_explicitly_empty_format_list_disables_every_export() {
        let cfg: ExportConfig = toml::from_str("export_formats = []").unwrap();
        assert!(!cfg.serves(ExportFormat::Csv));
        assert!(!cfg.serves(ExportFormat::Xlsx));
        assert!(!cfg.serves(ExportFormat::Parquet));
    }

    /// And a partial list disables exactly the formats it omits.
    #[test]
    fn a_partial_format_list_serves_only_what_it_names() {
        let cfg: ExportConfig = toml::from_str(r#"export_formats = ["csv"]"#).unwrap();
        assert!(cfg.serves(ExportFormat::Csv));
        assert!(!cfg.serves(ExportFormat::Xlsx));
        assert!(!cfg.serves(ExportFormat::Parquet));
    }

    /// An absent table serves everything — the same statement as the default test, made
    /// through the TOML path an operator actually exercises.
    #[test]
    fn an_absent_export_table_serves_every_format() {
        let cfg: ExportConfig = toml::from_str("").unwrap();
        assert!(cfg.serves(ExportFormat::Csv));
        assert!(cfg.serves(ExportFormat::Xlsx));
        assert!(cfg.serves(ExportFormat::Parquet));
    }

    #[test]
    fn deserializes_empty_toml_to_defaults() {
        let cfg: ExportConfig = toml::from_str("").unwrap();
        let default_cfg = ExportConfig::default();
        assert_eq!(cfg.csv_delimiter, default_cfg.csv_delimiter);
        assert_eq!(cfg.csv_include_bom, default_cfg.csv_include_bom);
        assert_eq!(cfg.xlsx_max_rows, default_cfg.xlsx_max_rows);
        assert_eq!(cfg.parquet_max_rows, default_cfg.parquet_max_rows);
        assert_eq!(cfg.xlsx_temp_dir, default_cfg.xlsx_temp_dir);
        assert_eq!(cfg.max_concurrent_xlsx, default_cfg.max_concurrent_xlsx);
        assert_eq!(cfg.export_formats, default_cfg.export_formats);
    }

    #[test]
    fn deserializes_full_toml_overrides_defaults() {
        let toml_src = r#"
            csv_delimiter = ";"
            csv_include_bom = false
            xlsx_max_rows = 50000
            parquet_max_rows = 250000
            xlsx_temp_dir = "/var/tmp/xlsx"
            max_concurrent_xlsx = 4
            export_formats = ["csv", "xlsx", "parquet"]
        "#;
        let cfg: ExportConfig = toml::from_str(toml_src).unwrap();
        assert_eq!(cfg.csv_delimiter, ';');
        assert!(!cfg.csv_include_bom);
        assert_eq!(cfg.xlsx_max_rows, 50_000);
        assert_eq!(cfg.parquet_max_rows, 250_000);
        assert_eq!(cfg.xlsx_temp_dir.as_deref(), Some(std::path::Path::new("/var/tmp/xlsx")));
        assert_eq!(cfg.max_concurrent_xlsx, 4);
        assert_eq!(
            cfg.export_formats,
            vec![ExportFormat::Csv, ExportFormat::Xlsx, ExportFormat::Parquet],
        );
    }

    #[test]
    fn export_format_deserializes_lowercase_kebab_strings() {
        // sanity: serde renames must accept the lowercase TOML values used by users
        let cfg: ExportConfig =
            toml::from_str(r#"export_formats = ["csv", "xlsx", "parquet"]"#).unwrap();
        assert_eq!(
            cfg.export_formats,
            vec![ExportFormat::Csv, ExportFormat::Xlsx, ExportFormat::Parquet],
        );
    }

    #[test]
    fn export_format_rejects_unknown_variant() {
        let result: Result<ExportConfig, _> = toml::from_str(r#"export_formats = ["yaml"]"#);
        assert!(result.is_err(), "unknown export format should fail to deserialize");
    }
}

// ---------------------------------------------------------------------------
// #1113 — the live-event branch's decisions
//
// The branch these serve is unreachable (`RestState.event_transport` is `None` at
// its only construction site, #1309), which is exactly why its decisions live in
// functions: an unreachable line cannot be tested, an extracted decision can.
// ---------------------------------------------------------------------------

#[cfg(feature = "observers")]
mod stream_decisions {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use fraiseql_core::security::SecurityContext;
    use fraiseql_observers::{
        event::{EntityEvent, EventKind},
        transport::TenantScope,
    };

    use crate::routes::rest::sse::{StreamEvent, stream_resume_refusal, stream_tenant_scope};

    /// The context a REST request actually arrives with: `SecurityContext::from_user`
    /// on the authenticated subject, optionally carrying a tenant — the shape
    /// `crate::extractors` builds.
    fn principal(tenant: Option<&str>) -> SecurityContext {
        let user = fraiseql_core::security::auth_middleware::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new("user-1"),
            scopes:       vec!["user".to_string()],
            expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: std::collections::HashMap::new(),
        };
        let ctx = SecurityContext::from_user(&user, "req-1113".to_string());
        match tenant {
            Some(t) => ctx.with_tenant(t.to_string()),
            None => ctx,
        }
    }

    // ── tenant scope ──────────────────────────────────────────────

    /// The defect: the subscription carried no tenant, and `tenant_id: None` means
    /// *every tenant*.
    #[test]
    fn a_multi_tenant_principal_scopes_the_subscription_to_its_own_tenant() {
        let ctx = principal(Some("tenant-a"));
        assert_eq!(
            stream_tenant_scope(Some(&ctx), true).expect("a tenanted principal is servable"),
            TenantScope::Tenant("tenant-a".to_string())
        );
    }

    /// Fail-closed. There is no tenant to scope by, and the deployment has said that
    /// matters.
    #[test]
    fn a_multi_tenant_deployment_refuses_a_principal_with_no_tenant() {
        let ctx = principal(None);
        let refusal =
            stream_tenant_scope(Some(&ctx), true).expect_err("an untenanted principal is refused");
        assert_eq!(refusal.status, StatusCode::FORBIDDEN);
        assert_eq!(refusal.code, "TENANT_SCOPE_REQUIRED");
    }

    /// `require_auth = false` leaves no principal at all. An absent principal carries
    /// no tenant, so it takes the same answer — not the unscoped stream that an
    /// `Option`-shaped rule would fall through to.
    #[test]
    fn a_multi_tenant_deployment_refuses_a_request_with_no_principal() {
        let refusal = stream_tenant_scope(None, true).expect_err("an anonymous request is refused");
        assert_eq!(refusal.status, StatusCode::FORBIDDEN);
        assert_eq!(refusal.code, "TENANT_SCOPE_REQUIRED");
    }

    /// Single-tenant stays permissive: tenant ids are typically absent throughout such
    /// a deployment, so scoping by an absent tenant would match nothing and the stream
    /// would open and stay silent. Same arm as `SubscriptionManager`'s tenant gate.
    #[test]
    fn a_single_tenant_deployment_subscribes_unscoped() {
        assert_eq!(
            stream_tenant_scope(Some(&principal(None)), false).expect("servable"),
            TenantScope::AllTenants
        );
        assert_eq!(stream_tenant_scope(None, false).expect("servable"), TenantScope::AllTenants);
    }

    /// A tenanted principal in a single-tenant deployment is still unscoped — the
    /// deployment's declaration decides, not the credential. Pins that the two inputs
    /// are not silently the same input.
    #[test]
    fn the_deployments_declaration_decides_not_the_credential() {
        assert_eq!(
            stream_tenant_scope(Some(&principal(Some("tenant-a"))), false).expect("servable"),
            TenantScope::AllTenants
        );
    }

    // ── Last-Event-ID ─────────────────────────────────────────────

    fn headers_with(last_event_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("last-event-id", HeaderValue::from_str(last_event_id).unwrap());
        headers
    }

    /// The defect: read into `_last_event_id` and dropped, so a reconnect silently
    /// skipped the gap on a stream that reported itself healthy.
    #[test]
    fn a_resume_request_is_refused_rather_than_ignored() {
        let refusal =
            stream_resume_refusal(&headers_with("41")).expect("a resume request is refused");
        assert_eq!(refusal.status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(refusal.code, "RESUMPTION_UNSUPPORTED");
        assert!(
            refusal.message.contains("41"),
            "the refusal must name the id it could not honour: {}",
            refusal.message
        );
    }

    #[test]
    fn a_fresh_delivery_is_not_a_resume_request() {
        assert!(stream_resume_refusal(&HeaderMap::new()).is_none());
    }

    /// `Last-Event-ID:` with an empty value is what a client sends before it has seen
    /// an event. Nothing was missed, so there is nothing to refuse.
    #[test]
    fn an_empty_last_event_id_is_not_a_resume_request() {
        assert!(stream_resume_refusal(&headers_with("")).is_none());
        assert!(stream_resume_refusal(&headers_with("   ")).is_none());
    }

    // ── the wire frame ────────────────────────────────────────────

    fn event(kind: EventKind, seq: Option<i64>) -> EntityEvent {
        let mut e = EntityEvent::new(
            kind,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            serde_json::json!({"total": 100}),
        );
        e.seq = seq;
        e
    }

    /// The defect: the id was `entity_event.id`, a UUID — an id no ordering can
    /// resolve to a resume point, so the stream advertised a resumability nothing
    /// could ever provide.
    #[test]
    fn the_wire_id_is_the_change_spine_sequence_not_the_event_uuid() {
        let e = event(EventKind::Created, Some(4_120));
        let wire = StreamEvent::from_entity_event(&e);
        assert_eq!(wire.id.as_deref(), Some("4120"));
        assert_ne!(
            wire.id.as_deref(),
            Some(e.id.to_string().as_str()),
            "the event UUID must not be the resume id"
        );
    }

    /// An event with no sequence carries **no `id:` field**. Per the SSE spec that
    /// leaves the client's last-event-id buffer unchanged, so a reconnect still names
    /// the last event that had one: at-least-once, never a skip. Emitting the UUID
    /// here would poison the buffer with a value no replay can resolve.
    #[test]
    fn an_event_with_no_sequence_carries_no_wire_id() {
        let e = event(EventKind::Created, None);
        assert_eq!(StreamEvent::from_entity_event(&e).id, None);
    }

    #[test]
    fn the_wire_event_type_follows_the_event_kind() {
        for (kind, expected) in [
            (EventKind::Created, "insert"),
            (EventKind::Updated, "update"),
            (EventKind::Deleted, "delete"),
            (EventKind::Custom, "custom"),
        ] {
            let e = event(kind, Some(1));
            assert_eq!(StreamEvent::from_entity_event(&e).event_type, expected, "{kind:?}");
        }
    }

    #[test]
    fn the_wire_payload_is_the_events_data() {
        let e = event(EventKind::Updated, Some(7));
        assert_eq!(StreamEvent::from_entity_event(&e).data, &serde_json::json!({"total": 100}));
    }
}
