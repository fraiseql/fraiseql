// Note: the #421 page-size precedence logic (`page_size_precedence`) and its unit
// tests moved to `fraiseql_core::runtime` alongside `RuntimeConfig::from_compiled_schema`,
// the single seam every server constructor now routes through (H16).

// ── executor_gate_config_tests: #379 — runtime [validation] merges into the
//    executor gate (runtime TOML > compiled schema, per field) ───────────────

mod executor_gate_config_tests {
    use fraiseql_core::schema::{CompiledSchema, ValidationConfig};

    use super::super::initialization::executor_runtime_config;
    use crate::server_config::ServerConfig;

    fn compiled_with(depth: Option<u32>, complexity: Option<u32>) -> CompiledSchema {
        CompiledSchema {
            validation_config: Some(ValidationConfig {
                max_query_depth:      depth,
                max_query_complexity: complexity,
                max_page_size:        None,
            }),
            ..CompiledSchema::default()
        }
    }

    /// With no runtime `[validation]` override, the helper leaves
    /// `query_validation` unset so the executor derives the gate from the
    /// compiled schema's own declared limits.
    #[test]
    fn no_runtime_override_leaves_derivation_to_the_executor() {
        let schema = compiled_with(Some(8), Some(100));
        let config = ServerConfig::default();
        let rt = executor_runtime_config(&schema, &config).expect("valid schema");
        assert!(
            rt.query_validation.is_none(),
            "without a runtime override the executor derives from the compiled schema"
        );
    }

    /// A runtime `[validation]` override merges per field: the runtime value
    /// wins where set, the compiled declared value fills the rest. This is the
    /// documented precedence (runtime TOML > compiled schema) — without the
    /// merge, a runtime override that loosens a compiled limit would be
    /// silently negated by the executor gate.
    #[test]
    fn runtime_override_merges_per_field_over_compiled() {
        let schema = compiled_with(Some(8), Some(100));
        let config = ServerConfig {
            validation: Some(ValidationConfig {
                max_query_depth:      None,
                max_query_complexity: Some(500),
                max_page_size:        None,
            }),
            ..ServerConfig::default()
        };
        let rt = executor_runtime_config(&schema, &config).expect("valid schema");
        let gate = rt.query_validation.expect("a runtime override must install the gate");
        assert_eq!(gate.max_complexity, 500, "runtime value wins for complexity");
        assert_eq!(gate.max_depth, 8, "compiled declared value fills undeclared depth");
    }

    /// The merged gate actually binds at the executor: a query passing the
    /// loosened runtime complexity limit executes (reaches the adapter), and a
    /// too-deep query is refused by the gate before any database access.
    #[tokio::test]
    async fn merged_gate_binds_at_the_executor() {
        use std::sync::Arc;

        use fraiseql_core::runtime::Executor;
        use fraiseql_test_utils::failing_adapter::FailingAdapter;

        let schema = compiled_with(Some(3), Some(100));
        let config = ServerConfig {
            validation: Some(ValidationConfig {
                max_query_depth:      None,
                max_query_complexity: Some(500),
                max_page_size:        None,
            }),
            ..ServerConfig::default()
        };
        let rt = executor_runtime_config(&schema, &config).expect("valid schema");
        let executor = Executor::with_config(schema, Arc::new(FailingAdapter::new()), rt);

        // Complexity ~301 (1 + 3×100): over the compiled 100, under the runtime
        // 500 — must pass the gate and fail *downstream* (the schema declares
        // no queries), proving the loosened limit is the one enforced.
        let loosened = "{ users(limit: 100) { id name email } }";
        let err = executor
            .execute(loosened, None)
            .await
            .expect_err("the empty schema cannot match any query once the gate admits it");
        assert!(
            !err.to_string().to_lowercase().contains("complex"),
            "a complexity-500-limit gate must not reject cost 301: {err}"
        );

        // Depth 5 against the compiled depth 3 (runtime silent on depth):
        // refused by the gate, never reaching the adapter.
        let deep = "{ users { a { b { c { d } } } } }";
        let err = executor
            .execute(deep, None)
            .await
            .expect_err("depth 5 must be refused by the compiled max_query_depth=3");
        assert!(
            err.to_string().to_lowercase().contains("deep"),
            "rejection must come from the depth gate, not the adapter: {err}"
        );
    }
}

// ── initialization_tests ──────────────────────────────────────────────────────

mod initialization_tests {
    use super::super::initialization::is_manifest_url_ssrf_blocked;

    /// H12: a field marked for at-rest encryption must refuse to boot — the write path does
    /// not encrypt, so it would be stored in plaintext.
    #[test]
    fn field_encryption_marker_refuses_boot() {
        use fraiseql_core::schema::{
            CompiledSchema, FieldDefinition, FieldEncryptionConfig, FieldType, TypeDefinition,
        };

        use super::super::initialization::field_encryption_unsupported_check;

        let mut user = TypeDefinition::new("User", "v_user");
        user.fields
            .push(FieldDefinition::new("email", FieldType::String).with_encryption(
                FieldEncryptionConfig {
                    key_reference: "keys/user-email".to_string(),
                    algorithm:     "AES-256-GCM".to_string(),
                },
            ));
        let schema = CompiledSchema {
            types: vec![user],
            ..CompiledSchema::default()
        };

        let result = field_encryption_unsupported_check(&schema);
        assert!(
            matches!(&result, Err(crate::ServerError::ConfigError(msg)) if msg.contains("User.email")),
            "a field marked for encryption must refuse to boot and name the field (H12): {result:?}"
        );
    }

    #[test]
    fn no_field_encryption_boots_fine() {
        use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};

        use super::super::initialization::field_encryption_unsupported_check;

