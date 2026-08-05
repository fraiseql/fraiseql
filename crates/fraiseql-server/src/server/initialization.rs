//! Private helper functions for reading security/config from the compiled schema
//! and building subsystem objects during server construction.

use std::sync::Arc;

use fraiseql_core::{db::traits::DatabaseAdapter, schema::CompiledSchema};
use tracing::{info, warn};

use super::{RateLimiter, Server, ServerError};

/// Merge the effective query depth/complexity limits by the documented
/// precedence: runtime TOML `[validation]` > compiled schema `[validation]`,
/// **per field**. Shared by the HTTP stage validator and the executor gate so
/// the two enforcement points cannot disagree about the effective limits.
pub(super) fn effective_validation_limits(
    runtime: Option<&fraiseql_core::schema::ValidationConfig>,
    compiled: Option<&fraiseql_core::schema::ValidationConfig>,
) -> (Option<u32>, Option<u32>) {
    let depth = runtime
        .and_then(|v| v.max_query_depth)
        .or_else(|| compiled.and_then(|v| v.max_query_depth));
    let complexity = runtime
        .and_then(|v| v.max_query_complexity)
        .or_else(|| compiled.and_then(|v| v.max_query_complexity));
    (depth, complexity)
}

/// Build the executor's [`RuntimeConfig`] for a server constructor (#379).
///
/// Wraps [`RuntimeConfig::from_compiled_schema`] (the H16 seam) and, when a
/// runtime `[validation]` override is configured, installs the per-field merged
/// limits as the executor's caller-set gate. Without this, the executor's
/// schema-derived gate would silently negate a runtime override that loosens a
/// compiled limit — the stage would admit the query and the engine would then
/// reject it. Reload safety: `validation_config` is boot-frozen (the reload
/// gate refuses a schema that changes it), so the values installed here cannot
/// go stale across a hot reload.
///
/// # Errors
///
/// Returns the validation message when the schema's format version is
/// incompatible with this runtime.
pub(super) fn executor_runtime_config(
    schema: &CompiledSchema,
    config: &crate::server_config::ServerConfig,
) -> Result<fraiseql_core::runtime::RuntimeConfig, String> {
    let mut rt = fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(schema)?;
    if config.validation.is_some() {
        let (depth, complexity) = effective_validation_limits(
            config.validation.as_ref(),
            schema.validation_config.as_ref(),
        );
        if depth.is_some() || complexity.is_some() {
            rt.query_validation = Some(fraiseql_core::security::QueryValidatorConfig {
                max_depth:      depth.map_or(usize::MAX, |d| d as usize),
                max_complexity: complexity.map_or(usize::MAX, |c| c as usize),
                max_size_bytes: usize::MAX,
                max_aliases:    usize::MAX,
            });
        }
    }
    Ok(rt)
}

/// Which per-replica-capable subsystems are running on per-process state (#874).
///
/// `false` means "distributed or not running" — a disabled subsystem holds no
/// state that could diverge between replicas.
pub(super) struct SharedStateBackends {
    /// PKCE OAuth state store is in-memory.
    pub pkce_in_memory:         bool,
    /// The rate limiter tracks budgets per process.
    pub rate_limiter_in_memory: bool,
    /// The token revocation store is per-process.
    pub revocation_in_memory:   bool,
}

