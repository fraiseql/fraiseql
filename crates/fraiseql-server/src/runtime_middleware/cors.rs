//! CORS middleware configuration and builder.

use axum::http::{HeaderName, Method};
use std::str::FromStr;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::config::cors::CorsConfig;
use fraiseql_error::ConfigError;

/// Build CORS layer from configuration
///
/// # Errors
///
/// Returns an error if the configuration is invalid (e.g., wildcard + credentials)
pub fn build_cors_layer(config: &CorsConfig) -> Result<CorsLayer, ConfigError> {
    let mut layer = CorsLayer::new();

    // Validate and set origins
    if config.origins.len() == 1 && config.origins[0] == "*" {
        if config.credentials {
            return Err(ConfigError::ValidationError {
                field: "cors".to_string(),
                message: "Cannot use wildcard origin with credentials=true".to_string(),
            });
        }
        layer = layer.allow_origin(AllowOrigin::any());
    } else {
        // Build predicate for wildcard matching
        let patterns: Vec<WildcardPattern> =
            config.origins.iter().map(|o| WildcardPattern::new(o)).collect();

        layer = layer.allow_origin(AllowOrigin::predicate(move |origin, _| {
            if let Ok(origin_str) = origin.to_str() {
                patterns.iter().any(|p| p.matches(origin_str))
            } else {
                false
            }
        }));
    }

    // Methods
    let methods: Vec<Method> = config
        .methods
        .iter()
        .filter_map(|m| Method::from_str(m).ok())
        .collect();
    if methods.is_empty() {
        return Err(ConfigError::ValidationError {
            field: "cors.methods".to_string(),
            message: "At least one valid HTTP method is required".to_string(),
        });
    }
    layer = layer.allow_methods(methods);

    // Headers
    let headers: Vec<HeaderName> = config
        .headers
        .iter()
        .filter_map(|h| HeaderName::from_str(h).ok())
        .collect();
    layer = layer.allow_headers(headers);

    // Credentials
    if config.credentials {
        layer = layer.allow_credentials(true);
    }

    // Max age
    layer = layer.max_age(std::time::Duration::from_secs(config.max_age));

    // Expose headers
    if !config.expose_headers.is_empty() {
        let expose: Vec<HeaderName> = config
            .expose_headers
            .iter()
            .filter_map(|h| HeaderName::from_str(h).ok())
            .collect();
        layer = layer.expose_headers(expose);
    }

    // Private network access header (for Chrome's Private Network Access)
    if config.private_network {
        layer = layer.allow_private_network(true);
    }

    Ok(layer)
}

/// Simple wildcard pattern matcher
#[derive(Clone)]
struct WildcardPattern {
    prefix: String,
    suffix: String,
    has_wildcard: bool,
}

impl WildcardPattern {
    fn new(pattern: &str) -> Self {
        if let Some(idx) = pattern.find('*') {
            Self {
                prefix: pattern[..idx].to_string(),
                suffix: pattern[idx + 1..].to_string(),
                has_wildcard: true,
            }
        } else {
            Self {
                prefix: pattern.to_string(),
                suffix: String::new(),
                has_wildcard: false,
            }
        }
    }

    fn matches(&self, value: &str) -> bool {
        if self.has_wildcard {
            value.starts_with(&self.prefix) && value.ends_with(&self.suffix)
        } else {
            value == self.prefix
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_pattern_exact() {
        let pattern = WildcardPattern::new("https://example.com");
        assert!(pattern.matches("https://example.com"));
        assert!(!pattern.matches("https://other.com"));
    }

    #[test]
    fn test_wildcard_pattern_prefix() {
        let pattern = WildcardPattern::new("https://*.example.com");
        assert!(pattern.matches("https://app.example.com"));
        assert!(pattern.matches("https://api.example.com"));
        assert!(!pattern.matches("https://example.com")); // No subdomain
        assert!(!pattern.matches("https://evil.com")); // Different domain
    }

    #[test]
    fn test_cors_credentials_wildcard_error() {
        let config = CorsConfig {
            enabled: true,
            origins: vec!["*".to_string()],
            credentials: true,
            ..Default::default()
        };

        let result = build_cors_layer(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_cors_valid_config() {
        let config = CorsConfig {
            enabled: true,
            origins: vec!["https://example.com".to_string()],
            credentials: true,
            ..Default::default()
        };

        let result = build_cors_layer(&config);
        assert!(result.is_ok());
    }
}
