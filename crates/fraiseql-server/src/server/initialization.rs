//! Private helper functions for reading security/config from the compiled schema
//! and building subsystem objects during server construction.

use std::sync::Arc;

use fraiseql_core::{db::traits::DatabaseAdapter, schema::CompiledSchema};
use tracing::{info, warn};

#[cfg(feature = "auth")]
use super::ServerError;
use super::{RateLimiter, Server};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    #[cfg(feature = "auth")]
    /// Build a `StateEncryptionService` from `security.state_encryption` in the compiled
    /// schema, if the section is present and `enabled = true`.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when `enabled = true` but the key environment
    /// variable is absent or invalid.  The server must not start in this state.
    pub(super) fn state_encryption_from_schema(
        schema: &CompiledSchema,
    ) -> crate::Result<Option<Arc<crate::auth::state_encryption::StateEncryptionService>>> {
        match schema.security.as_ref() {
            None => Ok(None),
            Some(s) => {
                let s_val = serde_json::to_value(s).map_err(|e| {
                    ServerError::ConfigError(format!("Failed to serialize security config: {e}"))
                })?;
                crate::auth::state_encryption::StateEncryptionService::from_compiled_schema(&s_val)
                    .map_err(|e| ServerError::ConfigError(e.to_string()))
            },
        }
    }

    /// Build a `PkceStateStore` from the compiled schema if `security.pkce.enabled = true`.
    ///
    /// When `redis_url` is set and the `redis-pkce` feature is compiled in, initialises
    /// a Redis-backed distributed store; otherwise falls back to the in-memory backend
    /// with a warning.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when `pkce.enabled = true` but
    /// `[security.state_encryption]` is missing or disabled while running in production
    /// mode (`FRAISEQL_ENV` is not `development`/`dev`). PKCE state tokens would
    /// otherwise be sent to the OIDC provider as the raw, unencrypted lookup key, so the
    /// server refuses to start rather than serve `/auth/start` with a false "state
    /// encryption is enforced" posture. In development mode this is a warning instead.
    #[cfg(feature = "auth")]
    #[allow(clippy::cognitive_complexity)] // Reason: conditional backend selection (Redis vs in-memory) with feature-gated branches
    pub(super) async fn pkce_store_from_schema(
        schema: &CompiledSchema,
        state_encryption: Option<&Arc<crate::auth::state_encryption::StateEncryptionService>>,
    ) -> crate::Result<Option<Arc<crate::auth::PkceStateStore>>> {
        let Some(security) = schema.security.as_ref() else {
            return Ok(None);
        };
        let Some(pkce_cfg) = security.additional.get("pkce") else {
            return Ok(None);
        };

        #[allow(clippy::items_after_statements)] // Reason: local deserialization helper struct scoped near its usage
        #[derive(serde::Deserialize)]
        struct PkceCfgMinimal {
            #[serde(default)]
            enabled:               bool,
            #[serde(default = "default_ttl")]
            state_ttl_secs:        u64,
            #[serde(default = "default_method")]
            code_challenge_method: String,
            redis_url:             Option<String>,
        }
        #[allow(clippy::items_after_statements)] // Reason: serde default fn for PkceCfgMinimal above
        const fn default_ttl() -> u64 {
            600
        }
        #[allow(clippy::items_after_statements)] // Reason: serde default fn for PkceCfgMinimal above
        fn default_method() -> String {
            "S256".into()
        }

        let cfg: PkceCfgMinimal = match serde_json::from_value(pkce_cfg.clone()) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(error = %e, "Failed to deserialize pkce config — disabling PKCE");
                return Ok(None);
            },
        };
        if !cfg.enabled {
            return Ok(None);
        }

        // SECURITY (#360): PKCE state tokens are sent to the OIDC provider; without
        // [security.state_encryption] they travel as the raw 32-byte lookup key. Refuse
        // to boot in production rather than serve /auth/start with a false "state
        // encryption is enforced" posture; development mode downgrades this to a warning.
        pkce_state_encryption_check(
            state_encryption.is_some(),
            crate::ServerConfig::is_production_mode(),
        )?;

        if cfg.code_challenge_method.eq_ignore_ascii_case("plain") {
            warn!(
                "pkce.code_challenge_method = \"plain\" is insecure. \
                 Use \"S256\" in all production environments."
            );
        }

        let enc = state_encryption.cloned();

        // Prefer the Redis backend when redis_url is configured and the feature is compiled in.
        #[cfg(feature = "redis-pkce")]
        if let Some(ref url) = cfg.redis_url {
            match crate::auth::PkceStateStore::new_redis(url, cfg.state_ttl_secs, enc.clone()).await
            {
                Ok(store) => {
                    info!(redis_url = %url, "PKCE state store: Redis backend");
                    return Ok(Some(Arc::new(store)));
                },
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        redis_url = %url,
                        "Failed to connect to Redis PKCE store — falling back to in-memory"
                    );
                },
            }
        }

        #[cfg(not(feature = "redis-pkce"))]
        if cfg.redis_url.is_some() {
            warn!(
                "pkce.redis_url is set but the `redis-pkce` Cargo feature is not compiled in. \
                 Rebuild with `--features redis-pkce` to enable the Redis PKCE backend. \
                 Falling back to in-memory storage."
            );
        }

        warn!(
            "PKCE state store: in-memory. In a multi-replica deployment, auth flows will fail \
             if /auth/start and /auth/callback hit different replicas. \
             Set [security.pkce] redis_url to enable the Redis backend, \
             or FRAISEQL_REQUIRE_REDIS=1 to enforce it at startup."
        );

        Ok(Some(Arc::new(crate::auth::PkceStateStore::new(cfg.state_ttl_secs, enc))))
    }

    /// Validate that distributed storage is configured when `FRAISEQL_REQUIRE_REDIS` is set.
    ///
    /// When `FRAISEQL_REQUIRE_REDIS=1` is present in the environment, the server refuses
    /// to start if the PKCE state store is using in-memory storage.  This prevents silent
    /// per-replica state isolation in multi-instance deployments.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` with an operator-actionable message when the
    /// constraint is violated.
    #[cfg(feature = "auth")]
    pub(super) fn check_redis_requirement(
        pkce_store: Option<&Arc<crate::auth::PkceStateStore>>,
    ) -> crate::Result<()> {
        if std::env::var("FRAISEQL_REQUIRE_REDIS").is_ok() {
            let pkce_in_memory = pkce_store.is_some_and(|s| s.is_in_memory());
            if pkce_in_memory {
                return Err(ServerError::ConfigError(concat!(
                    "FraiseQL failed to start\n\n",
                    "  FRAISEQL_REQUIRE_REDIS is set but PKCE auth state is using in-memory storage.\n",
                    "  In a multi-replica deployment, auth callbacks can fail if they hit a\n",
                    "  different replica than the one that handled /auth/start.\n\n",
                    "  To fix:\n",
                    "    [security.pkce]\n",
                    "    redis_url = \"redis://localhost:6379\"\n\n",
                    "    [security.rate_limiting]\n",
                    "    redis_url = \"redis://localhost:6379\"\n\n",
                    "  To allow in-memory (single-replica only):\n",
                    "    Unset FRAISEQL_REQUIRE_REDIS",
                )
                .into()));
            }
        }
        Ok(())
    }

    /// Build an `OidcServerClient` from the compiled schema JSON, if `[auth]` is present.
    #[cfg(feature = "auth")]
    pub(super) fn oidc_server_client_from_schema(
        schema: &CompiledSchema,
    ) -> Option<Arc<crate::auth::OidcServerClient>> {
        // The full schema JSON lives in the executor's compiled schema.
        // Access it via the security Value (which contains the embedded JSON blob).
        // We expose the root schema JSON here.
        let schema_json = serde_json::to_value(schema)
            .inspect_err(|e| warn!(error = %e, "Failed to serialize compiled schema for OIDC client construction"))
            .ok()?;
        crate::auth::OidcServerClient::from_compiled_schema(&schema_json)
    }

    /// Build a `RateLimiter` from the `security.rate_limiting` key embedded in the
    /// compiled schema, if present and `enabled = true`.
    ///
    /// When `redis_url` is set and the `redis-rate-limiting` feature is compiled in,
    /// initialises a Redis-backed distributed limiter; otherwise falls back to the
    /// in-memory backend (with a warning when `redis_url` is set but the feature is
    /// absent).
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when the `failed_login_*` brute-force
    /// settings are tuned away from their defaults while running in production — the
    /// binary has no first-factor login surface to enforce them (#356). See
    /// [`failed_login_lockout_check`].
    pub(super) async fn rate_limiter_from_schema(
        schema: &CompiledSchema,
    ) -> crate::Result<Option<Arc<RateLimiter>>> {
        let Some(sec): Option<crate::middleware::RateLimitingSecurityConfig> = schema
            .security
            .as_ref()
            .and_then(|s| s.additional.get("rate_limiting"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        else {
            return Ok(None);
        };

        if !sec.enabled {
            return Ok(None);
        }

        // SECURITY (#356): the binary performs no first-factor login, so it cannot
        // honour failed_login_max_attempts / failed_login_lockout_secs. Refuse to
        // boot in production when an operator has tuned them away from the defaults
        // (development downgrades to a warning).
        failed_login_lockout_check(
            sec.failed_login_max_attempts,
            sec.failed_login_lockout_secs,
            crate::ServerConfig::is_production_mode(),
        )?;

        // Warn when trust_proxy_headers is enabled without restricting which IPs are
        // trusted proxies — any client can then spoof X-Forwarded-For.
        if sec.trust_proxy_headers && sec.trusted_proxy_cidrs.as_ref().is_none_or(Vec::is_empty) {
            warn!(
                "Rate limiter: trust_proxy_headers = true but trusted_proxy_cidrs is not set. \
                 Any client can spoof X-Forwarded-For and bypass per-IP rate limits. \
                 Set trusted_proxy_cidrs in [security.rate_limiting] to restrict which \
                 proxy IPs are trusted (e.g. [\"10.0.0.0/8\"] for internal load balancers)."
            );
        }

        let config = crate::middleware::RateLimitConfig::from_security_config(&sec);

        let limiter: RateLimiter = if let Some(ref redis_url) = sec.redis_url {
            #[cfg(feature = "redis-rate-limiting")]
            {
                match RateLimiter::new_redis(redis_url, config.clone()).await {
                    Ok(rl) => {
                        info!(
                            url = redis_url.as_str(),
                            rps_per_ip = config.rps_per_ip,
                            burst_size = config.burst_size,
                            "Rate limiting: using Redis distributed backend"
                        );
                        rl.with_path_rules_from_security(&sec)
                    },
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "Failed to connect to Redis for rate limiting — \
                             falling back to in-memory backend"
                        );
                        RateLimiter::new(config).with_path_rules_from_security(&sec)
                    },
                }
            }
            #[cfg(not(feature = "redis-rate-limiting"))]
            {
                let _ = redis_url;
                warn!(
                    "rate_limiting.redis_url is set but the server was compiled without the \
                     'redis-rate-limiting' feature. Using in-memory backend."
                );
                RateLimiter::new(config).with_path_rules_from_security(&sec)
            }
        } else {
            info!(
                rps_per_ip = config.rps_per_ip,
                burst_size = config.burst_size,
                "Rate limiting: using in-memory backend"
            );
            RateLimiter::new(config).with_path_rules_from_security(&sec)
        };

        Ok(Some(Arc::new(limiter)))
    }

    /// Build an `ErrorSanitizer` from the `security.error_sanitization` key in the
    /// compiled schema's security blob (if present), falling back to a disabled sanitizer.
    pub(super) fn error_sanitizer_from_schema(
        schema: &CompiledSchema,
    ) -> Arc<crate::config::error_sanitization::ErrorSanitizer> {
        let sanitizer = schema
            .security
            .as_ref()
            .and_then(|s| s.additional.get("error_sanitization"))
            .and_then(|v| {
                serde_json::from_value::<
                    crate::config::error_sanitization::ErrorSanitizationConfig,
                >(v.clone())
                .ok()
            })
            .map_or_else(
                crate::config::error_sanitization::ErrorSanitizer::disabled,
                crate::config::error_sanitization::ErrorSanitizer::new,
            );
        Arc::new(sanitizer)
    }

    /// Build a `TrustedDocumentStore` from `security.trusted_documents` in the
    /// compiled schema, if present and `enabled = true`.
    ///
    /// Any background hot-reload task spawned for the store is pushed onto
    /// `tasks` so the server can await its termination during graceful shutdown.
    #[allow(clippy::cognitive_complexity)] // Reason: config parsing with multiple optional fields and validation
    pub(super) fn trusted_docs_from_schema(
        schema: &CompiledSchema,
        tasks: &mut tokio::task::JoinSet<()>,
    ) -> Option<Arc<crate::trusted_documents::TrustedDocumentStore>> {
        let security = schema.security.as_ref()?;
        let td_cfg = security.additional.get("trusted_documents")?;

        #[allow(clippy::items_after_statements)] // Reason: local deserialization helper struct scoped near its usage
        #[derive(serde::Deserialize)]
        struct TdCfgMinimal {
            #[serde(default)]
            enabled:              bool,
            #[serde(default)]
            mode:                 String,
            manifest_path:        Option<String>,
            #[allow(dead_code)]
            // Reason: serde deserialization target — manifest_url is a valid config field but this
            // minimal struct only reads manifest_path
            manifest_url: Option<String>,
            #[serde(default)]
            reload_interval_secs: u64,
        }

        let cfg: TdCfgMinimal = serde_json::from_value(td_cfg.clone())
            .inspect_err(|e| warn!(error = %e, "Failed to deserialize trusted_documents config — disabling trusted documents"))
            .ok()?;
        if !cfg.enabled {
            return None;
        }

        let mode = if cfg.mode.eq_ignore_ascii_case("strict") {
            crate::trusted_documents::TrustedDocumentMode::Strict
        } else {
            crate::trusted_documents::TrustedDocumentMode::Permissive
        };

        if let Some(ref path) = cfg.manifest_path {
            match crate::trusted_documents::TrustedDocumentStore::from_manifest_file(
                std::path::Path::new(path),
                mode,
            ) {
                Ok(store) => {
                    let store = Arc::new(store);
                    // Spawn hot-reload task if configured.
                    if cfg.reload_interval_secs > 0 {
                        if let Some(ref url) = cfg.manifest_url {
                            Self::spawn_trusted_docs_reload(
                                Arc::clone(&store),
                                url.clone(),
                                cfg.reload_interval_secs,
                                tasks,
                            );
                        } else {
                            warn!(
                                "trusted_documents.reload_interval_secs > 0 but no manifest_url set \
                                 — hot-reload disabled (file-based manifests must be reloaded manually)"
                            );
                        }
                    }
                    info!(
                        manifest = %path,
                        mode = ?mode,
                        "Trusted documents loaded"
                    );
                    Some(store)
                },
                Err(e) => {
                    tracing::error!(error = %e, "Failed to load trusted documents manifest");
                    None
                },
            }
        } else {
            warn!("trusted_documents.enabled = true but no manifest_path or manifest_url set");
            None
        }
    }

    /// Spawn a background task that periodically re-fetches the manifest from a URL.
    ///
    /// The spawned task is registered on `tasks` so the server can await its
    /// termination during graceful shutdown.
    pub(super) fn spawn_trusted_docs_reload(
        store: Arc<crate::trusted_documents::TrustedDocumentStore>,
        url: String,
        interval_secs: u64,
        tasks: &mut tokio::task::JoinSet<()>,
    ) {
        // SSRF guard: reject URLs that target private/loopback/link-local addresses.
        // The manifest URL is operator-configured, but a tampered compiled schema
        // could point it at internal services; block that at spawn time.
        if is_manifest_url_ssrf_blocked(&url) {
            tracing::error!(
                url = %url,
                "Trusted documents manifest URL targets a private/loopback address \
                 (SSRF protection) — hot-reload disabled"
            );
            return;
        }

        tasks.spawn(async move {
            const MANIFEST_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
            /// Maximum byte size accepted for a hot-reloaded trusted-documents manifest.
            /// Matches the cap enforced for file-based manifests in `trusted_documents.rs`.
            const MAX_TRUSTED_DOCS_RESPONSE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let client = reqwest::Client::builder()
                    .timeout(MANIFEST_FETCH_TIMEOUT)
                    .build()
                    .expect("reqwest client with timeout should always build");

                match client.get(&url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            match resp.bytes().await {
                                Ok(body_bytes) => {
                                    if body_bytes.len() > MAX_TRUSTED_DOCS_RESPONSE_BYTES {
                                        warn!(
                                            bytes = body_bytes.len(),
                                            max = MAX_TRUSTED_DOCS_RESPONSE_BYTES,
                                            "Trusted documents manifest response too large — skipping reload"
                                        );
                                    } else {
                                        #[derive(serde::Deserialize)]
                                        struct Manifest {
                                            documents: std::collections::HashMap<String, String>,
                                        }
                                        match serde_json::from_slice::<Manifest>(&body_bytes) {
                                            Ok(manifest) => {
                                                let count = manifest.documents.len();
                                                store.replace_documents(manifest.documents);
                                                info!(count, "Trusted documents manifest reloaded");
                                            },
                                            Err(e) => {
                                                warn!(error = %e, "Failed to parse trusted documents manifest");
                                            },
                                        }
                                    }
                                },
                                Err(e) => {
                                    warn!(error = %e, "Failed to read trusted documents manifest response");
                                },
                            }
                        } else {
                            warn!(
                                %status,
                                %url,
                                "Trusted documents manifest fetch returned non-success — skipping reload"
                            );
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, "Failed to fetch trusted documents manifest");
                    },
                }
            }
        });
    }
}