        let mut user = TypeDefinition::new("User", "v_user");
        user.fields.push(FieldDefinition::new("email", FieldType::String));
        let schema = CompiledSchema {
            types: vec![user],
            ..CompiledSchema::default()
        };
        assert!(
            field_encryption_unsupported_check(&schema).is_ok(),
            "a schema with no encryption-marked fields boots normally"
        );
    }

    /// #379: `[security] persisted_queries_only = true` forces the trusted-document
    /// store into Strict mode (reject any non-persisted operation), regardless of the
    /// declared `[security.trusted_documents].mode`. Without the flag, the declared
    /// mode is honored.
    #[test]
    fn persisted_queries_only_forces_strict_mode() {
        use super::super::initialization::effective_trusted_doc_mode;
        use crate::trusted_documents::TrustedDocumentMode;

        // The flag forces Strict even when the declared mode is permissive.
        assert_eq!(
            effective_trusted_doc_mode(
                fraiseql_core::schema::TrustedDocumentMode::Permissive,
                true
            ),
            TrustedDocumentMode::Strict,
            "persisted_queries_only=true must force Strict over a permissive declared mode"
        );
        assert_eq!(
            effective_trusted_doc_mode(fraiseql_core::schema::TrustedDocumentMode::Strict, true),
            TrustedDocumentMode::Strict
        );

        // Without the flag, the declared mode is honored.
        assert_eq!(
            effective_trusted_doc_mode(fraiseql_core::schema::TrustedDocumentMode::Strict, false),
            TrustedDocumentMode::Strict
        );
        assert_eq!(
            effective_trusted_doc_mode(
                fraiseql_core::schema::TrustedDocumentMode::Permissive,
                false
            ),
            TrustedDocumentMode::Permissive,
            "without the flag, a permissive schema stays permissive"
        );

        // An unknown/empty declared mode defaults to Permissive unless the flag forces Strict.
        // The typed mode's default is Permissive — the old "" (unset string) case
        // is unrepresentable since #977.
        assert_eq!(
            effective_trusted_doc_mode(
                fraiseql_core::schema::TrustedDocumentMode::default(),
                false
            ),
            TrustedDocumentMode::Permissive
        );
        assert_eq!(
            effective_trusted_doc_mode(fraiseql_core::schema::TrustedDocumentMode::default(), true),
            TrustedDocumentMode::Strict
        );
    }

    #[test]
    fn ssrf_blocks_localhost_by_name() {
        assert!(is_manifest_url_ssrf_blocked("http://localhost/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_localhost_uppercase() {
        assert!(is_manifest_url_ssrf_blocked("http://LOCALHOST/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_loopback() {
        assert!(is_manifest_url_ssrf_blocked("http://127.0.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_192_168() {
        assert!(is_manifest_url_ssrf_blocked("http://192.168.1.100/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_10_x() {
        assert!(is_manifest_url_ssrf_blocked("http://10.0.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_172_16() {
        assert!(is_manifest_url_ssrf_blocked("http://172.16.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_link_local() {
        assert!(is_manifest_url_ssrf_blocked("http://169.254.1.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_loopback() {
        assert!(is_manifest_url_ssrf_blocked("http://[::1]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_unspecified() {
        assert!(is_manifest_url_ssrf_blocked("http://[::]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_ula() {
        // fc00::/7 range
        assert!(is_manifest_url_ssrf_blocked("http://[fd00::1]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_unparseable_url() {
        assert!(is_manifest_url_ssrf_blocked("not a url at all"));
    }

    #[test]
    fn ssrf_allows_public_https() {
        assert!(!is_manifest_url_ssrf_blocked("https://cdn.example.com/manifest.json"));
    }

    #[test]
    fn ssrf_allows_public_ipv4() {
        // 93.184.216.34 is example.com — a real public address
        assert!(!is_manifest_url_ssrf_blocked("http://93.184.216.34/manifest.json"));
    }

    #[test]
    fn ssrf_allows_public_ipv6_global() {
        // Google public DNS — a real globally-routable address. Not 2001:db8::, which
        // this test used to treat as "public": it is the RFC 3849 documentation range
        // and the shared guard refuses it along with the IPv4 TEST-NETs.
        assert!(!is_manifest_url_ssrf_blocked("http://[2606:4700:4700::1111]/manifest.json"));
    }

    #[test]
    fn manifest_guard_refuses_every_blocked_corpus_entry() {
        use fraiseql_guard::net::vectors::{MUST_BLOCK, MUST_BLOCK_HOSTS, url_host};
        for (addr, why) in MUST_BLOCK {
            let url = format!("http://{}/manifest.json", url_host(addr));
            assert!(is_manifest_url_ssrf_blocked(&url), "must refuse {addr} ({why})");
        }
        for (host, why) in MUST_BLOCK_HOSTS {
            let url = format!("http://{host}/manifest.json");
            assert!(is_manifest_url_ssrf_blocked(&url), "must refuse {host} ({why})");
        }
    }

    #[test]
    fn manifest_guard_permits_every_allowed_corpus_entry() {
        use fraiseql_guard::net::vectors::{MUST_ALLOW, url_host};
        for addr in MUST_ALLOW {
            let url = format!("http://{}/manifest.json", url_host(addr));
            assert!(!is_manifest_url_ssrf_blocked(&url), "must permit {addr}");
        }
    }

    #[test]
    fn ssrf_blocks_mapped_metadata_literal() {
        // The manifest guard had drifted from the federation and Vault guards it claimed
        // to mirror: it missed every IPv4-mapped form, so this reached the metadata
        // service on a dual-stack host.
        assert!(is_manifest_url_ssrf_blocked("http://[::ffff:169.254.169.254]/manifest.json"));
        assert!(is_manifest_url_ssrf_blocked("http://[64:ff9b::169.254.169.254]/manifest.json"));
        assert!(is_manifest_url_ssrf_blocked("http://100.100.100.200/manifest.json"));
        assert!(is_manifest_url_ssrf_blocked("http://0.0.0.0/manifest.json"));
    }

    // #360: PKCE must not be served without [security.state_encryption] in production.
    #[cfg(feature = "auth")]
    #[test]
    fn pkce_without_state_encryption_is_fatal_in_production() {
        use super::super::initialization::pkce_state_encryption_check;
        let result = pkce_state_encryption_check(
            // has_state_encryption
            false, // is_production
            true,
        );
        assert!(
            result.is_err(),
            "PKCE without state encryption must refuse to boot in production (#360)"
        );
    }

    #[cfg(feature = "auth")]
    #[test]
    fn pkce_without_state_encryption_is_a_warning_in_development() {
        use super::super::initialization::pkce_state_encryption_check;
        assert!(
            pkce_state_encryption_check(false, false).is_ok(),
            "development mode downgrades the missing-state-encryption error to a warning"
        );
    }

    #[cfg(feature = "auth")]
    #[test]
    fn pkce_with_state_encryption_is_always_ok() {
        use super::super::initialization::pkce_state_encryption_check;
        assert!(pkce_state_encryption_check(true, true).is_ok());
        assert!(pkce_state_encryption_check(true, false).is_ok());
    }

    // H7: error sanitization defaults to ON in production when the schema declares
    // no explicit `error_sanitization` config, and OFF in development. An explicit
    // compiled config overrides either way.
    #[test]
    fn error_sanitizer_secure_default_is_environment_aware() {
        use super::super::initialization::build_error_sanitizer;

        // No explicit config: production sanitizes, development stays verbose.
        assert!(
            build_error_sanitizer(None, true).is_enabled(),
            "production must sanitize 5xx by default (H7)"
        );
        assert!(
            !build_error_sanitizer(None, false).is_enabled(),
            "development keeps verbose errors by default"
        );
    }

    #[test]
    fn explicit_error_sanitization_config_overrides_environment_default() {
        use super::super::initialization::build_error_sanitizer;
        use crate::config::error_sanitization::ErrorSanitizationConfig;

        // Operator explicitly disables in production → respected (not forced on).
        let off = ErrorSanitizationConfig {
            enabled: false,
            ..ErrorSanitizationConfig::default()
        };
        assert!(!build_error_sanitizer(Some(off), true).is_enabled());

        // Operator explicitly enables in development → respected.
        let on = ErrorSanitizationConfig {
            enabled: true,
            ..ErrorSanitizationConfig::default()
        };
        assert!(build_error_sanitizer(Some(on), false).is_enabled());
    }

    // #356: the binary cannot enforce failed_login_* lockout (no first-factor login).
    use super::super::initialization::failed_login_lockout_check;
    use crate::middleware::rate_limit::{
        DEFAULT_FAILED_LOGIN_LOCKOUT_SECS, DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS,
    };

    #[test]
    fn failed_login_default_values_boot_silently_even_in_production() {
        // Defaults ride along with any [security.rate_limiting] section and signal no
        // intent, so they must never block startup.
        assert!(
            failed_login_lockout_check(
                DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS,
                DEFAULT_FAILED_LOGIN_LOCKOUT_SECS,
                true,
            )
            .is_ok()
        );
    }

    #[test]
    fn failed_login_tuned_value_is_fatal_in_production() {
        // A tuned max_attempts expects a control the binary cannot provide.
        assert!(failed_login_lockout_check(5, DEFAULT_FAILED_LOGIN_LOCKOUT_SECS, true).is_err());
        // A tuned lockout window is equally fatal.
        assert!(failed_login_lockout_check(DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS, 60, true).is_err());
    }

    #[test]
    fn failed_login_tuned_value_is_a_warning_in_development() {
        assert!(failed_login_lockout_check(5, 60, false).is_ok());
    }

    // #618: trusting X-Forwarded-For from all proxies (empty CIDR list) now REFUSES to
    // boot in production and downgrades to a warning in development — the 2.13 deprecation
    // (#609) promised this. An explicit ["0.0.0.0/0"] opt-in is safe (non-empty list).
    use super::super::initialization::proxy_trust_check;

    #[test]
    fn proxy_trust_all_by_omission_refuses_boot_in_production() {
        // trust + no CIDRs (None) → refuse in production, with an actionable message.
        let err = proxy_trust_check(true, None, true).expect_err("must refuse to boot");
        let msg = format!("{err}");
        assert!(msg.contains("trusted_proxy_cidrs"), "names the fix: {msg}");
        assert!(msg.contains("0.0.0.0/0"), "names the explicit trust-all opt-in: {msg}");
    }

    #[test]
    fn proxy_trust_empty_list_refuses_boot_in_production() {
        // An explicitly empty list is the same permissive posture as omission.
        assert!(proxy_trust_check(true, Some(&[]), true).is_err());
    }

    #[test]
    fn proxy_trust_all_by_omission_is_a_warning_in_development() {
        // Development downgrades to a warning (boot proceeds), matching failed_login.
        assert!(proxy_trust_check(true, None, false).is_ok());
        assert!(proxy_trust_check(true, Some(&[]), false).is_ok());
    }

    #[test]
    fn proxy_trust_explicit_trust_all_is_ok_even_in_production() {
        // ["0.0.0.0/0"] is the sanctioned explicit opt-in — a deliberate, non-empty list,
        // so it neither errors nor warns, in production or development.
        let cidrs = vec!["0.0.0.0/0".to_string()];
        assert!(proxy_trust_check(true, Some(&cidrs), true).is_ok());
        assert!(proxy_trust_check(true, Some(&cidrs), false).is_ok());
    }

    #[test]
    fn proxy_trust_restricted_cidrs_are_ok_in_production() {
        let cidrs = vec!["10.0.0.0/8".to_string()];
        assert!(proxy_trust_check(true, Some(&cidrs), true).is_ok());
    }

    #[test]
    fn proxy_trust_disabled_is_ok() {
        // trust_proxy_headers = false → the CIDR list is irrelevant; never errors/warns.
        assert!(proxy_trust_check(false, None, true).is_ok());
        assert!(proxy_trust_check(false, None, false).is_ok());
    }

    // #350: a configured non-Postgres observer transport that cannot run must fail
    // loud (refuse boot in production), never silently fall back to PostgreSQL.
    #[cfg(feature = "observers")]
    mod observer_transport {
        use fraiseql_observers::config::TransportKind;

        use crate::server::initialization::observer_transport_check;

        #[test]
        fn postgres_is_always_ok() {
            // The default transport needs no broker and never blocks boot.
            assert!(observer_transport_check(TransportKind::Postgres, false, false, true).is_ok());
            assert!(observer_transport_check(TransportKind::Postgres, false, false, false).is_ok());
        }

        #[test]
        fn in_memory_is_always_ok() {
            // The in-memory transport is always compiled and needs no broker.
            assert!(observer_transport_check(TransportKind::InMemory, false, false, true).is_ok());
        }

        #[test]
        fn nats_not_compiled_in_is_fatal_in_production() {
            // transport = "nats" without the observers-nats feature cannot run.
            assert!(observer_transport_check(TransportKind::Nats, false, true, true).is_err());
        }

        #[test]
        fn nats_not_compiled_in_is_a_warning_in_development() {
            assert!(observer_transport_check(TransportKind::Nats, false, true, false).is_ok());
        }

        #[test]
        fn nats_without_url_is_fatal_in_production() {
            assert!(observer_transport_check(TransportKind::Nats, true, false, true).is_err());
        }

        #[test]
        fn nats_without_url_is_a_warning_in_development() {
            assert!(observer_transport_check(TransportKind::Nats, true, false, false).is_ok());
        }

        #[test]
        fn nats_compiled_with_url_is_ok() {
            assert!(observer_transport_check(TransportKind::Nats, true, true, true).is_ok());
        }
    }
}

// ── lifecycle_tests ───────────────────────────────────────────────────────────
//
// Drain semantics for the per-server lifecycle [`JoinSet`] introduced by F021.
// Replaces the previous fire-and-forget `tokio::spawn` calls. A drain after a
// graceful shutdown must abort and await every long-running lifecycle task so
// no background work survives the server's `serve_with_shutdown` return.

#[cfg(test)]
mod lifecycle_tests {
    use std::time::Duration;

    use super::super::lifecycle::drain_lifecycle_tasks;

    #[tokio::test]
    async fn drain_lifecycle_tasks_aborts_infinite_loops() {
        let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

        // Spawn three infinite loops — the exact pattern used by PKCE cleanup,
        // SIGUSR1 reload, and usage flush in lifecycle.rs. None of them would
        // ever return on their own.
        for _ in 0..3 {
            tasks.spawn(async {
                let mut ticker = tokio::time::interval(Duration::from_mins(1));
                loop {
                    ticker.tick().await;
                }
            });
        }

        // The drain helper must abort all three under the configured timeout.
        let drain =
            tokio::time::timeout(Duration::from_secs(5), drain_lifecycle_tasks(tasks, 5)).await;
        assert!(
            drain.is_ok(),
            "drain_lifecycle_tasks must abort infinite-loop tasks within the timeout"
        );
    }

    #[tokio::test]
    async fn drain_lifecycle_tasks_returns_quickly_for_empty_set() {
        // No tasks → drain returns immediately, well under the timeout.
        let tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        let drain =
            tokio::time::timeout(Duration::from_secs(1), drain_lifecycle_tasks(tasks, 5)).await;
        assert!(drain.is_ok(), "drain on an empty JoinSet must be a no-op");
    }
}

// ── rate_limit_boot_guard_tests ───────────────────────────────────────────────

/// The `#618` proxy-trust boot guard must be unreachable-around, and the documented
/// CLI > env > compiled-config precedence must hold (#778, #837, #774).
///
/// Three defects met in the rate-limiter construction path, none of them visible
/// from a successful boot:
///
/// * `#778` — the compiled `security.rate_limiting` was deserialized with `.ok()`, so one
///   wrong-typed field made the whole section vanish. `None` is indistinguishable from "not
///   configured", so rate limiting silently turned off *and* the boot guards sitting behind that
///   parse never ran.
/// * `#837` — those guards lived in the compiled-schema branch only, so the same configuration
///   expressed as `[rate_limiting]` in `fraiseql.toml` reached the limiter ungated:
///   `trust_proxy_headers = true` with no trusted CIDRs booted happily in production and honoured
///   `X-Real-IP` from every peer.
/// * `#774` — the compiled schema won unconditionally, so `FRAISEQL_RATE_LIMITING_ENABLED=false`
///   and the three numeric overrides were discarded without a word, inverting the precedence the
///   CLI documents.
///
/// A fourth was found while fixing them and is fixed here too: `trusted_proxy_cidrs`
/// entries that failed to parse were dropped with a warning *after* the guard had
/// inspected the string list, so `["10.0.0.0/8typo"]` passed the non-empty check and
/// yielded an empty trust list — the trust-everyone posture the guard exists to refuse.
mod rate_limit_boot_guard_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use fraiseql_core::schema::CompiledSchema;
    use serde_json::json;

    use super::super::initialization::resolve_rate_limiter_in;
    use crate::{
        ServerConfig,
        middleware::{RateLimitConfig, RateLimitOverrides},
    };

    const PRODUCTION: bool = true;
    const DEVELOPMENT: bool = false;

    fn schema_with_rate_limiting(section: serde_json::Value) -> CompiledSchema {
        // Null spells "absent", as the compiler serialises an unset section; a
        // non-null section must parse as the typed config — malformed sections
        // are refused at schema load since #977 (see
        // `a_malformed_compiled_section_is_refused_at_load`).
        let security = fraiseql_core::schema::SecurityConfig {
            rate_limiting: if section.is_null() {
                None
            } else {
                Some(serde_json::from_value(section).expect("valid rate_limiting JSON"))
            },
            ..fraiseql_core::schema::SecurityConfig::default()
        };
        CompiledSchema {
            security: Some(security),
            ..CompiledSchema::default()
        }
    }

    fn bare_schema() -> CompiledSchema {
        CompiledSchema::default()
    }

    /// A schema section that trusts proxy headers without naming any trusted proxy.
    fn trust_all_by_omission() -> serde_json::Value {
        json!({
            "enabled": true,
            "requests_per_second": 100,
            "burst_size": 50,
            "trust_proxy_headers": true,
            "trusted_proxy_cidrs": [],
        })
    }

    // ── #778: a malformed section refuses to boot ────────────────────────────
    //
    // Since #977 the refusal happens at schema **load**: `rate_limiting` is a
    // typed `deny_unknown_fields` field on `SecurityConfig`, so a malformed
    // section can never reach `resolve_rate_limiter_in` at all.

    #[test]
    fn a_malformed_compiled_section_is_refused_at_load() {
        // `requests_per_second` as a string is what an external generator or a hand
        // edit produces, and is exactly what `.ok()` used to swallow whole.
        let json = serde_json::json!({
            "types": [], "queries": [], "mutations": [], "subscriptions": [],
            "security": {
                "rate_limiting": {
                    "enabled": true,
                    "requests_per_second": "100",
                    "trust_proxy_headers": true,
                }
            }
        });
        let err = CompiledSchema::from_json(&json.to_string(), false)
            .expect_err("a present-but-unparseable [security.rate_limiting] must refuse to load");
        assert!(
            err.to_string().contains("security.rate_limiting"),
            "the error must name the section so an operator can find it; got: {err}"
        );
    }

    #[tokio::test]
    async fn an_absent_section_is_not_an_error() {
        let limiter = resolve_rate_limiter_in(&bare_schema(), &ServerConfig::default(), PRODUCTION)
            .await
            .expect("no rate-limit configuration anywhere is a valid deployment");
        assert!(limiter.is_none());
    }

    #[tokio::test]
    async fn an_explicit_json_null_section_is_not_an_error() {
        // The compiler serialises an absent `SecurityConfig.rate_limiting` as null.
        let schema = schema_with_rate_limiting(serde_json::Value::Null);
        let limiter = resolve_rate_limiter_in(&schema, &ServerConfig::default(), PRODUCTION)
            .await
            .expect("a null section means absent, not malformed");
        assert!(limiter.is_none());
    }

    // ── #837: the guard runs whichever source configures the limiter ─────────

    #[tokio::test]
    async fn the_proxy_trust_guard_runs_when_the_compiled_schema_configures_it() {
        let schema = schema_with_rate_limiting(trust_all_by_omission());
        assert!(
            resolve_rate_limiter_in(&schema, &ServerConfig::default(), PRODUCTION)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn the_proxy_trust_guard_runs_when_the_server_table_configures_it() {
        // The #837 branch: `[rate_limiting]` in the server's own fraiseql.toml built
        // a `RateLimiter` directly and never reached a guard.
        let config = ServerConfig {
            rate_limiting: Some(RateLimitConfig {
                enabled: true,
                trust_proxy_headers: true,
                trusted_proxy_cidrs: vec![],
                ..RateLimitConfig::default()
            }),
            ..ServerConfig::default()
        };

        assert!(
            resolve_rate_limiter_in(&bare_schema(), &config, PRODUCTION).await.is_err(),
            "trust_proxy_headers = true with no trusted CIDRs must refuse to boot in production \
             regardless of which source declares it (#837)"
        );
    }

    #[tokio::test]
    async fn the_proxy_trust_guard_runs_after_overrides_are_applied() {
        // An override can switch the limiter on over a base that was disabled, so the
        // guard has to run on the merged result, not only on the base.
        let config = ServerConfig {
            rate_limiting: Some(RateLimitConfig {
                enabled: false,
                trust_proxy_headers: true,
                trusted_proxy_cidrs: vec![],
                ..RateLimitConfig::default()
            }),
            rate_limit_overrides: RateLimitOverrides {
                enabled: Some(true),
                ..RateLimitOverrides::default()
            },
            ..ServerConfig::default()
        };

        assert!(resolve_rate_limiter_in(&bare_schema(), &config, PRODUCTION).await.is_err());
    }

    #[tokio::test]
    async fn the_proxy_trust_guard_is_a_warning_in_development() {
        let schema = schema_with_rate_limiting(trust_all_by_omission());
        assert!(
            resolve_rate_limiter_in(&schema, &ServerConfig::default(), DEVELOPMENT)
                .await
                .is_ok(),
            "development downgrades the guard to a warning"
        );
    }

    #[tokio::test]
    async fn an_unparseable_trusted_proxy_cidr_refuses_to_boot() {
        // Non-empty as a string list, so the guard's own check passes; empty once
        // parsed, which is the trust-everyone posture. Skipping it with a warning made
        // the guard inspect a list the middleware would never see.
        let schema = schema_with_rate_limiting(json!({
            "enabled": true,
            "requests_per_second": 100,
            "burst_size": 50,
            "trust_proxy_headers": true,
            "trusted_proxy_cidrs": ["10.0.0.0/8", "not-a-cidr"],
        }));

        let err = resolve_rate_limiter_in(&schema, &ServerConfig::default(), PRODUCTION)
            .await
            .err()
            .expect("an entry that is not a CIDR must refuse to boot, not be skipped");
        assert!(
            err.to_string().contains("not-a-cidr"),
            "the error must name the offending entry; got: {err}"
        );
    }

    #[tokio::test]
    async fn a_restricted_proxy_list_boots() {
        let schema = schema_with_rate_limiting(json!({
            "enabled": true,
            "requests_per_second": 100,
            "burst_size": 50,
            "trust_proxy_headers": true,
            "trusted_proxy_cidrs": ["10.0.0.0/8"],
        }));

        let limiter = resolve_rate_limiter_in(&schema, &ServerConfig::default(), PRODUCTION)
            .await
            .expect("a restricted trusted-proxy list is the sanctioned configuration");
        assert!(limiter.is_some());
    }

    // ── #774: CLI/env overrides outrank the compiled schema ──────────────────

    #[tokio::test]
    async fn an_env_override_can_disable_compiled_schema_rate_limiting() {
        let schema = schema_with_rate_limiting(json!({
            "enabled": true, "requests_per_second": 1, "burst_size": 1,
        }));
        let config = ServerConfig {
            rate_limit_overrides: RateLimitOverrides {
                enabled: Some(false),
                ..RateLimitOverrides::default()
            },
            ..ServerConfig::default()
        };

        let limiter = resolve_rate_limiter_in(&schema, &config, PRODUCTION).await.unwrap();
        assert!(
            limiter.is_none(),
            "FRAISEQL_RATE_LIMITING_ENABLED=false is the documented off-switch; the compiled \
             schema must not shadow it (#774)"
        );
    }

    #[tokio::test]
    async fn numeric_overrides_win_over_the_compiled_schema() {
        let schema = schema_with_rate_limiting(json!({
            "enabled": true, "requests_per_second": 1, "burst_size": 1,
        }));
        let config = ServerConfig {
            rate_limit_overrides: RateLimitOverrides {
                rps_per_ip: Some(1000),
                burst_size: Some(500),
                ..RateLimitOverrides::default()
            },
            ..ServerConfig::default()
        };

        let limiter = resolve_rate_limiter_in(&schema, &config, PRODUCTION)
            .await
            .unwrap()
            .expect("enabled");
        assert_eq!(limiter.config().rps_per_ip, 1000, "the override must reach the limiter");
        assert_eq!(limiter.config().burst_size, 500);
    }

    #[tokio::test]
    async fn an_unset_override_leaves_the_compiled_value_alone() {
        let schema = schema_with_rate_limiting(json!({
            "enabled": true, "requests_per_second": 7, "burst_size": 9,
        }));
        let config = ServerConfig {
            rate_limit_overrides: RateLimitOverrides {
                // Only one field overridden; the others must not fall back to struct
                // defaults, which is what a whole-struct merge would have done.
                rps_per_ip: Some(1000),
                ..RateLimitOverrides::default()
            },
            ..ServerConfig::default()
        };

        let limiter = resolve_rate_limiter_in(&schema, &config, PRODUCTION).await.unwrap().unwrap();
        assert_eq!(limiter.config().rps_per_ip, 1000);
        assert_eq!(limiter.config().burst_size, 9, "an unset override must not clobber the schema");
    }

    #[tokio::test]
    async fn the_compiled_schema_still_wins_over_the_server_table() {
        // Unchanged precedence between the two file-based sources, pinned so the
        // override work above cannot quietly reorder it.
        let schema = schema_with_rate_limiting(json!({
            "enabled": true, "requests_per_second": 42, "burst_size": 1,
        }));
        let config = ServerConfig {
            rate_limiting: Some(RateLimitConfig {
                enabled: true,
                rps_per_ip: 7,
                ..RateLimitConfig::default()
            }),
            ..ServerConfig::default()
        };

        let limiter = resolve_rate_limiter_in(&schema, &config, PRODUCTION).await.unwrap().unwrap();
        assert_eq!(limiter.config().rps_per_ip, 42);
    }

    #[tokio::test]
    async fn overrides_alone_can_enable_rate_limiting() {
        let config = ServerConfig {
            rate_limit_overrides: RateLimitOverrides {
                enabled: Some(true),
                rps_per_ip: Some(250),
                ..RateLimitOverrides::default()
            },
            ..ServerConfig::default()
        };

        let limiter = resolve_rate_limiter_in(&bare_schema(), &config, PRODUCTION)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(limiter.config().rps_per_ip, 250);
    }

    // ── #898: a configured Redis backend must not silently become per-process ──
    //
    // `redis_url` exists only because per-process budgets are wrong for a
    // multi-replica deployment. Falling back to in-memory on a connection failure
    // makes N replicas enforce N times the configured rate, with an `error!` line
    // at boot as the only evidence. These pin the refusal at the seam the server
    // constructors actually call.
    //
    // Deliberately not gated on `redis-rate-limiting`: with the feature the
    // connection to a closed port fails, without it the URL names a backend the
    // binary cannot run. Both are "the operator asked for a shared budget and
    // would not get one", both must refuse, and an ungated test runs in every leg
    // rather than only the all-features one.

    /// Well-formed URL, nothing listening — the deploy-window Redis outage from
    /// the issue, and the k8s pod-ordering race from #777.
    fn schema_with_redis_rate_limiting() -> CompiledSchema {
        schema_with_rate_limiting(json!({
            "enabled": true,
            "requests_per_second": 100,
            "burst_size": 50,
            "redis_url": "redis://127.0.0.1:6391",
        }))
    }

    #[tokio::test]
    async fn an_unreachable_redis_rate_limiter_refuses_to_boot_in_production() {
        let err = resolve_rate_limiter_in(
            &schema_with_redis_rate_limiting(),
            &ServerConfig::default(),
            PRODUCTION,
        )
        .await
        .err()
        .expect(
            "#898: a configured-but-unavailable rate-limit Redis must refuse to boot — \
             downgrading to in-memory enforces N times the configured rate across N \
             replicas while every startup log reads healthy",
        );
        assert!(
            err.to_string().contains("security.rate_limiting"),
            "the refusal must name the config section so the operator can act on it; got: {err}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_redis_rate_limiter_downgrades_only_in_development() {
        let limiter = resolve_rate_limiter_in(
            &schema_with_redis_rate_limiting(),
            &ServerConfig::default(),
            DEVELOPMENT,
        )
        .await
        .expect("a declared development environment still boots, on the in-memory fallback")
        .expect("rate limiting is enabled");
        assert!(
            !limiter.is_distributed(),
            "the development fallback is the per-process limiter — which is exactly why \
             the production path above must refuse instead"
        );
    }

    /// The downgrade and the `FRAISEQL_REQUIRE_REDIS` gate meet here: an operator
    /// who asserted "all shared state is distributed" must not get a development
    /// downgrade silently, either. Composed through the pure decision fn so no
    /// test mutates the real env var (the parallel runner would race it).
    #[tokio::test]
    async fn a_downgraded_limiter_violates_the_require_redis_assertion() {
        let limiter = resolve_rate_limiter_in(
            &schema_with_redis_rate_limiting(),
            &ServerConfig::default(),
            DEVELOPMENT,
        )
        .await
        .unwrap()
        .unwrap();

        let violations = crate::server::initialization::SharedStateBackends {
            pkce_in_memory:         false,
            rate_limiter_in_memory: !limiter.is_distributed(),
            revocation_in_memory:   false,
            saml_replay_in_memory:  false,
        }
        .per_process_subsystems();
        assert!(
            violations.iter().any(|s| s.contains("rate_limiting")),
            "a limiter that fell back to per-process must be named by the \
             FRAISEQL_REQUIRE_REDIS gate; got {violations:?}"
        );
    }
}

/// #770/#777 class — a configured Redis backend that cannot be provided must refuse
/// to boot in production instead of silently downgrading to per-process state.
#[cfg(feature = "redis-rate-limiting")]
mod redis_rate_limit_downgrade_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use fraiseql_core::schema::CompiledSchema;

    use super::super::initialization::resolve_rate_limiter_in;
    use crate::ServerConfig;

    const PRODUCTION: bool = true;
    const DEVELOPMENT: bool = false;

    fn schema_with_redis_rate_limiting() -> CompiledSchema {
        let security = fraiseql_core::schema::SecurityConfig {
            rate_limiting: Some(fraiseql_core::schema::RateLimitingSecurityConfig {
                enabled: true,
                requests_per_second: 100,
                burst_size: 50,
                // Well-formed URL, nothing listening.
                redis_url: Some("redis://127.0.0.1:6390".to_string()),
                ..Default::default()
            }),
            ..fraiseql_core::schema::SecurityConfig::default()
        };
        CompiledSchema {
            security: Some(security),
            ..CompiledSchema::default()
        }
    }

    #[tokio::test]
    async fn an_unreachable_redis_limiter_refuses_to_boot_in_production() {
        let err = resolve_rate_limiter_in(
            &schema_with_redis_rate_limiting(),
            &ServerConfig::default(),
            PRODUCTION,
        )
        .await
        .err()
        .expect(
            "a configured-but-unreachable rate-limit Redis must refuse to boot: the \
                 in-memory fallback enforces N times the configured rate across N replicas",
        );
        assert!(
            err.to_string().contains("rate_limiting"),
            "the refusal must name the config section; got: {err}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_redis_limiter_downgrades_with_a_warning_in_development() {
        let limiter = resolve_rate_limiter_in(
            &schema_with_redis_rate_limiting(),
            &ServerConfig::default(),
            DEVELOPMENT,
        )
        .await
        .expect("development boots on the in-memory fallback with a warning");
        assert!(limiter.is_some(), "the development fallback still builds a limiter");
    }
}

/// #777 — the PKCE state store must honour a configured Redis backend or refuse to
/// boot; and a malformed `[security.pkce]` section must be loud, not a silent
/// PKCE-off (#778 class).
#[cfg(feature = "auth")]
mod pkce_boot_guard_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use std::sync::Arc;

    use fraiseql_core::schema::CompiledSchema;
    // The only bare `json!` users in this module are the redis-pkce tests, so an
    // ungated import is an unused-import error in every combo that enables `auth`
    // without `redis-pkce` (e.g. the server-inbound-email matrix leg).
    #[cfg(feature = "redis-pkce")]
    use serde_json::json;

    use super::super::initialization::pkce_store_from_schema_in;
    use crate::auth::state_encryption::StateEncryptionService;

    const PRODUCTION: bool = true;
    #[cfg(feature = "redis-pkce")]
    const DEVELOPMENT: bool = false;

    fn schema_with_pkce(section: serde_json::Value) -> CompiledSchema {
        // Null spells "absent"; a non-null section must parse as the typed
        // config — malformed sections are refused at schema load since #977.
        let security = fraiseql_core::schema::SecurityConfig {
            pkce: if section.is_null() {
                None
            } else {
                Some(serde_json::from_value(section).expect("valid pkce JSON"))
            },
            ..fraiseql_core::schema::SecurityConfig::default()
        };
        CompiledSchema {
            security: Some(security),
            ..CompiledSchema::default()
        }
    }

    fn encryption() -> Arc<StateEncryptionService> {
        Arc::new(StateEncryptionService::from_raw_key(
            &[7u8; 32],
            crate::auth::state_encryption::EncryptionAlgorithm::Aes256Gcm,
        ))
    }

    #[test]
    fn a_malformed_pkce_section_is_refused_at_load() {
        // A string-typed number is what a hand edit or an external generator produces;
        // it used to be a warning that silently disabled PKCE. Since #977 the typed
        // schema seam refuses it before any subsystem constructor runs.
        let json = serde_json::json!({
            "types": [], "queries": [], "mutations": [], "subscriptions": [],
            "security": { "pkce": { "enabled": true, "state_ttl_secs": "600" } }
        });
        let err = CompiledSchema::from_json(&json.to_string(), false)
            .expect_err("a present-but-unparseable [security.pkce] must refuse to load");
        assert!(
            err.to_string().contains("security.pkce"),
            "the error must name the section; got: {err}"
        );
    }

    #[tokio::test]
    async fn a_null_pkce_section_is_treated_as_absent() {
        let schema = schema_with_pkce(serde_json::Value::Null);
        let enc = encryption();
        let store = pkce_store_from_schema_in(&schema, Some(&enc), PRODUCTION)
            .await
            .expect("a null section means absent, not malformed");
        assert!(store.is_none());
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    async fn an_unreachable_redis_pkce_store_refuses_to_boot_in_production() {
        // Well-formed URL, nothing listening — the k8s pod-ordering race from #777.
        // Before the fix this fell back to in-memory permanently: behind a load
        // balancer ~ (N-1)/N of logins then fail with "state not found", and the only
        // evidence is one error line at boot.
        let schema = schema_with_pkce(json!({
            "enabled": true,
            "redis_url": "redis://127.0.0.1:6390",
        }));
        let enc = encryption();
        let err = pkce_store_from_schema_in(&schema, Some(&enc), PRODUCTION)
            .await
            .err()
            .expect("a configured-but-unreachable PKCE Redis must refuse to boot");
        assert!(
            err.to_string().contains("security.pkce"),
            "the refusal must name the config section; got: {err}"
        );
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    async fn an_unreachable_redis_pkce_store_downgrades_in_development() {
        let schema = schema_with_pkce(json!({
            "enabled": true,
            "redis_url": "redis://127.0.0.1:6390",
        }));
        let enc = encryption();
        let store = pkce_store_from_schema_in(&schema, Some(&enc), DEVELOPMENT)
            .await
            .expect("development boots on the in-memory fallback with a warning");
        assert!(
            store.is_some_and(|s| s.is_in_memory()),
            "the development fallback is the in-memory store"
        );
    }
}

/// #874: the `FRAISEQL_REQUIRE_REDIS` gate must cover every running subsystem
/// holding shared auth state — PKCE, rate limiting, and token revocation —
/// not just the PKCE store. Tested through the pure decision fn so no test
/// touches the real env var (the parallel runner would race it).
mod require_redis_874 {
    use crate::server::initialization::SharedStateBackends;

    #[test]
    fn each_per_process_subsystem_is_named() {
        let b = SharedStateBackends {
            pkce_in_memory:         false,
            rate_limiter_in_memory: true,
            revocation_in_memory:   true,
            saml_replay_in_memory:  false,
        };
        let v = b.per_process_subsystems();
        assert!(
            v.iter().any(|s| s.contains("rate_limiting")),
            "an in-memory rate limiter must violate the distributed-state assertion: {v:?}"
        );
        assert!(
            v.iter().any(|s| s.contains("token_revocation")),
            "a per-process revocation store must violate the distributed-state assertion: {v:?}"
        );
    }

    #[test]
    fn absent_subsystems_are_not_violations() {
        let b = SharedStateBackends {
            pkce_in_memory:         false,
            rate_limiter_in_memory: false,
            revocation_in_memory:   false,
            saml_replay_in_memory:  false,
        };
        assert!(
            b.per_process_subsystems().is_empty(),
            "distributed-or-disabled subsystems hold no state that can diverge"
        );
    }

    /// The backend classifiers feeding the gate: in-memory is per-process,
    /// Postgres-backed revocation is shared (the database is common to every
    /// replica).
    #[tokio::test]
    async fn backend_classifiers_are_truthful() {
        let limiter =
            crate::middleware::RateLimiter::new(crate::middleware::RateLimitConfig::default());
        assert!(!limiter.is_distributed(), "the in-memory limiter is per-process");

        let manager = crate::token_revocation::TokenRevocationManager::new(
            std::sync::Arc::new(crate::token_revocation::InMemoryRevocationStore::new()),
            false,
            false,
            3600,
        );
        assert!(!manager.is_distributed(), "the in-memory revocation store is per-process");
    }
}

// ── session_state_boot_tests: #389 — [session_state] construction refuses to
//    boot rather than silently downgrade ─────────────────────────────────────

#[cfg(feature = "auth")]
mod session_state_boot_tests {
    use crate::{
        server::Server,
        server_config::{ServerConfig, SessionStateServerConfig},
    };

    type TestServer = Server<fraiseql_core::db::postgres::PostgresAdapter>;

    fn config_with(backend: &str) -> ServerConfig {
        ServerConfig {
            session_state: Some(SessionStateServerConfig {
                backend: backend.to_string(),
                ..SessionStateServerConfig::default()
            }),
            ..ServerConfig::default()
        }
    }

    /// No `[session_state]` section → no subsystem, no task, no surprises.
    #[tokio::test]
    async fn absent_section_builds_none() {
        let built = TestServer::build_session_state(&ServerConfig::default(), None)
            .await
            .expect("absent section is not an error");
        assert!(built.is_none());
    }

    /// The volatile dev backend needs no pool.
    #[tokio::test]
    async fn memory_backend_builds_without_a_pool() {
        let built = TestServer::build_session_state(&config_with("memory"), None)
            .await
            .expect("memory backend builds");
        assert!(built.is_some());
    }

    /// The P21 rule: a configured durable backend with nothing to be durable ON
    /// is a boot refusal, not a silent in-memory downgrade.
    #[tokio::test]
    async fn postgres_backend_without_a_pool_refuses_to_boot() {
        let err = TestServer::build_session_state(&config_with("postgres"), None)
            .await
            .expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("[session_state]"), "error names the section: {msg}");
        assert!(msg.contains("database pool"), "error names the missing piece: {msg}");
    }

    /// `validate()` refuses an unknown backend token and zero intervals before
    /// construction is ever attempted.
    #[test]
    fn validate_refuses_bad_values() {
        let err = config_with("redis").validate().expect_err("unsupported backend");
        assert!(err.contains("redis"), "names the offending token: {err}");

        let mut zero_ttl = config_with("memory");
        if let Some(ref mut ss) = zero_ttl.session_state {
            ss.default_ttl_secs = 0;
        }
        assert!(zero_ttl.validate().is_err(), "zero TTL refused");

        let mut zero_evict = config_with("memory");
        if let Some(ref mut ss) = zero_evict.session_state {
            ss.evict_interval_secs = 0;
        }
        assert!(zero_evict.validate().is_err(), "zero evict interval refused");
    }

    /// The section is strict: a typo'd key is a parse error, not an inert
    /// setting (`deny_unknown_fields`).
    #[test]
    fn unknown_key_is_a_parse_error() {
        let toml = r#"
            [session_state]
            backend = "memory"
            default_ttl_seconds = 60
        "#;
        let parsed: Result<ServerConfig, _> = toml::from_str(toml);
        assert!(parsed.is_err(), "default_ttl_seconds (typo) must be refused");

        let ok: ServerConfig = toml::from_str("[session_state]\nbackend = \"memory\"\n")
            .expect("bare section parses with defaults");
        let ss = ok.session_state.expect("section present");
        assert_eq!(ss.default_ttl_secs, 3600);
        assert_eq!(ss.evict_interval_secs, 300);
    }
}

// ── async_operations_boot_tests: #391 — [async_operations] refuses to
//    half-mount ────────────────────────────────────────────────────────────────

mod async_operations_boot_tests {
    use crate::{
        server::Server,
        server_config::{AsyncOperationsConfig, ServerConfig},
    };

    type TestServer = Server<fraiseql_core::db::postgres::PostgresAdapter>;

    fn config_with_ops() -> ServerConfig {
        ServerConfig {
            async_operations: Some(AsyncOperationsConfig {
                operations: vec!["largeExport".to_string()],
                ..AsyncOperationsConfig::default()
            }),
            ..ServerConfig::default()
        }
    }

    /// No section → no runtime, no routes, no workers.
    #[tokio::test]
    async fn absent_section_builds_none() {
        let built = TestServer::build_async_operations(&ServerConfig::default(), None)
            .await
            .expect("absent section is not an error");
        assert!(built.is_none());
    }

    /// A configured surface without durable storage must refuse to boot — the
    /// alternative is routes accepting submissions no worker can execute.
    #[tokio::test]
    async fn configured_without_a_pool_refuses_to_boot() {
        let err = TestServer::build_async_operations(&config_with_ops(), None)
            .await
            .expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("[async_operations]"), "error names the section: {msg}");
        assert!(msg.contains("database pool"), "error names the missing piece: {msg}");
    }

    /// `validate()` refuses inert shapes before construction is attempted.
    #[test]
    fn validate_refuses_inert_shapes() {
        let empty_allowlist = ServerConfig {
            async_operations: Some(AsyncOperationsConfig::default()),
            ..ServerConfig::default()
        };
        let err = empty_allowlist.validate().expect_err("empty allowlist refused");
        assert!(err.contains("operations"), "names the offending key: {err}");

        let mut zero_workers = config_with_ops();
        if let Some(ref mut ao) = zero_workers.async_operations {
            ao.workers = 0;
        }
        assert!(zero_workers.validate().is_err(), "zero workers refused");

        let mut zero_attempts = config_with_ops();
        if let Some(ref mut ao) = zero_attempts.async_operations {
            ao.max_attempts = 0;
        }
        assert!(zero_attempts.validate().is_err(), "zero max_attempts refused");
    }

    /// Strict section: a typo'd key is a parse error, not an inert setting.
    #[test]
    fn unknown_key_is_a_parse_error() {
        let toml = r#"
            [async_operations]
            operations = ["largeExport"]
            worker_count = 4
        "#;
        let parsed: Result<ServerConfig, _> = toml::from_str(toml);
        assert!(parsed.is_err(), "worker_count (typo) must be refused");
    }
}
