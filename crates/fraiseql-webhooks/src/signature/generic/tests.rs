#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_hmac_sha256() {
    let verifier = HmacSha256Verifier;
    let payload = b"test";
    let secret = "secret";

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}

#[test]
fn test_hmac_sha1() {
    let verifier = HmacSha1Verifier;
    let payload = b"test";
    let secret = "secret";

    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert!(verifier.verify(payload, &signature, secret, None, None).unwrap());
}
