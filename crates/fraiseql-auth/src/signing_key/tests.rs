#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use jsonwebtoken::{DecodingKey, Validation, decode, decode_header, jwk::Jwk};

use super::*;

#[derive(Serialize, serde::Deserialize, Debug, PartialEq)]
struct Claims {
    sub: String,
    exp: u64,
}

fn claims() -> Claims {
    Claims {
        sub: "alice".to_string(),
        exp: 4_102_444_800, // 2100-01-01
    }
}

/// Verify `token` against the JWK `jwk` publishes, as any OIDC validator would.
fn verify(token: &str, jwk: &Value) -> jsonwebtoken::errors::Result<Claims> {
    let jwk: Jwk = serde_json::from_value(jwk.clone()).unwrap();
    let key = DecodingKey::from_jwk(&jwk).unwrap();
    let mut validation = Validation::new(Algorithm::ES256);
    validation.validate_aud = false;
    decode::<Claims>(token, &key, &validation).map(|data| data.claims)
}

#[test]
fn a_token_it_signs_verifies_against_the_key_it_publishes() {
    let (key, _) = SigningKey::generate().unwrap();
    let token = key.sign("at+jwt", &claims()).unwrap();

    assert_eq!(verify(&token, key.jwk()).unwrap(), claims());
    let header = decode_header(&token).unwrap();
    assert_eq!(header.alg, Algorithm::ES256);
    assert_eq!(header.kid.as_deref(), Some(key.kid()));
    assert_eq!(header.typ.as_deref(), Some("at+jwt"));
}

#[test]
fn a_token_does_not_verify_against_another_keys_jwk() {
    let (signer, _) = SigningKey::generate().unwrap();
    let (other, _) = SigningKey::generate().unwrap();
    assert_ne!(signer.kid(), other.kid());

    let token = signer.sign("at+jwt", &claims()).unwrap();
    verify(&token, other.jwk()).unwrap_err();
}

#[test]
fn the_published_jwk_holds_the_public_half_only() {
    let (key, _) = SigningKey::generate().unwrap();
    let mut fields: Vec<&str> = key.jwk().as_object().unwrap().keys().map(String::as_str).collect();
    fields.sort_unstable();

    assert_eq!(fields, ["alg", "crv", "kid", "kty", "use", "x", "y"], "no private `d`");
    assert_eq!(key.jwk()["alg"], "ES256");
    assert_eq!(key.jwk()["use"], "sig");
    assert_eq!(key.jwk()["kid"], key.kid());
}

#[test]
fn a_key_read_back_from_its_pem_is_the_same_key() {
    let (key, pem) = SigningKey::generate().unwrap();
    let again = SigningKey::from_pem(key.kid().to_string(), &pem).unwrap();

    assert_eq!(again.jwk(), key.jwk());
    let token = again.sign("at+jwt", &claims()).unwrap();
    assert_eq!(verify(&token, key.jwk()).unwrap(), claims());
}

#[test]
fn the_sealing_aad_names_the_kid() {
    assert_eq!(aad("k1"), "fraiseql/auth.signing_key/v1/k1");
    assert_ne!(aad("k1"), aad("k2"));
}

#[test]
fn debug_never_prints_the_private_key() {
    let (key, pem) = SigningKey::generate().unwrap();
    let shown = format!("{key:?}");
    assert!(shown.contains(key.kid()));
    assert!(!shown.contains(pem.lines().nth(1).unwrap()), "{shown}");
}

#[test]
fn a_ring_publishes_its_keys_with_the_active_one_first() {
    let (active, _) = SigningKey::generate().unwrap();
    let (retired, _) = SigningKey::generate().unwrap();
    let ring = KeyRing {
        published: vec![active.jwk().clone(), retired.jwk().clone()],
        active,
    };

    let kids: Vec<&str> = ring.kids().collect();
    assert_eq!(kids, [ring.active().kid(), retired.kid()]);
    assert_eq!(ring.jwks()["keys"].as_array().unwrap().len(), 2);
}

const KEK_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

#[test]
fn a_kek_of_64_hex_digits_is_read() {
    let key = key_encryption_key_from_hex("VAR", KEK_HEX).unwrap();
    assert_eq!(key[0], 0x00);
    assert_eq!(key[31], 0x1f);
}

#[test]
fn a_short_kek_is_refused() {
    let err = key_encryption_key_from_hex("VAR", &KEK_HEX[..62]).unwrap_err();
    assert!(err.to_string().contains("31 bytes"), "{err}");
}

#[test]
fn a_long_kek_is_refused() {
    let err = key_encryption_key_from_hex("VAR", &format!("{KEK_HEX}00")).unwrap_err();
    assert!(err.to_string().contains("33 bytes"), "{err}");
}

#[test]
fn a_kek_that_is_not_hex_is_refused() {
    let err = key_encryption_key_from_hex("VAR", &"zz".repeat(32)).unwrap_err();
    assert!(err.to_string().contains("not hex"), "{err}");
}

#[test]
fn a_missing_kek_variable_is_refused_by_name() {
    let var = "FRAISEQL_AUTH_SIGNING_KEK_NEVER_SET_IN_ANY_TEST";
    let err = key_encryption_key_from_env(var).unwrap_err();
    assert!(err.to_string().contains(var), "{err}");
}
