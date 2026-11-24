//! SAML's instants at the end of the calendar a `Timestamp` holds.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::Timestamp;
use openssl::{
    asn1::Asn1Time, bn::BigNum, hash::MessageDigest, pkey::PKey, rsa::Rsa, x509::X509Builder,
};

use super::{certificate_not_after, not_on_or_after, warning_horizon};
use crate::saml::{SamlError, SamlIdpConfig, SamlIdpRegistry};

/// 9999-12-30T22:00:00Z, the last whole second a `Timestamp` holds.
const LAST_SECOND: i64 = 253_402_207_200;

/// A self-signed certificate valid until `not_after` (an ASN.1 time such as
/// `99991231235959Z`), DER-encoded.
fn certificate_until(not_after: &Asn1Time) -> Vec<u8> {
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut name = openssl::x509::X509Name::builder().unwrap();
    name.append_entry_by_text("CN", "idp.example.com").unwrap();
    let name = name.build();
    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder
        .set_serial_number(&BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap())
        .unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&key).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(not_after).unwrap();
    builder.sign(&key, MessageDigest::sha256()).unwrap();
    builder.build().to_der().unwrap()
}

fn idp_trusting(name: &str, der: &[u8]) -> SamlIdpConfig {
    SamlIdpConfig::builder(name, "https://sp.example.com/metadata", "https://sp.example.com/acs")
        .idp_parts("https://idp.example.com", "https://idp.example.com/sso", der)
        .unwrap()
        .build()
        .unwrap()
}

/// RFC 5280's "no well-defined expiration".
fn never() -> Asn1Time {
    Asn1Time::from_str("99991231235959Z").unwrap()
}

#[test]
fn a_certificate_expiry_within_the_calendar_is_read() {
    let expiry = certificate_not_after(30, 0).unwrap();
    let days = (expiry - Timestamp::now()).as_secs() / 86_400;
    assert!((29..=30).contains(&days), "{days}");
}

#[test]
fn a_certificate_expiry_past_the_calendar_is_no_expiry() {
    assert_eq!(certificate_not_after(3_000_000, 0), None);
}

#[test]
fn a_certificate_with_no_well_defined_expiry_has_none_and_is_not_warned_of() {
    let soon = certificate_until(&Asn1Time::days_from_now(5).unwrap());
    let never = certificate_until(&never());

    assert_eq!(idp_trusting("never", &never).signing_certificate_expiry(), None);

    let registry = SamlIdpRegistry::new()
        .with_config_idp(idp_trusting("never", &never))
        .with_config_idp(idp_trusting("soon", &soon));
    let warned: Vec<String> = registry
        .expiry_report(Timestamp::now(), 30)
        .into_iter()
        .map(|warning| warning.idp_name)
        .collect();
    assert_eq!(warned, ["soon"]);
}

#[test]
fn a_warning_horizon_past_the_calendar_is_its_last_instant() {
    let now = Timestamp::now();
    assert_eq!(
        warning_horizon(now, i64::MAX).to_rfc3339(),
        "9999-12-30T22:00:00.999999999+00:00"
    );
    assert_eq!((warning_horizon(now, 30) - now).as_secs(), 30 * 86_400);
}

#[test]
fn an_assertion_not_on_or_after_the_calendars_last_second_is_read() {
    let at = not_on_or_after(LAST_SECOND, 0).unwrap();
    assert_eq!(at.to_rfc3339(), "9999-12-30T22:00:00+00:00");
}

#[test]
fn an_assertion_not_on_or_after_past_the_calendar_is_refused() {
    for seconds in [LAST_SECOND + 1, 253_402_300_799] {
        assert!(matches!(not_on_or_after(seconds, 0), Err(SamlError::Malformed(_))), "{seconds}");
    }
}