// ── PKCE state-encryption requirement (#360) ─────────────────────────────────

/// Enforce that PKCE is not served without `[security.state_encryption]`.
///
/// PKCE state tokens are sent to the OIDC provider; without state encryption they
/// travel as the raw 32-byte lookup key. In production this is a hard error so the
/// server does not serve `/auth/start` with a false "state encryption is enforced"
/// posture. In development (`FRAISEQL_ENV=development`/`dev`) it is downgraded to a
/// warning so local auth flows still work.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when `has_state_encryption` is false and
/// `is_production` is true.
#[cfg(feature = "auth")]
pub(super) fn pkce_state_encryption_check(
    has_state_encryption: bool,
    is_production: bool,
) -> crate::Result<()> {
    if has_state_encryption {
        return Ok(());
    }
    if is_production {
        return Err(ServerError::ConfigError(
            concat!(
                "FraiseQL failed to start\n\n",
                "  [security.pkce] enabled = true but [security.state_encryption] is\n",
                "  missing or disabled. PKCE state tokens would be sent to the OIDC\n",
                "  provider unencrypted, so the documented \"state encryption is\n",
                "  enforced\" posture would be false.\n\n",
                "  To fix, enable state encryption:\n",
                "    [security.state_encryption]\n",
                "    enabled = true\n",
                "    # 32-byte key supplied via FRAISEQL_STATE_ENCRYPTION_KEY\n\n",
                "  For local development only:\n",
                "    Set FRAISEQL_ENV=development to downgrade this to a warning.",
            )
            .into(),
        ));
    }
    warn!(
        "pkce.enabled = true but state_encryption is disabled — PKCE state tokens are \
         sent to the OIDC provider unencrypted. Allowed only because \
         FRAISEQL_ENV=development; enable [security.state_encryption] before production."
    );
    Ok(())
}

