//! WebSocket protocol negotiation for GraphQL subscriptions.
//!
//! Supports both the modern `graphql-transport-ws` protocol and the legacy
//! `graphql-ws` (Apollo subscriptions-transport-ws) protocol. Messages are
//! translated to/from a unified internal representation using
//! [`ClientMessage`] / [`ServerMessage`] from `fraiseql-core`.

use fraiseql_core::runtime::protocol::{ClientMessage, ServerMessage};

/// Supported WebSocket sub-protocols for GraphQL subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsProtocol {
    /// Modern `graphql-transport-ws` protocol (enisdenjo/graphql-ws).
    ///
    /// Message types: `connection_init`, `connection_ack`, `ping`, `pong`,
    /// `subscribe`, `next`, `error`, `complete`.
    GraphqlTransportWs,

    /// Legacy `graphql-ws` protocol (Apollo subscriptions-transport-ws).
    ///
    /// Message types: `connection_init`, `connection_ack`, `start`, `data`,
    /// `error`, `stop`, `complete`, `ka` (keepalive).
    GraphqlWs,
}

impl WsProtocol {
    /// Parse the `Sec-WebSocket-Protocol` header value to select a protocol.
    ///
    /// The header may contain multiple comma-separated values; the first
    /// recognised protocol wins. Returns `None` if no known protocol is found.
    #[must_use]
    pub fn from_header(header: Option<&str>) -> Option<Self> {
        let header = header?;
        for token in header.split(',') {
            match token.trim() {
                "graphql-transport-ws" => return Some(Self::GraphqlTransportWs),
                "graphql-ws" => return Some(Self::GraphqlWs),
                _ => {}
            }
        }
        None
    }

    /// The protocol name to echo back in the WebSocket upgrade response.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GraphqlTransportWs => "graphql-transport-ws",
            Self::GraphqlWs => "graphql-ws",
        }
    }
}

/// Codec that translates between wire-format messages and the unified internal
/// [`ClientMessage`] / [`ServerMessage`] types.
pub struct ProtocolCodec {
    protocol: WsProtocol,
}

impl ProtocolCodec {
    /// Create a new codec for the given protocol.
    #[must_use]
    pub fn new(protocol: WsProtocol) -> Self {
        Self { protocol }
    }

    /// The negotiated protocol.
    #[must_use]
    pub fn protocol(&self) -> WsProtocol {
        self.protocol
    }

    /// Decode a raw JSON string from the WebSocket into a [`ClientMessage`].
    ///
    /// For `graphql-transport-ws` this is a passthrough deserialisation.
    /// For the legacy `graphql-ws` protocol, message types are translated:
    ///   - `start`  → `subscribe`
    ///   - `stop`   → `complete`
    ///
    /// # Errors
    ///
    /// Returns a [`ProtocolError`] if the JSON is malformed.
    pub fn decode(&self, raw: &str) -> Result<ClientMessage, ProtocolError> {
        match self.protocol {
            WsProtocol::GraphqlTransportWs => {
                serde_json::from_str(raw).map_err(|e| ProtocolError::InvalidJson(e.to_string()))
            }
            WsProtocol::GraphqlWs => {
                // Deserialise first, then remap legacy type strings.
                let mut msg: ClientMessage =
                    serde_json::from_str(raw).map_err(|e| ProtocolError::InvalidJson(e.to_string()))?;
                msg.message_type = translate_legacy_client_type(&msg.message_type).to_string();
                Ok(msg)
            }
        }
    }

    /// Encode a [`ServerMessage`] to a JSON string for sending over the WebSocket.
    ///
    /// For `graphql-transport-ws` this serialises directly.
    /// For the legacy `graphql-ws` protocol, message types are translated:
    ///   - `next`   → `data`
    ///   - `ping`   → `ka`  (keepalive, no payload)
    ///   - `pong`   → dropped (legacy protocol has no pong)
    ///
    /// Returns `None` for messages that should be suppressed (e.g. `pong` in legacy mode).
    ///
    /// # Errors
    ///
    /// Returns a [`ProtocolError`] if serialisation fails.
    pub fn encode(&self, msg: ServerMessage) -> Result<Option<String>, ProtocolError> {
        match self.protocol {
            WsProtocol::GraphqlTransportWs => {
                let json = msg.to_json().map_err(|e| ProtocolError::SerializationFailed(e.to_string()))?;
                Ok(Some(json))
            }
            WsProtocol::GraphqlWs => {
                let wire_type = translate_legacy_server_type(&msg.message_type);

                // `pong` has no legacy equivalent — suppress it.
                if wire_type.is_none() {
                    return Ok(None);
                }
                let wire_type = wire_type.unwrap();

                // `ka` is a bare keepalive with no payload.
                if wire_type == "ka" {
                    let ka = serde_json::json!({"type": "ka"});
                    return Ok(Some(ka.to_string()));
                }

                let mut value = serde_json::to_value(&msg)
                    .map_err(|e| ProtocolError::SerializationFailed(e.to_string()))?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("type".to_string(), serde_json::Value::String(wire_type.to_string()));
                }
                let json = serde_json::to_string(&value)
                    .map_err(|e| ProtocolError::SerializationFailed(e.to_string()))?;
                Ok(Some(json))
            }
        }
    }

    /// Whether the protocol uses periodic keepalive (`ka`) messages
    /// instead of `ping`/`pong`.
    #[must_use]
    pub fn uses_keepalive(&self) -> bool {
        self.protocol == WsProtocol::GraphqlWs
    }
}

