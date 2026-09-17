//! # Config Struct Tests: `ServerConfig` Defaults and Validation
//!
//! Tests `ServerConfig` default values, field validation, and serialization
//! round-trips. Verifies that configuration defaults match documented behavior.
//!
//! **Execution engine:** none (config struct only, no server started)
//! **Infrastructure:** none
//! **Parallelism:** safe (no shared mutable state)
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables use _ prefix
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures
#![allow(clippy::match_same_arms)] // Reason: test data clarity
#![allow(clippy::branches_sharing_code)] // Reason: test assertion clarity
#![allow(clippy::undocumented_unsafe_blocks)] // Reason: test exercises unsafe paths

use std::{io::Write, path::PathBuf};

use fraiseql_server::{CompiledSchemaLoader, ServerConfig};
use tempfile::NamedTempFile;

/// Test default configuration
#[test]
fn test_default_config() {
    let config = ServerConfig::default();

    assert_eq!(config.graphql_path, "/graphql");
    assert_eq!(config.health_path, "/health");
    assert_eq!(config.introspection_path, "/introspection");
    assert_eq!(config.schema_path, PathBuf::from("schema.compiled.json"));
    assert!(config.cors_enabled);
    assert!(!config.compression_enabled);
    assert!(config.tracing_enabled);
}

/// Test configuration serialization with serde
#[test]
fn test_config_serialization() {
    let config = ServerConfig::default();
    let toml_str = toml::to_string(&config).expect("Failed to serialize config");

    // Should be valid TOML
    assert!(toml_str.contains("schema_path"));
    assert!(toml_str.contains("bind_addr"));
}

/// Test configuration deserialization from TOML
#[test]
fn test_config_deserialization() {
    let toml_str = r#"
        schema_path = "custom_schema.json"
        graphql_path = "/api/graphql"
        health_path = "/api/health"
        cors_enabled = false
        compression_enabled = true
    "#;

    let config: ServerConfig = toml::from_str(toml_str).expect("Failed to deserialize config");

    assert_eq!(config.schema_path, PathBuf::from("custom_schema.json"));
    assert_eq!(config.graphql_path, "/api/graphql");
    assert_eq!(config.health_path, "/api/health");
    assert!(!config.cors_enabled);
    assert!(config.compression_enabled);
}

/// Test schema loader with non-existent file
#[tokio::test]
async fn test_schema_loader_missing_file() {
    let loader = CompiledSchemaLoader::new("/nonexistent/schema.json");
    let result = loader.load().await;

    assert!(result.is_err(), "expected Err loading nonexistent schema, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("not found"), "expected 'not found' in error, got: {err_msg}");
}

/// Test schema loader with invalid JSON
#[tokio::test]
async fn test_schema_loader_invalid_json() {
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    writeln!(temp_file, "{{invalid json").expect("Failed to write to temp file");

    let loader = CompiledSchemaLoader::new(temp_file.path());
    let result = loader.load().await;

    assert!(result.is_err(), "expected Err loading invalid JSON schema, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("parse") || err_msg.to_lowercase().contains("json"),
        "expected 'parse' or 'json' in error, got: {err_msg}"
    );
}

/// Test schema loader path getter
#[test]
fn test_schema_loader_path() {
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let loader = CompiledSchemaLoader::new(tmp.path());

    assert_eq!(loader.path(), tmp.path());
}

/// Test schema loader path display
#[test]
fn test_schema_loader_path_display() {
    let path = "/home/user/schema.compiled.json";
    let loader = CompiledSchemaLoader::new(path);

    let path_display = loader.path().display().to_string();
    assert_eq!(path_display, path);
}

/// Test multiple configurations can coexist
#[test]
fn test_multiple_configs() {
    let config1 = ServerConfig::default();
    let config2 = ServerConfig {
        schema_path: PathBuf::from("other.json"),
        ..ServerConfig::default()
    };

    assert_eq!(config1.schema_path, PathBuf::from("schema.compiled.json"));
    assert_eq!(config2.schema_path, PathBuf::from("other.json"));
}

/// Test server config with custom bind address
#[test]
fn test_config_custom_bind_addr() {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8080".parse().unwrap(),
        ..ServerConfig::default()
    };

    assert_eq!(config.bind_addr.ip().to_string(), "0.0.0.0");
    assert_eq!(config.bind_addr.port(), 8080);
}

/// Test pool min size is positive and consistent in default config.
#[test]
fn pool_min_size_is_positive_in_default_config() {
    let cfg = ServerConfig::default();
    assert!(
        cfg.pool_min_size > 0,
        "default pool_min_size should be > 0, got {}",
        cfg.pool_min_size
    );
    assert!(
        cfg.pool_min_size <= cfg.pool_max_size,
        "pool_min_size ({}) must not exceed pool_max_size ({})",
        cfg.pool_min_size,
        cfg.pool_max_size,
    );
}

/// Test pool timeout default is sensible.
#[test]
fn pool_timeout_default_is_positive() {
    let cfg = ServerConfig::default();
    assert!(cfg.pool_timeout_secs > 0, "pool_timeout_secs should be > 0");
    assert_eq!(cfg.pool_timeout_secs, 30, "expected default pool_timeout_secs = 30");
}

/// Test server config flags
#[test]
fn test_config_feature_flags() {
    let config = ServerConfig {
        cors_enabled: false,
        compression_enabled: false,
        tracing_enabled: false,
        apq_enabled: false,
        cache_enabled: false,
        ..ServerConfig::default()
    };

    assert!(!config.cors_enabled);
    assert!(!config.compression_enabled);
    assert!(!config.tracing_enabled);
    assert!(!config.apq_enabled);
    assert!(!config.cache_enabled);
}

