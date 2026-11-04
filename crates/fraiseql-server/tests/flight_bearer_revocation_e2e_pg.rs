//! A bearer the server has revoked opens no Arrow Flight session.
//!
//! Flight authenticates once, at the handshake: the bearer is validated and answered with
//! a short-lived session token every later call presents. A revoked bearer, by its `jti`
//! or by a `revoke-all` of its user, is refused on every HTTP route; the handshake is the
//! Flight route's equivalent, and is held to the same check.
//!
//! Driven through `Server::serve_with_shutdown`, the binary's entry (Flight does not start
//! on `serve_on_listener`), with Flight on a port of its own. Bearers come from the shared ES256
//! test issuer; revocation goes through the very manager the server enforces.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** binds ephemeral ports → run `--test-threads=1`.
#![cfg(all(feature = "arrow", feature = "auth"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_arrow::{
    arrow_flight::{
        Action, FlightData, FlightDescriptor, HandshakeRequest, Ticket,
        decode::FlightRecordBatchStream, error::FlightError,
        flight_service_client::FlightServiceClient,
    },
    tonic::{Code, Request, Status, transport::Endpoint},
};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition},
};
use fraiseql_server::{
    Server,
    server_config::ServerConfig,
    token_revocation::{
        InMemoryRevocationStore, RevocationError, RevocationStore, TokenRevocationManager,
    },
};
use fraiseql_test_support::{
    issuer::{Claims, TestIssuer},
    try_database_url,
};
use futures::{StreamExt as _, TryStreamExt as _};
use serde_json::json;

mod common;

use crate::common::test_issuer::oidc_config;

const SUBJECT: &str = "flight-revocation-user";
const SCHEMA: &str = "flight_bearer_revocation";

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut note = TypeDefinition::new("Note", format!("{SCHEMA}.v_note"));
    note.fields = vec![FieldDefinition::new("id", FieldType::Int)];
    schema.types.push(note);
    schema.queries.push(
        QueryDefinition::new("notes", "Note")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_note")),
    );
    schema.build_indexes();
    schema
}

async fn provision(url: &str) {
    let pool = sqlx::PgPool::connect(url).await.unwrap();
    for statement in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE VIEW {SCHEMA}.v_note AS \
             SELECT g AS id, jsonb_build_object('id', g) AS data FROM generate_series(1, 2) g"
        ),
    ] {
        // Reason: test SQL built from names and values this test defines.
        sqlx::query(sqlx::AssertSqlSafe(statement)).execute(&pool).await.unwrap();
    }
}

/// A running mount, the revocation manager it enforces, and the issuer its bearers come
/// from.
struct Rig {
    base:       String,
    flight:     String,
    revocation: Arc<TokenRevocationManager>,
    issuer:     TestIssuer,
    _shutdown:  tokio::sync::oneshot::Sender<()>,
}

/// Boot the binary's mount, Flight included. `require_jti` mirrors
/// `TokenRevocationConfig::require_jti`.
async fn serve(url: &str, require_jti: bool) -> Rig {
    serve_over(url, Arc::new(InMemoryRevocationStore::new()), require_jti).await
}

