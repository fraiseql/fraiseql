//! Federated subscription passthrough.
//!
//! When a subscription targets a field owned by a remote subgraph, the
//! `SubscriptionForwarder` connects to the remote subgraph via `WebSocket`
//! using the `graphql-transport-ws` protocol and proxies events back to
//! the client.

use std::{collections::HashMap, hash::BuildHasher};

use futures::{SinkExt, StreamExt, stream::SplitSink};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, warn};

use crate::http_resolver::validate_subgraph_url;

/// Determines which subgraph (if any) owns a given subscription field.
///
/// Returns `Some(url)` if the field is remote-owned, `None` if it should
/// be resolved locally.
pub fn lookup_remote_subscription<'a, S: BuildHasher>(
    field_name: &str,
    remote_fields: &'a HashMap<String, String, S>,
) -> Option<&'a str> {
    remote_fields.get(field_name).map(String::as_str)
}

/// Extract the root subscription field name from a GraphQL subscription query,
/// resolving aliases to the actual field name.
///
/// Given `subscription { myAlias: postCreated { body } }`, returns `"postCreated"`.
/// Given `subscription { postCreated { body } }`, returns `"postCreated"`.
pub fn extract_subscription_field_name(query: &str) -> Option<String> {
    let query = query.trim();

    let sub_idx = query.find("subscription")?;
    let after_sub = &query[sub_idx + "subscription".len()..];

    let brace_idx = after_sub.find('{')?;
    let after_brace = after_sub[brace_idx + 1..].trim_start();

    // Parse the first token — could be a field name or an alias
    let first_end = after_brace
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_brace.len());

    if first_end == 0 {
        return None;
    }

    let first_token = &after_brace[..first_end];
    let after_first = after_brace[first_end..].trim_start();

    // If the next non-whitespace char is ':', this is an alias
    if let Some(after_colon_raw) = after_first.strip_prefix(':') {
        let after_colon = after_colon_raw.trim_start();
        let field_end = after_colon
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after_colon.len());
        if field_end > 0 {
            return Some(after_colon[..field_end].to_string());
        }
    }

    Some(first_token.to_string())
}

