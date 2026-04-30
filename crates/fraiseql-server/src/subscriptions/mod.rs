//! Subscription infrastructure for FraiseQL
//!
//! This module provides:
//! - `EventBridge`: Connects `ChangeLogListener` with `SubscriptionManager`
//! - `WebSocket` handler: Implements graphql-ws protocol
//! - Subscription management: Tracks active subscriptions

pub mod broadcast;
pub mod event_bridge;
pub mod lifecycle;
pub mod protocol;
pub mod webhook_lifecycle;

pub use broadcast::{BroadcastConfig, BroadcastManager, BroadcastMessage};
pub use event_bridge::{EntityEvent, EventBridge, EventBridgeConfig};
pub use lifecycle::{NoopLifecycle, SubscriptionLifecycle};
pub use protocol::{ProtocolCodec, ProtocolError, WsProtocol};
pub use webhook_lifecycle::WebhookLifecycle;
