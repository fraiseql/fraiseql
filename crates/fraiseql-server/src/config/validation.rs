//! Configuration validation for `fraiseql.toml` settings.
//!
//! [`ConfigValidator`] checks a loaded [`RuntimeConfig`] for semantic errors
//! (e.g. missing required environment variables, invalid combinations of
//! settings) and collects all errors before returning so the developer sees
//! every problem in one pass.

use std::{collections::HashSet, env};

use fraiseql_error::ConfigError;

use crate::config::RuntimeConfig;

/// Validation result with all errors collected
pub struct ValidationResult {
    /// Collected configuration errors; non-empty means the config is invalid.
    pub errors:   Vec<ConfigError>,
    /// Non-fatal warnings about potentially unintended settings.
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create an empty validation result.
    pub const fn new() -> Self {
        Self {
            errors:   Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Return `true` if no errors were collected.
    pub const fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Return `true` if any errors were collected.
    pub const fn is_err(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Add a configuration error to the result.
    pub fn add_error(&mut self, error: ConfigError) {
        self.errors.push(error);
    }

    /// Add a non-fatal warning to the result.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Convert the validation result into a standard `Result`.
    ///
    /// # Errors
    ///
    /// Returns the single `ConfigError` if exactly one error was collected.
    /// Returns `ConfigError::MultipleErrors` if more than one error was collected.
    ///
    /// # Panics
    ///
    /// Cannot panic in practice — the `expect` on `into_iter().next()` is
    /// guarded by a preceding `len() == 1` check.
    pub fn into_result(self) -> Result<Vec<String>, ConfigError> {
        if self.errors.is_empty() {
            Ok(self.warnings)
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().expect("errors.len() == 1 confirmed above"))
        } else {
            Err(ConfigError::MultipleErrors {
                errors: self.errors,
            })
        }
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Comprehensive configuration validator
pub struct ConfigValidator<'a> {
    config:           &'a RuntimeConfig,
    result:           ValidationResult,
    checked_env_vars: HashSet<String>,
}

impl<'a> ConfigValidator<'a> {
    /// Create a new validator bound to the given runtime configuration.
    pub fn new(config: &'a RuntimeConfig) -> Self {
        Self {
            config,
            result: ValidationResult::new(),
            checked_env_vars: HashSet::new(),
        }
    }

    /// Run all validations
    pub fn validate(mut self) -> ValidationResult {
        self.validate_server();
        self.validate_database();
        self.validate_webhooks();
        self.validate_auth();
        self.validate_files();
        self.validate_cross_field();
        self.validate_env_vars();
        self.validate_placeholder_sections();
        self.result
    }

    /// Error on config sections that are parsed but have no runtime effect.
    ///
    /// Silently-ignored config is a common source of operational incidents. By
    /// refusing to start, we ensure operators know their configuration has no
    /// effect and must be removed or replaced.
    fn validate_placeholder_sections(&mut self) {
        if self.config.notifications.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "notifications".to_string(),
                message: "config section 'notifications' is not yet implemented; \
                          remove it from fraiseql.toml to proceed"
                    .to_string(),
            });
        }
        if self.config.logging.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "logging".to_string(),
                message: "config section 'logging' is not yet implemented; \
                          use the 'tracing' section for observability"
                    .to_string(),
            });
        }
        if self.config.search.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "search".to_string(),
                message: "config section 'search' is not yet implemented; \
                          remove it from fraiseql.toml to proceed"
                    .to_string(),
            });
        }
        if self.config.cache.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "cache".to_string(),
                message: "config section 'cache' is not yet implemented; \
                          use fraiseql_core::cache::CacheConfig for query-result caching"
                    .to_string(),
            });
        }
        if self.config.queues.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "queues".to_string(),
                message: "config section 'queues' is not yet implemented; \
                          remove it from fraiseql.toml to proceed"
                    .to_string(),
            });
        }
        if self.config.realtime.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "realtime".to_string(),
                message: "config section 'realtime' is not yet implemented; \
                          use the 'subscriptions' feature for real-time updates"
                    .to_string(),
            });
        }
        if self.config.custom_endpoints.is_some() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "custom_endpoints".to_string(),
                message: "config section 'custom_endpoints' is not yet implemented; \
                          remove it from fraiseql.toml to proceed"
                    .to_string(),
            });
        }
    }

    fn validate_server(&mut self) {
        // Port validation
        if self.config.server.port == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field:   "server.port".to_string(),
                message: "Port cannot be 0".to_string(),
            });
        }

        // Limits validation
        if let Some(limits) = &self.config.server.limits {
            if let Err(e) = crate::config::env::parse_size(&limits.max_request_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "server.limits.max_request_size".to_string(),
                    message: format!("Invalid size format: {}", e),
                });
            }

            if let Err(e) = crate::config::env::parse_duration(&limits.request_timeout) {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "server.limits.request_timeout".to_string(),
                    message: format!("Invalid duration format: {}", e),
                });
            }

            if limits.max_concurrent_requests == 0 {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "server.limits.max_concurrent_requests".to_string(),
                    message: "Must be greater than 0".to_string(),
                });
            }
        }

        // TLS validation
        if let Some(tls) = &self.config.server.tls {
            if !tls.cert_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "server.tls.cert_file".to_string(),
                    message: format!("Certificate file not found: {}", tls.cert_file.display()),
                });
            }
            if !tls.key_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "server.tls.key_file".to_string(),
                    message: format!("Key file not found: {}", tls.key_file.display()),
                });
            }
        }
    }

    fn validate_database(&mut self) {
        // Required env var
        if self.config.database.url_env.is_empty() {
            self.result.add_error(ConfigError::ValidationError {
                field:   "database.url_env".to_string(),
                message: "Database URL environment variable must be specified".to_string(),
            });
        } else {
            self.checked_env_vars.insert(self.config.database.url_env.clone());
        }

        // Pool size
        if self.config.database.pool_size == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field:   "database.pool_size".to_string(),
                message: "Pool size must be greater than 0".to_string(),
            });
        }

        // Replica env vars
        for (i, replica) in self.config.database.replicas.iter().enumerate() {
            if replica.url_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   format!("database.replicas[{}].url_env", i),
                    message: "Replica URL environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(replica.url_env.clone());
            }
        }
    }

    fn validate_webhooks(&mut self) {
        for (name, webhook) in &self.config.webhooks {
            // Secret env var required
            if webhook.secret_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   format!("webhooks.{}.secret_env", name),
                    message: "Webhook secret environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(webhook.secret_env.clone());
            }

            // Provider must be valid
            let valid_providers = [
                "stripe",
                "github",
                "shopify",
                "twilio",
                "sendgrid",
                "paddle",
                "slack",
                "discord",
                "linear",
                "svix",
                "clerk",
                "supabase",
                "novu",
                "resend",
                "generic_hmac",
            ];
            if !valid_providers.contains(&webhook.provider.as_str()) {
                self.result.add_warning(format!(
                    "Unknown webhook provider '{}' for webhook '{}'. Using generic_hmac.",
                    webhook.provider, name
                ));
            }
        }
    }

    fn validate_auth(&mut self) {
        if let Some(auth) = &self.config.auth {
            // JWT secret required if auth is enabled
            if auth.jwt.secret_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "auth.jwt.secret_env".to_string(),
                    message: "JWT secret environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(auth.jwt.secret_env.clone());
            }

            // Validate each provider
            for (name, provider) in &auth.providers {
                self.checked_env_vars.insert(provider.client_id_env.clone());
                self.checked_env_vars.insert(provider.client_secret_env.clone());

                // OIDC providers need issuer URL
                if provider.provider_type == "oidc" && provider.issuer_url.is_none() {
                    self.result.add_error(ConfigError::ValidationError {
                        field:   format!("auth.providers.{}.issuer_url", name),
                        message: "OIDC providers require issuer_url".to_string(),
                    });
                }
            }

            // Callback URL required if any OAuth provider is configured
            if !auth.providers.is_empty() && auth.callback_base_url.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "auth.callback_base_url".to_string(),
                    message: "callback_base_url is required when OAuth providers are configured"
                        .to_string(),
                });
            }
        }
    }

    fn validate_files(&mut self) {
        for (name, file_config) in &self.config.files {
            // Storage backend must be defined
            if !self.config.storage.contains_key(&file_config.storage) {
                self.result.add_error(ConfigError::ValidationError {
                    field:   format!("files.{}.storage", name),
                    message: format!(
                        "Storage backend '{}' not found in storage configuration",
                        file_config.storage
                    ),
                });
            }

            // Max size validation
            if let Err(e) = crate::config::env::parse_size(&file_config.max_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field:   format!("files.{}.max_size", name),
                    message: format!("Invalid size format: {}", e),
                });
            }
        }

        // Validate storage backends
        for (name, storage) in &self.config.storage {
            match storage.backend.as_str() {
                "s3" | "r2" | "gcs" => {
                    if storage.bucket.is_none() {
                        self.result.add_error(ConfigError::ValidationError {
                            field:   format!("storage.{}.bucket", name),
                            message: "Bucket name is required for cloud storage".to_string(),
                        });
                    }
                },
                "local" => {
                    if storage.path.is_none() {
                        self.result.add_error(ConfigError::ValidationError {
                            field:   format!("storage.{}.path", name),
                            message: "Path is required for local storage".to_string(),
                        });
                    }
                },
                _ => {
                    self.result.add_error(ConfigError::ValidationError {
                        field:   format!("storage.{}.backend", name),
                        message: format!("Unknown storage backend: {}", storage.backend),
                    });
                },
            }
        }
    }

    fn validate_cross_field(&mut self) {
        // Observers require notifications for email/slack actions
        for (name, observer) in &self.config.observers {
            for action in &observer.actions {
                match action.action_type.as_str() {
                    "email" | "slack" | "sms" | "push" => {
                        if self.config.notifications.is_none() {
                            self.result.add_error(ConfigError::ValidationError {
                                field: format!("observers.{}.actions", name),
                                message: format!(
                                    "Observer '{}' uses '{}' action but notifications are not configured",
                                    name, action.action_type
                                ),
                            });
                        }
                    },
                    _ => {},
                }
            }
        }

        // Rate limiting with Redis backend requires cache config
        if let Some(rate_limit) = &self.config.rate_limiting {
            if rate_limit.backend == "redis" && self.config.cache.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field:   "rate_limiting.backend".to_string(),
                    message: "Redis rate limiting requires cache configuration. \
                              Add a [cache] section to fraiseql.toml or change \
                              [rate_limiting] backend from 'redis' to 'memory'."
                        .to_string(),
                });
            }
        }
    }

    fn validate_env_vars(&mut self) {
        // Check all collected env vars exist
        for var_name in &self.checked_env_vars {
            if env::var(var_name).is_err() {
                self.result.add_error(ConfigError::MissingEnvVar {
                    name: var_name.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    // ─── ValidationResult ───────────────────────────────────────────────────

    #[test]
    fn empty_result_is_ok() {
        let result = ValidationResult::new();
        assert!(result.is_ok());
        assert!(!result.is_err());
    }

    #[test]
    fn result_with_error_is_err() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "test".into(),
            message: "bad".into(),
        });
        assert!(result.is_err());
        assert!(!result.is_ok());
    }

    #[test]
    fn result_with_only_warnings_is_ok() {
        let mut result = ValidationResult::new();
        result.add_warning("heads up");
        assert!(result.is_ok());
    }

    #[test]
    fn into_result_single_error() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "port".into(),
            message: "invalid".into(),
        });
        let err = result.into_result().unwrap_err();
        assert!(
            matches!(err, ConfigError::ValidationError { ref field, .. } if field == "port"),
            "single error must be unwrapped, not wrapped in MultipleErrors"
        );
    }

    #[test]
    fn into_result_multiple_errors() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "a".into(),
            message: "bad a".into(),
        });
        result.add_error(ConfigError::ValidationError {
            field:   "b".into(),
            message: "bad b".into(),
        });
        let err = result.into_result().unwrap_err();
        assert!(
            matches!(err, ConfigError::MultipleErrors { ref errors } if errors.len() == 2),
            "multiple errors must be wrapped in MultipleErrors"
        );
    }

    #[test]
    fn into_result_ok_returns_warnings() {
        let mut result = ValidationResult::new();
        result.add_warning("warn1");
        result.add_warning("warn2");
        let warnings = result.into_result().unwrap();
        assert_eq!(warnings.len(), 2);
    }

    // ─── ConfigValidator — server validation ────────────────────────────────

    /// Minimal valid TOML for constructing a `RuntimeConfig`.
    fn minimal_config(toml_override: &str) -> RuntimeConfig {
        let toml = format!(
            r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"

            {toml_override}
            "#
        );
        toml::from_str(&toml).unwrap()
    }

    #[test]
    fn valid_minimal_config_passes_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let config = minimal_config("");
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_ok(), "valid minimal config must pass: {:?}", result.errors);
        });
    }

    #[test]
    fn port_zero_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 0

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_err(), "port=0 must fail validation");
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field.contains("port"))
                }),
                "error must reference port field"
            );
        });
    }

    #[test]
    fn pool_size_zero_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"
                pool_size = 0
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_err(), "pool_size=0 must fail validation");
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field.contains("pool_size"))
                }),
                "error must reference pool_size field"
            );
        });
    }

    #[test]
    fn empty_database_url_env_fails_validation() {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = ""
        "#;
        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();
        assert!(result.is_err(), "empty url_env must fail validation");
    }

    #[test]
    fn placeholder_section_notifications_fails() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [notifications]
                enabled = true
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field == "notifications")
                }),
                "placeholder 'notifications' section must be rejected"
            );
        });
    }

    #[test]
    fn placeholder_section_logging_fails() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [logging]
                level = "debug"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field == "logging")
                }),
                "placeholder 'logging' section must be rejected"
            );
        });
    }

    #[test]
    fn invalid_max_request_size_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [server.limits]
                max_request_size = "not-a-size"

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. }
                        if field.contains("max_request_size"))
                }),
                "invalid max_request_size must fail validation"
            );
        });
    }

    #[test]
    fn zero_max_concurrent_requests_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [server.limits]
                max_concurrent_requests = 0

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. }
                        if field.contains("max_concurrent_requests"))
                }),
                "max_concurrent_requests=0 must fail validation"
            );
        });
    }

    #[test]
    fn redis_rate_limiting_without_cache_error_references_fraiseql_toml() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [rate_limiting]
                default = "100/minute"
                backend = "redis"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            let has_toml_ref = result.errors.iter().any(|e| {
                matches!(e, ConfigError::ValidationError { ref message, .. }
                    if message.contains("fraiseql.toml"))
            });
            assert!(
                has_toml_ref,
                "error message must reference fraiseql.toml; errors: {:?}",
                result.errors
            );
        });
    }

    #[test]
    fn multiple_errors_collected_in_one_pass() {
        let toml = r#"
            [server]
            port = 0

            [database]
            url_env = ""
            pool_size = 0
        "#;
        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();
        assert!(
            result.errors.len() >= 3,
            "validator must collect all errors in one pass, got {} errors: {:?}",
            result.errors.len(),
            result.errors
        );
    }
}
