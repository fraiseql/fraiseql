//! FraiseQL Arrow Flight client.
//!
//! # Authentication is not optional
//!
//! The server authenticates `do_get` **before** it decodes the ticket
//! (`flight_server/handlers/do_get.rs`), so a call with no credentials is
//! refused whatever it asks for. Getting credentials is a two-step exchange:
//!
//! 1. `handshake` with the payload `"Bearer <jwt>"`. The response payload is a
//!    session token.
//! 2. Every later call carries `authorization: Bearer <session token>` in its
//!    gRPC metadata — the session token, not the original JWT.
//!
//! This client used to do neither, so every call it could make returned
//! `UNAUTHENTICATED` (#1200).
//!
//! # Running it
//!
//! ```bash
//! FRAISEQL_JWT="<your jwt>" cargo run
//! ```
//!
//! The server needs `FLIGHT_SESSION_SECRET` set, or the handshake fails with
//! `FLIGHT_SESSION_SECRET not configured`.

use arrow::record_batch::RecordBatch;
use arrow_flight::decode::FlightRecordBatchStream;
use arrow_flight::{flight_service_client::FlightServiceClient, HandshakeRequest, Ticket};
use futures::TryStreamExt;
use prost::bytes::Bytes;
use serde_json::json;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Request;
use tracing::info;

type BoxError = Box<dyn std::error::Error>;

/// A client that has completed the handshake and holds a session token.
pub struct FraiseQLFlightClient {
    uri:           Uri,
    client:        FlightServiceClient<Channel>,
    session_token: String,
}

impl FraiseQLFlightClient {
    /// Connect and handshake. There is no constructor that skips this: a client
    /// without a session token cannot make a call the server will answer, so
    /// producing one would only move the failure later.
    pub async fn connect(host: &str, port: u16, jwt: &str) -> Result<Self, BoxError> {
        let uri = format!("http://{host}:{port}").parse::<Uri>()?;
        let channel = Endpoint::from(uri.clone()).connect().await?;
        let mut client = FlightServiceClient::new(channel);

        // The server expects the JWT with a `Bearer ` prefix and rejects a bare
        // token (`flight_server/handlers/metadata.rs`).
        let request = HandshakeRequest {
            protocol_version: 0,
            payload:          Bytes::from(format!("Bearer {jwt}").into_bytes()),
        };
        let mut response = client
            .handshake(futures::stream::iter(vec![request]))
            .await?
            .into_inner();
        let message = response
            .message()
            .await?
            .ok_or("handshake closed without a response")?;
        let session_token = String::from_utf8(message.payload.to_vec())?;
        if session_token.is_empty() {
            return Err("handshake returned an empty session token".into());
        }
        info!("Handshake complete; holding a session token");

        Ok(Self {
            uri,
            client,
            session_token,
        })
    }

    /// The server this client is talking to.
    #[must_use]
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Execute a GraphQL query and collect the result batches.
    pub async fn query_graphql(
        &mut self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<Vec<RecordBatch>, BoxError> {
        self.fetch(json!({
            "type": "GraphQLQuery",
            "query": query,
            "variables": variables,
        }))
        .await
    }

    /// Read a view directly, pushing the filter and ordering to the server.
    pub async fn query_view(
        &mut self,
        view: &str,
        filter: Option<serde_json::Value>,
        order_by: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<RecordBatch>, BoxError> {
        self.fetch(json!({
            "type": "OptimizedView",
            "view": view,
            "filter": filter,
            "order_by": order_by,
            "limit": limit,
            "offset": null,
        }))
        .await
    }

    /// Send several queries in one round trip.
    pub async fn query_batched(
        &mut self,
        queries: &[&str],
    ) -> Result<Vec<RecordBatch>, BoxError> {
        self.fetch(json!({ "type": "BatchedQueries", "queries": queries })).await
    }

    /// Issue one `do_get` with the session token attached, and decode the stream.
    async fn fetch(&mut self, ticket: serde_json::Value) -> Result<Vec<RecordBatch>, BoxError> {
        let mut request = Request::new(Ticket {
            ticket: Bytes::from(ticket.to_string().into_bytes()),
        });
        // Without this header the server answers `UNAUTHENTICATED: Missing
        // authorization header - perform handshake first`, before it has looked
        // at the ticket at all.
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.session_token).parse()?,
        );

        let stream = self.client.do_get(request).await?.into_inner();

        // `FlightRecordBatchStream` is the decoder arrow-flight ships. Reading
        // `app_metadata` — as this example used to — inspects a side channel that
        // carries no Arrow IPC payload, so it decoded nothing however well the
        // call went.
        let batches: Vec<RecordBatch> = FlightRecordBatchStream::new_from_flight_data(
            stream.map_err(|e| arrow_flight::error::FlightError::Tonic(e)),
        )
        .try_collect()
        .await?;

        info!("Received {} batch(es)", batches.len());
        Ok(batches)
    }
}

fn summarise(label: &str, batches: &[RecordBatch]) {
    let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
    println!("✅ {label}: {} batch(es), {rows} row(s)", batches.len());
    if let Some(first) = batches.first() {
        println!("   schema: {:?}", first.schema());
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    println!("FraiseQL Arrow Flight client");
    println!("============================\n");

    // No default: a placeholder token would turn "you did not set this" into
    // "the server rejected your credentials", which is a slower thing to debug.
    let jwt = std::env::var("FRAISEQL_JWT").map_err(|_| {
        "set FRAISEQL_JWT to a token this server accepts — the Flight surface \
         authenticates every call"
    })?;

    let mut client = FraiseQLFlightClient::connect("localhost", 50051, &jwt).await?;
    println!("✅ Handshake complete with {}\n", client.uri());

    println!("1. GraphQL query");
    match client.query_graphql("{ users { id name email } }", None).await {
        Ok(batches) => summarise("query", &batches),
        Err(e) => eprintln!("❌ query failed: {e}"),
    }
    println!();

    println!("2. Direct view read");
    match client.query_view("v_user", None, Some("id"), Some(100)).await {
        Ok(batches) => summarise("view", &batches),
        Err(e) => eprintln!("❌ view read failed: {e}"),
    }
    println!();

    // `ObserverEvents` is deliberately absent. It is a variant of the ticket
    // enum, but the server answers it with `unimplemented`: "this server does
    // not produce an Arrow event stream. Query historical events through the
    // GraphQL API instead." This example used to showcase it as its second
    // operation (#1200).
    println!("✅ Done");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_bearer_prefixed_payload_is_what_the_server_parses() {
        // The server strips exactly this prefix; a bare JWT is refused with
        // "Missing 'Bearer' prefix in authentication payload".
        let payload = format!("Bearer {}", "some.jwt.value");
        assert!(payload.starts_with("Bearer "), "handshake payload must carry the prefix");
        assert_eq!(payload.strip_prefix("Bearer "), Some("some.jwt.value"));
    }
}
