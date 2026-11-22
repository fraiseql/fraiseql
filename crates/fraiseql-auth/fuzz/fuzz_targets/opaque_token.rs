#![no_main]
//! The opaque reset and email-verification token a user presents: `selector.verifier`,
//! each half base64url without padding.
//!
//! Beyond "no panic", an accepted token has exactly one spelling: each half re-encodes
//! to the text presented, so the selector the store is looked up by is the selector the
//! token was issued with, and no second spelling of one token redeems. And the verifier
//! hash compared against the store is the SHA-256 of the presented verifier's bytes.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use fraiseql_auth::local_password::parse_opaque_token;
use libfuzzer_sys::fuzz_target;
use sha2::{Digest as _, Sha256};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let Some((selector, verifier_hash)) = parse_opaque_token(&text) else {
        return;
    };

    let (selector_text, verifier_text) =
        text.split_once('.').expect("an accepted token has a separator");
    assert_eq!(selector, selector_text, "the lookup selector is not the one presented");

    let selector_bytes = URL_SAFE_NO_PAD.decode(selector_text).expect("accepted selector");
    let verifier_bytes = URL_SAFE_NO_PAD.decode(verifier_text).expect("accepted verifier");
    assert_eq!(
        URL_SAFE_NO_PAD.encode(&selector_bytes),
        selector_text,
        "a selector was accepted in a second spelling"
    );
    assert_eq!(
        URL_SAFE_NO_PAD.encode(&verifier_bytes),
        verifier_text,
        "a verifier was accepted in a second spelling"
    );
    assert_eq!(verifier_hash, Sha256::digest(&verifier_bytes).to_vec());
});
