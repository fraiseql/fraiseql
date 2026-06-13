#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test buffers use small, known lengths

//! Connection-level robustness tests for the wire decode loop (audit H42).
//!
//! These drive a *real* [`Connection`] through `startup` + `simple_query` against
//! a scripted in-process fake PostgreSQL backend over a loopback TCP socket — no
//! live database required. They assert the framing fixes end-to-end:
//!
//! * an `EmptyQueryResponse` ('I') lets `simple_query("")` complete instead of
//!   hanging on an unrecognized tag;
//! * an out-of-band `NotificationResponse` ('A') does not wedge the connection;
//! * a malformed message surfaces `WireError::Protocol` within a bounded time
//!   rather than looping forever buffering bytes.

use std::time::Duration;

use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};
use fraiseql_wire::protocol::message::BackendMessage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ── Raw backend-message builders ──────────────────────────────────────────

fn auth_ok() -> Vec<u8> {
    vec![b'R', 0, 0, 0, 8, 0, 0, 0, 0]
}

fn ready_for_query() -> Vec<u8> {
    vec![b'Z', 0, 0, 0, 5, b'I']
}

fn empty_query_response() -> Vec<u8> {
    vec![b'I', 0, 0, 0, 4]
}

fn command_complete(tag: &str) -> Vec<u8> {
    let body_len = 4 + tag.len() + 1; // len(4) + tag + null
    let mut out = vec![b'C'];
    out.extend_from_slice(&(body_len as u32).to_be_bytes());
    out.extend_from_slice(tag.as_bytes());
    out.push(0);
    out
}

fn notification(process_id: i32, channel: &str, payload: &str) -> Vec<u8> {
    let body_len = 4 + 4 + channel.len() + 1 + payload.len() + 1;
    let mut out = vec![b'A'];
    out.extend_from_slice(&(body_len as u32).to_be_bytes());
    out.extend_from_slice(&process_id.to_be_bytes());
    out.extend_from_slice(channel.as_bytes());
    out.push(0);
    out.extend_from_slice(payload.as_bytes());
    out.push(0);
    out
}

fn malformed() -> Vec<u8> {
    // Unknown tag '!' with a minimal valid length — a complete frame the decoder
    // must reject as InvalidData, not buffer toward forever.
    vec![b'!', 0, 0, 0, 4]
}

/// Bind a loopback listener and spawn a scripted fake backend.
///
/// The script: read the client startup → reply `AuthenticationOk` +
/// `ReadyForQuery` → read the client query → write `query_response`. The
/// connection is then held open until the client drops, so a hung client times
/// out in the test rather than seeing a premature EOF.
async fn spawn_fake_backend(query_response: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();

        // Drain the client startup message, then complete auth.
        drain_one_read(&mut sock).await;
        let mut hello = auth_ok();
        hello.extend_from_slice(&ready_for_query());
        sock.write_all(&hello).await.unwrap();
        sock.flush().await.unwrap();

        // Drain the client query, then emit the scripted response.
        drain_one_read(&mut sock).await;
        sock.write_all(&query_response).await.unwrap();
        sock.flush().await.unwrap();

        // Hold the socket open so the client never sees an accidental EOF.
        let mut sink = [0u8; 64];
        loop {
            match sock.read(&mut sink).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    port
}

async fn drain_one_read(sock: &mut TcpStream) {
    let mut buf = [0u8; 1024];
    let _ = sock.read(&mut buf).await.unwrap();
}

async fn connect_and_start(port: u16) -> Connection {
    let transport = Transport::connect_tcp("127.0.0.1", port).await.unwrap();
    let mut conn = Connection::new(transport);
    let config = ConnectionConfig::new("testdb", "testuser");
    conn.startup(&config).await.unwrap();
    conn
}

#[tokio::test]
async fn empty_query_completes_without_hanging() {
    let mut response = empty_query_response();
    response.extend_from_slice(&ready_for_query());
    let port = spawn_fake_backend(response).await;

    let mut conn = connect_and_start(port).await;
    let result = tokio::time::timeout(Duration::from_secs(5), conn.simple_query(""))
        .await
        .expect("simple_query(\"\") must not hang on EmptyQueryResponse")
        .expect("simple_query(\"\") must succeed");

    assert!(
        result
            .iter()
            .any(|m| matches!(m, BackendMessage::EmptyQueryResponse)),
        "response must contain the decoded EmptyQueryResponse"
    );
    assert!(
        result
            .iter()
            .any(|m| matches!(m, BackendMessage::ReadyForQuery { .. })),
        "response must terminate with ReadyForQuery"
    );
}

#[tokio::test]
async fn async_notification_does_not_wedge_connection() {
    // An out-of-band NOTIFY arrives between the row data and ReadyForQuery.
    let mut response = notification(99, "events", "payload");
    response.extend_from_slice(&command_complete("SELECT 0"));
    response.extend_from_slice(&ready_for_query());
    let port = spawn_fake_backend(response).await;

    let mut conn = connect_and_start(port).await;
    let result = tokio::time::timeout(Duration::from_secs(5), conn.simple_query("LISTEN events"))
        .await
        .expect("a NOTIFY must not wedge the connection")
        .expect("simple_query must succeed past the notification");

    assert!(
        result
            .iter()
            .any(|m| matches!(m, BackendMessage::NotificationResponse { .. })),
        "the decoded NotificationResponse must be surfaced"
    );
}

#[tokio::test]
async fn malformed_message_surfaces_protocol_error_in_bounded_time() {
    let port = spawn_fake_backend(malformed()).await;

    let mut conn = connect_and_start(port).await;
    let outcome = tokio::time::timeout(Duration::from_secs(5), conn.simple_query("SELECT 1"))
        .await
        .expect("a malformed message must fail fast, not buffer forever");

    let err = outcome.expect_err("a malformed message must surface an error");
    let msg = err.to_string();
    assert!(
        msg.contains("decode") || msg.contains("Protocol") || msg.contains("protocol"),
        "expected a protocol decode error, got: {msg}"
    );
}
