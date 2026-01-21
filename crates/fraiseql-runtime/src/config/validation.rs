use std::env;
use std::collections::HashSet;

use crate::config::RuntimeConfig;
use fraiseql_error::ConfigError;

/// Validation result with all errors collected
pub struct ValidationResult {
    pub errors: Vec<ConfigError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, error: ConfigError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub fn into_result(self) -> Result<Vec<String>, ConfigError> {
        if self.errors.is_empty() {
            Ok(self.warnings)
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(ConfigError::MultipleErrors { errors: self.errors })
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
    config: &'a RuntimeConfig,
    result: ValidationResult,
    checked_env_vars: HashSet<String>,
}

impl<'a> ConfigValidator<'a> {
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
        self.result
    }

    fn validate_server(&mut self) {
        // Port validation
        if self.config.server.port == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field: "server.port".to_string(),
                message: "Port cannot be 0".to_string(),
            });
        }

        // Limits validation
        if let Some(limits) = &self.config.server.limits {
            if let Err(e) = crate::config::env::parse_size(&limits.max_request_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.max_request_size".to_string(),
                    message: format!("Invalid size format: {}", e),
                });
            }

            if let Err(e) = crate::config::env::parse_duration(&limits.request_timeout) {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.request_timeout".to_string(),
                    message: format!("Invalid duration format: {}", e),
                });
            }

            if limits.max_concurrent_requests == 0 {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.limits.max_concurrent_requests".to_string(),
                    message: "Must be greater than 0".to_string(),
                });
            }
        }

        // TLS validation
        if let Some(tls) = &self.config.server.tls {
            if !tls.cert_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.tls.cert_file".to_string(),
                    message: format!("Certificate file not found: {:?}", tls.cert_file),
                });
            }
            if !tls.key_file.exists() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "server.tls.key_file".to_string(),
                    message: format!("Key file not found: {:?}", tls.key_file),
                });
            }
        }
    }

    fn validate_database(&mut self) {
        // Required env var
        if self.config.database.url_env.is_empty() {
            self.result.add_error(ConfigError::ValidationError {
                field: "database.url_env".to_string(),
                message: "Database URL environment variable must be specified".to_string(),
            });
        } else {
            self.checked_env_vars.insert(self.config.database.url_env.clone());
        }

        // Pool size
        if self.config.database.pool_size == 0 {
            self.result.add_error(ConfigError::ValidationError {
                field: "database.pool_size".to_string(),
                message: "Pool size must be greater than 0".to_string(),
            });
        }

        // Replica env vars
        for (i, replica) in self.config.database.replicas.iter().enumerate() {
            if replica.url_env.is_empty() {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("database.replicas[{}].url_env", i),
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
                    field: format!("webhooks.{}.secret_env", name),
                    message: "Webhook secret environment variable must be specified".to_string(),
                });
            } else {
                self.checked_env_vars.insert(webhook.secret_env.clone());
            }

            // Provider must be valid
            let valid_providers = [
                "stripe", "github", "shopify", "twilio", "sendgrid",
                "paddle", "slack", "discord", "linear", "svix",
                "clerk", "supabase", "novu", "resend", "generic_hmac"
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
                    field: "auth.jwt.secret_env".to_string(),
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
                        field: format!("auth.providers.{}.issuer_url", name),
                        message: "OIDC providers require issuer_url".to_string(),
                    });
                }
            }

            // Callback URL required if any OAuth provider is configured
            if !auth.providers.is_empty() && auth.callback_base_url.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "auth.callback_base_url".to_string(),
                    message: "callback_base_url is required when OAuth providers are configured".to_string(),
                });
            }
        }
    }

    fn validate_files(&mut self) {
        for (name, file_config) in &self.config.files {
            // Storage backend must be defined
            if !self.config.storage.contains_key(&file_config.storage) {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("files.{}.storage", name),
                    message: format!(
                        "Storage backend '{}' not found in storage configuration",
                        file_config.storage
                    ),
                });
            }

            // Max size validation
            if let Err(e) = crate::config::env::parse_size(&file_config.max_size) {
                self.result.add_error(ConfigError::ValidationError {
                    field: format!("files.{}.max_size", name),
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
                            field: format!("storage.{}.bucket", name),
                            message: "Bucket name is required for cloud storage".to_string(),
                        });
                    }
                }
                "local" => {
                    if storage.path.is_none() {
                        self.result.add_error(ConfigError::ValidationError {
                            field: format!("storage.{}.path", name),
                            message: "Path is required for local storage".to_string(),
                        });
                    }
                }
                _ => {
                    self.result.add_error(ConfigError::ValidationError {
                        field: format!("storage.{}.backend", name),
                        message: format!("Unknown storage backend: {}", storage.backend),
                    });
                }
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
                    }
                    _ => {}
                }
            }
        }

        // Rate limiting with Redis backend requires cache config
        if let Some(rate_limit) = &self.config.rate_limiting {
            if rate_limit.backend == "redis" && self.config.cache.is_none() {
                self.result.add_error(ConfigError::ValidationError {
                    field: "rate_limiting.backend".to_string(),
                    message: "Redis rate limiting requires cache configuration".to_string(),
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
