use fraiseql_core::db::postgres::PostgresTlsConfig;

use super::{DatabaseTlsConfig, ServerConfig};

/// `ServerConfig` sections that only exist when their build feature is compiled in.
///
/// With `deny_unknown_fields` (#839), a binary built without a feature refuses a
/// config that declares the feature's section. serde's "unknown field" message
/// cannot say *why* the field is unknown, so [`ServerConfig::from_file`] appends
/// the build-feature hint for any of these keys found in the raw TOML.
const FEATURE_GATED_SECTIONS: &[(&str, &str, bool)] = &[
    ("flight_bind_addr", "arrow", cfg!(feature = "arrow")),
    ("observers", "observers", cfg!(feature = "observers")),
    ("sources", "sources", cfg!(feature = "sources")),
    ("webhooks", "inbound", cfg!(feature = "inbound")),
    ("mailbox", "inbound-email", cfg!(feature = "inbound-email")),
    ("send", "inbound-email", cfg!(feature = "inbound-email")),
    ("export", "rest", cfg!(feature = "rest")),
    ("identity", "auth", cfg!(feature = "auth")),
    ("saml", "auth-saml", cfg!(feature = "auth-saml")),
];

/// Grouping tables `ServerConfig` has never had, mapped to the keys that do the job.
///
/// These are the sections of the deleted `fraiseql_core::config::FraiseQLConfig`
/// (#909): a parallel config tree that parsed a whole `[server]`/`[database]`/`[cache]`
/// file and that nothing outside its own module ever read. `deny_unknown_fields`
/// already refuses them, but serde's "unknown field `cache`, expected one of …"
/// followed by a hundred names does not tell an operator that `response_cache_enabled`
/// is spelled `cache_enabled` and lives at the top level. Naming the working knob is
/// the difference between a refusal and a usable one.
const REMOVED_SECTIONS: &[(&str, &str)] = &[
    ("server", "`bind_addr`, `max_request_body_bytes`"),
    (
        "database",
        "`database_url`, `pool_max_size`, `pool_min_size`, `pool_timeout_secs`, `[database_tls]`",
    ),
    ("cors", "`cors_enabled`, `cors_origins`"),
    (
        "rate_limit",
        "`[rate_limiting]`, and the compiled schema's `security.rate_limiting`",
    ),
    (
        "cache",
        "`cache_enabled` (query result cache), `apq_enabled` (persisted queries)",
    ),
    ("collation", "nothing — locale-aware collation was never wired to a config key"),
];

/// Append a build-feature hint to a config parse error for every compiled-out
/// section the raw TOML declares.
///
/// Takes the section table as a parameter so the mechanism is testable under any
/// feature set — a test gated on `cfg(not(feature = …))` would silently never run
/// in the all-features CI leg.
pub(super) fn enrich_parse_error(
    sections: &[(&str, &str, bool)],
    content: &str,
    mut msg: String,
) -> String {
    use std::fmt::Write as _;
    if let Ok(table) = toml::from_str::<toml::Table>(content) {
        for (key, feature, compiled_in) in sections {
            if !compiled_in && table.contains_key(*key) {
                // Infallible on String; the write! form avoids format!'s extra allocation.
                let _ = write!(
                    msg,
                    "\nnote: `{key}` requires a binary built with the `{feature}` feature; \
                     this build compiled it out, so the key is unknown here. Rebuild with \
                     `--features {feature}` or remove the section."
                );
            }
        }
        for (key, replacement) in REMOVED_SECTIONS {
            if table.contains_key(*key) {
                let _ = write!(
                    msg,
                    "\nnote: there is no `[{key}]` table in a server config file; the keys \
                     are top-level. Use {replacement}."
                );
            }
        }
    }
    msg
}

