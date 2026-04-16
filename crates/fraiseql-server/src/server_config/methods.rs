use super::ServerConfig;

impl ServerConfig {
    /// Load server configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error string if the file cannot be read or the TOML cannot be parsed.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Cannot read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Invalid TOML config: {e}"))
    }

    /// Check if running in production mode.
    ///
    /// Production mode is detected via `FRAISEQL_ENV` environment variable.
    /// - `production` or `prod` (or any value other than `development`/`dev`) → production mode
    /// - `development` or `dev` → development mode
    #[must_use]
    pub fn is_production_mode() -> bool {
        let env = std::env::var("FRAISEQL_ENV")
            .unwrap_or_else(|_| "production".to_string())
            .to_lowercase();
        env != "development" && env != "dev"
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `metrics_enabled` is true but `metrics_token` is not set
    /// - `metrics_token` is set but too short (< 16 characters)
    /// - `auth` config is set but invalid (e.g., empty issuer)
    /// - `tls` is enabled but cert or key path is missing
    /// - TLS minimum version is invalid
    /// - In production mode: `playground_enabled` is true
    /// - In production mode: `cors_enabled` is true but `cors_origins` is empty
    pub fn validate(&self) -> Result<(), String> {
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

        // Validate database TLS config if present
        if let Some(ref db_tls) = self.database_tls {
            // Validate PostgreSQL SSL mode
            if ![
                "disable",
                "allow",
                "prefer",
                "require",
                "verify-ca",
                "verify-full",
            ]
            .contains(&db_tls.postgres_ssl_mode.as_str())
            {
                return Err("Invalid postgres_ssl_mode. Must be one of: \
                     disable, allow, prefer, require, verify-ca, verify-full"
                    .to_string());
            }

            // Validate CA bundle path if provided
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