// ── Failed-login lockout enforceability (#356) ────────────────────────────────

/// Reject a `failed_login_*` brute-force configuration the binary cannot enforce.
///
/// The off-the-shelf `fraiseql-server` binary performs no first-factor login of its
/// own: OIDC/JWT bearer tokens are validated cryptographically (first-factor auth is
/// delegated to the identity provider), API keys and admin bearer tokens are
/// high-entropy machine credentials (the admin paths already have their own
/// `admin_auth_max_failures` lockout), and TOTP MFA is a library-only feature that
/// `main.rs` never mounts. There is therefore no place to apply
/// `failed_login_max_attempts` / `failed_login_lockout_secs`.
///
/// When an operator tunes these away from the documented defaults they expect a
/// brute-force control the binary cannot provide, so in production this refuses to
/// boot (a silently-ignored security control is the exact failure mode #356
/// reports). Development mode (`FRAISEQL_ENV=development`/`dev`) downgrades it to a
/// warning. Untouched (default) values are accepted silently — they ride along with
/// any `[security.rate_limiting]` section and signal no intent.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when the values are non-default and
/// `is_production` is true.
pub(super) fn failed_login_lockout_check(
    max_attempts: u32,
    lockout_secs: u64,
    is_production: bool,
) -> crate::Result<()> {
    let tuned = max_attempts != crate::middleware::rate_limit::DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS
        || lockout_secs != crate::middleware::rate_limit::DEFAULT_FAILED_LOGIN_LOCKOUT_SECS;
    if !tuned {
        return Ok(());
    }
    if is_production {
        return Err(crate::ServerError::ConfigError(
            concat!(
                "FraiseQL failed to start\n\n",
                "  [security.rate_limiting] failed_login_max_attempts / failed_login_lockout_secs\n",
                "  are set, but the fraiseql-server binary performs no first-factor login and\n",
                "  cannot enforce a failed-login lockout. OIDC/JWT is validated cryptographically\n",
                "  (first-factor auth is delegated to your identity provider), and TOTP MFA is a\n",
                "  library-only feature this binary does not mount.\n\n",
                "  Enforce brute-force protection where the first factor is actually checked:\n",
                "    - at your identity provider (login attempt limits / lockout), or\n",
                "    - at the edge (nginx / Cloudflare / a WAF) in front of FraiseQL.\n\n",
                "  Then remove failed_login_max_attempts / failed_login_lockout_secs from\n",
                "  [security.rate_limiting] (per-IP / per-endpoint rate limits still apply).\n\n",
                "  For local development only:\n",
                "    Set FRAISEQL_ENV=development to downgrade this to a warning.",
            )
            .into(),
        ));
    }
    warn!(
        "[security.rate_limiting] failed_login_* is set but this binary performs no \
         first-factor login and cannot enforce a failed-login lockout. Allowed only because \
         FRAISEQL_ENV=development; enforce brute-force protection at your identity provider or \
         edge proxy. Per-IP / per-endpoint rate limits still apply."
    );
    Ok(())
}

