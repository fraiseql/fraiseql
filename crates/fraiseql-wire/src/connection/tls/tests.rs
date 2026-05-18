use super::*;

/// Install a crypto provider for rustls tests.
/// This is needed because multiple crypto providers (ring and aws-lc-rs)
/// may be enabled via transitive dependencies, requiring explicit selection.
fn install_crypto_provider() {
    // Try to install ring as the default provider, ignore if already installed
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[test]
fn test_tls_config_builder_defaults() {
    let tls = TlsConfigBuilder::default();
    assert!(!tls.danger_accept_invalid_certs);
    assert!(!tls.danger_accept_invalid_hostnames);
    assert!(tls.verify_hostname);
    assert!(tls.ca_cert_path.is_none());
}

#[test]
fn test_tls_config_builder_with_hostname_verification() {
    install_crypto_provider();

    let tls = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("Failed to build TLS config");

    assert!(tls.verify_hostname());
    assert!(!tls.danger_accept_invalid_certs());
}

#[test]
fn test_tls_config_builder_with_custom_ca() {
    // This test would require an actual PEM file
}

#[test]
fn test_parse_server_name_valid() {
    let _name = parse_server_name("localhost").expect("localhost should be a valid server name");
    let _name =
        parse_server_name("example.com").expect("example.com should be a valid server name");
    let _name = parse_server_name("db.internal.example.com")
        .expect("subdomain should be a valid server name");
}

#[test]
fn test_parse_server_name_trailing_dot() {
    let _name = parse_server_name("example.com.")
        .expect("trailing dot should be accepted as valid server name");
}

#[test]
fn test_parse_server_name_with_port() {
    // ServerName expects just hostname, not host:port.
    // Whether this succeeds or fails depends on the rustls version,
    // so we only verify it doesn't panic.
    let _result = parse_server_name("example.com:5432");
}

#[test]
fn test_tls_config_debug() {
    install_crypto_provider();

    let tls = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("Failed to build TLS config");

    let debug_str = format!("{:?}", tls);
    assert!(debug_str.contains("TlsConfig"));
    assert!(debug_str.contains("verify_hostname"));
}

#[test]
#[cfg(not(debug_assertions))]
fn test_danger_mode_returns_error_in_release_build() {
    // This test only runs in release builds; danger mode must return an error
    let result = TlsConfig::builder()
        .danger_accept_invalid_certs(true)
        .build();
    assert!(
        result.is_err(),
        "danger mode must be rejected in release builds"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not permitted in release builds"),
        "error message must explain the restriction",
    );
}

#[test]
fn test_danger_mode_allowed_in_debug_build() {
    install_crypto_provider();

    let config = TlsConfig::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("danger mode should be allowed in debug builds");

    assert!(config.danger_accept_invalid_certs());
}

#[test]
fn test_normal_tls_config_works() {
    install_crypto_provider();

    let config = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("normal TLS config should build successfully");

    assert!(!config.danger_accept_invalid_certs());
}