/// [`serve`], over `store`.
async fn serve_over(url: &str, store: Arc<dyn RevocationStore>, require_jti: bool) -> Rig {
    // Read when the Flight service is built.
    std::env::set_var("FLIGHT_SESSION_SECRET", "flight-bearer-revocation-session-secret");
    provision(url).await;
    let issuer = TestIssuer::start().await;
    let revocation = Arc::new(TokenRevocationManager::new(store, require_jti, false, 86_400));

    let free_port = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let (http_addr, flight_addr) = (free_port(), free_port());
    let config = ServerConfig {
        cors_enabled: false,
        bind_addr: http_addr,
        database_url: url.to_string(),
        flight_bind_addr: flight_addr,
        auth: Some(oidc_config(&issuer, true)),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .unwrap()
        .with_revocation_manager(Arc::clone(&revocation));

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_with_shutdown(async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    Rig {
        base: format!("http://{http_addr}"),
        flight: format!("http://{flight_addr}"),
        revocation,
        issuer,
        _shutdown: tx,
    }
}

impl Rig {
    fn mint(&self, jti: &str, issued_secs_ago: i64) -> String {
        self.mint_claims(Some(jti), issued_secs_ago)
    }

    fn mint_claims(&self, jti: Option<&str>, issued_secs_ago: i64) -> String {
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap();
        let claims: Claims = self
            .issuer
            .claims()
            .set("sub", SUBJECT)
            .set("iat", now - issued_secs_ago)
            .set("exp", now + 3600);
        let claims = match jti {
            Some(jti) => claims.set("jti", jti),
            None => claims.without("jti"),
        };
        self.issuer.mint(&claims)
    }

    async fn http_status(&self, bearer: &str) -> u16 {
        reqwest::Client::new()
            .post(format!("{}/graphql", self.base))
            .bearer_auth(bearer)
            .json(&json!({ "query": "{ notes { id } }" }))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn client(&self) -> FlightServiceClient<fraiseql_arrow::tonic::transport::Channel> {
        let channel = Endpoint::from_shared(self.flight.clone()).unwrap().connect().await.unwrap();
        FlightServiceClient::new(channel)
    }

    /// The session token the handshake answers `bearer` with.
    async fn handshake(&self, bearer: &str) -> Result<String, Status> {
        let request = HandshakeRequest {
            protocol_version: 0,
            payload:          format!("Bearer {bearer}").into_bytes().into(),
        };
        let mut answers = self
            .client()
            .await
            .handshake(futures::stream::iter([request]))
            .await?
            .into_inner();
        let session = answers.next().await.expect("the handshake answered nothing")?.payload;
        Ok(String::from_utf8(session.to_vec()).unwrap())
    }

    /// The rows `{ notes { id } }` streams over `session`.
    async fn rows(&self, session: &str) -> Result<usize, Status> {
        let ticket = json!({ "type": "GraphQLQuery", "query": "{ notes { id } }" }).to_string();
        let mut request = Request::new(Ticket {
            ticket: ticket.into_bytes().into(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {session}").parse().unwrap());
        let data = self
            .client()
            .await
            .do_get(request)
            .await?
            .into_inner()
            .map_err(FlightError::from);
        let batches: Vec<_> = FlightRecordBatchStream::new_from_flight_data(data)
            .try_collect()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(batches.iter().map(|batch| batch.num_rows()).sum())
    }

    /// `do_put` of one message over `session`: the call's own status, or `Ok` if served.
    async fn put(&self, session: &str) -> Result<(), Status> {
        let data = futures::stream::iter([FlightData {
            flight_descriptor: Some(FlightDescriptor::new_path(vec!["t".to_string()])),
            ..Default::default()
        }]);
        let mut request = Request::new(data);
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {session}").parse().unwrap());
        let mut answers = self.client().await.do_put(request).await?.into_inner();
        while let Some(answer) = answers.next().await {
            answer?;
        }
        Ok(())
    }

    /// `do_action` `HealthCheck` over `session`.
    async fn health_check(&self, session: &str) -> Result<(), Status> {
        let mut request = Request::new(Action {
            r#type: "HealthCheck".to_string(),
            body:   Vec::new().into(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {session}").parse().unwrap());
        let mut answers = self.client().await.do_action(request).await?.into_inner();
        while let Some(answer) = answers.next().await {
            answer?;
        }
        Ok(())
    }
}

/// `outcome` is the revocation refusal: Unauthenticated, naming the revocation.
fn assert_revoked<T: std::fmt::Debug>(outcome: Result<T, Status>, what: &str) {
    match outcome {
        Ok(served) => panic!("{what}: served after revocation: {served:?}"),
        Err(status) => {
            assert_eq!(status.code(), Code::Unauthenticated, "{what}: {status}");
            assert!(status.message().to_lowercase().contains("revoked"), "{what}: {status}");
        },
    }
}

fn url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// The accepting half: an unrevoked bearer opens a session and reads over it.
#[tokio::test]
async fn an_unrevoked_bearer_opens_a_session_and_reads() {
    let Some(url) = url_or_skip("an_unrevoked_bearer_opens_a_session_and_reads") else {
        return;
    };
    let rig = serve(&url, false).await;
    rig.revocation.revoke_all_for_user("some-other-user").await.unwrap();
    rig.revocation.revoke("some-other-jti", 3600).await.unwrap();

    let session = rig.handshake(&rig.mint("jti-kept", 60)).await.unwrap();
    assert_eq!(rig.rows(&session).await.unwrap(), 2);
    rig.health_check(&session).await.unwrap();
}

#[tokio::test]
async fn a_bearer_revoked_by_its_jti_opens_no_session() {
    let Some(url) = url_or_skip("a_bearer_revoked_by_its_jti_opens_no_session") else {
        return;
    };
    let rig = serve(&url, false).await;
    let bearer = rig.mint("jti-revoked", 0);
    rig.revocation.revoke("jti-revoked", 3600).await.unwrap();

    assert_eq!(rig.http_status(&bearer).await, 401, "control: HTTP refuses it");
    assert_revoked(rig.handshake(&bearer).await, "a revoked jti");
}

#[tokio::test]
async fn a_bearer_revoked_by_revoke_all_opens_no_session() {
    let Some(url) = url_or_skip("a_bearer_revoked_by_revoke_all_opens_no_session") else {
        return;
    };
    let rig = serve(&url, false).await;
    let bearer = rig.mint("jti-before-revoke-all", 60);
    rig.revocation.revoke_all_for_user(SUBJECT).await.unwrap();

    assert_eq!(rig.http_status(&bearer).await, 401, "control: HTTP refuses it");
    assert_revoked(rig.handshake(&bearer).await, "revoke-all");
}

/// A session opened before its bearer is revoked by `jti` is refused on its next call:
/// a read, a write and an action.
#[tokio::test]
async fn a_session_opened_before_its_jti_is_revoked_is_refused_after() {
    let Some(url) = url_or_skip("a_session_opened_before_its_jti_is_revoked_is_refused_after")
    else {
        return;
    };
    let rig = serve(&url, false).await;
    let session = rig.handshake(&rig.mint("jti-later-revoked", 0)).await.unwrap();
    assert_eq!(rig.rows(&session).await.unwrap(), 2, "served before the revocation");
    rig.health_check(&session).await.unwrap();
    rig.revocation.revoke("jti-later-revoked", 3600).await.unwrap();

    assert_revoked(rig.rows(&session).await, "do_get");
    assert_revoked(rig.put(&session).await, "do_put");
    assert_revoked(rig.health_check(&session).await, "do_action");
}

/// The same after a `revoke-all` of the session's user.
#[tokio::test]
async fn a_session_opened_before_revoke_all_is_refused_after() {
    let Some(url) = url_or_skip("a_session_opened_before_revoke_all_is_refused_after") else {
        return;
    };
    let rig = serve(&url, false).await;
    let session = rig.handshake(&rig.mint("jti-before-epoch", 60)).await.unwrap();
    assert_eq!(rig.rows(&session).await.unwrap(), 2, "served before the revocation");
    rig.revocation.revoke_all_for_user(SUBJECT).await.unwrap();

    assert_revoked(rig.rows(&session).await, "do_get");
    assert_revoked(rig.put(&session).await, "do_put");
    assert_revoked(rig.health_check(&session).await, "do_action");
}

/// `require_jti`: a bearer with no `jti` cannot be asked about, so it opens no session;
/// one carrying a `jti` still does.
#[tokio::test]
async fn require_jti_refuses_a_jti_less_bearer_at_the_handshake() {
    let Some(url) = url_or_skip("require_jti_refuses_a_jti_less_bearer_at_the_handshake") else {
        return;
    };
    let rig = serve(&url, true).await;
    let bearer = rig.mint_claims(None, 0);

    assert_eq!(rig.http_status(&bearer).await, 401, "control: HTTP refuses it");
    match rig.handshake(&bearer).await {
        Ok(_) => panic!("a jti-less bearer opened a session under require_jti"),
        Err(status) => assert_eq!(status.code(), Code::Unauthenticated, "{status}"),
    }
    let session = rig.handshake(&rig.mint("jti-present", 0)).await.unwrap();
    assert_eq!(rig.rows(&session).await.unwrap(), 2);
}

/// The accepting half of `revoke-all`: a bearer issued after its subject's revoke-all opens
/// a session, and is served over it, since the session is asked about by the bearer's own
/// `iat`.
#[tokio::test]
async fn a_bearer_issued_after_its_subjects_revoke_all_is_served() {
    let Some(url) = url_or_skip("a_bearer_issued_after_its_subjects_revoke_all_is_served") else {
        return;
    };
    let rig = serve(&url, false).await;
    rig.revocation.revoke_all_for_user(SUBJECT).await.unwrap();
    // `iat` has a resolution of one second: issue strictly after the epoch.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let bearer = rig.mint("jti-after-epoch", 0);

    assert_ne!(rig.http_status(&bearer).await, 401, "control: HTTP serves it");
    let session = rig.handshake(&bearer).await.unwrap();
    assert_eq!(rig.rows(&session).await.unwrap(), 2);
    rig.health_check(&session).await.unwrap();
}

/// A revocation store that cannot answer.
struct UnreachableStore;

#[async_trait::async_trait]
impl RevocationStore for UnreachableStore {
    async fn is_revoked(&self, _jti: &str) -> Result<bool, RevocationError> {
        Err(RevocationError::Backend("unreachable".to_string()))
    }

    async fn revoke(&self, _jti: &str, _ttl_secs: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("unreachable".to_string()))
    }

    async fn revoke_all_for_user(&self, _sub: &str, _ttl_secs: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("unreachable".to_string()))
    }

    async fn user_revoked_after(&self, _sub: &str) -> Result<Option<i64>, RevocationError> {
        Err(RevocationError::Backend("unreachable".to_string()))
    }
}

/// Fail-closed, as HTTP is: while the store cannot answer, a bearer opens no session.
#[tokio::test]
async fn a_bearer_opens_no_session_while_the_revocation_store_cannot_answer() {
    let Some(url) =
        url_or_skip("a_bearer_opens_no_session_while_the_revocation_store_cannot_answer")
    else {
        return;
    };
    let rig = serve_over(&url, Arc::new(UnreachableStore), false).await;
    let bearer = rig.mint("jti-unanswered", 0);

    assert_eq!(rig.http_status(&bearer).await, 401, "control: HTTP refuses it");
    match rig.handshake(&bearer).await {
        Ok(_) => panic!("a session was opened while the revocation store could not answer"),
        Err(status) => {
            assert_eq!(status.code(), Code::Unauthenticated, "{status}");
            assert!(status.message().contains("unavailable"), "{status}");
        },
    }
}

/// The document this suite serves loads, checked with no database.
#[test]
fn the_document_loads_without_a_database() {
    CompiledSchema::from_json(&serde_json::to_string(&schema()).unwrap(), false).unwrap();
}
