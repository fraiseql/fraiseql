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

// ── #781: every verifier accepts a genuine, provider-generated delivery ───────
//
// Each fixture's signature is computed by the PROVIDER'S documented algorithm,
// implemented independently here — never by calling the verifier under test.
// That distinction is the whole point: the LemonSqueezy verifier's own tests
// were self-consistent (they generated Base64 expectations with the very code
// under test) and green while every genuine hex-signed delivery bounced 401.

mod genuine_delivery_fixtures {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use hmac::{Hmac, KeyInit as _, Mac as _};
    use sha1::Sha1;
    use sha2::Sha256;

    use crate::signature::ProviderRegistry;

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

    /// One genuine delivery as the provider would send it.
    struct Fixture {
        provider:  &'static str,
        body:      Vec<u8>,
        signature: String,
        secret:    String,
        timestamp: Option<String>,
        url:       Option<String>,
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
            url:       None,
        });

        // GitHub: `sha256=<hex HMAC-SHA256(body)>`.
        all.push(Fixture {
            provider:  "github",
            body:      BODY.to_vec(),
            signature: format!("sha256={}", hex::encode(hmac_sha256(SECRET, BODY))),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });

        // Shopify: Base64 HMAC-SHA256(body).
        all.push(Fixture {
            provider:  "shopify",
            body:      BODY.to_vec(),
            signature: BASE64.encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });

        // Postmark: Base64 HMAC-SHA256(body).
        all.push(Fixture {
            provider:  "postmark",
            body:      BODY.to_vec(),
            signature: BASE64.encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });

        // GitLab: static token equality.
        all.push(Fixture {
            provider:  "gitlab",
            body:      BODY.to_vec(),
            signature: SECRET.into(),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });

        // Slack: `v0=<hex HMAC-SHA256("v0:{ts}:{body}")>` + timestamp header.
        let slack_base = format!("v0:{ts}:{}", String::from_utf8_lossy(BODY));
        all.push(Fixture {
            provider:  "slack",
            body:      BODY.to_vec(),
            signature: format!("v0={}", hex::encode(hmac_sha256(SECRET, slack_base.as_bytes()))),
            secret:    SECRET.into(),
            timestamp: Some(ts.clone()),
            url:       None,
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
            url:       None,
        });

        // Lemon Squeezy: hex HMAC-SHA256(body) — `hash_hmac('sha256', body, secret)`
        // in their PHP docs outputs HEX. (The verifier compared Base64: #781.)
        all.push(Fixture {
            provider:  "lemonsqueezy",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
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
            url:       Some(twilio_url.into()),
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
                url: Some(url),
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
                url:       None,
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
                url:       None,
            });
        }

        // Generic HMAC verifiers: hex output.
        all.push(Fixture {
            provider:  "hmac-sha256",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha256(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });
        all.push(Fixture {
            provider:  "hmac-sha1",
            body:      BODY.to_vec(),
            signature: hex::encode(hmac_sha1(SECRET, BODY)),
            secret:    SECRET.into(),
            timestamp: None,
            url:       None,
        });

        all
    }

    #[test]
    fn every_genuine_delivery_verifies() {
        let registry = ProviderRegistry::new();
        for f in fixtures() {
            let verifier = registry.get(f.provider).expect(f.provider);
            let result = verifier.verify(
                &f.body,
                &f.signature,
                &f.secret,
                f.timestamp.as_deref(),
                f.url.as_deref(),
            );
            assert!(
                matches!(result, Ok(true)),
                "{}: a genuine, provider-signed delivery must verify; got {result:?}",
                f.provider
            );
        }
    }

    #[test]
    fn every_tampered_delivery_is_rejected() {
        let registry = ProviderRegistry::new();
        for f in fixtures() {
            let verifier = registry.get(f.provider).expect(f.provider);
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
            let result = verifier.verify(
                &body,
                &signature,
                &f.secret,
                f.timestamp.as_deref(),
                f.url.as_deref(),
            );
            assert!(
                !matches!(result, Ok(true)),
                "{}: a tampered delivery must not verify; got {result:?}",
                f.provider
            );
        }
    }

    /// A provider cannot be added to the registry without fixtures here: this is
    /// the harness the phase demanded, so instance N+1 of "self-consistent tests,
    /// broken against the real provider" cannot land silently.
    #[test]
    fn every_registered_provider_has_genuine_and_tampered_fixtures() {
        let registry = ProviderRegistry::new();
        let mut registered = registry.providers();
        registered.sort();
        let mut covered: Vec<String> = fixtures().iter().map(|f| f.provider.to_string()).collect();
        covered.sort();
        covered.dedup();
        assert_eq!(
            registered, covered,
            "every registered provider needs a genuine + tampered fixture in this file"
        );
    }
}
