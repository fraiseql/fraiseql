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
