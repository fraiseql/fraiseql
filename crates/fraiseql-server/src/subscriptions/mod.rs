//! Subscription infrastructure for FraiseQL
//!
//! This module provides:
//! - EventBridge: Connects ChangeLogListener with SubscriptionManager
//! - WebSocket handler: Implements graphql-ws protocol
//! - Subscription management: Tracks active subscriptions

pub mod event_bridge;

pub use event_bridge::{EntityEvent, EventBridge, EventBridgeConfig};
