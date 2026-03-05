#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Subscription protocol state-machine tests.
//!
//! Validates the four graphql-ws state transitions that were absent from
//! existing tests:
//!
//! | State | What is tested |
//! |-------|---------------|
//! | `ConnectionAck` | `connection_init` → `connection_ack` round-trip |
//! | `Error` frame | Subscription error delivered without a prior `next` |
//! | `Complete` handshake | Bidirectional `complete` messages |
//! | `Ping`/`Pong` | Keepalive exchange and legacy `ka` translation |
//!
//! Tests use the public `ProtocolCodec` API only; no WebSocket connection is
//! required.
//!
//! **Execution engine:** none (codec + message constructors only)
//! **Infrastructure:** none
//! **Parallelism:** safe

use fraiseql_core::runtime::protocol::{ClientMessageType, GraphQLError, ServerMessage};
use fraiseql_server::subscriptions::protocol::{ProtocolCodec, WsProtocol};

// ── Cycle 6.1: ConnectionAck ─────────────────────────────────────────────────

/// `connection_init` arrives → codec decodes it as `ConnectionInit` → server
/// replies with `connection_ack` → codec encodes that reply.
#[test]
fn connection_init_produces_connection_ack_modern() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    // Client → server
    let raw = r#"{"type":"connection_init"}"#;
    let client_msg = codec.decode(raw).unwrap();
    assert_eq!(
        client_msg.parsed_type(),
        Some(ClientMessageType::ConnectionInit),
        "message type must be ConnectionInit"
    );
    assert!(client_msg.id.is_none(), "connection_init carries no operation id");

    // Server → client: acknowledge the connection
    let ack = ServerMessage::connection_ack(None);
    let wire = codec.encode(ack).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed["type"], "connection_ack");
    assert!(parsed.get("id").is_none() || parsed["id"].is_null());
    assert!(parsed.get("payload").is_none() || parsed["payload"].is_null());
}

/// `connection_ack` may carry an optional payload (e.g. server capabilities).
#[test]
fn connection_ack_with_payload_round_trips_correctly() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    let server_info = serde_json::json!({"version": "2.0", "extensions": ["persisted-queries"]});
    let ack = ServerMessage::connection_ack(Some(server_info.clone()));
    let wire = codec.encode(ack).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed["type"], "connection_ack");
    assert_eq!(parsed["payload"]["version"], "2.0");
}

/// Legacy `graphql-ws` protocol also produces `connection_ack` (type unchanged).
#[test]
fn connection_init_produces_connection_ack_legacy() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);

    let raw = r#"{"type":"connection_init"}"#;
    let client_msg = codec.decode(raw).unwrap();
    assert_eq!(client_msg.parsed_type(), Some(ClientMessageType::ConnectionInit));

    let ack = ServerMessage::connection_ack(None);
    let wire = codec.encode(ack).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(
        parsed["type"], "connection_ack",
        "legacy protocol must use identical `connection_ack` type string"
    );
}

// ── Cycle 6.2: Error frame ───────────────────────────────────────────────────

/// Server sends an `error` frame in response to a bad subscription, without
/// ever sending a `next` frame first.
#[test]
fn error_frame_delivered_without_prior_next_modern() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    // Client subscribes with a bad query.
    let raw = r#"{"type":"subscribe","id":"op_1","payload":{"query":"subscription { bad }"}}"#;
    let client_msg = codec.decode(raw).unwrap();
    assert_eq!(client_msg.parsed_type(), Some(ClientMessageType::Subscribe));

    let op_id = client_msg.id.as_deref().unwrap();

    // Server replies with error (no next sent first).
    let errors = vec![GraphQLError::with_code("Cannot query field 'bad'", "GRAPHQL_VALIDATION_FAILED")];
    let err_msg = ServerMessage::error(op_id, errors);
    let wire = codec.encode(err_msg).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed["type"], "error");
    assert_eq!(parsed["id"], "op_1");
    let payload = parsed["payload"].as_array().unwrap();
    assert_eq!(payload.len(), 1);
    assert!(payload[0]["message"].as_str().unwrap().contains("bad"));
}

/// Multiple errors can be packed into a single `error` frame.
#[test]
fn error_frame_carries_multiple_errors() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    let errors = vec![
        GraphQLError::with_code("Field 'x' not found", "FIELD_NOT_FOUND"),
        GraphQLError::with_code("Argument 'limit' is required", "ARGUMENT_REQUIRED"),
    ];
    let err_msg = ServerMessage::error("op_2", errors);
    let wire = codec.encode(err_msg).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    let payload = parsed["payload"].as_array().unwrap();
    assert_eq!(payload.len(), 2, "both errors must be present in the payload");
}