// ── Observer transport selection (#350) ──────────────────────────────────────

/// Reject a configured observer transport the binary cannot run.
///
/// The off-the-shelf binary ships with PostgreSQL LISTEN/NOTIFY always
/// available; NATS `JetStream` is gated behind the `observers-nats` feature and
/// needs a broker URL. When an operator selects `transport = "nats"` (via
/// `[observers.runtime.transport]` or `FRAISEQL_OBSERVER_TRANSPORT`) but the
/// binary cannot actually run it — the feature is not compiled in, or no URL is
/// configured — the server must say so loudly rather than silently fall back to
/// PostgreSQL and serve with a false "running on NATS" posture (the #350 bug).
///
/// In production this is a hard error so the server refuses to boot; in
/// development (`FRAISEQL_ENV=development`/`dev`) it is downgraded to a warning
/// so local runs still come up (on PostgreSQL). `Postgres` and the testing-only
/// `InMemory` transport need no broker and are always accepted. A future
/// (`#[non_exhaustive]`) transport the binary does not understand is treated as
/// unsupported.
///
/// This is a pure, race-free decision function (mirrors
/// [`pkce_state_encryption_check`] and [`failed_login_lockout_check`]): the
/// caller supplies the resolved transport, whether the feature is compiled in,
/// whether a NATS URL is present, and the production flag.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when a non-Postgres transport cannot run
/// and `is_production` is true.
#[cfg(feature = "observers")]
pub(super) fn observer_transport_check(
    kind: fraiseql_observers::config::TransportKind,
    compiled_in: bool,
    nats_url_present: bool,
    is_production: bool,
) -> crate::Result<()> {
    use fraiseql_observers::config::TransportKind;

    // Postgres (default) and the in-memory testing transport need no broker.
    match kind {
        TransportKind::Postgres | TransportKind::InMemory => return Ok(()),
        TransportKind::Nats => {},
        // A transport variant added in a future fraiseql-observers release that
        // this binary was not built to drive: refuse it rather than guess.
        _ => {
            return refuse_or_warn_transport(
                is_production,
                UNKNOWN_TRANSPORT_MSG,
                UNKNOWN_TRANSPORT_WARN,
            );
        },
    }

    if !compiled_in {
        return refuse_or_warn_transport(
            is_production,
            NATS_NOT_COMPILED_MSG,
            NATS_NOT_COMPILED_WARN,
        );
    }
    if !nats_url_present {
        return refuse_or_warn_transport(is_production, NATS_NO_URL_MSG, NATS_NO_URL_WARN);
    }
    Ok(())
}