// ═══════════════════════════════════════════════════════════════════════════
// #1337: a mistyped key is refused, in every section — not only at the top
// ═══════════════════════════════════════════════════════════════════════════
//
// #839 put `deny_unknown_fields` on `ServerConfig`. serde does **not** propagate it into
// nested structs, so every `[section]` whose own struct lacked it accepted a typo and
// discarded it in silence — `[rate_limiting] enabeld = true` booted the section on its
// defaults without a word. Several of those are security switches.
//
// Table-driven on purpose: the list IS the assertion. One test per section would make
// "a section nobody added a test for" the same silence one layer up.

/// `(section TOML with a correct key, the same section with that key mistyped)`.
///
/// The correct spelling is not decoration — it is the twin. A struct that refused
/// *everything* would satisfy every refusal below, and so would a section name that
/// simply does not exist.
fn section_cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "auth",
            "[auth]\nissuer = \"https://idp.test\"\n",
            "[auth]\nisseur = \"https://idp.test\"\n",
        ),
        // ⚠ The mistyped key must be an **optional** one. Misspelling `secret_env`
        // (required, no `serde(default)`) is refused as a *missing field* whether or not
        // the struct denies unknown ones — so it would pass before the fix and prove
        // nothing. `issuer` has a default, so only `deny_unknown_fields` can refuse it.
        (
            "auth_hs256",
            "[auth_hs256]\nsecret_env = \"S\"\nissuer = \"i\"\n",
            "[auth_hs256]\nsecret_env = \"S\"\nisseur = \"i\"\n",
        ),
        // `cert_path`/`key_path` are required (no `serde(default)`), so a bare
        // `enabled` would fail as a *missing* field and tell us nothing about unknown
        // ones. The twin has to actually parse.
        // Same trap: `enabled`, `cert_path` and `key_path` are all required, so a typo in
        // any of them is a missing-field error regardless. `require_client_cert` has a
        // default — and it is one of the security switches this issue is about.
        (
            "tls",
            "[tls]\nenabled = true\ncert_path = \"c.pem\"\nkey_path = \"k.pem\"\nrequire_client_cert = true\n",
            "[tls]\nenabled = true\ncert_path = \"c.pem\"\nkey_path = \"k.pem\"\nrequire_client_certs = true\n",
        ),
        // ⚠ This twin was originally written as `mode = "disable"` and *passed* — because
        // the section discarded unknown keys, which is the whole defect. The real field
        // is `postgres_ssl_mode`. The fix caught the fixture.
        (
            "database_tls",
            "[database_tls]\npostgres_ssl_mode = \"disable\"\n",
            "[database_tls]\npostgres_ssl_mdoe = \"disable\"\n",
        ),
        (
            "rate_limiting",
            "[rate_limiting]\nenabled = true\n",
            "[rate_limiting]\nenabeld = true\n",
        ),
        (
            "admission_control",
            "[admission_control]\nmax_concurrent = 4\n",
            "[admission_control]\nmax_concurrrent = 4\n",
        ),
        (
            "pool_tuning",
            "[pool_tuning]\nenabled = true\n",
            "[pool_tuning]\nenabeld = true\n",
        ),
        (
            "usage",
            "[usage]\nflush_interval_secs = 5\n",
            "[usage]\nflush_interval_sec = 5\n",
        ),
        // `[tenancy]` nests: `runtime` is its own struct, so this also proves the walk
        // reaches a section's children rather than stopping at the top of it.
        (
            "tenancy",
            "[tenancy.runtime]\nenabled = true\n",
            "[tenancy.runtime]\nenabeld = true\n",
        ),
        // `[sources]` is behind `#[cfg(feature = "sources")]` on `ServerConfig`, so in a
        // build without it the section is unknown at the *top* level and the twin fails
        // for a reason that has nothing to do with this issue.
        #[cfg(feature = "sources")]
        ("sources", "[sources]\nenabled = true\n", "[sources]\nenabeld = true\n"),
    ]
}

#[test]
fn a_mistyped_key_is_refused_in_every_section() {
    let mut served_silently = Vec::new();

    for (section, correct, mistyped) in section_cases() {
        // The twin first: if the correct spelling does not parse, the mistyped case
        // below proves nothing — the section name or key might simply be wrong.
        assert!(
            toml::from_str::<ServerConfig>(correct).is_ok(),
            "[{section}]: the *correctly* spelled config must parse, or this case is \
             measuring a broken fixture rather than the defect.\n{correct}"
        );

        if toml::from_str::<ServerConfig>(mistyped).is_ok() {
            served_silently.push(section);
        }
    }

    assert!(
        served_silently.is_empty(),
        "#1337: these sections accept a mistyped key and discard it silently, so the \
         setting stays at its default and nothing says so — `[auth] require_jti`, \
         `[tls] require_client_cert` and `[rate_limiting] enabled` are security \
         switches. serde does not propagate `deny_unknown_fields` into nested structs, \
         so each section struct needs its own: {served_silently:?}"
    );
}

/// The refusal has to name the key, or an operator cannot act on it.
#[test]
fn the_refusal_names_the_offending_key() {
    let err = toml::from_str::<ServerConfig>("[rate_limiting]\nenabeld = true\n")
        .expect_err("a mistyped key must be refused");
    let message = err.to_string();

    assert!(
        message.contains("enabeld"),
        "the error must name the key the operator got wrong — 'unknown field' alone \
         sends them looking through the whole section. Got: {message}"
    );
}