/// Error type for subscription forwarding operations.
#[derive(Debug)]
pub enum ForwardError {
    /// The subgraph URL failed SSRF validation.
    SsrfBlocked(String),
    /// `WebSocket` connection to the remote subgraph failed.
    ConnectionFailed(String),
    /// The remote subgraph did not acknowledge the connection.
    InitFailed(String),
    /// The remote subgraph sent a protocol error.
    ProtocolError(String),
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForwardError::SsrfBlocked(msg) => write!(f, "SSRF blocked: {msg}"),
            ForwardError::ConnectionFailed(msg) => write!(f, "connection failed: {msg}"),
            ForwardError::InitFailed(msg) => write!(f, "init failed: {msg}"),
            ForwardError::ProtocolError(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for ForwardError {}

/// Subscription forwarder for delegating to a remote subgraph.
///
/// Connects to the remote subgraph via `WebSocket` using the
/// `graphql-transport-ws` protocol, sends the subscription operation,
/// and relays `next`/`error`/`complete` messages back via `event_tx`.
#[derive(Debug)]
pub struct SubscriptionForwarder {
    subgraph_url: String,
}

impl SubscriptionForwarder {
    /// Create a new forwarder targeting the given subgraph URL.
    ///
    /// # Errors
    ///
    /// Returns `ForwardError::SsrfBlocked` if the URL fails SSRF validation.
    pub fn new(subgraph_url: &str) -> Result<Self, ForwardError> {
        validate_subgraph_url(subgraph_url)
            .map_err(|e| ForwardError::SsrfBlocked(e.to_string()))?;
        Ok(Self {
            subgraph_url: subgraph_url.to_string(),
        })
    }

    /// Forward a subscription to the remote subgraph.
    ///
    /// Connects via `WebSocket`, performs the `graphql-transport-ws` handshake,
    /// sends the `subscribe` message, and relays all `next`/`error`/`complete`
    /// messages to `event_tx`. The channel is closed when the remote subscription
    /// completes or errors.
    ///
    /// # Errors
    ///
    /// Returns `ForwardError` if connection, handshake, or protocol negotiation fails.
    pub async fn forward(
        &self,
        operation_id: &str,
        query: &str,
        variables: serde_json::Value,
        event_tx: tokio::sync::mpsc::Sender<ForwardedEvent>,
    ) -> Result<(), ForwardError> {
        // Convert HTTP(S) URL to WS(S) URL if needed
        let ws_url = http_to_ws_url(&self.subgraph_url);

        debug!(url = %ws_url, "Connecting to remote subgraph for subscription");

        let (ws_stream, _response) =
            tokio_tungstenite::connect_async_with_config(&ws_url, None, false)
                .await
                .map_err(|e| ForwardError::ConnectionFailed(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Send connection_init
        let init_msg = serde_json::json!({"type": "connection_init"});
        send_json(&mut write, &init_msg).await?;

        // Wait for connection_ack
        let ack_timeout = tokio::time::Duration::from_secs(5);
        let ack = tokio::time::timeout(ack_timeout, async {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            if val.get("type").and_then(|t| t.as_str()) == Some("connection_ack") {
                                return Ok(());
                            }
                        }
                    },
                    Ok(WsMessage::Close(_)) => {
                        return Err(ForwardError::InitFailed(
                            "remote closed during init".to_string(),
                        ));
                    },
                    Err(e) => {
                        return Err(ForwardError::ConnectionFailed(e.to_string()));
                    },
                    _ => {},
                }
            }
            Err(ForwardError::InitFailed("stream ended before ack".to_string()))
        })
        .await
        .map_err(|_| ForwardError::InitFailed("connection_ack timeout (5s)".to_string()))?;

        ack?;

        debug!("Remote subgraph connection acknowledged");

        // Send subscribe message
        let subscribe_msg = serde_json::json!({
            "id": operation_id,
            "type": "subscribe",
            "payload": {
                "query": query,
                "variables": variables,
            }
        });
        send_json(&mut write, &subscribe_msg).await?;

        // Relay events until complete/error
        while let Some(msg) = read.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match msg_type {
                        "next" => {
                            let payload =
                                val.get("payload").cloned().unwrap_or(serde_json::Value::Null);
                            if event_tx.send(ForwardedEvent::Next(payload)).await.is_err() {
                                break; // Client disconnected
                            }
                        },
                        "error" => {
                            let payload =
                                val.get("payload").cloned().unwrap_or(serde_json::Value::Null);
                            let _ = event_tx.send(ForwardedEvent::Error(payload)).await;
                            break;
                        },
                        "complete" => {
                            let _ = event_tx.send(ForwardedEvent::Complete).await;
                            break;
                        },
                        _ => {},
                    }
                },
                Ok(WsMessage::Close(_)) => {
                    let _ = event_tx.send(ForwardedEvent::Complete).await;
                    break;
                },
                Err(e) => {
                    warn!(error = %e, "Remote subgraph WebSocket error");
                    let _ = event_tx
                        .send(ForwardedEvent::Error(serde_json::json!([{
                            "message": format!("Remote subgraph error: {e}"),
                        }])))
                        .await;
                    break;
                },
                _ => {},
            }
        }

        // Best-effort close
        let _ = write.close().await;

        Ok(())
    }
}

/// Event forwarded from a remote subgraph subscription.
#[derive(Debug, Clone)]
pub enum ForwardedEvent {
    /// A `next` payload from the remote subgraph.
    Next(serde_json::Value),
    /// An `error` payload from the remote subgraph.
    Error(serde_json::Value),
    /// The remote subscription completed.
    Complete,
}

/// Convert an HTTP(S) URL to a WS(S) URL.
fn http_to_ws_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        // Already a ws:// or wss:// URL
        url.to_string()
    }
}

/// Send a JSON message over the `WebSocket`.
async fn send_json<S>(
    sink: &mut SplitSink<S, WsMessage>,
    value: &serde_json::Value,
) -> Result<(), ForwardError>
where
    S: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let json =
        serde_json::to_string(value).map_err(|e| ForwardError::ProtocolError(e.to_string()))?;
    sink.send(WsMessage::Text(json.into()))
        .await
        .map_err(|e| ForwardError::ConnectionFailed(e.to_string()))
}

#[cfg(test)]
mod tests;
