#![no_main]
//! A JWKS document as an issuer serves it at its `jwks_uri`, decoded as `JwksFetcher`
//! decodes the response body, then every key through `Jwk::decoding_key`, the step a
//! token's `kid` reaches.
//!
//! Beyond "no panic":
//! - The set round-trips through JSON (`Serialize` is part of `Jwk`'s contract: key material
//!   crosses a `&str` seam that way).
//! - `decoding_key` answers the same twice, and an `oct` key is always refused.
//! - Lookup agrees with the list: `find` returns the first key with that `kid`, `kids` lists every
//!   `kid` in order, and a set is never missing its own keys.

use fraiseql_jwks::JwkSet;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(set) = serde_json::from_slice::<JwkSet>(data) else {
        return;
    };

    let text = serde_json::to_string(&set).expect("a decoded set serializes");
    let again: JwkSet = serde_json::from_str(&text).expect("a serialized set decodes");
    assert_eq!(set, again, "the set changed through JSON");

    for key in &set.keys {
        let first = key.decoding_key().is_ok();
        assert_eq!(first, key.decoding_key().is_ok(), "decoding_key answered differently twice");
        if key.kty == "oct" {
            assert!(!first, "an `oct` key was accepted as a verification key");
        }
    }

    let listed: Vec<&str> = set.keys.iter().filter_map(|k| k.kid.as_deref()).collect();
    assert_eq!(set.kids().collect::<Vec<_>>(), listed, "kids() disagrees with the keys");
    for kid in &listed {
        let found = set.find(kid).expect("a listed kid is found");
        let first = set.keys.iter().find(|k| k.kid.as_deref() == Some(*kid));
        assert_eq!(Some(found), first, "find returned a key other than the first with its kid");
    }
    assert!(set.kids_missing_from(&set).is_empty(), "a set is missing its own keys");
});
