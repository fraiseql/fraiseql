//! Tests for the `config` module.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_default_config() {
    let config = FraiseQLConfig::default();
    assert_eq!(config.port, 8000);
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.server.port, 8000);
    assert_eq!(config.server.host, "0.0.0.0");
}

#[test]
fn test_builder() {
    let config = FraiseQLConfig::builder()
        .database_url("postgresql://localhost/test")
        .port(9000)
        .build()
        .unwrap();

    assert_eq!(config.port, 9000);
    assert_eq!(config.server.port, 9000);
    assert!(!config.database_url.is_empty());
    assert!(!config.database.url.is_empty());
}

#[test]
fn test_builder_requires_database_url() {
    let result = FraiseQLConfig::builder().build();
    assert!(
        matches!(result, Err(FraiseQLError::Configuration { .. })),
        "expected Configuration error when database URL is absent, got: {result:?}"
    );
}

#[test]
fn test_from_toml_minimal() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert_eq!(config.database.url, "postgresql://localhost/test");
    assert_eq!(config.database_url, "postgresql://localhost/test");
}

#[test]
fn test_from_toml_full() {
    let toml = r#"
[server]
host = "127.0.0.1"
port = 9000
workers = 4
max_body_size = 2097152
request_logging = true

[database]
url = "postgresql://localhost/mydb"
max_connections = 20
min_connections = 2
connect_timeout_secs = 15
query_timeout_secs = 60
idle_timeout_secs = 300
ssl_mode = "require"

[cors]
enabled = true
allowed_origins = ["http://localhost:3000", "https://app.example.com"]
allow_credentials = true

[auth]
enabled = true
provider = "jwt"
jwt_secret = "my-secret-key"
jwt_algorithm = "HS256"
exclude_paths = ["/health", "/metrics"]

[rate_limit]
enabled = true
requests_per_window = 200
window_secs = 120
key_by = "user"

[cache]
apq_enabled = true
apq_ttl_secs = 3600
response_cache_enabled = true
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();

    // Server
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.server.workers, 4);

    // Database
    assert_eq!(config.database.url, "postgresql://localhost/mydb");
    assert_eq!(config.database.max_connections, 20);
    assert_eq!(config.database.ssl_mode, SslMode::Require);

    // CORS
    assert!(config.cors.enabled);
    assert_eq!(config.cors.allowed_origins.len(), 2);
    assert!(config.cors.allow_credentials);

    // Auth
    assert!(config.auth.enabled);
    assert_eq!(config.auth.provider, AuthProvider::Jwt);
    assert_eq!(config.auth.jwt_secret, Some("my-secret-key".to_string()));

    // Rate Limit
    assert!(config.rate_limit.enabled);
    assert_eq!(config.rate_limit.requests_per_window, 200);
    assert_eq!(config.rate_limit.key_by, RateLimitKey::User);

    // Cache
    assert!(config.cache.apq_enabled);
    assert!(config.cache.response_cache_enabled);
}

#[test]
fn test_env_var_expansion() {
    temp_env::with_vars(
        [
            ("TEST_DB_URL", Some("postgresql://user:pass@host/db")),
            ("TEST_JWT_SECRET", Some("super-secret")),
        ],
        || {
            let toml = r#"
[database]
url = "${TEST_DB_URL}"

[auth]
enabled = true
provider = "jwt"
jwt_secret = "${TEST_JWT_SECRET}"
"#;
            let config = FraiseQLConfig::from_toml(toml).unwrap();

            assert_eq!(config.database.url, "postgresql://user:pass@host/db");
            assert_eq!(config.auth.jwt_secret, Some("super-secret".to_string()));
        },
    );
}

