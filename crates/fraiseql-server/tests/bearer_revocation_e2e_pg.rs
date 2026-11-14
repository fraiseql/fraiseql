//! Token revocation is enforced on every bearer the server authenticates.
//!
//! `[security.token_revocation]` is a compiled-schema setting, and Studio's
//! `POST /admin/v1/users/{id}/revoke` ("revoke all of a user's active sessions") calls
//! `TokenRevocationManager::revoke_all_for_user`. An auth layer that never consulted the
//! store answered `{"success": true, "message": "All sessions revoked"}` while every one
//! of that user's tokens kept working (#1112) — the #749 fabricated-success shape. Both
//! revocation shapes (a single `jti`, the `revoke-all` epoch), the claim shapes that must
//! not slip past them, the accepting half, and `require_jti`.
//!
//! Driven through `Server::serve_on_listener` over a real socket — the mount the binary
//! actually serves. The auth layer is a `route_layer`, so nothing short of the real
//! mount observes it. Bearers come from the shared ES256 test issuer.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** binds an ephemeral port → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::ServerConfig,
    token_revocation::{InMemoryRevocationStore, RevocationStore, TokenRevocationManager},
};
use fraiseql_test_support::{
    issuer::{Claims, TestIssuer},
    try_database_url,
};
use serde_json::json;

mod common;

use crate::common::test_issuer::oidc_config;

const SUBJECT: &str = "revocation-user";

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

/// A running mount plus the revocation manager it enforces, so the test can revoke
/// through the very manager the auth layer consults, and the issuer its bearers come
/// from. The issuer outlives the server: the validator reads its JWKS on first use.
struct Rig {
    base:       String,
    revocation: Arc<TokenRevocationManager>,
    issuer:     TestIssuer,
    _shutdown:  tokio::sync::oneshot::Sender<()>,
}

/// Boot the real mount, trusting a fresh test issuer, with a test-held revocation
/// manager.
///
/// `require_jti` mirrors `TokenRevocationConfig::require_jti`: with it set, a bearer
/// carrying no `jti` is refused outright.
async fn serve(url: &str, require_jti: bool) -> Rig {
    let issuer = TestIssuer::start().await;
    let store: Arc<dyn RevocationStore> = Arc::new(InMemoryRevocationStore::new());
    let revocation = Arc::new(TokenRevocationManager::new(store, require_jti, false, 86_400));

    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled = true with no origins.
        cors_enabled: false,
        database_url: url.to_string(),
        auth: Some(oidc_config(&issuer, true)),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .expect("Server::new with [auth]")
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
        issuer,
        _shutdown: tx,
    }
}

impl Rig {
    /// The claims of a bearer this server accepts, issued `issued_secs_ago` seconds ago
    /// and carrying `jti`, or no `jti` at all. `jti` is optional because the server's
    /// own session minting carries `iat` always and `jti` only as a custom claim — the
    /// `revoke-all` epoch must work without one.
    fn claims(&self, jti: Option<&str>, issued_secs_ago: i64) -> Claims {
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        )
        .expect("epoch seconds fit i64");
        let claims = self
            .issuer
            .claims()
            .set("sub", SUBJECT)
            .set("iat", now - issued_secs_ago)
            .set("exp", now + 3600);
        match jti {
            Some(jti) => claims.set("jti", jti),
            None => claims.without("jti"),
        }
    }

    /// A bearer carrying [`Self::claims`].
    fn mint_token(&self, jti: Option<&str>, issued_secs_ago: i64) -> String {
        self.issuer.mint(&self.claims(jti, issued_secs_ago))
    }

    /// [`Self::claims`] without a `jti`, with `edit` applied, for claim shapes
    /// [`Self::mint_token`] cannot express.
    fn mint_edited(&self, issued_secs_ago: i64, edit: impl FnOnce(Claims) -> Claims) -> String {
        self.issuer.mint(&edit(self.claims(None, issued_secs_ago)))
    }
}

/// The bearer authenticates before `revoke-all` and is refused after it.
async fn assert_revoke_all_refuses(rig: &Rig, token: &str, what: &str) {
    assert_ne!(post_status(&rig.base, token).await, 401, "{what}: must authenticate before");
    rig.revocation
        .revoke_all_for_user(SUBJECT)
        .await
        .expect("revoke-all must succeed");
    assert_eq!(
        post_status(&rig.base, token).await,
        401,
        "{what}: must be refused after revoke-all"
    );
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
    }
    url
}

