#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::*;

const NO_KEYS: SchemeConfig = SchemeConfig::none();

/// Every scheme in `KNOWN_SCHEMES` is constructible **given what it needs**.
///
/// What a scheme needs is not uniform: the four in the `jwt-jwks` family verify
/// against keys their sender's publisher serves, so they need a key source. That
/// is supplied here rather than exempting them, because the list is what the
/// refusal message offers an operator and what the fixture-coverage test in
/// `signature::tests` iterates — a name in it that cannot be built is a name
/// offered to an operator who then cannot use it.
#[test]
fn every_known_scheme_builds() {
    for provider in KNOWN_SCHEMES {
        let context = SchemeContext::with_tolerance(300)
            .with_jwks(crate::signature::jwt_jwks::tests::LocalKeys::new());
        // The generic `jwt-jwks` is also the one scheme that must be TOLD where its
        // credential is: no provider puts a JWT in the HMAC families'
        // `header:X-Signature`, so inheriting that default would only ever produce a
        // 401 per delivery. Its three presets fix their own.
        let config = if *provider == "jwt-jwks" {
            SchemeConfig {
                credential: Some(CredentialLocation::Body),
                ..SchemeConfig::none()
            }
        } else {
            NO_KEYS
        };
        let built = build_scheme(provider, &config, &context);
        assert!(
            built.is_ok(),
            "{provider} is advertised in KNOWN_SCHEMES, so it must be constructible: \
             the list is what the refusal message offers an operator and what the \
             fixture-coverage test in `signature::tests` iterates"
        );
    }
}

/// A scheme that verifies against a published key set, on a route that named no
/// `jwks_uri`, is refused at **boot**.
///
/// The receiver builds a key source only for a route that configured one, so
/// `None` here is exactly that route — and the alternative to refusing is a
/// mounted route that 5xxes on its first genuine delivery with nothing in the
/// boot log to explain it.
#[test]
fn a_token_scheme_without_a_key_source_is_refused_at_boot() {
    let token_schemes = ["jwt-jwks", "hanko", "kinde", "fusionauth"];
    for provider in token_schemes {
        let error = build_scheme(provider, &NO_KEYS, &SchemeContext::with_tolerance(300))
            .err()
            .unwrap_or_else(|| panic!("{provider} has no key source, so it must be refused"));
        let message = error.to_string();
        assert!(
            message.contains("jwks_uri"),
            "and the refusal must name the key an operator has to set: {message}"
        );
    }

    // The counterweight: every OTHER scheme builds without one, so the refusal
    // above is about needing a key source and not about the empty context.
    for provider in KNOWN_SCHEMES.iter().filter(|name| !token_schemes.contains(name)) {
        build_scheme(provider, &NO_KEYS, &SchemeContext::with_tolerance(300))
            .unwrap_or_else(|error| panic!("{provider} needs no key source: {error}"));
    }
}

#[test]
fn an_unknown_scheme_is_refused_and_offers_the_known_ones() {
    let error = build_scheme("hmac-sha255", &NO_KEYS, &SchemeContext::with_tolerance(300))
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
        let error = build_scheme("stripe", &config, &SchemeContext::with_tolerance(300))
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
        encoding: Some(SignatureEncoding::Base64),
        prefix: Some("sha256=".to_string()),
        ..SchemeConfig::none()
    };
    for provider in ["hmac-sha256", "hmac-sha1"] {
        build_scheme(provider, &config, &SchemeContext::with_tolerance(300))
            .expect("a generic scheme reads them");
    }
    // That the keys are *honoured* — not merely accepted — is
    // `signature::generic::tests`, which drives a real request through each one.
}

/// `header_prefix` is the key that broke the old two-way split (#1323).
///
/// Before it, "scheme-relevant" was binary — a preset read nothing, a generic HMAC
/// family read all three keys — so a fourth key read by exactly one *other* scheme
/// had nowhere to be refused, and would have been accepted here and ignored. That
/// is the silent drop #1321 removed, one key later.
#[test]
fn a_generic_scheme_refuses_the_header_prefix_it_does_not_read() {
    let config = SchemeConfig {
        header_prefix: Some("svix".to_string()),
        ..NO_KEYS
    };
    for provider in ["hmac-sha256", "hmac-sha1"] {
        let error = build_scheme(provider, &config, &SchemeContext::with_tolerance(300))
            .map(|_| ())
            .expect_err("the generic HMAC families read one header, not a triple");
        let message = error.to_string();
        assert!(message.contains("header_prefix"), "must name the key; got: {message}");
        assert!(message.contains(provider), "must name the scheme; got: {message}");
        assert!(
            message.contains("credential"),
            "must say what the scheme DOES read, or the operator is left guessing which \
             of the four keys belongs here; got: {message}"
        );
    }
}

#[test]
fn the_standard_webhooks_scheme_reads_its_header_prefix_and_nothing_else() {
    let with_prefix = SchemeConfig {
        header_prefix: Some("svix".to_string()),
        ..NO_KEYS
    };
    build_scheme("standard-webhooks", &with_prefix, &SchemeContext::with_tolerance(300))
        .expect("`header_prefix` is the one key this scheme reads");

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
        let error = build_scheme("standard-webhooks", &config, &SchemeContext::with_tolerance(300))
            .map(|_| ())
            .expect_err("the spec fixes the credential, its encoding and its version tag");
        assert!(error.to_string().contains(key), "must name the key; got: {error}");
    }
}

/// `clerk` IS the `svix` prefix, so configuring one on a `clerk` route is a
/// contradiction rather than a refinement — and refusing it keeps the two spellings
/// of one scheme from disagreeing.
#[test]
fn the_clerk_preset_refuses_a_header_prefix() {
    let config = SchemeConfig {
        header_prefix: Some("webhook".to_string()),
        ..NO_KEYS
    };
    let error = build_scheme("clerk", &config, &SchemeContext::with_tolerance(300))
        .map(|_| ())
        .expect_err("clerk is the svix prefix");
    let message = error.to_string();
    assert!(message.contains("header_prefix"), "must name the key; got: {message}");
    assert!(message.contains("clerk"), "must name the scheme; got: {message}");
}

#[test]
fn a_header_prefix_that_cannot_form_a_header_name_is_refused_naming_the_value() {
    // Spelling is deliberately NOT checked — restricting the value to `webhook` and
    // `svix` would foreclose a third Standard Webhooks sender, which is the
    // compiled-in-provider-detail defect #1321 removed. What is checked is that the
    // value can form `{prefix}-id` at all.
    for bad in ["", "webhook-", "svix header", "x_hub"] {
        let config = SchemeConfig {
            header_prefix: Some(bad.to_string()),
            ..NO_KEYS
        };
        let error = build_scheme("standard-webhooks", &config, &SchemeContext::with_tolerance(300))
            .map(|_| ())
            .expect_err("cannot form a header name");
        let message = error.to_string();
        assert!(
            message.contains(bad) || bad.is_empty(),
            "must name the value it cannot use; got: {message}"
        );
        assert!(message.contains("-id"), "must show what it is joined to; got: {message}");
    }
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

    let built = build_scheme("hmac-sha256", &NO_KEYS, &SchemeContext::with_tolerance(300)).unwrap();
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
        let error = build_scheme("hmac-sha256", &config, &SchemeContext::with_tolerance(300))
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
