//! Token revocation must be enforced under `[auth_hs256]`, not only under `[auth]`.
//!
//! `[security.token_revocation]` is an auth-mode-independent compiled-schema setting,
//! and Studio's `POST /admin/v1/users/{id}/revoke` ("revoke all of a user's active
//! sessions") calls `TokenRevocationManager::revoke_all_for_user` whatever the
//! deployment's auth mode is. But only `oidc_auth_middleware` ever consulted the
//! store, so under `[auth_hs256]` an operator responding to a compromised account got
//! `{"success": true, "message": "All sessions revoked"}` while every one of that
//! user's tokens kept working — the #749 fabricated-success shape, surviving in the
//! other auth mode.
//!
//! `[auth.social]` and `[auth.local]` both *require* `[auth_hs256]` (the callback mints
//! HS256-signed sessions this server validates itself), so this is the auth mode where
//! "log out everywhere" matters most.
//!
//! Driven through `Server::serve_on_listener` over a real socket — the mount the binary
//! actually serves. The auth layer is a `route_layer`, so nothing short of the real
//! mount observes it.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** binds an ephemeral port and sets a process-global env var for the
//! signing key → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{Hs256Config, ServerConfig},
    token_revocation::{InMemoryRevocationStore, RevocationStore, TokenRevocationManager},
};
use fraiseql_test_support::try_database_url;
use serde_json::json;

const SECRET_ENV: &str = "FRAISEQL_TEST_P19_REVOKE_HS256_SECRET";
const SECRET: &str = "p19-revocation-signing-key-at-least-32-bytes";
const ISSUER: &str = "fraiseql-revocation-test";
const AUDIENCE: &str = "fraiseql-revocation-api";
const SUBJECT: &str = "revocation-user";

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

/// A running mount plus the revocation manager it enforces, so the test can revoke
/// through the very manager the auth layer consults.
struct Rig {
    base:       String,
    revocation: Arc<TokenRevocationManager>,
    _shutdown:  tokio::sync::oneshot::Sender<()>,
}

/// Boot the real mount under `[auth_hs256]` with a test-held revocation manager.
///
/// `require_jti` mirrors `TokenRevocationConfig::require_jti`: with it set, a bearer
/// carrying no `jti` is refused outright, exactly as under OIDC.
async fn serve(url: &str, require_jti: bool) -> Rig {
    let store: Arc<dyn RevocationStore> = Arc::new(InMemoryRevocationStore::new());
    let revocation = Arc::new(TokenRevocationManager::new(store, require_jti, false, 86_400));

    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled = true with no origins.
        cors_enabled: false,
        database_url: url.to_string(),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .expect("Server::new with [auth_hs256]")
        .with_revocation_manager(Arc::clone(&revocation));

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    Rig {
        base: format!("http://127.0.0.1:{port}"),
        revocation,
        _shutdown: tx,
    }
}

/// Mint an HS256 bearer this server accepts. `jti` is optional because the
/// server's own session minting (`fraiseql_auth::Claims`) carries `iat` always and
/// `jti` only as a custom claim — the `revoke-all` epoch must work without one.
fn mint_token(jti: Option<&str>, issued_secs_ago: i64) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let mut claims = json!({
        "sub": SUBJECT,
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now - issued_secs_ago,
        "exp": now + 3600,
    });
    if let Some(jti) = jti {
        claims["jti"] = json!(jti);
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

/// POST a trivial GraphQL document with `token` as the bearer; return the status.
async fn post_status(base: &str, token: &str) -> u16 {
    reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .bearer_auth(token)
        .json(&json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("request")
        .status()
        .as_u16()
}

fn url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    } else {
        // Fixed valid value, set unconditionally: this suite is the only reader of
        // this name, and it is read at boot.
        std::env::set_var(SECRET_ENV, SECRET);
    }
    url
}

/// The defect: a `revoke-all` epoch — Studio's "revoke all sessions", the
/// compromised-account path — must stop a still-valid HS256 bearer.
#[tokio::test]
async fn revoke_all_refuses_a_still_valid_hs256_bearer() {
    let Some(url) = url_or_skip("revoke_all_refuses_a_still_valid_hs256_bearer") else {
        return;
    };
    let rig = serve(&url, false).await;

    // Issued a minute ago so it is unambiguously at-or-before the epoch recorded
    // below (the epoch check is `iat <= epoch`, at one-second resolution).
    let token = mint_token(None, 60);
    assert_ne!(
        post_status(&rig.base, &token).await,
        401,
        "a freshly minted, unrevoked bearer must authenticate"
    );

    rig.revocation
        .revoke_all_for_user(SUBJECT)
        .await
        .expect("revoke-all must succeed");

    assert_eq!(
        post_status(&rig.base, &token).await,
        401,
        "after revoke-all the same bearer must be refused: Studio reports the sessions \
         revoked, so a token that keeps working is a fabricated success (#749's shape)"
    );
}

/// Single-token revocation by `jti`, the `POST /auth/revoke` path.
#[tokio::test]
async fn revoked_jti_refuses_the_hs256_bearer() {
    let Some(url) = url_or_skip("revoked_jti_refuses_the_hs256_bearer") else {
        return;
    };
    let rig = serve(&url, false).await;

    let token = mint_token(Some("hs256-jti-1"), 0);
    assert_ne!(
        post_status(&rig.base, &token).await,
        401,
        "an unrevoked bearer carrying a jti must authenticate"
    );

    rig.revocation.revoke("hs256-jti-1", 3600).await.expect("revoke must succeed");

    assert_eq!(
        post_status(&rig.base, &token).await,
        401,
        "a bearer whose jti is revoked must be refused"
    );
}

/// The accepting half: revocation enforcement must not refuse everything. A
/// different user's revoke-all, and an unrelated revoked jti, leave this bearer
/// working.
#[tokio::test]
async fn revocation_enforcement_leaves_unrevoked_bearers_working() {
    let Some(url) = url_or_skip("revocation_enforcement_leaves_unrevoked_bearers_working") else {
        return;
    };
    let rig = serve(&url, false).await;

    rig.revocation.revoke_all_for_user("some-other-user").await.expect("revoke-all");
    rig.revocation.revoke("some-other-jti", 3600).await.expect("revoke");

    let token = mint_token(Some("hs256-jti-untouched"), 60);
    assert_ne!(
        post_status(&rig.base, &token).await,
        401,
        "another user's revoke-all and an unrelated revoked jti must not refuse this bearer"
    );
}

/// `require_jti = true` is the configured-strictness knob, and it must mean the same
/// thing under HS256 as under OIDC: a bearer with no `jti` cannot be revocation-checked,
/// so it is refused.
#[tokio::test]
async fn require_jti_refuses_a_jti_less_hs256_bearer() {
    let Some(url) = url_or_skip("require_jti_refuses_a_jti_less_hs256_bearer") else {
        return;
    };
    let rig = serve(&url, true).await;

    assert_eq!(
        post_status(&rig.base, &mint_token(None, 0)).await,
        401,
        "with require_jti the jti-less bearer is unrevocable, so it must be refused"
    );
    assert_ne!(
        post_status(&rig.base, &mint_token(Some("hs256-jti-2"), 0)).await,
        401,
        "the same configuration must still accept a bearer that carries a jti"
    );
}
