//! The first-party issuer's key ring in `auth.signing_key`: one active key, retired keys
//! published for exactly their window, private halves sealed to their own `kid`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates the shared `auth.signing_key` on setup → run
//! `--test-threads=1`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use aes_gcm::{
    Aes256Gcm, KeyInit as _, Nonce,
    aead::{Aead as _, Payload},
};
use fraiseql_auth::{
    AuthError, KeyWrap, PgSigningKeyStore,
    signing_key::{ROTATION_PERIOD_SECS, SIGNING_KEY_SCHEMA_SQL},
};
use fraiseql_test_support::try_database_url;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, jwk::Jwk};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// A real AEAD, so a wrong `aad` or a wrong key is refused, never ignored.
struct TestWrap(Aes256Gcm);

impl std::fmt::Debug for TestWrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TestWrap")
    }
}

impl TestWrap {
    fn new(byte: u8) -> Arc<Self> {
        Arc::new(Self(Aes256Gcm::new(&[byte; 32].into())))
    }
}

impl KeyWrap for TestWrap {
    fn seal(&self, plaintext: &str, aad: &str) -> fraiseql_auth::Result<Vec<u8>> {
        let nonce: [u8; 12] = rand::random();
        let mut sealed = nonce.to_vec();
        sealed.extend(
            self.0
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext.as_bytes(),
                        aad: aad.as_bytes(),
                    },
                )
                .unwrap(),
        );
        Ok(sealed)
    }

    fn open(&self, ciphertext: &[u8], aad: &str) -> fraiseql_auth::Result<String> {
        let (nonce, body) = ciphertext.split_at(12);
        let plain = self
            .0
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| AuthError::Internal {
                message: "the signing key does not open".to_string(),
            })?;
        Ok(String::from_utf8(plain).unwrap())
    }
}

/// A fixed instant, so every window below is exact.
const T0: u64 = 1_900_000_000;
/// The window a retired key stays published for in these tests.
const WINDOW: u64 = 3_600;

async fn fresh() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(8).connect(&url).await.unwrap();
    // Dropped, not truncated: every test applies the DDL itself, so an index the DDL
    // stopped creating cannot linger from an earlier run.
    sqlx::query("DROP TABLE IF EXISTS auth.signing_key")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(SIGNING_KEY_SCHEMA_SQL).execute(&pool).await.unwrap();
    Some(pool)
}

macro_rules! pool_or_skip {
    () => {
        match fresh().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping signing-key test: DATABASE_URL not set");
                return;
            },
        }
    };
}

fn store(pool: &PgPool) -> PgSigningKeyStore {
    PgSigningKeyStore::new(pool.clone(), TestWrap::new(7))
}