/// Legacy protocol preserves the `error` type string unchanged.
#[test]
fn error_frame_type_unchanged_in_legacy_protocol() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);

    let err_msg = ServerMessage::error("op_1", vec![GraphQLError::new("something went wrong")]);
    let wire = codec.encode(err_msg).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed["type"], "error", "legacy protocol does not rename `error`");
}

// ── Cycle 6.3: Complete handshake ───────────────────────────────────────────

/// Client sends `complete` to terminate a subscription; server acknowledges
/// with its own `complete` carrying the same operation id.
#[test]
fn complete_handshake_client_then_server_modern() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    // Client terminates the subscription.
    let raw = r#"{"type":"complete","id":"op_1"}"#;
    let client_msg = codec.decode(raw).unwrap();
    assert_eq!(client_msg.parsed_type(), Some(ClientMessageType::Complete));
    let op_id = client_msg.id.as_deref().unwrap();
    assert_eq!(op_id, "op_1");

    // Server echoes complete for the same operation.
    let server_complete = ServerMessage::complete(op_id);
    let wire = codec.encode(server_complete).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed["type"], "complete");
    assert_eq!(parsed["id"], "op_1");
    assert!(
        parsed.get("payload").is_none() || parsed["payload"].is_null(),
        "`complete` must not carry a payload"
    );
}

/// Legacy `stop` → `complete` translation, and server echoes `complete`.
#[test]
fn complete_handshake_legacy_stop_becomes_complete() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);

    // Legacy client sends `stop` to terminate.
    let raw = r#"{"type":"stop","id":"op_x"}"#;
    let client_msg = codec.decode(raw).unwrap();
    assert_eq!(
        client_msg.parsed_type(),
        Some(ClientMessageType::Complete),
        "legacy `stop` must be translated to `complete` by the codec"
    );

    // Server echoes complete.
    let server_complete = ServerMessage::complete("op_x");
    let wire = codec.encode(server_complete).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed["type"], "complete");
}

/// `complete` payload must always be absent (null/missing) per the spec.
#[test]
fn complete_has_no_payload() {
    let msg = ServerMessage::complete("op_abc");
    assert!(msg.payload.is_none(), "`complete` must have no payload field");
    let json = msg.to_json().unwrap();
    // The serialised JSON must not include a `payload` key.
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("payload").is_none(),
        "`payload` must be omitted from `complete` JSON (skip_serializing_if)"
    );
}

// ── Cycle 6.4: Ping / Pong keepalive ────────────────────────────────────────

/// Modern protocol: server sends `ping`, client decodes it and replies `pong`.
#[test]
fn ping_pong_round_trip_modern() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    // Server → client: keepalive ping.
    let ping = ServerMessage::ping(None);
    let server_wire = codec.encode(ping).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&server_wire).unwrap();
    assert_eq!(parsed["type"], "ping");

    // Client → server: pong response (same codec, decode client message).
    let raw_pong = r#"{"type":"pong"}"#;
    let client_pong = codec.decode(raw_pong).unwrap();
    assert_eq!(client_pong.parsed_type(), Some(ClientMessageType::Pong));

    // Server encodes its own pong reply (echo).
    let server_pong = ServerMessage::pong(None);
    let pong_wire = codec.encode(server_pong).unwrap().unwrap();
    let parsed_pong: serde_json::Value = serde_json::from_str(&pong_wire).unwrap();
    assert_eq!(parsed_pong["type"], "pong");
}

/// Modern `ping` may carry an optional payload that the pong must mirror.
#[test]
fn ping_with_payload_modern() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);

    let payload = serde_json::json!({"timestamp": 1_700_000_000u64});
    let ping = ServerMessage::ping(Some(payload.clone()));
    let wire = codec.encode(ping).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed["type"], "ping");
    assert_eq!(parsed["payload"]["timestamp"], 1_700_000_000u64);
}

/// Legacy protocol: `ping` becomes `ka` (keepalive), `pong` is suppressed.
#[test]
fn ping_becomes_ka_and_pong_suppressed_legacy() {
    let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);

    // Ping → ka
    let ping = ServerMessage::ping(None);
    let wire = codec.encode(ping).unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed["type"], "ka", "legacy protocol must translate `ping` to `ka`");
    // ka carries no payload or id.
    assert!(
        parsed.get("id").is_none() || parsed["id"].is_null(),
        "`ka` must not carry an `id`"
    );

    // Pong → suppressed (None)
    let pong = ServerMessage::pong(None);
    let result = codec.encode(pong).unwrap();
    assert!(result.is_none(), "legacy protocol must suppress `pong`");
}

/// `uses_keepalive` reflects which protocol is in use.
#[test]
fn uses_keepalive_distinguishes_protocols() {
    let modern = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
    let legacy = ProtocolCodec::new(WsProtocol::GraphqlWs);

    assert!(!modern.uses_keepalive(), "modern protocol uses ping/pong, not ka");
    assert!(legacy.uses_keepalive(), "legacy protocol uses ka keepalive");
}
