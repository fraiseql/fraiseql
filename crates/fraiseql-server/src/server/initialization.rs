//! Private helper functions for reading security/config from the compiled schema
//! and building subsystem objects during server construction.

use super::*;

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
                let s_val = serde_json::to_value(s)
                    .map_err(|e| ServerError::ConfigError(
                        format!("Failed to serialize security config: {e}")
                    ))?;
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
    #[cfg(feature = "auth")]
    pub(super) async fn pkce_store_from_schema(
        schema: &CompiledSchema,
        state_encryption: Option<&Arc<crate::auth::state_encryption::StateEncryptionService>>,
    ) -> Option<Arc<crate::auth::PkceStateStore>> {
        let security = schema.security.as_ref()?;
        let pkce_cfg = security.additional.get("pkce")?;

        #[derive(serde::Deserialize)]
        struct PkceCfgMinimal {
            #[serde(default)]
            enabled:        bool,
            #[serde(default = "default_ttl")]
            state_ttl_secs: u64,
            #[serde(default = "default_method")]
            code_challenge_method: String,
            redis_url: Option<String>,
        }
        const fn default_ttl()    -> u64    { 600 }
        fn default_method() -> String { "S256".into() }

        let cfg: PkceCfgMinimal = serde_json::from_value(pkce_cfg.clone())
            .inspect_err(|e| warn!(error = %e, "Failed to deserialize pkce config — disabling PKCE"))
            .ok()?;
        if !cfg.enabled {
            return None;
        }

        if state_encryption.is_none() {
            warn!(
                "pkce.enabled = true but state_encryption is disabled. \
                 PKCE state tokens are sent to the OIDC provider unencrypted. \
                 Enable [security.state_encryption] in production for full protection."
            );
        }

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
            match crate::auth::PkceStateStore::new_redis(url, cfg.state_ttl_secs, enc.clone())
                .await
            {
                Ok(store) => {
                    info!(redis_url = %url, "PKCE state store: Redis backend");
                    return Some(Arc::new(store));
                }
                Err(e) => {
                    error!(
                        error = %e,
                        redis_url = %url,
                        "Failed to connect to Redis PKCE store — falling back to in-memory"
                    );
                }
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

        Some(Arc::new(crate::auth::PkceStateStore::new(cfg.state_ttl_secs, enc)))
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
    pub(super) async fn rate_limiter_from_schema(schema: &CompiledSchema) -> Option<Arc<RateLimiter>> {
        let sec: crate::middleware::RateLimitingSecurityConfig = schema
            .security
            .as_ref()
            .and_then(|s| s.additional.get("rate_limiting"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())?;

        if !sec.enabled {
            return None;
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
                        error!(
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

        Some(Arc::new(limiter))
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
            .map(crate::config::error_sanitization::ErrorSanitizer::new)
            .unwrap_or_else(crate::config::error_sanitization::ErrorSanitizer::disabled);
        Arc::new(sanitizer)
    }

    /// Build a `TrustedDocumentStore` from `security.trusted_documents` in the
    /// compiled schema, if present and `enabled = true`.
    pub(super) fn trusted_docs_from_schema(
        schema: &CompiledSchema,
    ) -> Option<Arc<crate::trusted_documents::TrustedDocumentStore>> {
        let security = schema.security.as_ref()?;
        let td_cfg = security.additional.get("trusted_documents")?;

        #[derive(serde::Deserialize)]
        struct TdCfgMinimal {
            #[serde(default)]
            enabled: bool,
            #[serde(default)]
            mode: String,
            manifest_path: Option<String>,
            // Reason: serde deserialization target — `manifest_url` is a valid config field
            // used for hot-reload path detection; this minimal struct only reads `manifest_path`.
            #[allow(dead_code)]
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
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to load trusted documents manifest");
                    None
                }
            }
        } else {
            warn!("trusted_documents.enabled = true but no manifest_path or manifest_url set");
            None
        }
    }

    /// Spawn a background task that periodically re-fetches the manifest from a URL.
    pub(super) fn spawn_trusted_docs_reload(
        store: Arc<crate::trusted_documents::TrustedDocumentStore>,
        url: String,
        interval_secs: u64,
    ) {
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                const MANIFEST_FETCH_TIMEOUT: std::time::Duration =
                    std::time::Duration::from_secs(30);
                let client = reqwest::Client::builder()
                    .timeout(MANIFEST_FETCH_TIMEOUT)
                    .build()
                    .expect("reqwest client with timeout should always build");
                match client.get(&url).send().await {
                    Ok(resp) => match resp.text().await {
                        Ok(body) => {
                            #[derive(serde::Deserialize)]
                            struct Manifest {
                                documents: std::collections::HashMap<String, String>,
                            }
                            match serde_json::from_str::<Manifest>(&body) {
                                Ok(manifest) => {
                                    let count = manifest.documents.len();
                                    store.replace_documents(manifest.documents).await;
                                    info!(
                                        count,
                                        "Trusted documents manifest reloaded"
                                    );
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to parse trusted documents manifest");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to read trusted documents manifest response");
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, "Failed to fetch trusted documents manifest");
                    }
                }
            }
        });
    }
}
