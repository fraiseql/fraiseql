//! Axum route wiring for the realtime `WebSocket` endpoint.
//!
//! `realtime_router()` returns a standalone [`axum::Router`] that mounts
//! `/realtime/v1`. Merge it into the server's base router when the compiled
//! schema enables realtime:
//!
//! ```rust,ignore
//! if schema_config.realtime.as_ref().is_some_and(|r| r.enabled) {
//!     let state = build_realtime_state(&schema_config, &validator);
//!     app = app.merge(realtime_router(state));
//! }
//! ```
//!
//! # Schema config
//!
//! The `"realtime"` key in `schema.compiled.json` is deserialised into
//! [`RealtimeSchemaConfig`].  Only entities listed in [`RealtimeSchemaConfig::entities`]
//! will accept subscriptions — any subscribe request for an unlisted entity
//! returns `{ "type": "error", "message": "unknown entity: …" }`.

use axum::{Router, routing::get};
use serde::Deserialize;

use super::server::{RealtimeState, ws_handler};

/// Configuration for the realtime subsystem, parsed from `schema.compiled.json`.
///
/// Embedded under the top-level `"realtime"` key:
///
/// ```json
/// {
///   "realtime": {
///     "enabled": true,
///     "entities": ["Post", "Comment"],
///     "max_connections_per_context": 25
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RealtimeSchemaConfig {
    /// Whether the realtime subsystem is enabled.
    ///
    /// When `false`, the server must not mount the `/realtime/v1` route.
    pub enabled: bool,

    /// Entity names that accept realtime subscriptions.
    ///
    /// Subscribe requests for entities not listed here are rejected with an
    /// error message. An empty list means no entities accept subscriptions
    /// even when `enabled` is `true`.
    #[serde(default)]
    pub entities: Vec<String>,

    /// Override for [`RealtimeConfig::max_connections_per_context`].
    ///
    /// Falls back to the `RealtimeConfig` default when absent.
    #[serde(default)]
    pub max_connections_per_context: Option<usize>,

    /// Override for [`RealtimeConfig::max_subscriptions_per_entity`].
    ///
    /// Falls back to the `RealtimeConfig` default when absent.
    #[serde(default)]
    pub max_subscriptions_per_entity: Option<usize>,

    /// Override for [`RealtimeConfig::event_channel_capacity`].
    ///
    /// Falls back to the `RealtimeConfig` default when absent.
    #[serde(default)]
    pub event_channel_capacity: Option<usize>,
}

/// Build a router that mounts the realtime `WebSocket` endpoint at `/realtime/v1`.
///
/// Pass the returned router to [`axum::Router::merge`] when assembling the
/// server. The route is only mounted when `RealtimeSchemaConfig::enabled` is
/// `true` — callers are responsible for checking the flag before calling this
/// function.
///
/// # Example
///
/// ```rust,ignore
/// let realtime = realtime_router(state);
/// let app = base_router.merge(realtime);
/// ```
#[must_use]
pub fn realtime_router(state: RealtimeState) -> Router {
    Router::new()
        .route("/realtime/v1", get(ws_handler))
        .with_state(state)
}