/// Translate a legacy client message type to the modern equivalent.
fn translate_legacy_client_type(legacy: &str) -> &str {
    match legacy {
        "start" => "subscribe",
        "stop" => "complete",
        // `connection_init`, `connection_terminate` pass through unchanged.
        other => other,
    }
}

/// Translate a modern server message type to the legacy wire format.
///
/// Returns `None` for message types that have no legacy equivalent (e.g. `pong`).
fn translate_legacy_server_type(modern: &str) -> Option<&str> {
    match modern {
        "next" => Some("data"),
        "ping" => Some("ka"),
        "pong" => None,
        // `connection_ack`, `error`, `complete` are identical.
        other => Some(other),
    }
}

/// Protocol-level errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// The raw message was not valid JSON.
    InvalidJson(String),
    /// Serialisation of a server message failed.
    SerializationFailed(String),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            Self::SerializationFailed(e) => write!(f, "serialization failed: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use fraiseql_core::runtime::protocol::ServerMessage;

    // ── WsProtocol::from_header ──────────────────────────────────

    #[test]
    fn from_header_transport_ws() {
        assert_eq!(
            WsProtocol::from_header(Some("graphql-transport-ws")),
            Some(WsProtocol::GraphqlTransportWs)
        );
    }

    #[test]
    fn from_header_legacy_ws() {
        assert_eq!(
            WsProtocol::from_header(Some("graphql-ws")),
            Some(WsProtocol::GraphqlWs)
        );
    }

    #[test]
    fn from_header_multiple_prefers_first_known() {
        // Client may offer both; we pick the first recognised one.
        assert_eq!(
            WsProtocol::from_header(Some("graphql-ws, graphql-transport-ws")),
            Some(WsProtocol::GraphqlWs)
        );
        assert_eq!(
            WsProtocol::from_header(Some("graphql-transport-ws, graphql-ws")),
            Some(WsProtocol::GraphqlTransportWs)
        );
    }

    #[test]
    fn from_header_unknown_returns_none() {
        assert_eq!(WsProtocol::from_header(Some("unknown-protocol")), None);
    }

    #[test]
    fn from_header_none_returns_none() {
        assert_eq!(WsProtocol::from_header(None), None);
    }

    // ── ProtocolCodec::decode (modern) ───────────────────────────

    #[test]
    fn decode_transport_ws_subscribe() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let raw = r#"{"type":"subscribe","id":"1","payload":{"query":"subscription { x }"}}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "subscribe");
        assert_eq!(msg.id, Some("1".to_string()));
    }

    #[test]
    fn decode_transport_ws_invalid_json() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        assert!(codec.decode("not json").is_err());
    }

    // ── ProtocolCodec::decode (legacy) ───────────────────────────

    #[test]
    fn decode_legacy_start_becomes_subscribe() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"start","id":"1","payload":{"query":"subscription { x }"}}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "subscribe");
    }

    #[test]
    fn decode_legacy_stop_becomes_complete() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"stop","id":"1"}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "complete");
    }

    #[test]
    fn decode_legacy_connection_init_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"connection_init"}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "connection_init");
    }

    // ── ProtocolCodec::encode (modern) ───────────────────────────

    #[test]
    fn encode_transport_ws_next() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let msg = ServerMessage::next("1", serde_json::json!({"x": 1}));
        let json = codec.encode(msg).unwrap().unwrap();
        assert!(json.contains("\"next\""));
    }

    #[test]
    fn encode_transport_ws_ping() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let msg = ServerMessage::ping(None);
        let json = codec.encode(msg).unwrap().unwrap();
        assert!(json.contains("\"ping\""));
    }

    // ── ProtocolCodec::encode (legacy) ───────────────────────────

    #[test]
    fn encode_legacy_next_becomes_data() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::next("1", serde_json::json!({"x": 1}));
        let json = codec.encode(msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "data");
    }

    #[test]
    fn encode_legacy_ping_becomes_ka() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::ping(None);
        let json = codec.encode(msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "ka");
        // ka has no payload or id
        assert!(parsed.get("payload").is_none() || parsed["payload"].is_null());
    }

    #[test]
    fn encode_legacy_pong_is_suppressed() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::pong(None);
        let result = codec.encode(msg).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn encode_legacy_connection_ack_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::connection_ack(None);
        let json = codec.encode(msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "connection_ack");
    }

    #[test]
    fn encode_legacy_error_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::error(
            "1",
            vec![fraiseql_core::runtime::protocol::GraphQLError::new("test")],
        );
        let json = codec.encode(msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "error");
    }

    // ── uses_keepalive ───────────────────────────────────────────

    #[test]
    fn uses_keepalive_legacy() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        assert!(codec.uses_keepalive());
    }

    #[test]
    fn uses_keepalive_modern() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        assert!(!codec.uses_keepalive());
    }
}
