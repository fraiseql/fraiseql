//! The state-encryption service a server boots with, from the compiled section.
//!
//! The section is read as the compiled schema types it: the algorithm it names is the one
//! the service encrypts with, an enabled section whose key is not set refuses boot, and a
//! section that is absent or disabled builds no service.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::schema::{
    CompiledSchema, EncryptionAlgorithm, SecurityConfig, StateEncryptionConfig,
};

use super::Server;
use crate::auth::state_encryption::{
    EncryptionAlgorithm as ServiceAlgorithm, StateEncryptionService,
};

const KEY_ENV: &str = "FRAISEQL_TEST_STATE_ENCRYPTION_BOOT_KEY";
const KEY: [u8; 32] = [0xab; 32];

fn schema_with(section: Option<StateEncryptionConfig>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.security = Some(SecurityConfig {
        state_encryption: section,
        ..SecurityConfig::default()
    });
    schema
}

fn enabled(algorithm: EncryptionAlgorithm, key_env: Option<&str>) -> StateEncryptionConfig {
    StateEncryptionConfig {
        enabled: true,
        algorithm,
        key_env: key_env.map(str::to_owned),
        ..StateEncryptionConfig::default()
    }
}

fn booted_with_key(
    var: &str,
    schema: &CompiledSchema,
) -> crate::Result<Option<Arc<StateEncryptionService>>> {
    temp_env::with_var(var, Some("ab".repeat(32)), || Server::state_encryption_from_schema(schema))
}

/// Whether a token `service` encrypts is read by a service with the same key and `algorithm`.
fn encrypts_with(service: &StateEncryptionService, algorithm: ServiceAlgorithm) -> bool {
    let token = service.encrypt(b"oauth state").unwrap();
    StateEncryptionService::from_raw_key(&KEY, algorithm).decrypt(&token).is_ok()
}

#[test]
fn the_service_encrypts_with_the_algorithm_the_section_names() {
    let cases = [
        (
            EncryptionAlgorithm::Chacha20Poly1305,
            ServiceAlgorithm::Chacha20Poly1305,
            ServiceAlgorithm::Aes256Gcm,
        ),
        (
            EncryptionAlgorithm::Aes256Gcm,
            ServiceAlgorithm::Aes256Gcm,
            ServiceAlgorithm::Chacha20Poly1305,
        ),
    ];
    for (named, used, other) in cases {
        let schema = schema_with(Some(enabled(named.clone(), Some(KEY_ENV))));
        let service = booted_with_key(KEY_ENV, &schema).unwrap().unwrap();
        assert!(
            encrypts_with(&service, used),
            "{named:?}: not encrypted with the named algorithm"
        );
        assert!(!encrypts_with(&service, other), "{named:?}: encrypted with the other algorithm");
    }
}

#[test]
fn an_enabled_section_whose_key_is_not_set_refuses_boot() {
    let schema = schema_with(Some(enabled(EncryptionAlgorithm::default(), Some(KEY_ENV))));
    let refusal =
        temp_env::with_var_unset(KEY_ENV, || Server::state_encryption_from_schema(&schema))
            .unwrap_err()
            .to_string();
    assert!(refusal.contains(KEY_ENV), "the refusal names the variable: {refusal}");
}

#[test]
fn a_section_naming_no_variable_reads_the_default_one() {
    let schema = schema_with(Some(enabled(EncryptionAlgorithm::default(), None)));
    assert!(booted_with_key("STATE_ENCRYPTION_KEY", &schema).unwrap().is_some());
}

#[test]
fn an_absent_or_disabled_section_builds_no_service() {
    let disabled = StateEncryptionConfig {
        enabled: false,
        key_env: Some(KEY_ENV.to_owned()),
        ..StateEncryptionConfig::default()
    };
    for schema in [
        schema_with(Some(disabled)),
        schema_with(None),
        CompiledSchema::new(),
    ] {
        assert!(booted_with_key(KEY_ENV, &schema).unwrap().is_none());
    }
}
