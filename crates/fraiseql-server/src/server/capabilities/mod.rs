//! One [`Capability`] impl per [`REGISTRY`](crate::capability::REGISTRY) entry: the code
//! that decides whether a capability is configured, and starts it.
//!
//! Each impl is a unit struct. A method whose capability is not compiled in keeps the
//! trait's default body, so every impl exists on every build and the registry names one
//! for each entry without a `#[cfg]` of its own.

mod arrow_flight;
mod cdc_outbound;
mod federation;
mod functions;
mod grpc;
mod identity;
mod inbound_email;
mod inbound_webhooks;
mod mcp;
mod observers;
mod rest;
mod saml;
mod session_state;
mod sources;
mod subscription_kafka;

pub use arrow_flight::ArrowFlight;
pub use cdc_outbound::CdcOutbound;
pub use federation::Federation;
pub use functions::Functions;
pub use grpc::Grpc;
pub use identity::Identity;
pub use inbound_email::InboundEmail;
pub use inbound_webhooks::InboundWebhooks;
pub use mcp::Mcp;
pub use observers::Observers;
pub use rest::Rest;
pub use saml::Saml;
pub use session_state::SessionState;
pub use sources::Sources;
pub use subscription_kafka::SubscriptionKafka;

use super::{Server, start::EntryPoint};
use crate::routes::graphql::AppState;

/// What a capability reads to decide whether this server is configured to run it.
#[allow(dead_code)] // Reason: read by the impls of compiled-in capabilities; a build with none reads none
pub struct ServeCx<'a> {
    /// The server being served.
    pub server: &'a Server,
    /// The state its routes serve from.
    pub app:    &'a AppState,
}

/// What a capability's start reads and writes.
#[allow(dead_code)] // Reason: read by the impls of compiled-in capabilities; a build with none reads none
pub struct RunCx<'a> {
    /// The server being served. Its `tasks` are where a started capability runs.
    pub server: &'a mut Server,
    /// The state its routes serve from.
    pub app:    &'a AppState,
    /// The entry point serving it.
    pub entry:  EntryPoint,
}
