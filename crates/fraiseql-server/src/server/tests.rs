// ── page_size_precedence_tests ──────────────────────────────────────────────────

mod page_size_precedence_tests {
    use super::super::builder::page_size_precedence;

    #[test]
    fn default_applies_when_nothing_configured() {
        assert_eq!(page_size_precedence(None, None), Some(1000));
    }

    #[test]
    fn compiled_value_overrides_default() {
        assert_eq!(page_size_precedence(None, Some(250)), Some(250));
    }

    #[test]
    fn env_number_overrides_compiled() {
        assert_eq!(page_size_precedence(Some("500"), Some(250)), Some(500));
    }

    #[test]
    fn env_none_or_zero_disables_the_ceiling() {
        assert_eq!(page_size_precedence(Some("none"), Some(250)), None);
        assert_eq!(page_size_precedence(Some("0"), Some(250)), None);
    }

    #[test]
    fn unparseable_env_falls_through_to_compiled() {
        assert_eq!(page_size_precedence(Some("lots"), Some(250)), Some(250));
    }
}

// ── initialization_tests ──────────────────────────────────────────────────────

mod initialization_tests {
    use super::super::initialization::is_manifest_url_ssrf_blocked;

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
