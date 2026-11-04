use std::sync::Arc;

use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::wire_policy;
use crate::routes::graphql::AppState;

/// A served Flight service builds its principals with the schema's tenant claim, as
/// every other transport does.
#[test]
fn a_served_flight_service_builds_principals_with_the_schemas_tenant_claim() {
    let schema = CompiledSchema::default();
    let tenant_claim = schema.tenant_claim().to_string();
    let app = AppState::new(Arc::new(Executor::new(schema, Arc::new(FailingAdapter::new()))));
    let mut service = fraiseql_arrow::FraiseQLFlightService::new();
    wire_policy(&mut service, app);
    assert_eq!(service.tenant_claim(), Some(tenant_claim.as_str()));
}

/// The served Flight surface consults the server's revocation store: a token the store
/// refuses opens no session, and a session ends once its token is revoked.
///
/// Boots the real server with its Flight service (`Server::with_flight_service`, then
/// `serve_with_shutdown`, the binary's entry point and the one Flight is started from,
/// through `wire_policy`) and drives it with a Flight client over a real socket. The store is the
/// server's own `TokenRevocationManager`, so what Flight refuses is what `/graphql` refuses.
#[cfg(feature = "auth")]
#[allow(clippy::unwrap_used, clippy::panic)] // Reason: test module, a failed setup must fail loudly
mod revocation_e2e {
    use std::sync::Arc;

    use fraiseql_arrow::arrow_flight::{
        Action, HandshakeRequest, flight_service_client::FlightServiceClient,
    };
    use tonic::{Code, transport::Channel};

    use crate::{
        Server,
        server_config::ServerConfig,
        testing::idp::TestIdp,
        token_revocation::{InMemoryRevocationStore, TokenRevocationManager},
    };

    const SECRET: &str = "flight-revocation-e2e-session-key";
    const SUBJECT: &str = "flight-e2e-subject";

    struct Served {
        manager: Arc<TokenRevocationManager>,
        flight:  FlightServiceClient<Channel>,
        _stop:   tokio::sync::oneshot::Sender<()>,
        /// Serves the keys the server's validator fetches, for as long as the test runs.
        idp:     TestIdp,
    }

    /// The server, with `require_jti` as given, serving Flight on a free port.
    async fn served(require_jti: bool) -> Served {
        let idp = TestIdp::start().await;
        let manager = Arc::new(TokenRevocationManager::new(
            Arc::new(InMemoryRevocationStore::new()),
            require_jti,
            false,
            3600,
        ));
        let free = || {
            let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            free.local_addr().unwrap()
        };
        let flight_addr = free();
        let config = ServerConfig {
            cors_enabled: false,
            bind_addr: free(),
            flight_bind_addr: flight_addr,
            ..ServerConfig::default()
        };
        let mut service = fraiseql_arrow::FraiseQLFlightService::new().with_session_secret(SECRET);
        service.set_oidc_validator(idp.validator(false));
        let server = Box::pin(Server::with_flight_service(
            config,
            fraiseql_core::schema::CompiledSchema::new(),
            Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
            None,
            Some(service),
        ))
        .await
        .unwrap()
        .with_revocation_manager(Arc::clone(&manager));

        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            server
                .serve_with_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .expect("the server serves");
        });

        let endpoint = format!("http://{flight_addr}");
        let mut attempts = 0;
        let flight = loop {
            match tonic::transport::Endpoint::from_shared(endpoint.clone())
                .unwrap()
                .connect()
                .await
            {
                Ok(channel) => break FlightServiceClient::new(channel),
                Err(_) if attempts < 100 => {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                },
                Err(e) => panic!("Flight never came up on {flight_addr}: {e}"),
            }
        };
        Served {
            manager,
            flight,
            _stop: stop,
            idp,
        }
    }

    /// Handshake with `bearer`: the session token, or the refusal.
    async fn handshake(
        flight: &mut FlightServiceClient<Channel>,
        bearer: &str,
    ) -> Result<String, tonic::Status> {
        let request = tokio_stream::iter(vec![HandshakeRequest {
            protocol_version: 0,
            payload:          format!("Bearer {bearer}").into_bytes().into(),
        }]);
        let mut responses = flight.handshake(request).await?.into_inner();
        let response = responses.message().await?.expect("a handshake answer");
        Ok(String::from_utf8(response.payload.to_vec()).unwrap())
    }

    async fn health(
        flight: &mut FlightServiceClient<Channel>,
        session: &str,
    ) -> Result<(), tonic::Status> {
        let mut request = tonic::Request::new(Action {
            r#type: "HealthCheck".to_string(),
            body:   Vec::new().into(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {session}").parse().unwrap());
        flight.do_action(request).await.map(|_| ())
    }

    fn assert_refused(result: Result<impl std::fmt::Debug, tonic::Status>, reason: &str) {
        let status = result.expect_err("refused");
        assert_eq!(status.code(), Code::Unauthenticated, "{status:?}");
        assert_eq!(status.message(), reason, "worded as the HTTP transports word it");
    }

    #[tokio::test]
    async fn a_revoked_token_opens_no_session() {
        let mut served = served(false).await;
        served.manager.revoke("j-revoked", 3600).await.unwrap();
        assert_refused(
            handshake(&mut served.flight, &served.idp.token(SUBJECT, "j-revoked")).await,
            "Token has been revoked",
        );
    }

    #[tokio::test]
    async fn a_session_ends_when_its_token_is_revoked() {
        let mut served = served(false).await;
        let session = handshake(&mut served.flight, &served.idp.token(SUBJECT, "j-live"))
            .await
            .expect("a live token opens a session");
        health(&mut served.flight, &session).await.expect("the session is served");

        served.manager.revoke("j-live", 3600).await.unwrap();
        assert_refused(health(&mut served.flight, &session).await, "Token has been revoked");
    }

    /// Parity with HTTP: a token without a `jti`, where the store requires one.
    #[tokio::test]
    async fn a_token_without_jti_is_refused_where_one_is_required() {
        let mut served = served(true).await;
        let now = fraiseql_time::Timestamp::now().as_second();
        assert_refused(
            handshake(&mut served.flight, &served.idp.token_with(SUBJECT, None, now)).await,
            "Token lacks required jti claim",
        );
        // An empty `jti` is no `jti`, as on HTTP.
        assert_refused(
            handshake(&mut served.flight, &served.idp.token_with(SUBJECT, Some(""), now)).await,
            "Token lacks required jti claim",
        );
    }

    /// Parity with HTTP: a token without a `jti`, where none is required, is admitted, and
    /// still ends with its subject's revoke-all.
    #[tokio::test]
    async fn a_token_without_jti_ends_with_its_subjects_revoke_all() {
        let mut served = served(false).await;
        let issued = fraiseql_time::Timestamp::now().as_second() - 60;
        let session = handshake(&mut served.flight, &served.idp.token_with(SUBJECT, None, issued))
            .await
            .expect("no jti, none required: admitted");
        health(&mut served.flight, &session).await.expect("served");

        served.manager.revoke_all_for_user(SUBJECT).await.unwrap();
        assert_refused(health(&mut served.flight, &session).await, "Token has been revoked");
        assert_refused(
            handshake(&mut served.flight, &served.idp.token_with(SUBJECT, None, issued)).await,
            "Token has been revoked",
        );
    }
}