impl SharedStateBackends {
    /// The names of every subsystem violating the distributed-state requirement.
    pub(super) fn per_process_subsystems(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.pkce_in_memory {
            v.push("PKCE auth state ([security.pkce])");
        }
        if self.rate_limiter_in_memory {
            v.push("rate limiting ([security.rate_limiting])");
        }
        if self.revocation_in_memory {
            v.push("token revocation ([security.token_revocation])");
        }
        v
    }
}

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

    /// Validate that distributed storage is configured when `FRAISEQL_REQUIRE_REDIS` is set.
    ///
    /// When `FRAISEQL_REQUIRE_REDIS=1` is present in the environment, the server refuses
    /// to start if **any** running subsystem holding shared auth state — the PKCE state
    /// store, the rate limiter, or the token revocation manager — is per-process (#874).
    /// The gate used to inspect only the PKCE store, so the operator's explicit
    /// "all shared auth state is distributed" assertion verified one third of the claim:
    /// a logout revoked on one replica stayed accepted by the others, and per-IP limits
    /// ran at N times the configured rate.
    ///
    /// A subsystem that is *not running at all* is not a violation: absent state cannot
    /// diverge between replicas.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` naming every per-process subsystem when the
    /// constraint is violated.
    pub(super) fn check_redis_requirement(backends: &SharedStateBackends) -> crate::Result<()> {
        if std::env::var("FRAISEQL_REQUIRE_REDIS").is_err() {
            return Ok(());
        }
        let violations = backends.per_process_subsystems();
        if violations.is_empty() {
            return Ok(());
        }
        Err(ServerError::ConfigError(format!(
            "FraiseQL failed to start\n\n  FRAISEQL_REQUIRE_REDIS is set but the following \
             subsystems hold per-process state: {}.\n  In a multi-replica deployment that \
             means auth callbacks can fail across replicas, revoked tokens stay accepted by \
             other replicas, and rate limits multiply by the replica count.\n\n  To fix, give \
             each of them a shared backend:\n    [security.pkce]            redis_url = \
             \"redis://…\"\n    [security.rate_limiting]   redis_url = \"redis://…\"\n    \
             [security.token_revocation] backend = \"postgres\" (or redis_url = \"redis://…\")\
             \n\n  To allow per-process state (single-replica only): unset \
             FRAISEQL_REQUIRE_REDIS",
            violations.join(", ")
        )))
    }

    /// Build an `OidcServerClient` from the compiled schema JSON, if `[auth]` is present.
    #[cfg(feature = "auth")]
    pub(super) async fn oidc_server_client_from_schema(
        schema: &CompiledSchema,
    ) -> Option<Arc<crate::auth::OidcServerClient>> {
        // The full schema JSON lives in the executor's compiled schema.
        // Access it via the security Value (which contains the embedded JSON blob).
        // We expose the root schema JSON here.
        let schema_json = serde_json::to_value(schema)
            .inspect_err(|e| warn!(error = %e, "Failed to serialize compiled schema for OIDC client construction"))
            .ok()?;
        // #621: resolves the OIDC discovery document at boot (network I/O), so this
        // is async.
        crate::auth::OidcServerClient::from_compiled_schema(&schema_json).await
    }

    /// Build an `ErrorSanitizer` from the `security.error_sanitization` key in the
    /// compiled schema's security blob (if present).
    ///
    /// When the schema declares no explicit `error_sanitization` config, the default
    /// is **environment-aware** (H7): in production (`FRAISEQL_ENV` not
    /// `development`/`dev`) sanitization is enabled so a default deployment never
    /// leaks raw DB/SQL error text on 5xx; in development it stays disabled for
    /// verbose-error ergonomics. An explicit compiled config overrides either way.
    pub(super) fn error_sanitizer_from_schema(
        schema: &CompiledSchema,
    ) -> Arc<crate::config::error_sanitization::ErrorSanitizer> {
        let compiled = schema.security.as_ref().and_then(|s| s.error_sanitization.clone());
        Arc::new(build_error_sanitizer(compiled, crate::ServerConfig::is_production_mode()))
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

        // #379: `[security] persisted_queries_only = true` is a top-level shorthand that
        // forces the trusted-document store into Strict mode below. It only takes effect
        // when a trusted-documents manifest is configured (there must be persisted
        // operations to allow-list); warn loudly otherwise so the flag never fails silent.
        let persisted_queries_only = security.persisted_queries_only;

        let Some(cfg) = security.trusted_documents.as_ref() else {
            if persisted_queries_only {
                warn!(
                    "security.persisted_queries_only = true but no [security.trusted_documents] \
                     is configured — the flag has no effect. Configure a trusted-documents \
                     manifest (manifest_path or manifest_url) so persisted queries can be \
                     allow-listed."
                );
            }
            return None;
        };

        if !cfg.enabled {
            if persisted_queries_only {
                warn!(
                    "security.persisted_queries_only = true but [security.trusted_documents].enabled \
                     = false — the flag has no effect; enable trusted documents with a manifest."
                );
            }
            return None;
        }

        let mode = effective_trusted_doc_mode(cfg.mode, persisted_queries_only);

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
            // Built once, outside the loop: a fresh `reqwest::Client` per tick
            // discards the connection pool and the TLS session cache, so every
            // poll paid a full handshake (#731).
            let client = reqwest::Client::builder()
                .timeout(MANIFEST_FETCH_TIMEOUT)
                .build()
                .expect("reqwest client with timeout should always build");
            loop {
                ticker.tick().await;

                match client.get(&url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            match read_capped_body(resp, MAX_TRUSTED_DOCS_RESPONSE_BYTES).await {
                                Ok(body_bytes) => {
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

// ── Trusted-documents enforcement mode (#379) ────────────────────────────────

/// Resolve the effective trusted-document enforcement mode.
///
/// `[security] persisted_queries_only = true` forces [`TrustedDocumentMode::Strict`]
/// — reject any operation that is not a persisted/trusted document — regardless of
/// the declared `[security.trusted_documents].mode`. This lets an operator lock the
/// server to persisted operations with a single top-level flag instead of having to
/// also set `mode = "strict"` (#379).
///
/// [`TrustedDocumentMode::Strict`]: crate::trusted_documents::TrustedDocumentMode::Strict
pub(super) const fn effective_trusted_doc_mode(
    declared_mode: fraiseql_core::schema::TrustedDocumentMode,
    persisted_queries_only: bool,
) -> crate::trusted_documents::TrustedDocumentMode {
    if persisted_queries_only
        || matches!(declared_mode, fraiseql_core::schema::TrustedDocumentMode::Strict)
    {
        crate::trusted_documents::TrustedDocumentMode::Strict
    } else {
        crate::trusted_documents::TrustedDocumentMode::Permissive
    }
}

// ── Error-sanitization secure default (H7) ───────────────────────────────────

/// Resolve the `ErrorSanitizer` for a deployment given the (optional) compiled
/// `error_sanitization` config and whether the server is running in production.
///
/// Precedence (H7 secure-by-default):
/// - An **explicit** compiled config always wins, in either environment — the operator opted in or
///   out deliberately.
/// - With **no** compiled config, the default is environment-aware: **enabled in production** (so a
///   default deployment does not render raw DB/SQL error text on 5xx) and **disabled in
///   development** (verbose errors aid local debugging).
///
/// The pure `ErrorSanitizationConfig::default()` (shared with `fraiseql-cli`) is
/// left untouched at `enabled = false`; the secure default lives here, at the
/// server boot seam, so it does not change compile-time/authoring behavior.
pub(super) fn build_error_sanitizer(
    compiled: Option<crate::config::error_sanitization::ErrorSanitizationConfig>,
    is_production: bool,
) -> crate::config::error_sanitization::ErrorSanitizer {
    use crate::config::error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer};
    match compiled {
        Some(cfg) => ErrorSanitizer::new(cfg),
        None if is_production => ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            ..Default::default()
        }),
        None => ErrorSanitizer::disabled(),
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

/// Startup check for an unrestricted `X-Forwarded-For` trust posture (#609/#618).
///
/// When `trust_proxy_headers` is enabled but no CIDR range restricts which direct peers
/// may set `X-Forwarded-For` — the trust-every-proxy-by-omission posture — any client can
/// spoof its IP and bypass per-IP rate limiting (or poison IP-derived logging). In
/// **production** this refuses to boot (`ServerError::ConfigError`); in **development** it
/// downgrades to a warning, matching [`failed_login_lockout_check`].
///
/// Safe configurations return `Ok(())` with no warning: proxy trust off, or a **non-empty**
/// CIDR list — including `["0.0.0.0/0"]`, the sanctioned explicit "trust every proxy" opt-in,
/// a valid CIDR that `extract_real_ip` already treats as trust-all.
///
/// 2.13 shipped this as a deprecation warning promising a 2.14 refuse-to-boot (#618); this
/// is that promotion. Operators who genuinely want trust-all keep it working by writing
/// `trusted_proxy_cidrs = ["0.0.0.0/0"]` explicitly.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when `trust_proxy_headers = true`, the resolved CIDR
/// list is empty, and `is_production` is true.
/// Build a `PkceStateStore` from the compiled schema if `security.pkce.enabled = true`.
///
/// When `redis_url` is set and the `redis-pkce` feature is compiled in, initialises
/// a Redis-backed distributed store.
///
/// `is_production` is a parameter rather than an `env::var` read so the guards can
/// be exercised for both modes without mutating process-global state (mirrors
/// [`resolve_rate_limiter_in`]).
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when:
///
/// * the `pkce` section is present but does not deserialize — a malformed section used to be a
///   warning that silently disabled PKCE, the #778 fail-open;
/// * `pkce.enabled = true` but `[security.state_encryption]` is missing or disabled while
///   `is_production` (#360) — PKCE state tokens would otherwise be sent to the OIDC provider as the
///   raw, unencrypted lookup key;
/// * `pkce.redis_url` is configured but the Redis store cannot be built — the URL is malformed, the
///   connection fails, or the `redis-pkce` feature is not compiled in — while `is_production`
///   (#777). The operator asked for distributed PKCE state; an in-memory store is not a degraded
///   version of that behind a load balancer, it is a broken login flow. Development mode downgrades
///   this to a warning.
#[cfg(feature = "auth")]
#[allow(clippy::cognitive_complexity)] // Reason: conditional backend selection (Redis vs in-memory) with feature-gated branches
pub(super) async fn pkce_store_from_schema_in(
    schema: &CompiledSchema,
    state_encryption: Option<&Arc<crate::auth::state_encryption::StateEncryptionService>>,
    is_production: bool,
) -> crate::Result<Option<Arc<crate::auth::PkceStateStore>>> {
    let Some(security) = schema.security.as_ref() else {
        return Ok(None);
    };
    let Some(cfg) = security.pkce.as_ref() else {
        return Ok(None);
    };
    if !cfg.enabled {
        return Ok(None);
    }

    // SECURITY (#360): PKCE state tokens are sent to the OIDC provider; without
    // [security.state_encryption] they travel as the raw 32-byte lookup key. Refuse
    // to boot in production rather than serve /auth/start with a false "state
    // encryption is enforced" posture; development mode downgrades this to a warning.
    pkce_state_encryption_check(state_encryption.is_some(), is_production)?;

    if matches!(cfg.code_challenge_method, fraiseql_core::schema::CodeChallengeMethod::Plain) {
        warn!(
            "pkce.code_challenge_method = \"plain\" is insecure. \
             Use \"S256\" in all production environments."
        );
    }

    let enc = state_encryption.cloned();

    // Prefer the Redis backend when redis_url is configured and the feature is compiled in.
    #[cfg(feature = "redis-pkce")]
    if let Some(ref url) = cfg.redis_url {
        match crate::auth::PkceStateStore::new_redis(url, cfg.state_ttl_secs, enc.clone()).await {
            Ok(store) => {
                info!(redis_url = %url, "PKCE state store: Redis backend");
                return Ok(Some(Arc::new(store)));
            },
            Err(e) => {
                redis_backend_unavailable_check(
                    "[security.pkce] redis_url",
                    &e.to_string(),
                    PKCE_REDIS_CONSEQUENCE,
                    is_production,
                )?;
            },
        }
    }

    #[cfg(not(feature = "redis-pkce"))]
    if cfg.redis_url.is_some() {
        redis_backend_unavailable_check(
            "[security.pkce] redis_url",
            "the `redis-pkce` Cargo feature is not compiled into this binary",
            PKCE_REDIS_CONSEQUENCE,
            is_production,
        )?;
    }

    if cfg.redis_url.is_none() {
        warn!(
            "PKCE state store: in-memory. In a multi-replica deployment, auth flows will fail \
             if /auth/start and /auth/callback hit different replicas. \
             Set [security.pkce] redis_url to enable the Redis backend, \
             or FRAISEQL_REQUIRE_REDIS=1 to enforce it at startup."
        );
    }

    Ok(Some(Arc::new(crate::auth::PkceStateStore::new(cfg.state_ttl_secs, enc))))
}

/// What breaks when configured-Redis PKCE state silently lands in memory (#777).
#[cfg(feature = "auth")]
const PKCE_REDIS_CONSEQUENCE: &str = "PKCE login state would live only in this process's memory: \
     behind a load balancer, /auth/callback fails with \"state not found\" whenever it lands on a \
     different replica than /auth/start, and a restart drops every in-flight login";

/// Resolve the rate limiter both server constructors use.
///
/// Replaces a block that was duplicated verbatim in `builder.rs` and
/// `extensions.rs` and that carried two defects between them:
///
/// * the boot guards ran only inside `rate_limiter_from_schema`, so a `[rate_limiting]` table in
///   `fraiseql.toml` reached `RateLimiter::new` with no gate at all — `trust_proxy_headers = true`
///   and an empty `trusted_proxy_cidrs` booted happily in production and trusted every peer's
///   `X-Real-IP`, while the identical intent expressed in the compiled schema refused to boot
///   (#837); and
/// * the compiled schema won unconditionally, so the documented CLI > env > config precedence was
///   inverted and `FRAISEQL_RATE_LIMITING_ENABLED=false` could not turn throttling off (#774).
///
/// Resolution order, lowest to highest: server `[rate_limiting]`, compiled
/// schema `[security.rate_limiting]`, then CLI/env overrides — each applied
/// over the last, with the guards run once on whatever comes out.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when the compiled section is malformed, a
/// CIDR does not parse, or a boot guard refuses the effective configuration.
pub(super) async fn resolve_rate_limiter(
    schema: &CompiledSchema,
    config: &crate::ServerConfig,
) -> crate::Result<Option<Arc<RateLimiter>>> {
    resolve_rate_limiter_in(schema, config, crate::ServerConfig::is_production_mode()).await
}

/// [`resolve_rate_limiter`] with the deployment mode passed in.
///
/// `is_production` is a parameter rather than an `env::var` read so the guards can be
/// exercised for both modes without mutating process-global state — the same shape
/// `proxy_trust_check` and `failed_login_lockout_check` already use.
pub(super) async fn resolve_rate_limiter_in(
    schema: &CompiledSchema,
    config: &crate::ServerConfig,
    is_production: bool,
) -> crate::Result<Option<Arc<RateLimiter>>> {
    let schema_sec = rate_limiting_from_schema(schema);
    let overrides = &config.rate_limit_overrides;

    // Base: compiled schema first, then the server's own table. Both are checked, so
    // whichever supplies the values also faces the guards.
    let (mut effective, path_rules_source) = match (&schema_sec, &config.rate_limiting) {
        (Some(sec), _) => (rate_limit_config_checked(sec, is_production)?, Some(sec)),
        (None, Some(server_cfg)) => {
            proxy_trust_check_parsed(
                server_cfg.trust_proxy_headers,
                &server_cfg.trusted_proxy_cidrs,
                is_production,
            )?;
            (server_cfg.clone(), None)
        },
        // Nothing configured anywhere. An override may still switch it on, in which
        // case the defaults apply.
        (None, None) if overrides.enables() => {
            (crate::middleware::RateLimitConfig::default(), None)
        },
        (None, None) => return Ok(None),
    };

    overrides.apply_to(&mut effective);

    if !effective.enabled {
        info!("Rate limiting disabled by configuration");
        return Ok(None);
    }

    // A limiter enabled with a budget of zero denies every request. Nobody configures
    // that on purpose — it is what a producer omitting `requests_per_second` used to
    // yield, since the consumer struct's derived `Default` made the missing key `0`
    // (#893). Refusing is the difference between a boot error naming the key and a
    // deployment that 429s all traffic with a healthy-looking startup log.
    if effective.rps_per_ip == 0 || effective.burst_size == 0 {
        return Err(crate::ServerError::ConfigError(format!(
            "rate limiting is enabled but its budget is zero (rps_per_ip = {}, burst_size = {}), \
             so every request would be rejected. Set a positive requests_per_second and \
             burst_size, or disable rate limiting.",
            effective.rps_per_ip, effective.burst_size
        )));
    }

    // Re-check: an override can turn proxy-header trust on over a base that was
    // clean, and a guard that only ran on the base would miss it.
    proxy_trust_check_parsed(
        effective.trust_proxy_headers,
        &effective.trusted_proxy_cidrs,
        is_production,
    )?;

    let limiter = build_rate_limiter(effective, path_rules_source, is_production).await?;
    Ok(Some(Arc::new(limiter)))
}

/// Construct the limiter, choosing the Redis or in-memory backend.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when `redis_url` is configured but the Redis
/// backend cannot be built and `is_production` is true (#770/#777 class): the
/// operator asked for one shared budget across replicas, and an in-memory fallback
/// enforces N times the configured rate while the startup log reads healthy.
async fn build_rate_limiter(
    config: crate::middleware::RateLimitConfig,
    sec: Option<&crate::middleware::RateLimitingSecurityConfig>,
    is_production: bool,
) -> crate::Result<RateLimiter> {
    let with_rules = |limiter: RateLimiter| match sec {
        Some(sec) => limiter.with_path_rules_from_security(sec),
        None => limiter,
    };

    let redis_url = sec.and_then(|s| s.redis_url.as_deref());

    #[cfg(feature = "redis-rate-limiting")]
    if let Some(url) = redis_url {
        match RateLimiter::new_redis(url, config.clone()).await {
            Ok(rl) => {
                info!(
                    url,
                    rps_per_ip = config.rps_per_ip,
                    burst_size = config.burst_size,
                    "Rate limiting: using Redis distributed backend"
                );
                return Ok(with_rules(rl));
            },
            Err(e) => {
                redis_backend_unavailable_check(
                    "[security.rate_limiting] redis_url",
                    &e.to_string(),
                    RATE_LIMIT_REDIS_CONSEQUENCE,
                    is_production,
                )?;
            },
        }
    }

    #[cfg(not(feature = "redis-rate-limiting"))]
    if redis_url.is_some() {
        redis_backend_unavailable_check(
            "[security.rate_limiting] redis_url",
            "the `redis-rate-limiting` Cargo feature is not compiled into this binary",
            RATE_LIMIT_REDIS_CONSEQUENCE,
            is_production,
        )?;
    }

    info!(
        rps_per_ip = config.rps_per_ip,
        burst_size = config.burst_size,
        "Rate limiting: using in-memory backend"
    );
    Ok(with_rules(RateLimiter::new(config)))
}

/// What breaks when configured-Redis rate limiting silently lands in memory.
const RATE_LIMIT_REDIS_CONSEQUENCE: &str = "rate-limit budgets would be tracked per process, so a \
     deployment of N replicas enforces N times the configured rate while every replica's \
     startup log reads healthy";

/// Read `security.rate_limiting` out of the compiled schema.
///
/// The section is a typed field on `SecurityConfig` (#977), so a malformed or
/// misspelled section is a **load** error before this function is reached — the
/// history matters: an earlier string lookup ended in `.ok()`, which turned any
/// type mismatch into `None`, indistinguishable from "no rate limiting
/// configured", so the server booted with throttling silently off *and* skipped
/// the `#609`/`#618` proxy-trust and `#356` failed-login boot guards that live
/// behind this parse (#778).
fn rate_limiting_from_schema(
    schema: &CompiledSchema,
) -> Option<crate::middleware::RateLimitingSecurityConfig> {
    schema.security.as_ref().and_then(|s| s.rate_limiting.clone())
}

/// Lower a security-block rate-limit section onto a `RateLimitConfig`, running every
/// boot guard that applies to it.
///
/// The single place the `#356` failed-login and `#609`/`#618` proxy-trust checks run,
/// so that a config reaching the limiter by any route faces the same gate.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when a CIDR does not parse, when
/// `trust_proxy_headers` is on without a trusted-proxy list in production, or when
/// the `failed_login_*` settings are tuned away from their defaults in production.
fn rate_limit_config_checked(
    sec: &crate::middleware::RateLimitingSecurityConfig,
    is_production: bool,
) -> crate::Result<crate::middleware::RateLimitConfig> {
    // SECURITY (#356): the binary performs no first-factor login, so it cannot
    // honour failed_login_max_attempts / failed_login_lockout_secs. Refuse to
    // boot in production when an operator has tuned them away from the defaults
    // (development downgrades to a warning).
    failed_login_lockout_check(
        sec.failed_login_max_attempts,
        sec.failed_login_lockout_secs,
        is_production,
    )?;

    // Parse before checking. `proxy_trust_check` reads the *string* list, so an
    // unparseable entry would make it look non-empty while the parsed list the
    // middleware actually consults came out empty — trusting every peer.
    let config = crate::middleware::RateLimitConfig::try_from_security_config(sec)
        .map_err(crate::ServerError::ConfigError)?;

    // Refuse to boot in production when trust_proxy_headers is enabled without
    // restricting which IPs are trusted proxies — any client could then spoof
    // X-Forwarded-For and bypass per-IP rate limits (#609/#618). Explicit
    // ["0.0.0.0/0"] opts into trust-all deliberately; development downgrades to a warning.
    proxy_trust_check_parsed(
        config.trust_proxy_headers,
        &config.trusted_proxy_cidrs,
        is_production,
    )?;

    Ok(config)
}

/// [`proxy_trust_check`] over the parsed CIDR list the middleware will actually use.
pub(super) fn proxy_trust_check_parsed(
    trust_proxy_headers: bool,
    trusted_proxy_cidrs: &[ipnet::IpNet],
    is_production: bool,
) -> crate::Result<()> {
    let as_strings: Vec<String> = trusted_proxy_cidrs.iter().map(ToString::to_string).collect();
    proxy_trust_check(trust_proxy_headers, Some(&as_strings), is_production)
}

pub(super) fn proxy_trust_check(
    trust_proxy_headers: bool,
    trusted_proxy_cidrs: Option<&[String]>,
    is_production: bool,
) -> crate::Result<()> {
    let trust_all_by_omission =
        trust_proxy_headers && trusted_proxy_cidrs.is_none_or(<[String]>::is_empty);
    if !trust_all_by_omission {
        return Ok(());
    }
    if is_production {
        return Err(crate::ServerError::ConfigError(
            concat!(
                "FraiseQL failed to start\n\n",
                "  [security.rate_limiting] trust_proxy_headers = true, but trusted_proxy_cidrs\n",
                "  is empty or unset. Every direct peer would be trusted to set X-Forwarded-For,\n",
                "  so any client could spoof its IP and bypass per-IP rate limiting (and poison\n",
                "  IP-derived logging).\n\n",
                "  Restrict which peers are trusted proxies:\n",
                "    trusted_proxy_cidrs = [\"10.0.0.0/8\"]   # your load balancer / proxy ranges\n\n",
                "  Or, to keep trusting every proxy on purpose, opt in explicitly:\n",
                "    trusted_proxy_cidrs = [\"0.0.0.0/0\"]\n\n",
                "  For local development only:\n",
                "    Set FRAISEQL_ENV=development to downgrade this to a warning.",
            )
            .into(),
        ));
    }
    warn!(
        "[security.rate_limiting] trust_proxy_headers = true but trusted_proxy_cidrs is empty. \
         Any client can spoof X-Forwarded-For and bypass per-IP rate limits. Allowed only \
         because FRAISEQL_ENV=development; set trusted_proxy_cidrs to your proxy ranges (e.g. \
         [\"10.0.0.0/8\"]), or [\"0.0.0.0/0\"] to keep trusting every proxy explicitly."
    );
    Ok(())
}

// ── Configured Redis backend unavailable (#770 / #777) ───────────────────────

/// Refuse a configured-but-unavailable Redis backend instead of silently
/// downgrading to in-memory state.
///
/// An operator who configured a Redis URL asked for state shared across
/// replicas. Whatever the subsystem — PKCE login state, rate-limit budgets,
/// token revocation — a per-process fallback is not a degraded version of that
/// service; it is a silently absent one wearing a healthy startup log. The only
/// sanctioned fallback is an explicit one: remove the Redis configuration, or
/// declare a development environment.
///
/// In production this is a hard error so the server refuses to boot; in
/// development (`FRAISEQL_ENV=development`/`dev`) it is downgraded to a warning
/// so local runs still come up. Pure and race-free like its siblings
/// ([`pkce_state_encryption_check`], [`observer_transport_check`]): the caller
/// passes the deployment mode, the config key it read, the failure cause, and
/// the subsystem-specific consequence line.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when `is_production` is true.
pub fn redis_backend_unavailable_check(
    config_key: &str,
    cause: &str,
    consequence: &str,
    is_production: bool,
) -> crate::Result<()> {
    if is_production {
        return Err(crate::ServerError::ConfigError(format!(
            "FraiseQL failed to start\n\n  \
             {config_key} is configured but the Redis backend is unavailable: {cause}.\n\n  \
             Falling back to in-memory would silently disable what the configuration \
             promises:\n  {consequence}.\n\n  \
             To fix, choose one:\n    \
             - make Redis reachable at the configured URL (and build with the matching \
             Cargo feature)\n    \
             - remove the Redis URL from {config_key} to accept per-process, \
             single-replica state\n\n  \
             For local development only:\n    \
             Set FRAISEQL_ENV=development to downgrade this to a warning."
        )));
    }
    warn!(
        config_key,
        cause,
        "configured Redis backend is unavailable — falling back to in-memory. {consequence}. \
         Allowed only because FRAISEQL_ENV=development."
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

/// Read a response body, refusing as soon as it exceeds `max_bytes`.
///
/// `Response::bytes()` buffers the **whole** body and only then can the caller
/// check its size, so a hostile or misbehaving manifest server could make the
/// server allocate arbitrarily much before the 10 `MiB` cap was consulted (#731).
/// Streaming the chunks and bailing at the ceiling makes the cap an actual limit
/// on memory rather than a post-hoc verdict — the `Content-Length` header is only
/// an advisory pre-check because a chunked response has none.
async fn read_capped_body(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if let Some(declared) = response.content_length() {
        if declared > max_bytes as u64 {
            return Err(format!("response declares {declared} bytes, max {max_bytes}"));
        }
    }
    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > max_bytes {
            return Err(format!("response exceeds {max_bytes} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

// ── SSRF guard for manifest hot-reload URL ────────────────────────────────────

/// Returns `true` when `url` resolves to a private, loopback, or link-local
/// address that the server must not fetch (SSRF protection).
///
/// The address ranges are [`fraiseql_guard::net`]'s, shared with every other
/// outbound guard. The copy this replaced claimed to use "the same pattern as the
/// federation and Vault SSRF guards" and had in fact drifted from both: it missed
/// CGNAT, `0.0.0.0/8`, `IPv6` link-local and every `IPv4`-mapped form, so
/// `http://[::ffff:169.254.169.254]/manifest.json` was accepted.
pub(super) fn is_manifest_url_ssrf_blocked(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        // Unparseable URL — block it; the actual fetch would fail anyway.
        return true;
    };
    let Some(host) = parsed.host_str() else {
        return true;
    };
    fraiseql_guard::net::blocked_host_reason(host).is_some()
}

/// Refuse to boot when a multi-tenant schema cannot isolate its tenants.
///
/// This is the static half of the gate — declarations only, no database access.
/// Multi-tenant + caching + no RLS declaration is a hard refusal: cache keys carry
/// no tenant, so isolation rests entirely on RLS producing different WHERE clauses
/// per caller. Without it, two tenants issuing the same query share one cache entry.
///
/// It lives here, and every constructor calls it, because it previously existed
/// twice inline (`Server::new` and `with_relay_pagination`) and **not at all** in
/// `with_flight_service`. That drift was invisible while the gate was dead: nothing
/// could set `security.multi_tenant`, so `is_multi_tenant()` was universally false
/// and all three constructors behaved identically (#758). Giving the flag a producer
/// makes the drift live, so the duplication had to go with it.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when the schema is multi-tenant, caching is
/// on, and no RLS is declared.
pub fn tenant_isolation_declaration_check(
    schema: &CompiledSchema,
    cache_enabled: bool,
) -> crate::Result<()> {
    if !cache_enabled || schema.has_rls_configured() {
        return Ok(());
    }

    if schema.is_multi_tenant() {
        return Err(crate::ServerError::ConfigError(format!(
            "Cache is enabled for a multi-tenant schema (tenancy.mode = {}) but no \
             Row-Level Security is declared. Cache keys do not carry a tenant, so two \
             tenants issuing the same query would share one cached response. In \
             fraiseql.toml either declare `[security.rls] enabled = true` (and define the \
             policies in the database), disable caching with `cache_enabled = false` in the \
             server configuration, or \
             set `[security] multi_tenant = false` with `[tenancy] mode = \"none\"` to \
             acknowledge single-tenant mode.",
            schema.tenancy_mode()
        )));
    }

    // Single-tenant with cache and no RLS: safe, but warn in case of misconfiguration.
    warn!(
        "Query-result caching is enabled but no Row-Level Security is declared in the \
         compiled schema. This is safe for single-tenant deployments. For multi-tenant \
         deployments, declare `[security.rls] enabled = true` and set `[security] \
         multi_tenant = true`."
    );
    Ok(())
}

/// Verify a declared RLS posture against the live database, and refuse to boot when
/// the declaration is not true.
///
/// A declaration in the compiled schema is only an operator's claim — FraiseQL does
/// not author RLS policies. This turns the claim into something checkable:
/// [`CachedDatabaseAdapter::validate_rls_active`] reads the catalog and reports every
/// source relation that is not actually protected.
///
/// Only runs when the schema is multi-tenant *and* declares RLS; a schema that
/// declares nothing has already been handled by
/// [`tenant_isolation_declaration_check`].
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when `enforcement` is
/// [`RlsEnforcement::Error`] and any source relation is unprotected.
pub(super) async fn verify_declared_rls<A: DatabaseAdapter>(
    schema: &CompiledSchema,
    cached: &fraiseql_core::cache::CachedDatabaseAdapter<A>,
    enforcement: fraiseql_core::cache::RlsEnforcement,
) -> crate::Result<()> {
    if !schema.is_multi_tenant() || !schema.has_rls_configured() {
        return Ok(());
    }
    cached
        .enforce_rls(schema, enforcement)
        .await
        .map_err(|e| crate::ServerError::ConfigError(e.to_string()))
}

/// Warn when the compiled schema declares per-query cache TTLs but the server's
/// result cache is off (#623): every `cache_ttl_seconds` — SDK-authored or
/// lowered from `[[caching.rules]]` — is silently inert then, and an operator
/// who wrote cache rules believes caching is active.
pub(super) fn warn_on_inert_cache_ttls(schema: &CompiledSchema, cache_enabled: bool) {
    if cache_enabled {
        return;
    }
    let declared: Vec<&str> = schema
        .queries
        .iter()
        .filter(|q| q.cache_ttl_seconds.is_some())
        .map(|q| q.name.as_str())
        .collect();
    if !declared.is_empty() {
        warn!(
            queries = ?declared,
            "The compiled schema declares cache TTLs for {} query/queries, but the server's \
             result cache is disabled (`cache_enabled = false`) — the TTLs (and any \
             [[caching.rules]] they came from) have no effect. Enable `cache_enabled` or \
             remove the declarations.",
            declared.len()
        );
    }
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
pub fn field_encryption_unsupported_check(schema: &CompiledSchema) -> crate::Result<()> {
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