impl ServerConfig {
    /// Load server configuration from a TOML file.
    ///
    /// The file is deserialized directly into `ServerConfig` — keys are top-level,
    /// and an unknown key is a hard error rather than a silent drop (#839). When
    /// the unknown key is a section this build compiled out (e.g. `[observers]`
    /// without the `observers` feature), the error names the missing build feature;
    /// when it is one of the grouping tables this file has never had (`[cache]`,
    /// `[database]`, …), it names the keys that do the job (#909).
    ///
    /// # Errors
    ///
    /// Returns an error string if the file cannot be read or the TOML cannot be parsed.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Cannot read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| {
            enrich_parse_error(
                FEATURE_GATED_SECTIONS,
                &content,
                format!("Invalid TOML config: {e}"),
            )
        })
    }

    /// Check if running in production mode.
    ///
    /// Delegates to [`fraiseql_guard::deployment::is_production`], the workspace's
    /// single production detector. Behaviour is unchanged for this crate — unset
    /// or unrecognised `FRAISEQL_ENV` means production — but `fraiseql-observers`
    /// used to answer the opposite for the same variable, which is what let an
    /// SSRF bypass survive into production deployments (#836).
    #[must_use]
    pub fn is_production_mode() -> bool {
        fraiseql_guard::deployment::is_production()
    }

    /// Transport security for the PostgreSQL connection pool.
    ///
    /// The single reader of `[database_tls]`. Absent the section, this is libpq's
    /// `prefer`: negotiate TLS when the server offers it, fall back to cleartext
    /// otherwise — the behaviour a deployment that terminates TLS at a proxy
    /// already relies on.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message when the section names an ssl mode the
    /// connector cannot honour, contradicts itself, or still sets one of the
    /// removed URL-scheme switches.
    pub fn postgres_tls(&self) -> Result<PostgresTlsConfig, String> {
        self.database_tls
            .as_ref()
            .map_or_else(|| Ok(PostgresTlsConfig::default()), DatabaseTlsConfig::postgres_tls)
    }

    /// The replica **routing policy** this server applies — the pin window, the
    /// staleness budget and the probe cadence, without any URLs (#957).
    ///
    /// Split out from [`read_replicas`](Self::read_replicas) because tenant pools
    /// carry their own replica topology but must not carry their own policy: the
    /// tenant executor factory stamps this onto every tenant pool, the way it
    /// already stamps `[database_tls]` (#801).
    ///
    /// The defaults themselves live on
    /// [`ReadReplicaPolicy::default`](fraiseql_core::db::postgres::ReadReplicaPolicy) —
    /// one seam, so the server pool and every tenant pool cannot drift on them.
    #[must_use]
    pub fn read_replica_policy(&self) -> fraiseql_core::db::postgres::ReadReplicaPolicy {
        let defaults = fraiseql_core::db::postgres::ReadReplicaPolicy::default();
        fraiseql_core::db::postgres::ReadReplicaPolicy {
            pin_after_write:       self
                .read_replica_pin_after_write_ms
                .map_or(defaults.pin_after_write, std::time::Duration::from_millis),
            max_lag:               self
                .read_replica_max_lag_ms
                .map(std::time::Duration::from_millis),
            health_probe_interval: self
                .read_replica_health_probe_interval_ms
                .map_or(defaults.health_probe_interval, std::time::Duration::from_millis),
        }
    }

    /// Read-replica configuration for the primary pool set, lowered onto the
    /// `fraiseql-db` type both binaries hand to `PostgresAdapter::with_pool_config`
    /// (#407). `None` when no replicas are configured.
    #[must_use]
    pub fn read_replicas(&self) -> Option<fraiseql_core::db::postgres::ReadReplicaConfig> {
        self.read_replica_policy().with_urls(self.read_replica_urls.clone())
    }

    /// Effective `@stream` continuation batch size (#387).
    ///
    /// The 100-row default lives here — the single seam — so every consumer
    /// agrees on it.
    #[must_use]
    pub fn graphql_incremental_batch_size(&self) -> u32 {
        self.graphql_incremental_batch_size.unwrap_or(100)
    }

    /// Refuse a read-replica configuration on a wire-backend build (#407).
    ///
    /// The wire backend has no replica routing, so accepting the config would
    /// serve every read from the primary while the operator believes reads are
    /// offloaded. Called by the binary's wire adapter construction; a
    /// wire-backend server with replicas configured must refuse to boot.
    ///
    /// # Errors
    ///
    /// Returns a human-readable refusal when `read_replica_urls` is non-empty.
    #[cfg(feature = "wire-backend")]
    pub fn wire_backend_rejects_read_replicas(&self) -> Result<(), String> {
        if self.read_replica_urls.is_empty() {
            Ok(())
        } else {
            Err("read_replica_urls is configured, but this binary was built with the \
                 `wire-backend` feature, which does not support read replicas. Remove the \
                 replica configuration or use the standard PostgreSQL backend."
                .to_string())
        }
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `metrics_enabled` is true but `metrics_token` is not set
    /// - `[database_tls]` cannot be lowered onto a working connector
    /// - `metrics_token` is set but too short (< 16 characters)
    /// - `auth` config is set but invalid (e.g., empty issuer)
    /// - `tls` is enabled but cert or key path is missing
    /// - TLS minimum version is invalid
    /// - In production mode: `playground_enabled` is true
    /// - In production mode: `cors_enabled` is true but `cors_origins` is empty
    pub fn validate(&self) -> Result<(), String> {
        #[cfg(feature = "auth")]
        if let Some(ref ss) = self.session_state {
            ss.validate()?;
        }

        if let Some(ref ao) = self.async_operations {
            ao.validate()?;
        }

        if self.metrics_enabled {
            match &self.metrics_token {
                None => {
                    return Err("metrics_enabled is true but metrics_token is not set. \
                         Set FRAISEQL_METRICS_TOKEN or metrics_token in config."
                        .to_string());
                },
                Some(token) if token.len() < 16 => {
                    return Err(
                        "metrics_token must be at least 16 characters for security.".to_string()
                    );
                },
                Some(_) => {},
            }
        }

        // Admin API validation
        if self.admin_api_enabled {
            match &self.admin_token {
                None => {
                    return Err("admin_api_enabled is true but admin_token is not set. \
                         Set FRAISEQL_ADMIN_TOKEN or admin_token in config."
                        .to_string());
                },
                Some(token) if token.len() < 32 => {
                    return Err(
                        "admin_token must be at least 32 characters for security.".to_string()
                    );
                },
                Some(_) => {},
            }

            // Validate the optional read-only token when provided.
            if let Some(ref ro_token) = self.admin_readonly_token {
                if ro_token.len() < 32 {
                    return Err(
                        "admin_readonly_token must be at least 32 characters for security."
                            .to_string(),
                    );
                }
                if Some(ro_token) == self.admin_token.as_ref() {
                    return Err("admin_readonly_token must differ from admin_token.".to_string());
                }
            }
        }

        // Incremental delivery (#387, #958): refuse inert or degenerate shapes loudly.
        match self.graphql_incremental_batch_size {
            Some(_) if !self.enable_graphql_incremental => {
                return Err("graphql_incremental_batch_size is set but \
                     enable_graphql_incremental is false — the batch size only applies to \
                     an incremental delivery. Enable enable_graphql_incremental or remove \
                     the setting."
                    .to_string());
            },
            Some(0) => {
                return Err("graphql_incremental_batch_size must be at least 1.".to_string());
            },
            _ => {},
        }

        // Read replicas (#407): refuse inert or malformed shapes loudly.
        if self.read_replica_pin_after_write_ms.is_some() && self.read_replica_urls.is_empty() {
            return Err("read_replica_pin_after_write_ms is set but read_replica_urls is \
                 empty — the pin window only applies to replica routing. Configure \
                 read_replica_urls or remove the pin setting."
                .to_string());
        }
        if self.read_replica_urls.iter().any(|u| u.trim().is_empty()) {
            return Err("read_replica_urls contains an empty URL.".to_string());
        }
        // Bounded staleness (#957): same rule — a lag budget or a probe cadence
        // without replicas describes routing that does not happen.
        if self.read_replica_max_lag_ms.is_some() && self.read_replica_urls.is_empty() {
            return Err("read_replica_max_lag_ms is set but read_replica_urls is empty — a \
                 staleness budget only applies to replica routing. Configure \
                 read_replica_urls or remove the budget."
                .to_string());
        }
        if self.read_replica_health_probe_interval_ms.is_some() && self.read_replica_urls.is_empty()
        {
            return Err("read_replica_health_probe_interval_ms is set but read_replica_urls \
                 is empty — there is nothing to probe. Configure read_replica_urls or remove \
                 the interval."
                .to_string());
        }
        // The adapter refuses these too, at pool construction. Refusing here as
        // well is what makes it a *configuration* error the operator sees from
        // `validate()` — including on the wire-backend build, which never reaches
        // `PostgresAdapter::with_pool_config` at all.
        if self.read_replica_health_probe_interval_ms == Some(0) {
            return Err("read_replica_health_probe_interval_ms is 0 — probing every replica \
                 in a tight loop is never intended. Set a positive interval or remove it."
                .to_string());
        }
        if let Some(max_lag) = self.read_replica_max_lag_ms {
            let probe = self.read_replica_health_probe_interval_ms.unwrap_or(1000);
            if max_lag <= probe {
                return Err(format!(
                    "read_replica_max_lag_ms ({max_lag}) must be greater than \
                     read_replica_health_probe_interval_ms ({probe}): a replica's eligibility \
                     ages its last probe, so a staleness budget no larger than the probe \
                     period would drop even a fully caught-up replica out of rotation for \
                     part of every cycle. Lower the probe interval or raise the budget."
                ));
            }
        }

        // Validate OIDC config if present
        if let Some(ref auth) = self.auth {
            auth.validate().map_err(|e| e.to_string())?;
        }

        // OIDC and HS256 are mutually exclusive.
        if self.auth.is_some() && self.auth_hs256.is_some() {
            return Err("Both [auth] (OIDC) and [auth_hs256] are configured. Pick one — \
                 HS256 is intended for integration testing and internal services; \
                 OIDC is intended for public-facing production."
                .to_string());
        }

        // Validate HS256 config if present: the secret env var must be set.
        if let Some(ref hs) = self.auth_hs256 {
            if hs.secret_env.trim().is_empty() {
                return Err("auth_hs256.secret_env must not be empty".to_string());
            }
            hs.load_secret()?;
        }

        // Validate TLS config if present and enabled
        if let Some(ref tls) = self.tls {
            if tls.enabled {
                if !tls.cert_path.exists() {
                    return Err(format!(
                        "TLS enabled but certificate file not found: {}",
                        tls.cert_path.display()
                    ));
                }
                if !tls.key_path.exists() {
                    return Err(format!(
                        "TLS enabled but key file not found: {}",
                        tls.key_path.display()
                    ));
                }

                // Validate TLS version
                if !["1.2", "1.3"].contains(&tls.min_version.as_str()) {
                    return Err("TLS min_version must be '1.2' or '1.3'".to_string());
                }

                // Validate mTLS config if required
                if tls.require_client_cert {
                    if let Some(ref ca_path) = tls.client_ca_path {
                        if !ca_path.exists() {
                            return Err(format!("Client CA file not found: {}", ca_path.display()));
                        }
                    } else {
                        return Err(
                            "require_client_cert is true but client_ca_path is not set".to_string()
                        );
                    }
                }
            }
        }

        // Pool invariants
        if self.pool_max_size == 0 {
            return Err("pool_max_size must be at least 1".to_string());
        }
        if self.pool_min_size > self.pool_max_size {
            return Err(format!(
                "pool_min_size ({}) must not exceed pool_max_size ({})",
                self.pool_min_size, self.pool_max_size
            ));
        }
        if self.pool_timeout_secs == 0 {
            return Err("pool_timeout_secs must be > 0. A zero-second timeout would cause every \
                 connection acquisition to fail immediately. Use a positive value (e.g. 30) \
                 or remove the field to use the default (30s)."
                .to_string());
        }

        // Validate database TLS config by *building* it, rather than by checking the
        // ssl mode against a second, hand-maintained allow-list. The old list accepted
        // `allow` and `verify-ca`, which the connector cannot honour, so validation
        // passing said nothing about whether the setting could take effect (#801).
        self.postgres_tls()?;
        if let Some(ref db_tls) = self.database_tls {
            if let Some(ref ca_path) = db_tls.ca_bundle_path {
                if !ca_path.exists() {
                    return Err(format!("CA bundle file not found: {}", ca_path.display()));
                }
            }
        }

        // Rate limiting sanity check
        if let Some(ref rl) = self.rate_limiting {
            if rl.rps_per_ip > 0 && rl.rps_per_user > 0 && rl.rps_per_ip > rl.rps_per_user {
                tracing::warn!(
                    rps_per_ip = rl.rps_per_ip,
                    rps_per_user = rl.rps_per_user,
                    "rps_per_ip exceeds rps_per_user — authenticated users are more \
                     restricted than anonymous IPs"
                );
            }
        }

        // Production safety validation
        #[cfg(feature = "auth-saml")]
        if let Some(ref saml) = self.saml {
            saml.validate()?;
            if self.auth_hs256.is_none() {
                return Err("[saml] requires [auth_hs256]: a verified assertion mints a \
                            session token that this server must itself be able to \
                            validate, and HS256 is the self-contained signing path. \
                            Configure [auth_hs256] (secret_env, issuer, audience) or \
                            remove [saml]."
                    .to_string());
            }
        }

        if let Some(ref scim) = self.scim {
            scim.validate()?;
            if scim.enabled && self.admin_token.is_none() {
                return Err("[scim] enabled = true requires admin_token: provisioning \
                            credentials are minted through /api/scim/tokens, which sits \
                            behind the admin bearer. Without it the SCIM surface would be \
                            mounted with no way to issue a credential for it."
                    .to_string());
            }
        }

        // The SQL console (#962). Three separate things have to be true for the
        // most powerful endpoint on the server to exist, and each missing one is
        // refused by name at boot rather than producing a route that quietly is
        // not there — an operator who configured a console and got a 404 has no
        // way to tell "not mounted" from "wrong URL".
        if let Some(ref admin_sql) = self.admin_sql {
            admin_sql.validate()?;
            if admin_sql.enabled {
                if !cfg!(feature = "admin-sql") {
                    return Err("[admin_sql] enabled = true requires the `admin-sql` cargo \
                                feature, which this binary was not built with. The endpoint \
                                executes operator-supplied SQL, so it is not compiled in by \
                                default. Rebuild with --features admin-sql, or set \
                                [admin_sql] enabled = false."
                        .to_string());
                }
                if !self.admin_api_enabled {
                    return Err("[admin_sql] enabled = true requires admin_api_enabled = true: \
                                the SQL console is mounted on the admin API and authenticated \
                                by its tokens."
                        .to_string());
                }
                // No `admin_token.is_none()` check here: the branch above has
                // already established `admin_api_enabled`, and this function
                // refuses that with no `admin_token` further up. The mount is
                // structurally inside `if let Some(write_token) = admin_token`,
                // so "never reachable without a credential" is enforced by
                // construction rather than by a check that could drift.
            }
        }

        if Self::is_production_mode() {
            // Playground should be disabled in production
            if self.playground_enabled {
                return Err("playground_enabled is true in production mode. \
                     Disable the playground or set FRAISEQL_ENV=development. \
                     The playground exposes sensitive schema information."
                    .to_string());
            }

            // CORS origins must be explicitly configured in production
            if self.cors_enabled && self.cors_origins.is_empty() {
                return Err("cors_enabled is true but cors_origins is empty in production mode. \
                     This allows requests from ANY origin, which is a security risk. \
                     Explicitly configure cors_origins with your allowed domains, \
                     or disable CORS and set FRAISEQL_ENV=development to bypass this check."
                    .to_string());
            }
        }

        Ok(())
    }

    /// Check if authentication is enabled.
    #[must_use]
    pub const fn auth_enabled(&self) -> bool {
        self.auth.is_some()
    }
}
