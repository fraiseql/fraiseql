#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics are acceptable

use super::*;

const NO_KEYS: SchemeConfig = SchemeConfig {
    credential: None,
    encoding:   None,
    prefix:     None,
};

#[test]
fn every_known_scheme_builds() {
    for provider in KNOWN_SCHEMES {
        let built = build_scheme(provider, &NO_KEYS, 300);
        assert!(
            built.is_ok(),
            "{provider} is advertised in KNOWN_SCHEMES, so it must be constructible: \
             the list is what the refusal message offers an operator and what the \
             fixture-coverage test in `signature::tests` iterates"
        );
    }
}

#[test]
fn an_unknown_scheme_is_refused_and_offers_the_known_ones() {
    let error = build_scheme("hmac-sha255", &NO_KEYS, 300)
        .map(|_| ())
        .expect_err("not a scheme");
    let message = error.to_string();
    assert!(message.contains("hmac-sha255"), "must name the bad value; got: {message}");
    assert!(message.contains("hmac-sha256"), "must offer the near miss; got: {message}");
}

#[test]
fn a_preset_refuses_every_scheme_key_by_name() {
    for (key, config) in [
        (
            "credential",
            SchemeConfig {
                credential: Some(CredentialLocation::Header("X-Anything".to_string())),
                ..NO_KEYS
            },
        ),
        (
            "encoding",
            SchemeConfig {
                encoding: Some(SignatureEncoding::Base64),
                ..NO_KEYS
            },
        ),
        (
            "prefix",
            SchemeConfig {
                prefix: Some("sha256=".to_string()),
                ..NO_KEYS
            },
        ),
    ] {
        let error = build_scheme("stripe", &config, 300)
            .map(|_| ())
            .expect_err("stripe fixes its own signing details");
        let message = error.to_string();
        assert!(message.contains(key), "must name the ignored key; got: {message}");
        assert!(message.contains("stripe"), "must name the scheme; got: {message}");
    }
}

#[test]
fn a_generic_scheme_reads_every_scheme_key() {
    let config = SchemeConfig {
        credential: Some(CredentialLocation::Header("X-Lago-Signature".to_string())),
        encoding:   Some(SignatureEncoding::Base64),
        prefix:     Some("sha256=".to_string()),
    };
    for provider in ["hmac-sha256", "hmac-sha1"] {
        build_scheme(provider, &config, 300).expect("a generic scheme reads them");
    }
    // That the keys are *honoured* — not merely accepted — is
    // `signature::generic::tests`, which drives a real request through each one.
}

/// The permissive default: absent keys mean `header:X-Signature`, hex, no prefix.
///
/// Asserted where it is observable — a credential under `X-Signature` verifies and
/// one under another name is not found. Since #1321 the scheme locates its own
/// credential, so there is no accessor to read the answer back from, and adding one
/// would give the answer a second place to be right.
#[test]
fn a_generic_scheme_with_no_keys_keeps_the_pre_1321_default() {
    use std::collections::BTreeMap;

    use hmac::{Hmac, KeyInit as _, Mac as _};
    use sha2::Sha256;

    use crate::InboundRequest;

    let payload = b"test";
    let secret = "secret";
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    let built = build_scheme("hmac-sha256", &NO_KEYS, 300).unwrap();
    for (header, expected_hit) in [("x-signature", true), ("x-lago-signature", false)] {
        let headers = BTreeMap::from([(header.to_string(), signature.clone())]);
        let result = built.verify(&InboundRequest::new(&headers, payload, None), secret);
        assert_eq!(
            result.is_ok(),
            expected_hit,
            "{header}: the pre-#1321 default is `X-Signature` and nothing else; got {result:?}"
        );
    }
}

#[test]
fn a_body_credential_is_refused_by_the_hmac_schemes_naming_the_location() {
    for location in [
        CredentialLocation::Body,
        CredentialLocation::BodyField("token".to_string()),
    ] {
        let config = SchemeConfig {
            credential: Some(location.clone()),
            ..NO_KEYS
        };
        let error = build_scheme("hmac-sha256", &config, 300)
            .map(|_| ())
            .expect_err("the HMAC schemes read a header today");
        let message = error.to_string();
        assert!(
            message.contains(&location.to_string()),
            "must name the location it cannot honour, so the operator is not left \
             guessing which key is wrong; got: {message}"
        );
    }
}

#[test]
fn the_credential_grammar_round_trips() {
    for (text, parsed) in [
        (
            "header:X-Lago-Signature",
            CredentialLocation::Header("X-Lago-Signature".to_string()),
        ),
        ("body", CredentialLocation::Body),
        ("body:token", CredentialLocation::BodyField("token".to_string())),
    ] {
        assert_eq!(text.parse::<CredentialLocation>().unwrap(), parsed);
        assert_eq!(parsed.to_string(), text);
    }
}

#[test]
fn a_credential_outside_the_grammar_is_refused_naming_the_forms() {
    // `X-Lago-Signature` is the shape an operator reaches for first: a bare header
    // name, with the `header:` selector left off. Accepting it would make the
    // grammar ambiguous the moment #1322 adds `body:` — so it is refused, and the
    // refusal has to show the three forms rather than just say "invalid".
    for bad in ["X-Lago-Signature", "header:", "body:", "cookie:session", ""] {
        let error = bad.parse::<CredentialLocation>().expect_err("outside the grammar");
        assert!(error.contains("header:<Name>"), "must show the forms; got: {error}");
    }
}

#[test]
fn an_encoding_decodes_what_its_senders_write() {
    assert_eq!(SignatureEncoding::Hex.decode("00ff").unwrap(), vec![0x00, 0xff]);
    // Hex is case-insensitive, and the comparison is on bytes, so a correct MAC
    // written in upper case verifies. Before #1321 the two encodings were compared
    // as text and this one did not.
    assert_eq!(SignatureEncoding::Hex.decode("00FF").unwrap(), vec![0x00, 0xff]);
    assert_eq!(SignatureEncoding::Base64.decode("AP8=").unwrap(), vec![0x00, 0xff]);

    // Sender-supplied bytes that do not parse are the sender's fault: InvalidFormat
    // maps to 401, KeyMaterial maps to 5xx (#1045).
    for (encoding, bad) in [
        (SignatureEncoding::Hex, "zz"),
        (SignatureEncoding::Base64, "not base64!"),
    ] {
        assert!(matches!(encoding.decode(bad), Err(SignatureError::InvalidFormat)));
    }
}
