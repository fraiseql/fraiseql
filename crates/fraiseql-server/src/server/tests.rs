// Note: the #421 page-size precedence logic (`page_size_precedence`) and its unit
// tests moved to `fraiseql_core::runtime` alongside `RuntimeConfig::from_compiled_schema`,
// the single seam every server constructor now routes through (H16).

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
            effective_trusted_doc_mode("permissive", true),
            TrustedDocumentMode::Strict,
            "persisted_queries_only=true must force Strict over a permissive declared mode"
        );
        assert_eq!(effective_trusted_doc_mode("strict", true), TrustedDocumentMode::Strict);

        // Without the flag, the declared mode is honored.
        assert_eq!(effective_trusted_doc_mode("strict", false), TrustedDocumentMode::Strict);
        assert_eq!(
            effective_trusted_doc_mode("permissive", false),
            TrustedDocumentMode::Permissive,
            "without the flag, a permissive schema stays permissive"
        );

        // An unknown/empty declared mode defaults to Permissive unless the flag forces Strict.
        assert_eq!(effective_trusted_doc_mode("", false), TrustedDocumentMode::Permissive);
        assert_eq!(effective_trusted_doc_mode("", true), TrustedDocumentMode::Strict);
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
        // 2001:db8:: is documentation range — treated as public by is_manifest_url_ssrf_blocked
        assert!(!is_manifest_url_ssrf_blocked("http://[2001:db8::1]/manifest.json"));
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
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
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