#[test]
fn test_auth_validation_jwt_requires_secret() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[auth]
enabled = true
provider = "jwt"
"#;
    let result = FraiseQLConfig::from_toml(toml);
    // from_toml succeeds but validate would fail
    let config = result.unwrap();
    let validation = config.validate();
    assert!(
        matches!(validation, Err(FraiseQLError::Configuration { .. })),
        "expected Configuration error for missing jwt_secret, got: {validation:?}"
    );
    assert!(validation.unwrap_err().to_string().contains("jwt_secret is required"));
}

#[test]
fn test_auth_validation_auth0_requires_domain() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[auth]
enabled = true
provider = "auth0"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    let validation = config.validate();
    assert!(
        matches!(validation, Err(FraiseQLError::Configuration { .. })),
        "expected Configuration error for missing auth0 domain, got: {validation:?}"
    );
    assert!(validation.unwrap_err().to_string().contains("domain is required"));
}

#[test]
fn test_to_toml() {
    let config = FraiseQLConfig::builder()
        .database_url("postgresql://localhost/test")
        .port(9000)
        .build()
        .unwrap();

    let toml_str = config.to_toml();
    assert!(toml_str.contains("[server]"));
    assert!(toml_str.contains("[database]"));
    assert!(toml_str.contains("port = 9000"));
}

#[test]
fn test_cors_config_defaults() {
    let cors = CorsConfig::default();
    assert!(cors.enabled);
    assert!(cors.allowed_origins.is_empty()); // Empty = allow all
    assert!(cors.allowed_methods.contains(&"POST".to_string()));
    assert!(cors.allowed_headers.contains(&"Authorization".to_string()));
}

#[test]
fn test_rate_limit_key_variants() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[rate_limit]
key_by = "api_key"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert_eq!(config.rate_limit.key_by, RateLimitKey::ApiKey);
}

#[test]
fn test_ssl_mode_variants() {
    for (ssl_str, expected) in [
        ("disable", SslMode::Disable),
        ("prefer", SslMode::Prefer),
        ("require", SslMode::Require),
        ("verify-ca", SslMode::VerifyCa),
        ("verify-full", SslMode::VerifyFull),
    ] {
        let toml = format!(
            r#"
[database]
url = "postgresql://localhost/test"
ssl_mode = "{ssl_str}"
"#
        );
        let config = FraiseQLConfig::from_toml(&toml).unwrap();
        assert_eq!(config.database.ssl_mode, expected);
    }
}

#[test]
fn test_legacy_field_sync() {
    let config = FraiseQLConfig::builder()
        .database_url("postgresql://localhost/test")
        .host("192.168.1.1")
        .port(4000)
        .max_connections(50)
        .query_timeout(120)
        .build()
        .unwrap();

    // Both legacy and new fields should match
    assert_eq!(config.host, "192.168.1.1");
    assert_eq!(config.server.host, "192.168.1.1");
    assert_eq!(config.port, 4000);
    assert_eq!(config.server.port, 4000);
    assert_eq!(config.max_connections, 50);
    assert_eq!(config.database.max_connections, 50);
    assert_eq!(config.query_timeout_secs, 120);
    assert_eq!(config.database.query_timeout_secs, 120);
}

#[test]
fn test_auth_providers() {
    for (provider_str, expected) in [
        ("none", AuthProvider::None),
        ("jwt", AuthProvider::Jwt),
        ("auth0", AuthProvider::Auth0),
        ("clerk", AuthProvider::Clerk),
        ("webhook", AuthProvider::Webhook),
    ] {
        let toml = format!(
            r#"
[database]
url = "postgresql://localhost/test"

[auth]
provider = "{provider_str}"
"#
        );
        let config = FraiseQLConfig::from_toml(&toml).unwrap();
        assert_eq!(config.auth.provider, expected);
    }
}

#[test]
fn test_collation_config_default() {
    let config = CollationConfig::default();
    assert!(config.enabled);
    assert_eq!(config.fallback_locale, "en-US");
    assert!(config.allowed_locales.contains(&"en-US".to_string()));
    assert!(config.allowed_locales.contains(&"fr-FR".to_string()));
    assert_eq!(config.on_invalid_locale, InvalidLocaleStrategy::Fallback);
    assert!(config.database_overrides.is_none());
}