async fn active_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth.signing_key WHERE retired_at IS NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn verifies(token: &str, jwks: &Value) -> bool {
    let header = jsonwebtoken::decode_header(token).unwrap();
    let Some(jwk) = jwks["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|jwk| jwk["kid"].as_str() == header.kid.as_deref())
    else {
        return false;
    };
    let jwk: Jwk = serde_json::from_value(jwk.clone()).unwrap();
    let mut validation = Validation::new(Algorithm::ES256);
    validation.validate_aud = false;
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    decode::<Value>(token, &DecodingKey::from_jwk(&jwk).unwrap(), &validation).is_ok()
}

#[tokio::test]
async fn an_empty_ring_gets_exactly_one_active_key_and_keeps_it() {
    let pool = pool_or_skip!();
    let store = store(&pool);

    let first = store.ensure_active(T0).await.unwrap().expect("an empty ring gets a key");
    assert_eq!(store.ensure_active(T0 + 60).await.unwrap(), None, "a fresh key is kept");

    let ring = store.load(T0 + 60, WINDOW).await.unwrap();
    assert_eq!(ring.active().kid(), first);
    assert_eq!(ring.kids().collect::<Vec<_>>(), [first.as_str()]);
    assert_eq!(active_count(&pool).await, 1);
}

#[tokio::test]
async fn the_active_key_is_replaced_once_it_is_thirty_days_old() {
    let pool = pool_or_skip!();
    let store = store(&pool);
    let first = store.ensure_active(T0).await.unwrap().unwrap();

    assert_eq!(store.ensure_active(T0 + ROTATION_PERIOD_SECS - 1).await.unwrap(), None);
    let second = store.ensure_active(T0 + ROTATION_PERIOD_SECS).await.unwrap().unwrap();

    assert_ne!(first, second);
    let ring = store.load(T0 + ROTATION_PERIOD_SECS, WINDOW).await.unwrap();
    assert_eq!(ring.active().kid(), second);
    assert_eq!(ring.kids().collect::<Vec<_>>(), [second.as_str(), first.as_str()]);
}

#[tokio::test]
async fn a_retired_key_is_published_for_its_window_and_not_after() {
    let pool = pool_or_skip!();
    let store = store(&pool);
    store.ensure_active(T0).await.unwrap();
    let old = store.load(T0, WINDOW).await.unwrap();
    let token = old.active().sign("at+jwt", &serde_json::json!({ "sub": "alice" })).unwrap();

    let retired_at = T0 + 100;
    store.rotate(retired_at).await.unwrap();

    let inside = store.load(retired_at + WINDOW - 1, WINDOW).await.unwrap();
    assert!(verifies(&token, &inside.jwks()), "still published one second before the end");
    assert_ne!(inside.active().kid(), old.active().kid(), "but it no longer signs");

    let after = store.load(retired_at + WINDOW, WINDOW).await.unwrap();
    assert!(!verifies(&token, &after.jwks()), "unpublished once the window has passed");
    assert_eq!(after.kids().count(), 1);
}

#[tokio::test]
async fn the_window_is_read_at_load_so_a_longer_window_reaches_keys_already_retired() {
    let pool = pool_or_skip!();
    let store = store(&pool);
    store.ensure_active(T0).await.unwrap();
    let retired = store.load(T0, WINDOW).await.unwrap().active().kid().to_string();
    store.rotate(T0).await.unwrap();

    let at = T0 + WINDOW + 10;
    let short = store.load(at, WINDOW).await.unwrap();
    assert!(!short.kids().any(|kid| kid == retired));
    let longer = store.load(at, WINDOW * 2).await.unwrap();
    assert!(longer.kids().any(|kid| kid == retired), "the same row, a longer window");
}

#[tokio::test]
async fn the_sweep_deletes_only_keys_past_their_window() {
    let pool = pool_or_skip!();
    let store = store(&pool);
    store.ensure_active(T0).await.unwrap();
    store.rotate(T0 + 10).await.unwrap(); // retires key 1 at T0+10
    store.rotate(T0 + 20).await.unwrap(); // retires key 2 at T0+20

    assert_eq!(store.sweep(T0 + 10 + WINDOW - 1, WINDOW).await.unwrap(), 0);
    assert_eq!(store.sweep(T0 + 10 + WINDOW, WINDOW).await.unwrap(), 1);
    assert_eq!(store.sweep(T0 + 20 + WINDOW, WINDOW).await.unwrap(), 1);
    assert_eq!(store.sweep(T0 * 2, WINDOW).await.unwrap(), 0, "the active key is never swept");
    assert_eq!(active_count(&pool).await, 1);
}

#[tokio::test]
async fn the_database_refuses_a_second_active_key() {
    let pool = pool_or_skip!();
    store(&pool).ensure_active(T0).await.unwrap();

    let err = sqlx::query(
        "INSERT INTO auth.signing_key (kid, alg, public_jwk, private_key_enc, created_at) \
         VALUES ('intruder', 'ES256', '{}', '\\x00', 1)",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    let code = err.as_database_error().and_then(|e| e.code()).map(|c| c.into_owned());
    assert_eq!(code.as_deref(), Some("23505"), "unique violation, got {err}");
}

#[tokio::test]
async fn concurrent_rotations_leave_one_active_key() {
    let pool = pool_or_skip!();
    let store = Arc::new(store(&pool));
    store.ensure_active(T0).await.unwrap();

    let rotations: Vec<_> = (0..8_u64)
        .map(|i| {
            let store = Arc::clone(&store);
            tokio::spawn(async move { store.rotate(T0 + i).await })
        })
        .collect();
    for rotation in rotations {
        rotation.await.unwrap().unwrap();
    }

    assert_eq!(active_count(&pool).await, 1);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM auth.signing_key")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 9, "the first key and eight rotations");
}

#[tokio::test]
async fn a_sealed_key_moved_to_another_row_does_not_open() {
    let pool = pool_or_skip!();
    let store = store(&pool);
    store.ensure_active(T0).await.unwrap();
    store.rotate(T0 + 1).await.unwrap();

    sqlx::query(
        "UPDATE auth.signing_key SET private_key_enc = \
           (SELECT private_key_enc FROM auth.signing_key WHERE retired_at IS NOT NULL) \
         WHERE retired_at IS NULL",
    )
    .execute(&pool)
    .await
    .unwrap();

    store.load(T0 + 2, WINDOW).await.unwrap_err();
}

#[tokio::test]
async fn a_ring_sealed_under_one_key_does_not_open_under_another() {
    let pool = pool_or_skip!();
    store(&pool).ensure_active(T0).await.unwrap();

    PgSigningKeyStore::new(pool.clone(), TestWrap::new(8))
        .load(T0, WINDOW)
        .await
        .unwrap_err();
}

#[tokio::test]
async fn the_private_key_is_never_stored_in_the_clear() {
    let pool = pool_or_skip!();
    store(&pool).ensure_active(T0).await.unwrap();

    let sealed: Vec<u8> = sqlx::query_scalar("SELECT private_key_enc FROM auth.signing_key")
        .fetch_one(&pool)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&sealed);
    assert!(!text.contains("PRIVATE KEY"), "the PEM is stored sealed");
    let jwk: Value = sqlx::query_scalar("SELECT public_jwk FROM auth.signing_key")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(jwk.get("d").is_none(), "the published column holds no private part");
}

mod live {
    use std::time::Duration;

    use fraiseql_auth::LiveKeyRing;
    use serde_json::json;

    use super::*;

    const AN_HOUR: Duration = Duration::from_hours(1);

    async fn live(pool: &PgPool, cooldown: Duration) -> LiveKeyRing {
        LiveKeyRing::open(store(pool), T0, WINDOW, cooldown).await.unwrap()
    }

    fn active_kid(ring: &LiveKeyRing) -> String {
        ring.current().unwrap().active().kid().to_string()
    }

    #[tokio::test]
    async fn a_ring_not_read_yet_signs_nothing_and_reads_the_table_on_first_lookup() {
        let pool = pool_or_skip!();
        let kid = store(&pool).ensure_active(T0).await.unwrap().unwrap();
        let ring = LiveKeyRing::new(store(&pool), WINDOW, AN_HOUR);

        assert!(ring.current().is_err(), "nothing to sign with before the first read");
        assert!(ring.published_key(&kid, T0).await.unwrap().is_some());
        assert_eq!(active_kid(&ring), kid);
    }

    #[tokio::test]
    async fn opening_an_empty_ring_creates_its_key_and_signs_with_it() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;

        assert_eq!(active_count(&pool).await, 1);
        let current = ring.current().unwrap();
        let token = current.active().sign("at+jwt", &json!({ "sub": "u" })).unwrap();
        assert!(verifies(&token, &current.jwks()), "the ring's own JWKS verifies its token");
        assert_eq!(current.published_count(), 1);
        assert_eq!(
            ring.published_key(&active_kid(&ring), T0).await.unwrap(),
            current.jwks()["keys"].get(0).cloned()
        );
    }

    #[tokio::test]
    async fn a_key_another_replica_rotated_in_is_found_on_first_sight() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        let rotated = store(&pool).rotate(T0 + 1).await.unwrap();

        let found = ring.published_key(&rotated, T0 + 1).await.unwrap();
        assert_eq!(found.and_then(|jwk| jwk["kid"].as_str().map(str::to_owned)), Some(rotated));
    }

    #[tokio::test]
    async fn unknown_kids_reread_the_table_at_most_once_per_cooldown() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        let first = store(&pool).rotate(T0 + 1).await.unwrap();
        assert!(
            ring.published_key(&first, T0 + 1).await.unwrap().is_some(),
            "first miss re-reads"
        );

        // A burst of kids nobody published, then a real one: none of them re-reads.
        let second = store(&pool).rotate(T0 + 2).await.unwrap();
        for random in ["kid-1", "kid-2", "kid-3"] {
            assert!(ring.published_key(random, T0 + 2).await.unwrap().is_none());
        }
        assert!(
            ring.published_key(&second, T0 + 2).await.unwrap().is_none(),
            "inside the cooldown, a key rotated in since the last re-read is not looked up"
        );
    }

    #[tokio::test]
    async fn after_the_cooldown_an_unknown_kid_rereads_again() {
        let pool = pool_or_skip!();
        let ring = live(&pool, Duration::ZERO).await;
        let first = store(&pool).rotate(T0 + 1).await.unwrap();
        assert!(ring.published_key(&first, T0 + 1).await.unwrap().is_some());
        let second = store(&pool).rotate(T0 + 2).await.unwrap();
        assert!(ring.published_key(&second, T0 + 2).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn invalidating_rereads_on_the_next_lookup_whatever_the_cooldown() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        let first = store(&pool).rotate(T0 + 1).await.unwrap();
        assert!(ring.published_key(&first, T0 + 1).await.unwrap().is_some());
        let second = store(&pool).rotate(T0 + 2).await.unwrap();

        ring.invalidate();
        assert!(ring.published_key(&second, T0 + 2).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn reloading_reports_how_many_keys_the_ring_publishes() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        store(&pool).rotate(T0 + 1).await.unwrap();
        assert_eq!(ring.reload(T0 + 1).await.unwrap(), 2, "the active key and the retired one");
    }

    #[tokio::test]
    async fn a_tick_picks_up_another_replicas_rotation_and_signs_with_it() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        let rotated = store(&pool).rotate(T0 + 1).await.unwrap();
        assert_ne!(active_kid(&ring), rotated, "not seen before the tick");

        ring.tick(T0 + 1).await.unwrap();
        assert_eq!(active_kid(&ring), rotated);
    }

    #[tokio::test]
    async fn a_tick_rotates_a_key_past_its_period_and_sweeps_keys_past_their_window() {
        let pool = pool_or_skip!();
        let ring = live(&pool, AN_HOUR).await;
        let first = active_kid(&ring);

        let later = T0 + ROTATION_PERIOD_SECS;
        ring.tick(later).await.unwrap();
        assert_ne!(active_kid(&ring), first, "rotated at its period");
        assert!(
            ring.current().unwrap().published_key(&first).is_some(),
            "the old key is still published"
        );

        ring.tick(later + WINDOW).await.unwrap();
        assert!(ring.current().unwrap().published_key(&first).is_none(), "past its window");
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM auth.signing_key")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 1, "and swept from the table");
    }

    fn claims(lifetime: u64) -> fraiseql_auth::Claims {
        fraiseql_auth::Claims {
            sub:   "u".to_string(),
            iat:   T0,
            exp:   T0 + lifetime,
            nbf:   Some(T0),
            iss:   "https://api.example.com".to_string(),
            aud:   vec!["my-api".to_string()],
            extra: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn the_signer_signs_an_access_token_with_the_active_key() {
        use fraiseql_auth::{AccessTokenSigner as _, RingSigner};

        let pool = pool_or_skip!();
        let ring = std::sync::Arc::new(live(&pool, AN_HOUR).await);
        let token = RingSigner::new(std::sync::Arc::clone(&ring), 900)
            .sign_access_token(&claims(900))
            .unwrap();

        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.typ.as_deref(), Some("at+jwt"));
        assert_eq!(header.kid.as_deref(), Some(active_kid(&ring).as_str()));
        assert!(verifies(&token, &ring.current().unwrap().jwks()));
    }

    #[tokio::test]
    async fn the_signer_refuses_a_token_meant_to_outlive_the_ring_window() {
        use fraiseql_auth::{AccessTokenSigner as _, RingSigner};

        let pool = pool_or_skip!();
        let ring = std::sync::Arc::new(live(&pool, AN_HOUR).await);
        let signer = RingSigner::new(ring, 900);
        assert!(signer.sign_access_token(&claims(901)).is_err(), "one second too long");
    }

    #[tokio::test]
    async fn the_signer_signs_nothing_before_the_ring_was_first_read() {
        use fraiseql_auth::{AccessTokenSigner as _, RingSigner};

        let pool = pool_or_skip!();
        store(&pool).ensure_active(T0).await.unwrap();
        let ring = std::sync::Arc::new(LiveKeyRing::new(store(&pool), WINDOW, AN_HOUR));
        assert!(RingSigner::new(ring, 900).sign_access_token(&claims(900)).is_err());
    }
}
