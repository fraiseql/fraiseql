use super::*;

#[test]
fn test_constant_time_eq_equal() {
    assert!(constant_time_eq(b"test", b"test"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn test_constant_time_eq_not_equal() {
    assert!(!constant_time_eq(b"test", b"fail"));
    assert!(!constant_time_eq(b"test", b"tes"));
    assert!(!constant_time_eq(b"test", b""));
}

// ── M-webhook-replay-drift: shared timestamp-freshness check ──────────────────

#[test]
fn freshness_accepts_timestamp_inside_window() {
    // 100s old, 300s tolerance → fresh.
    assert!(check_timestamp_freshness(1_000_100, "1000000", 300).is_ok());
}

#[test]
fn freshness_rejects_stale_timestamp() {
    // 1000s old, 300s tolerance → stale.
    assert!(matches!(
        check_timestamp_freshness(1_001_000, "1000000", 300),
        Err(SignatureError::TimestampExpired)
    ));
}

#[test]
fn freshness_rejects_future_timestamp_beyond_window() {
    // 1000s in the future, 300s tolerance → rejected.
    assert!(matches!(
        check_timestamp_freshness(1_000_000, "1001000", 300),
        Err(SignatureError::TimestampExpired)
    ));
}

#[test]
fn freshness_rejects_non_numeric_timestamp() {
    assert!(matches!(
        check_timestamp_freshness(1_000_000, "not-a-number", 300),
        Err(SignatureError::InvalidFormat)
    ));
}

#[test]
fn freshness_huge_tolerance_does_not_wrap_to_reject_everything() {
    // A `u64` tolerance larger than `i64::MAX` must saturate, NOT wrap negative.
    // The old `seconds as i64` cast wrapped, yielding a negative window that
    // rejected every request (M-webhook-replay-drift). A fresh request must
    // still verify under an effectively-infinite tolerance.
    assert!(check_timestamp_freshness(1_000_000, "1000000", u64::MAX).is_ok());
    // And even a wildly out-of-window timestamp is accepted (window is infinite).
    assert!(check_timestamp_freshness(i64::MAX, "0", u64::MAX).is_ok());
}

// ── #1049: the freshness gate does unchecked arithmetic on an unauthenticated header ──

mod extreme_timestamps {
    //! `check_timestamp_freshness` runs **before** the HMAC comparison in all five
    //! timestamped verifiers, on a header the server threads through raw. `now - ts`
    //! and `.abs()` were both unchecked, so an extreme value overflowed: a panic in
    //! any debug-assertions build, and a silent wrap in release.
    //!
    //! The tolerance side had already been hardened against exactly this (saturating
    //! `try_from`, tested above); the `now - ts` side had not.
    //!
    //! ⚠ The issue's own reproduction does **not** reproduce. `i64::MIN` as the
    //! timestamp is correctly *rejected*: `now - i64::MIN` wraps to roughly `-9.22e18`,
    //! whose `.abs()` is far outside any tolerance. The fail-open needs `ts` to be
    //! exactly `now - 2^63`, so that the wrap lands precisely on `i64::MIN`, where
    //! `.abs()` is the identity. Both cases are pinned below.

    use super::super::{SignatureError, check_timestamp_freshness};

    const NOW: i64 = 1_700_000_000;

    #[test]
    fn the_single_value_that_wrapped_onto_i64_min_is_rejected() {
        // `now - ts == 2^63` overflows to exactly `i64::MIN`, and `i64::MIN.abs()` is
        // `i64::MIN` in release, which is *not* greater than the tolerance — so the
        // replay gate returned Ok for an arbitrarily old timestamp. This is the one
        // input per clock second for which the guard failed open.
        let ts = NOW.wrapping_sub(i64::MIN);
        assert!(
            matches!(
                check_timestamp_freshness(NOW, &ts.to_string(), 300),
                Err(SignatureError::TimestampExpired)
            ),
            "a timestamp 2^63 seconds away must be expired, not accepted"
        );
    }

    #[test]
    fn i64_min_is_rejected_without_overflowing() {
        // The issue's quoted input. It was always *rejected* — but only after an
        // unchecked subtraction that panics under debug assertions, which is how this
        // test fails on the unfixed tree rather than by assertion.
        assert!(
            matches!(
                check_timestamp_freshness(NOW, &i64::MIN.to_string(), 300),
                Err(SignatureError::TimestampExpired)
            ),
            "an i64::MIN timestamp must be expired"
        );
    }

    #[test]
    fn i64_max_is_rejected_without_overflowing() {
        assert!(
            matches!(
                check_timestamp_freshness(NOW, &i64::MAX.to_string(), 300),
                Err(SignatureError::TimestampExpired)
            ),
            "an i64::MAX timestamp must be expired"
        );
    }

    #[test]
    fn a_fresh_timestamp_still_passes() {
        // The guard against overshooting: saturating the arithmetic must not start
        // rejecting ordinary deliveries.
        assert!(check_timestamp_freshness(NOW, &NOW.to_string(), 300).is_ok());
        assert!(check_timestamp_freshness(NOW, &(NOW - 299).to_string(), 300).is_ok());
    }
}

// ── #781: every verifier accepts a genuine, provider-generated delivery ───────
//
// Each fixture's signature is computed by the PROVIDER'S documented algorithm,
// implemented independently here — never by calling the verifier under test.
// That distinction is the whole point: the LemonSqueezy verifier's own tests
// were self-consistent (they generated Base64 expectations with the very code
// under test) and green while every genuine hex-signed delivery bounced 401.

mod genuine_delivery_fixtures {
    // Reason: test code — a fixture that cannot be built must stop the run, and the
    // per-scheme header table below deliberately keeps one arm per scheme even where
    // two schemes happen to read the same header name.
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::match_same_arms
    )]

    use std::collections::BTreeMap;

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use hmac::{Hmac, KeyInit as _, Mac as _};
    use sha1::Sha1;
    use sha2::Sha256;

    use crate::{
        request::InboundRequest,
        scheme::{KNOWN_SCHEMES, SchemeConfig, build_scheme},
        signature::Verified,
    };

    fn hmac_sha256(secret: &str, message: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(message);
        mac.finalize().into_bytes().to_vec()
    }

    fn hmac_sha1(secret: &str, message: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(message);
        mac.finalize().into_bytes().to_vec()
    }

    fn now() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    /// Where each scheme reads the values a delivery carries in headers.
    ///
    /// The fixtures below build a real request, so they have to put each value where
    /// the scheme looks — and a wrong entry here fails loudly rather than quietly
    /// weakening a case, because the genuine delivery stops verifying.
    struct HeaderNames {
        signature: &'static str,
        timestamp: Option<&'static str>,
        /// The event-id header, for a scheme whose signed content covers one
        /// (#1323). `None` for every scheme that signs the body alone.
        id:        Option<&'static str>,
    }

    /// A scheme that reads a signature header and nothing else.
    const fn signature_only(signature: &'static str) -> HeaderNames {
        HeaderNames {
            signature,
            timestamp: None,
            id: None,
        }
    }

    /// A scheme that reads a signature and a timestamp header.
    const fn timestamped(signature: &'static str, timestamp: &'static str) -> HeaderNames {
        HeaderNames {
            signature,
            timestamp: Some(timestamp),
            id: None,
        }
    }

    fn header_names(provider: &str) -> HeaderNames {
        match provider {
            "stripe" => signature_only("Stripe-Signature"),
            "github" => signature_only("X-Hub-Signature-256"),
            "shopify" => signature_only("X-Shopify-Hmac-Sha256"),
            "postmark" => signature_only("X-Postmark-Signature"),
            "gitlab" => signature_only("X-Gitlab-Token"),
            "slack" => timestamped("X-Slack-Signature", "X-Slack-Request-Timestamp"),
            "paddle" => signature_only("Paddle-Signature"),
            // `X-Signature` is also the generic schemes' default, below — separate
            // arms because the two answers are the same by coincidence, not by rule.
            "lemonsqueezy" => signature_only("X-Signature"),
            "twilio" => signature_only("X-Twilio-Signature"),
            "discord" => timestamped("X-Signature-Ed25519", "X-Signature-Timestamp"),
            "sendgrid" => timestamped(
                "X-Twilio-Email-Event-Webhook-Signature",
                "X-Twilio-Email-Event-Webhook-Timestamp",
            ),
            // The Standard Webhooks triple (#1323). `clerk` is the same scheme under
            // Svix's header spelling, which is the whole of what the preset fixes —
            // so two arms, and a fixture under the wrong spelling stops verifying.
            "standard-webhooks" => HeaderNames {
                signature: "webhook-signature",
                timestamp: Some("webhook-timestamp"),
                id:        Some("webhook-id"),
            },
            "clerk" => HeaderNames {
                signature: "svix-signature",
                timestamp: Some("svix-timestamp"),
                id:        Some("svix-id"),
            },
            "hmac-sha256" | "hmac-sha1" => signature_only("X-Signature"),
            // A scheme with no entry here has no fixture either, which
            // `every_registered_provider_has_genuine_and_tampered_fixtures` is the
            // gate for; answering with the generic default would let it through.
            other => panic!("{other} has no header names here; add them with its fixture"),
        }
    }

    /// Assemble the request a fixture describes, with each value under the header
    /// its scheme reads.
    fn request_of(f: &Fixture, signature: &str) -> BTreeMap<String, String> {
        let names = header_names(f.provider);
        let mut headers = BTreeMap::new();
        headers.insert(names.signature.to_ascii_lowercase(), signature.to_string());
        if let (Some(name), Some(value)) = (names.timestamp, f.timestamp.as_deref()) {
            headers.insert(name.to_ascii_lowercase(), value.to_string());
        }
        if let (Some(name), Some(value)) = (names.id, f.id.as_deref()) {
            headers.insert(name.to_ascii_lowercase(), value.to_string());
        }
        headers
    }

    /// One genuine delivery as the provider would send it, and **what verifying it
    /// must establish**.
    ///
    /// `verified` is the expected [`Verified`] value rather than a boolean, because
    /// since #1321 verification answers *what it authenticated* and the answer
    /// differs by scheme: `Verified::Body` for the twelve that sign the body alone,
    /// and `Verified::BodyWithId` carrying the signed id for Standard Webhooks
    /// (#1323). Asserting the variant is what keeps a scheme from reporting the
    /// weaker answer and still passing — `Verified::Body` from a Standard Webhooks
    /// scheme would drop the id the replay defence keys on.
    struct Fixture {
        provider:  &'static str,
        body:      Vec<u8>,
        signature: String,
        secret:    String,
        timestamp: Option<String>,
        /// The event id, for a scheme whose signed content covers one.
        id:        Option<String>,
        url:       Option<String>,
        verified:  Verified,
    }

    const BODY: &[u8] = br#"{"id":"evt_1","type":"order_created","total":1900}"#;
    const SECRET: &str = "whsec_fixture_secret";

    /// Build one genuine fixture per registered provider, each signed by the
    /// provider's documented algorithm.
    fn fixtures() -> Vec<Fixture> {
        let ts = now();
        let mut all = Vec::new();

        // Stripe: HMAC-SHA256("{t}.{body}") hex, header `t=..,v1=..`. Genuine
        // deliveries during signing-secret rotation carry MULTIPLE v1 entries and
        // the matching one need not be last (#787): model that shape directly.
        let stripe_mac = hex::encode(hmac_sha256(
            SECRET,
            format!("{ts}.")
                .as_bytes()
                .iter()
                .chain(BODY)
                .copied()
                .collect::<Vec<u8>>()
                .as_slice(),
        ));
        all.push(Fixture {
            provider:  "stripe",
            body:      BODY.to_vec(),
            signature: format!(
                "t={ts},v1={stripe_mac},v1={}",
                hex::encode(hmac_sha256("whsec_rotated_out", BODY))
            ),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // GitHub: `sha256=<hex HMAC-SHA256(body)>`.
        all.push(Fixture {
            provider:  "github",
            body:      BODY.to_vec(),
            signature: format!("sha256={}", hex::encode(hmac_sha256(SECRET, BODY))),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Shopify: Base64 HMAC-SHA256(body).
        all.push(Fixture {
            provider:  "shopify",
            body:      BODY.to_vec(),
            signature: BASE64.encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Postmark: Base64 HMAC-SHA256(body).
        all.push(Fixture {
            provider:  "postmark",
            body:      BODY.to_vec(),
            signature: BASE64.encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // GitLab: static token equality.
        all.push(Fixture {
            provider:  "gitlab",
            body:      BODY.to_vec(),
            signature: SECRET.into(),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Slack: `v0=<hex HMAC-SHA256("v0:{ts}:{body}")>` + timestamp header.
        let slack_base = format!("v0:{ts}:{}", String::from_utf8_lossy(BODY));
        all.push(Fixture {
            provider:  "slack",
            body:      BODY.to_vec(),
            signature: format!("v0={}", hex::encode(hmac_sha256(SECRET, slack_base.as_bytes()))),
            secret:    SECRET.into(),
            timestamp: Some(ts.clone()),
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Paddle: `ts=<ts>;h1=<hex HMAC-SHA256("{ts}:{body}")>`.
        let paddle_signed: Vec<u8> =
            ts.as_bytes().iter().chain(b":").chain(BODY).copied().collect();
        all.push(Fixture {
            provider:  "paddle",
            body:      BODY.to_vec(),
            signature: format!("ts={ts};h1={}", hex::encode(hmac_sha256(SECRET, &paddle_signed))),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Lemon Squeezy: hex HMAC-SHA256(body) — `hash_hmac('sha256', body, secret)`
        // in their PHP docs outputs HEX. (The verifier compared Base64: #781.)
        all.push(Fixture {
            provider:  "lemonsqueezy",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        // Twilio: Base64 HMAC-SHA1(url + sorted form params), form-encoded body.
        let twilio_url = "https://hooks.example.com/webhooks/sms";
        let twilio_body = b"Body=Hello+world&From=%2B15550001111&To=%2B15550002222".to_vec();
        // Sorted by decoded key: Body, From, To — decoded values concatenated.
        let twilio_signing = format!("{twilio_url}BodyHello worldFrom+15550001111To+15550002222");
        all.push(Fixture {
            provider:  "twilio",
            body:      twilio_body,
            signature: BASE64.encode(hmac_sha1(SECRET, twilio_signing.as_bytes())),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       Some(twilio_url.into()),
            verified:  Verified::Body,
        });

        // Twilio, JSON body (#1069): Twilio appends `bodySHA256=<hex>` to the request URI
        // and signs the URI including it. A second fixture rather than a replacement,
        // because Twilio really does have two signing strings and both must hold — and
        // because the form-only fixture is precisely why `every_tampered_delivery_is_rejected`
        // never reached the JSON branch, where flipping a body byte used to change nothing
        // the signature covered.
        {
            use sha2::Digest as _;
            let base = "https://hooks.example.com/webhooks/twilio";
            let body = br#"{"id":"twilio_json_1","type":"message.status"}"#.to_vec();
            let url = format!("{base}?bodySHA256={}", hex::encode(Sha256::digest(&body)));
            all.push(Fixture {
                provider: "twilio",
                signature: BASE64.encode(hmac_sha1(SECRET, url.as_bytes())),
                body,
                secret: SECRET.into(),
                timestamp: None,
                id: None,
                url: Some(url),
                verified: Verified::Body,
            });
        }

        // Discord: Ed25519 over `{ts}{body}`, hex signature, hex public key as secret.
        {
            use ed25519_dalek::{Signer as _, SigningKey};
            let signing_key = SigningKey::from_bytes(&[7u8; 32]);
            let message: Vec<u8> = ts.as_bytes().iter().chain(BODY).copied().collect();
            let sig = signing_key.sign(&message);
            all.push(Fixture {
                provider:  "discord",
                body:      BODY.to_vec(),
                signature: hex::encode(sig.to_bytes()),
                secret:    hex::encode(signing_key.verifying_key().to_bytes()),
                timestamp: Some(ts.clone()),
                id:        None,
                url:       None,
                verified:  Verified::Body,
            });
        }

        // SendGrid: ECDSA P-256 over `{ts}{body}`, Base64 DER signature, PEM public key.
        {
            use p256::{
                ecdsa::{DerSignature, SigningKey, signature::Signer as _},
                pkcs8::EncodePublicKey as _,
            };
            let signing_key = SigningKey::from_slice(&[11u8; 32]).unwrap();
            let message: Vec<u8> = ts.as_bytes().iter().chain(BODY).copied().collect();
            let sig: DerSignature = signing_key.sign(&message);
            let pem = signing_key
                .verifying_key()
                .to_public_key_der()
                .unwrap()
                .to_pem("PUBLIC KEY", p256::pkcs8::LineEnding::default())
                .unwrap();
            all.push(Fixture {
                provider:  "sendgrid",
                body:      BODY.to_vec(),
                signature: BASE64.encode(sig.to_bytes()),
                secret:    pem,
                timestamp: Some(ts.clone()),
                id:        None,
                url:       None,
                verified:  Verified::Body,
            });
        }

        // Standard Webhooks (#1323), and `clerk` which is the same scheme under
        // Svix's header spelling. Signed content is `{id}.{timestamp}.{body}`, the
        // key is base64 behind `whsec_`, and the credential is a space-separated list
        // of `v1,<base64>` entries — so the fixture carries an id, and verification
        // must report it.
        //
        // The secret is the spec reference library's published one (24 bytes) rather
        // than this file's `SECRET`, which is not base64 at all. The *published*
        // vectors, including Svix's 18-byte key, live in
        // `tests/standard_webhooks_test.rs`.
        {
            const SW_SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";
            let key = BASE64.decode(SW_SECRET.strip_prefix("whsec_").unwrap()).unwrap();
            for (provider, id) in [
                ("standard-webhooks", "msg_sw_fixture"),
                ("clerk", "msg_clerk_fixture"),
            ] {
                let mut signed = format!("{id}.{ts}.").into_bytes();
                signed.extend_from_slice(BODY);
                let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
                mac.update(&signed);
                all.push(Fixture {
                    provider,
                    body: BODY.to_vec(),
                    // Two entries, the matching one SECOND: a sender mid-rotation
                    // sends one per active secret in no guaranteed order, and taking
                    // only the first was #787's shape for Stripe.
                    signature: format!(
                        "v1,{} v1,{}",
                        BASE64.encode(hmac_sha256("whsec_rotated_out", &signed)),
                        BASE64.encode(mac.finalize().into_bytes())
                    ),
                    secret: SW_SECRET.into(),
                    timestamp: Some(ts.clone()),
                    id: Some(id.to_string()),
                    url: None,
                    verified: Verified::BodyWithId { id: id.to_string() },
                });
            }
        }

        // Generic HMAC verifiers: hex output.
        all.push(Fixture {
            provider:  "hmac-sha256",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });
        all.push(Fixture {
            provider:  "hmac-sha1",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha1(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            id:        None,
            url:       None,
            verified:  Verified::Body,
        });

        all
    }

    #[test]
    fn every_genuine_delivery_verifies() {
        for f in fixtures() {
            let verifier =
                build_scheme(f.provider, &SchemeConfig::default(), 300).expect(f.provider);
            let headers = request_of(&f, &f.signature);
            let result = verifier
                .verify(&InboundRequest::new(&headers, &f.body, f.url.as_deref()), &f.secret);
            assert_eq!(
                result.as_ref().ok(),
                Some(&f.verified),
                "{}: a genuine, provider-signed delivery must verify AND report what it \
                 authenticated. A scheme whose signed content covers an id must hand that \
                 id back — answering `Verified::Body` instead drops the value the replay \
                 defence keys on, and would pass a weaker assertion. Got {result:?}",
                f.provider
            );
        }
    }

    #[test]
    fn every_tampered_delivery_is_rejected() {
        for f in fixtures() {
            let verifier =
                build_scheme(f.provider, &SchemeConfig::default(), 300).expect(f.provider);
            // GitLab's scheme signs nothing (static token), so tamper the token;
            // for everyone else, tamper the body the signature covers.
            let (body, signature) = if f.provider == "gitlab" {
                (f.body.clone(), format!("{}x", f.signature))
            } else {
                let mut body = f.body.clone();
                let last = body.len() - 1;
                body[last] ^= 1;
                (body, f.signature.clone())
            };
            let headers = request_of(&f, &signature);
            let result =
                verifier.verify(&InboundRequest::new(&headers, &body, f.url.as_deref()), &f.secret);
            assert!(
                result.is_err(),
                "{}: a tampered delivery must not verify — and must say so as an error, \
                 not as some other `Ok` variant a caller could mistake for success; got \
                 {result:?}",
                f.provider
            );
        }
    }

    /// A scheme cannot be added to `KNOWN_SCHEMES` without fixtures here: this is
    /// the harness the phase demanded, so instance N+1 of "self-consistent tests,
    /// broken against the real provider" cannot land silently.
    #[test]
    fn every_registered_provider_has_genuine_and_tampered_fixtures() {
        let mut registered: Vec<String> =
            KNOWN_SCHEMES.iter().map(|name| (*name).to_string()).collect();
        registered.sort();
        let mut covered: Vec<String> = fixtures().iter().map(|f| f.provider.to_string()).collect();
        covered.sort();
        covered.dedup();
        assert_eq!(
            registered, covered,
            "every scheme in KNOWN_SCHEMES needs a genuine + tampered fixture in this file"
        );
    }
}
