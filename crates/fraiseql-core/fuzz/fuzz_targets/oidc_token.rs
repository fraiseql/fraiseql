#![no_main]
//! Bearer-token validation through `OidcValidator`, the one validator every bearer
//! reaches. Any string must be answered with `Ok` or `Err`, never a panic.
//!
//! The validator trusts one issuer whose key this process holds, so no request is
//! made. Its key is the P-256 base point: a valid public key that anyone can sign
//! for, which does not matter here, since the target asks only that no input
//! panics the header, `kid` and key handling ahead of the signature check.

use std::sync::{Arc, OnceLock};

use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_jwks::{BoxFuture, Jwk, JwksError, JwksKeys};
use libfuzzer_sys::fuzz_target;

const ISSUER: &str = "https://fuzz.example.com";
const KID: &str = "fuzz";

#[derive(Debug)]
struct HeldKey(Jwk);

impl JwksKeys for HeldKey {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        Box::pin(async move { Ok((kid == KID).then(|| self.0.clone())) })
    }

    fn invalidate(&self) {}

    fn refresh(&self) -> BoxFuture<'_, Result<usize, JwksError>> {
        Box::pin(async { Ok(1) })
    }
}

fn harness() -> &'static (tokio::runtime::Runtime, OidcValidator) {
    static HARNESS: OnceLock<(tokio::runtime::Runtime, OidcValidator)> = OnceLock::new();
    HARNESS.get_or_init(|| {
        let key = Jwk {
            kty:     "EC".to_string(),
            kid:     Some(KID.to_string()),
            alg:     Some("ES256".to_string()),
            key_use: Some("sig".to_string()),
            n:       None,
            e:       None,
            x:       Some("axfR8uEsQkf4vOblY6RA8ncDfYEt6zOg9KE5RdiYwpY".to_string()),
            y:       Some("T-NC4v4af5uO5-tKfA-eFivOM1drMV7Oy7ZAaDe_UfU".to_string()),
            crv:     Some("P-256".to_string()),
            x5c:     Vec::new(),
        };
        let config = OidcConfig {
            issuer: Some(ISSUER.to_string()),
            audience: Some("fuzz-api".to_string()),
            required: true,
            allowed_algorithms: vec!["ES256".to_string()],
            ..OidcConfig::default()
        };
        let validator = OidcValidator::from_issuer_keys(config, Arc::new(HeldKey(key)))
            .expect("a held-key validator builds");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a current-thread runtime builds");
        (runtime, validator)
    })
}

fuzz_target!(|data: &str| {
    let (runtime, validator) = harness();
    let _ = runtime.block_on(validator.validate_token(data));
});
