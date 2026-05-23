#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::io::Write as _;

use super::*;

// Minimal PEM fixture with valid markers — just enough for load() validation.
// The content is syntactically correct PEM marker structure but the base64 payload
// is not a real certificate, so apply() would fail on these. That's fine: we
// test load() and apply() error-path separately.
const FAKE_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
        MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAtestcert==\n\
        -----END CERTIFICATE-----\n";

const FAKE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
        MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCtestkey==\n\
        -----END PRIVATE KEY-----\n";

fn write_temp_pem(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

// ── load() tests ───────────────────────────────────────────────────────

#[test]
fn disabled_config_loads_nothing() {
    let config = MtlsConfig {
        enabled:         false,
        client_cert_pem: Some("/does/not/exist.pem".to_string()),
        root_ca_pem:     None,
    };
    // Must not touch the filesystem
    let material = MtlsMaterial::load(&config).unwrap();
    assert!(material.identity_pem.is_none());
    assert!(material.ca_cert.is_none());
}

#[test]
fn enabled_true_no_cert_path_returns_error() {
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: None,
        root_ca_pem:     None,
    };
    let err = MtlsMaterial::load(&config).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("no client_cert_pem configured"),
        "error must mention missing path: {msg}"
    );
}

#[test]
fn enabled_true_missing_file_returns_error() {
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some("/nonexistent/path/cert.pem".to_string()),
        root_ca_pem:     None,
    };
    let err = MtlsMaterial::load(&config).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("failed to open client cert file"),
        "error must mention file open failure: {msg}"
    );
}

#[test]
fn enabled_true_missing_key_marker_returns_error() {
    // File with cert but no key marker
    let cert_only = FAKE_CERT_PEM;
    let f = write_temp_pem(cert_only);
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some(f.path().to_str().unwrap().to_string()),
        root_ca_pem:     None,
    };
    let err = MtlsMaterial::load(&config).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("must contain at least one certificate and one private key"),
        "error must describe missing key marker: {msg}"
    );
}

#[test]
fn enabled_true_missing_cert_marker_returns_error() {
    // File with key but no cert marker
    let key_only = FAKE_KEY_PEM;
    let f = write_temp_pem(key_only);
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some(f.path().to_str().unwrap().to_string()),
        root_ca_pem:     None,
    };
    let err = MtlsMaterial::load(&config).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("must contain at least one certificate and one private key"),
        "error must describe missing cert marker: {msg}"
    );
}

#[test]
fn load_valid_markers_returns_some_identity() {
    // PEM with both markers present → load() succeeds; apply() would fail
    // because the payload is not real, but load() only checks markers.
    let combined = format!("{FAKE_CERT_PEM}{FAKE_KEY_PEM}");
    let f = write_temp_pem(&combined);
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some(f.path().to_str().unwrap().to_string()),
        root_ca_pem:     None,
    };
    let material = MtlsMaterial::load(&config).unwrap();
    assert!(material.identity_pem.is_some(), "identity_pem must be Some for valid markers");
    assert!(material.ca_cert.is_none(), "ca_cert must be None when root_ca_pem is unset");
}

#[test]
fn load_with_ec_key_marker_succeeds() {
    // EC private key marker is also accepted.
    let ec_combined = format!(
        "{FAKE_CERT_PEM}-----BEGIN EC PRIVATE KEY-----\n\
             testdata==\n\
             -----END EC PRIVATE KEY-----\n"
    );
    let f = write_temp_pem(&ec_combined);
    let config = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some(f.path().to_str().unwrap().to_string()),
        root_ca_pem:     None,
    };
    let material = MtlsMaterial::load(&config).unwrap();
    assert!(material.identity_pem.is_some());
}

#[test]
fn identity_pem_is_zeroized_on_drop() {
    // Verify that Zeroizing<Vec<u8>> is the concrete type — this is a static
    // check that the field is actually Zeroizing-wrapped, not a plain Vec.
    // We verify by constructing the type directly and inspecting the field.
    let material = MtlsMaterial {
        identity_pem: Some(Zeroizing::new(b"secret_key_bytes".to_vec())),
        ca_cert:      None,
    };
    assert!(material.identity_pem.is_some());
    // Dropping `material` here triggers Zeroizing's Drop impl, which
    // overwrites the bytes. We cannot observe this at runtime in safe Rust,
    // but the field type Zeroizing<Vec<u8>> is the compile-time guarantee.
}

// ── apply() error path ─────────────────────────────────────────────────

#[test]
fn apply_with_garbage_identity_returns_error() {
    let material = MtlsMaterial {
        identity_pem: Some(Zeroizing::new(b"not-a-real-pem".to_vec())),
        ca_cert:      None,
    };
    let builder = reqwest::Client::builder();
    let err = material.apply(builder).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("failed to load client identity"),
        "error must mention identity loading: {msg}"
    );
}

// ── HttpEntityResolver integration ─────────────────────────────────────

#[test]
fn http_resolver_accepts_none_mtls() {
    use crate::http_resolver::{HttpClientConfig, HttpEntityResolver};
    let config = HttpClientConfig::default();
    HttpEntityResolver::new(config, None).unwrap();
}

#[test]
fn http_resolver_accepts_disabled_mtls() {
    use crate::http_resolver::{HttpClientConfig, HttpEntityResolver};
    let config = HttpClientConfig::default();
    let mtls = MtlsConfig {
        enabled:         false,
        client_cert_pem: Some("/does/not/exist.pem".to_string()),
        root_ca_pem:     None,
    };
    // disabled mTLS: no file I/O, resolver builds fine
    HttpEntityResolver::new(config, Some(&mtls)).unwrap();
}

#[test]
fn http_resolver_enabled_mtls_missing_file_returns_error() {
    use crate::http_resolver::{HttpClientConfig, HttpEntityResolver};
    let config = HttpClientConfig::default();
    let mtls = MtlsConfig {
        enabled:         true,
        client_cert_pem: Some("/nonexistent/path.pem".to_string()),
        root_ca_pem:     None,
    };
    match HttpEntityResolver::new(config, Some(&mtls)) {
        Ok(_) => panic!("expected error for missing cert file"),
        Err(e) => {
            let msg = format!("{e}");
            assert!(
                msg.contains("failed to open client cert file"),
                "error must propagate from load(): {msg}"
            );
        },
    }
}