#[cfg(feature = "observers")]
const NATS_NOT_COMPILED_MSG: &str = concat!(
    "FraiseQL failed to start\n\n",
    "  [observers.runtime.transport] transport = \"nats\" (or\n",
    "  FRAISEQL_OBSERVER_TRANSPORT=nats) was selected, but this binary was not\n",
    "  built with NATS support, so the observer runtime cannot run on NATS and\n",
    "  would silently fall back to PostgreSQL LISTEN/NOTIFY.\n\n",
    "  To fix, build/run a binary with the NATS transport compiled in:\n",
    "    cargo build -p fraiseql-server --features observers-nats\n\n",
    "  Or select the PostgreSQL transport explicitly:\n",
    "    [observers.runtime.transport]\n",
    "    transport = \"postgres\"\n\n",
    "  For local development only:\n",
    "    Set FRAISEQL_ENV=development to downgrade this to a warning (runs on PostgreSQL).",
);

#[cfg(feature = "observers")]
const NATS_NOT_COMPILED_WARN: &str = "observer transport = \"nats\" selected but this binary lacks the observers-nats feature; \
     the observer runtime will run on PostgreSQL. Allowed only because FRAISEQL_ENV=development; \
     build with --features observers-nats before production.";

#[cfg(feature = "observers")]
const NATS_NO_URL_MSG: &str = concat!(
    "FraiseQL failed to start\n\n",
    "  [observers.runtime.transport] transport = \"nats\" was selected, but no NATS\n",
    "  broker URL is configured, so the observer runtime cannot connect.\n\n",
    "  To fix, set the broker URL:\n",
    "    [observers.runtime.transport.nats]\n",
    "    url = \"nats://your-broker:4222\"\n",
    "  (or export FRAISEQL_NATS_URL).\n\n",
    "  For local development only:\n",
    "    Set FRAISEQL_ENV=development to downgrade this to a warning (runs on PostgreSQL).",
);

