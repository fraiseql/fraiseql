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
    assert!(tls.ca_cert_path.is_none());
}

#[test]
fn test_tls_config_builder_with_hostname_verification() {
    install_crypto_provider();

    let tls = TlsConfig::builder()
        .build()
        .expect("Failed to build TLS config");

    assert!(!tls.danger_accept_invalid_certs());
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
        .build()
        .expect("Failed to build TLS config");

    let debug_str = format!("{:?}", tls);
    assert!(debug_str.contains("TlsConfig"));
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
        .build()
        .expect("normal TLS config should build successfully");

    assert!(!config.danger_accept_invalid_certs());
}

// ── #887: load_custom_ca must report what it actually loaded ──────────────────
//
// `found_certs` counted PEM *items read*, not certificates rustls *accepted*, so a
// CA file whose certificates are all rejected returned `Ok` with an empty trust
// store. Not a bypass — an empty store rejects every server certificate — but the
// operator then debugs an opaque verification failure attributed to the server,
// while the actual fault is the CA file they configured. Fabricated success:
// a function reporting work it did not do.
//
// This replaces the empty `test_tls_config_builder_with_custom_ca` body that
// #895 deleted.

/// Valid PEM armour around bytes that are not a parsable certificate.
///
/// `rustls_pemfile` yields this as `Item::X509Certificate` — it only base64-decodes
/// the body — and `add_parsable_certificates` then rejects the DER. That gap is
/// exactly where the miscount lived.
const UNPARSABLE_DER_IN_VALID_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
     bm90IGEgY2VydGlmaWNhdGUsIGp1c3Qgc29tZSBieXRlcyB3aXRoIHZhbGlkIFBF\n\
     TSBhcm1vdXIgYXJvdW5kIHRoZW0u\n\
     -----END CERTIFICATE-----\n";

fn ca_file(contents: &str) -> tempfile::NamedTempFile {
    use std::io::Write as _;
    let mut f = tempfile::NamedTempFile::new().expect("temp file");
    f.write_all(contents.as_bytes()).expect("write CA fixture");
    f.flush().expect("flush CA fixture");
    f
}

#[test]
fn a_ca_file_whose_certificates_are_all_rejected_is_an_error() {
    install_crypto_provider();
    let file = ca_file(UNPARSABLE_DER_IN_VALID_PEM);
    let path = file.path().to_string_lossy().to_string();

    let err = TlsConfig::builder().ca_cert_path(&path).build().expect_err(
        "#887: every certificate in the file was rejected, so the trust store is empty \
             and nothing is trusted — load_custom_ca must not report success",
    );

    assert!(
        err.to_string().contains(&path),
        "the error must name the CA file so the operator debugs their configuration rather \
         than the server's certificate; got: {err}"
    );
}

#[test]
fn a_ca_file_with_no_certificate_items_is_an_error() {
    install_crypto_provider();
    let file = ca_file("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n");
    let path = file.path().to_string_lossy().to_string();

    let err = TlsConfig::builder()
        .ca_cert_path(&path)
        .build()
        .expect_err("a file with no certificate in it cannot establish trust");
    assert!(
        err.to_string().contains(&path),
        "the error must name the CA file; got: {err}"
    );
}

/// A real, parsable self-signed CA. The counterweight that keeps the two tests
/// above honest: without it they would also pass if `load_custom_ca` had simply
/// been changed to reject everything.
const USABLE_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDGTCCAgGgAwIBAgIUDrN/jDVUI95u8it2un64FpFHc2UwDQYJKoZIhvcNAQEL
BQAwGzEZMBcGA1UEAwwQRnJhaXNlUUwgVGVzdCBDQTAgFw0yNjA4MDYyMDEwMDZa
GA8yMTI2MDcxMzIwMTAwNlowGzEZMBcGA1UEAwwQRnJhaXNlUUwgVGVzdCBDQTCC
ASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKiSb6y6/Yg3eQFQakQCZpRD
7fbm8sK+cSQT73bB7O2dNnBm7zrN5qvGg9CvyPFaKMQOLVx956rtEHlgP1wjdz8C
f4jdG7VrM22JEMshHak46LAMFDpCBK6eWf7QMpy0G9yG/0xek5LgBWlO8vVwmHmR
ILjG5Ae/q6nJm7f0BBrSjmrvhy22MHYDqdijKRMA7VQSkKPZ1/mX9gHqgSpmtlMa
JGRo6J+MJNX+ibTfz6wVdp0ulerNUCl7xfNXfcj61RQsVOCs4oUbbW9ExO1ZcKn/
Mm/ipXFgUCT60sON1E+ie4Xnqt6gdOJmQU0qMU74ZT5Q566QOlYub3HKWuhGZsUC
AwEAAaNTMFEwHQYDVR0OBBYEFDtPIOLuwXtMpyTYXtjpJKs6/OXGMB8GA1UdIwQY
MBaAFDtPIOLuwXtMpyTYXtjpJKs6/OXGMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZI
hvcNAQELBQADggEBAE526DVZnjV0aNH38Sjq14hfNG/yJ66phLuYWE3ZDJ6Q4nPi
k7mrAcy+xK45vioU0KMsZEWP3aCEQB46jheSgAQMMS/kEbUuAdNLwheb+XoRGGqY
me/7ndvCXOWD+8zZIHPrJ2xr/iLvagWQyU+m3N9Z8i7eAaSFbhe4Y7n2y8wwWO15
QqzB1cDe+JlFTiRYDrn6SZ0CYsXV8DJu0PZxH+vO7mbL1qKyu5+EFu0SAUnrziOs
U9sod4SSOJlvyGK1rw1HVjoIrH8c6hXe49EZ1f7w4fMP6/lsgiHGSA6QlsAPzNFj
Kvuc+jiufKtrxRCMGB+jj+fauwzws1mII2skvRU=
-----END CERTIFICATE-----
";

#[test]
fn a_ca_file_with_a_usable_certificate_loads() {
    install_crypto_provider();
    let file = ca_file(USABLE_CA);
    let path = file.path().to_string_lossy().to_string();

    TlsConfig::builder()
        .ca_cert_path(&path)
        .build()
        .expect("a CA file rustls can parse must load");
}