/// The defect: a `revoke-all` epoch — Studio's "revoke all sessions", the
/// compromised-account path — must stop a still-valid bearer.
#[tokio::test]
async fn revoke_all_refuses_a_still_valid_bearer() {
    let Some(url) = url_or_skip("revoke_all_refuses_a_still_valid_bearer") else {
        return;
    };
    let rig = serve(&url, false).await;

    // Issued a minute ago so it is unambiguously at-or-before the epoch recorded
    // below (the epoch check is `iat <= epoch`, at one-second resolution).
    let token = rig.mint_token(None, 60);
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

/// A bearer with no `iat` cannot show it was issued after a `revoke-all`, so the
/// `revoke-all` refuses it like any older bearer.
#[tokio::test]
async fn revoke_all_refuses_a_bearer_without_iat() {
    let Some(url) = url_or_skip("revoke_all_refuses_a_bearer_without_iat") else {
        return;
    };
    let rig = serve(&url, false).await;
    let token = rig.mint_edited(60, |claims| claims.without("iat"));
    assert_revoke_all_refuses(&rig, &token, "a bearer without iat").await;
}

/// The validator refuses a `jti` that is not a string before any revocation check
/// runs, so such a bearer never reaches it: a number cannot stand in for the `jti` a
/// `revoke` names, nor hide the bearer's `iat` from a `revoke-all`. Nothing is revoked
/// here; the same bearer with a string `jti` authenticates (`revoked_jti_refuses_the_bearer`).
#[tokio::test]
async fn a_bearer_whose_jti_is_a_number_is_refused() {
    let Some(url) = url_or_skip("a_bearer_whose_jti_is_a_number_is_refused") else {
        return;
    };
    let rig = serve(&url, false).await;
    let token = rig.mint_edited(0, |claims| claims.set("jti", 42));
    assert_eq!(post_status(&rig.base, &token).await, 401, "jti = 42");
}

/// The validator refuses an `iat` that is not an integer before any revocation
/// check runs, so such a bearer never reaches it.
#[tokio::test]
async fn a_bearer_whose_iat_is_not_an_integer_is_refused() {
    let Some(url) = url_or_skip("a_bearer_whose_iat_is_not_an_integer_is_refused") else {
        return;
    };
    let rig = serve(&url, false).await;
    for iat in [json!(1000.5), json!("1000")] {
        let token = rig.mint_edited(60, |claims| claims.set("iat", iat.clone()));
        assert_eq!(post_status(&rig.base, &token).await, 401, "iat = {iat}");
    }
}

/// Single-token revocation by `jti`, the `POST /auth/revoke` path.
#[tokio::test]
async fn revoked_jti_refuses_the_bearer() {
    let Some(url) = url_or_skip("revoked_jti_refuses_the_bearer") else {
        return;
    };
    let rig = serve(&url, false).await;

    let token = rig.mint_token(Some("jti-1"), 0);
    assert_ne!(
        post_status(&rig.base, &token).await,
        401,
        "an unrevoked bearer carrying a jti must authenticate"
    );

    rig.revocation.revoke("jti-1", 3600).await.expect("revoke must succeed");

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

    let token = rig.mint_token(Some("jti-untouched"), 60);
    assert_ne!(
        post_status(&rig.base, &token).await,
        401,
        "another user's revoke-all and an unrelated revoked jti must not refuse this bearer"
    );
}

/// `require_jti = true` is the configured-strictness knob: a bearer with no `jti` cannot
/// be revocation-checked, so it is refused.
#[tokio::test]
async fn require_jti_refuses_a_jti_less_bearer() {
    let Some(url) = url_or_skip("require_jti_refuses_a_jti_less_bearer") else {
        return;
    };
    let rig = serve(&url, true).await;

    assert_eq!(
        post_status(&rig.base, &rig.mint_token(None, 0)).await,
        401,
        "with require_jti the jti-less bearer is unrevocable, so it must be refused"
    );
    assert_ne!(
        post_status(&rig.base, &rig.mint_token(Some("jti-2"), 0)).await,
        401,
        "the same configuration must still accept a bearer that carries a jti"
    );
}

/// The document this suite serves loads, checked with no database.
///
/// Every other test here reaches the document only after it has found a database, so in
/// a run without one they skip before it is built and a load-time refusal of it reports
/// as a pass (`2b843cd27`: 12 tests red for a session under a green preflight).
/// This one needs nothing but the loader, so that refusal cannot hide.
///
/// This document is deserialised with `from_value` and never meets `finish_load`.
/// `from_value` ignores a top-level key it does not know, so what can refuse it is a
/// field it does not send becoming required, or a field changing type.
#[test]
fn the_document_loads_without_a_database() {
    schema();
}