#[cfg(feature = "observers")]
const NATS_NO_URL_WARN: &str = "observer transport = \"nats\" selected but no NATS broker URL is configured; the observer \
     runtime will run on PostgreSQL. Allowed only because FRAISEQL_ENV=development; set \
     [observers.runtime.transport.nats] url before production.";

#[cfg(feature = "observers")]
const UNKNOWN_TRANSPORT_MSG: &str = concat!(
    "FraiseQL failed to start\n\n",
    "  [observers.runtime.transport] selected an observer transport this binary\n",
    "  does not know how to run. Upgrade fraiseql-server, or select a supported\n",
    "  transport (\"postgres\" or \"nats\").\n\n",
    "  For local development only:\n",
    "    Set FRAISEQL_ENV=development to downgrade this to a warning (runs on PostgreSQL).",
);

#[cfg(feature = "observers")]
const UNKNOWN_TRANSPORT_WARN: &str = "observer transport selection is not supported by this binary; the observer runtime will run \
     on PostgreSQL. Allowed only because FRAISEQL_ENV=development; upgrade fraiseql-server or \
     select a supported transport before production.";

/// Either refuse to boot (production) or warn and continue on PostgreSQL (dev).
#[cfg(feature = "observers")]
fn refuse_or_warn_transport(
    is_production: bool,
    prod_msg: &'static str,
    dev_warn: &'static str,
) -> crate::Result<()> {
    if is_production {
        return Err(crate::ServerError::ConfigError(prod_msg.into()));
    }
    warn!("{dev_warn}");
    Ok(())
}