#[test]
fn test_collation_config_from_toml() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true
fallback_locale = "en-GB"
on_invalid_locale = "error"
allowed_locales = ["en-GB", "fr-FR", "de-DE"]
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();

    assert!(config.collation.enabled);
    assert_eq!(config.collation.fallback_locale, "en-GB");
    assert_eq!(config.collation.on_invalid_locale, InvalidLocaleStrategy::Error);
    assert_eq!(config.collation.allowed_locales.len(), 3);
    assert!(config.collation.allowed_locales.contains(&"de-DE".to_string()));
}

#[test]
fn test_collation_with_postgres_overrides() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true
fallback_locale = "en-US"

[collation.database_overrides.postgres]
use_icu = false
provider = "libc"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();

    let overrides = config.collation.database_overrides.as_ref().unwrap();
    let pg_config = overrides.postgres.as_ref().unwrap();
    assert!(!pg_config.use_icu);
    assert_eq!(pg_config.provider, "libc");
}

#[test]
fn test_collation_with_mysql_overrides() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true

[collation.database_overrides.mysql]
charset = "utf8mb4"
suffix = "_0900_ai_ci"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();

    let overrides = config.collation.database_overrides.as_ref().unwrap();
    let mysql_config = overrides.mysql.as_ref().unwrap();
    assert_eq!(mysql_config.charset, "utf8mb4");
    assert_eq!(mysql_config.suffix, "_0900_ai_ci");
}

#[test]
fn test_collation_with_sqlite_overrides() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true

[collation.database_overrides.sqlite]
use_nocase = false
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();

    let overrides = config.collation.database_overrides.as_ref().unwrap();
    let sqlite_config = overrides.sqlite.as_ref().unwrap();
    assert!(!sqlite_config.use_nocase);
}

#[test]
fn test_invalid_locale_strategy_variants() {
    for (strategy_str, expected) in [
        ("fallback", InvalidLocaleStrategy::Fallback),
        ("database_default", InvalidLocaleStrategy::DatabaseDefault),
        ("error", InvalidLocaleStrategy::Error),
    ] {
        let toml = format!(
            r#"
[database]
url = "postgresql://localhost/test"

[collation]
on_invalid_locale = "{strategy_str}"
"#
        );
        let config = FraiseQLConfig::from_toml(&toml).unwrap();
        assert_eq!(config.collation.on_invalid_locale, expected);
    }
}

#[test]
fn test_mutation_timing_default_disabled() {
    let config = FraiseQLConfig::default();
    assert!(!config.database.mutation_timing.enabled);
    assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
}

#[test]
fn test_mutation_timing_from_toml() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[database.mutation_timing]
enabled = true
variable_name = "app.started_at"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert!(config.database.mutation_timing.enabled);
    assert_eq!(config.database.mutation_timing.variable_name, "app.started_at");
}

#[test]
fn test_mutation_timing_from_toml_default_variable() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[database.mutation_timing]
enabled = true
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert!(config.database.mutation_timing.enabled);
    assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
}

#[test]
fn test_mutation_timing_absent_uses_defaults() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert!(!config.database.mutation_timing.enabled);
    assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
}

#[test]
fn test_collation_disabled() {
    let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = false
"#;
    let config = FraiseQLConfig::from_toml(toml).unwrap();
    assert!(!config.collation.enabled);
}

#[test]
fn test_collation_config_builder() {
    let collation = CollationConfig {
        enabled: false,
        fallback_locale: "de-DE".to_string(),
        ..Default::default()
    };

    let config = FraiseQLConfig::builder()
        .database_url("postgresql://localhost/test")
        .collation(collation)
        .build()
        .unwrap();

    assert!(!config.collation.enabled);
    assert_eq!(config.collation.fallback_locale, "de-DE");
}
