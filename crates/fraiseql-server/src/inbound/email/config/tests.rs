//! Parse tests for the unified `[mailbox.<name>]` config (both halves).

#![allow(clippy::unwrap_used)] // Reason: test code

use super::{MailboxConfig, SmtpTlsMode};

#[test]
fn parses_both_halves_with_defaults() {
    // One connected account carries both its IMAP receive half and its SMTP send
    // half under one section; unspecified fields take their defaults.
    let toml = r#"
[imap]
host = "imap.example.com"
username = "sales@example.com"
password_env = "SALES_IMAP_PW"

[smtp]
host = "smtp.example.com"
address = "sales@example.com"
username = "sales@example.com"
password_env = "SALES_SMTP_PW"
"#;
    let mailbox: MailboxConfig = toml::from_str(toml).unwrap();

    let imap = mailbox.imap.expect("imap half");
    assert_eq!(imap.host, "imap.example.com");
    assert_eq!(imap.port, 993, "IMAPS default");
    assert_eq!(imap.mailbox, "INBOX", "default folder");

    let smtp = mailbox.smtp.expect("smtp half");
    assert_eq!(smtp.host, "smtp.example.com");
    assert_eq!(smtp.address, "sales@example.com");
    assert_eq!(smtp.port, 587, "STARTTLS submission default");
    assert_eq!(smtp.tls, SmtpTlsMode::StartTls, "default TLS mode");
    assert_eq!(smtp.timeout_secs, 30);
}

#[test]
fn a_send_only_mailbox_has_no_imap_half() {
    let toml = r#"
[smtp]
host = "smtp.example.com"
address = "sales@example.com"
username = "sales@example.com"
password_env = "SALES_SMTP_PW"
tls = "tls"
"#;
    let mailbox: MailboxConfig = toml::from_str(toml).unwrap();
    assert!(mailbox.imap.is_none(), "send-only: no poll worker");
    assert_eq!(mailbox.smtp.unwrap().tls, SmtpTlsMode::Tls);
}

#[test]
fn an_unknown_smtp_key_fails_the_parse() {
    // The SMTP block is strict (deny_unknown_fields): a typo fails at boot rather
    // than being silently ignored.
    let toml = r#"
[smtp]
host = "smtp.example.com"
address = "sales@example.com"
username = "sales@example.com"
password_env = "SALES_SMTP_PW"
frmo = "typo@example.com"
"#;
    assert!(toml::from_str::<MailboxConfig>(toml).is_err());
}

/// An `[imap]` half pointing `ca_file` at `path`.
fn imap_with_ca_file(path: &std::path::Path) -> super::ImapConfig {
    let toml = format!(
        "host = \"imap.example.com\"\nusername = \"u\"\npassword_env = \"PW\"\nca_file = {:?}\n",
        path.display().to_string()
    );
    toml::from_str(&toml).unwrap()
}

/// A self-signed certificate, PEM-encoded, generated here so none is committed.
fn certificate_pem() -> Vec<u8> {
    use openssl::{asn1::Asn1Time, hash::MessageDigest, pkey::PKey, rsa::Rsa, x509::X509};
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut name = openssl::x509::X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "mailbox test CA").unwrap();
    let name = name.build();
    let mut cert = X509::builder().unwrap();
    cert.set_version(2).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(&name).unwrap();
    cert.set_pubkey(&key).unwrap();
    cert.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    cert.set_not_after(&Asn1Time::days_from_now(1).unwrap()).unwrap();
    cert.sign(&key, MessageDigest::sha256()).unwrap();
    cert.build().to_pem().unwrap()
}

#[test]
fn ca_file_is_optional_and_off_by_default() {
    let imap: super::ImapConfig =
        toml::from_str("host = \"h\"\nusername = \"u\"\npassword_env = \"PW\"\n").unwrap();
    assert!(imap.ca_file.is_none(), "webpki-only unless a mailbox names a CA bundle");
    imap.validate("support").unwrap();
}

#[test]
fn a_ca_file_holding_a_certificate_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ca.pem");
    std::fs::write(&path, certificate_pem()).unwrap();
    imap_with_ca_file(&path).validate("support").unwrap();
}

#[test]
fn a_missing_ca_file_is_refused_naming_the_mailbox() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.pem");
    let error = imap_with_ca_file(&path).validate("support").unwrap_err();
    assert!(error.contains("[mailbox.support.imap] ca_file"), "{error}");
    assert!(error.contains("absent.pem"), "{error}");
}

#[test]
fn an_unreadable_ca_file_is_refused_naming_the_mailbox() {
    // A directory cannot be read as a file, whoever runs the test.
    let dir = tempfile::tempdir().unwrap();
    let error = imap_with_ca_file(dir.path()).validate("support").unwrap_err();
    assert!(error.contains("[mailbox.support.imap] ca_file"), "{error}");
    assert!(error.contains("cannot be read"), "{error}");
}

#[test]
fn a_ca_file_with_no_certificate_is_refused_naming_the_mailbox() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key-only.pem");
    let key = openssl::pkey::PKey::from_rsa(openssl::rsa::Rsa::generate(2048).unwrap()).unwrap();
    std::fs::write(&path, key.private_key_to_pem_pkcs8().unwrap()).unwrap();
    let error = imap_with_ca_file(&path).validate("support").unwrap_err();
    assert!(error.contains("[mailbox.support.imap] ca_file"), "{error}");
    assert!(error.contains("no certificate"), "{error}");
}

#[test]
fn server_config_validation_refuses_a_mailbox_with_a_bad_ca_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.pem");
    let mut config = crate::ServerConfig::default();
    config.mailbox.insert(
        "support".to_owned(),
        super::MailboxConfig {
            imap: Some(imap_with_ca_file(&path)),
            smtp: None,
        },
    );
    let error = config.validate().unwrap_err();
    assert!(error.contains("[mailbox.support.imap] ca_file"), "{error}");
}