// ── SSRF guard for manifest hot-reload URL ────────────────────────────────────

/// Returns `true` when `url` resolves to a private, loopback, or link-local
/// address that the server must not fetch (SSRF protection).
///
/// This uses the same URL-parser + bracket-strip pattern used in the federation
/// and Vault SSRF guards (S18-H3, S19-I2b) to correctly handle `IPv6` literals.
pub(super) fn is_manifest_url_ssrf_blocked(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        // Unparseable URL — block it; the actual fetch would fail anyway.
        return true;
    };
    let host_raw = parsed.host_str().unwrap_or("");
    // Strip IPv6 brackets: url crate returns "[::1]", IpAddr::parse needs "::1".
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback() || addr.is_private() || addr.is_link_local();
    }
    if let Ok(addr) = host.parse::<std::net::Ipv6Addr>() {
        // Block loopback (::1), unspecified (::), and ULA (fc00::/7).
        return addr.is_loopback()
            || addr.is_unspecified()
            || (addr.segments()[0] & 0xFE00) == 0xFC00;
    }
    false
}

/// Refuse to boot when the compiled schema marks any field for at-rest encryption.
///
/// Write-path field encryption is **not implemented** in this release (H12): the mutation
/// executor never encrypts on write — `FieldEncryptionService::encrypt_variables` has no
/// caller — so a field marked `encryption` is stored in **plaintext** while the read path
/// attempts to decrypt it, returning a 500 (`Field decryption failed`) on every read. Worse,
/// when the `secrets` feature is absent the field round-trips silently in plaintext, so an
/// operator believes sensitive columns are encrypted at rest when they are not.
///
/// Rather than silently storing sensitive data in plaintext, the server refuses to start and
/// names the offending field(s). This is the honest interim until end-to-end field encryption
/// (write-path call, array/nested recursion, `(type, field)` keying, ciphertext versioning,
/// and key KDF/zeroize) is implemented.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when any field in the schema declares `encryption`.
pub(super) fn field_encryption_unsupported_check(schema: &CompiledSchema) -> crate::Result<()> {
    let encrypted: Vec<String> = schema
        .types
        .iter()
        .flat_map(|t| {
            t.fields
                .iter()
                .filter(|f| f.encryption.is_some())
                .map(move |f| format!("{}.{}", t.name, f.name))
        })
        .collect();

    if encrypted.is_empty() {
        return Ok(());
    }

    Err(crate::ServerError::ConfigError(format!(
        "Field-level at-rest encryption is configured for {} but is not supported in this \
         release: the mutation path does not encrypt on write, so these field(s) would be \
         stored in plaintext and then fail to decrypt on read (HTTP 500). Remove the \
         `encryption` marker from these field(s) — and any `[security.field_encryption]` \
         config — to start the server.",
        encrypted.join(", ")
    )))
}
