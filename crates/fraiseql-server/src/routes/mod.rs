//! HTTP routes.

// After-mutation function-trigger dispatch (#460). The planner is always
// compiled (and unit-tested); the I/O-capable spawn path is gated behind
// `functions-runtime`, so the items are dead in a default build.
#[cfg_attr(not(feature = "functions-runtime"), allow(dead_code))]
pub(crate) mod after_mutation;
pub mod api;
#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "functions")]
pub mod functions;
pub mod graphql;
pub mod health;
pub mod introspection;
pub mod jwks_admin;
pub mod metrics;
pub mod playground;
pub mod realtime;
pub mod storage;
pub mod studio;
pub mod subscriptions;
pub mod well_known;

#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "rest")]
pub mod rest;

#[cfg(feature = "auth")]
pub use auth::{
    AuthMeState, AuthPkceState, RevocationRouteState, auth_callback, auth_me, auth_start,
    revoke_all_tokens, revoke_token,
};
pub use graphql::{graphql_get_handler, graphql_handler};
#[cfg(feature = "federation")]
pub use health::federation_health_handler;
pub use health::{health_handler, readiness_handler};
pub use introspection::introspection_handler;
pub use metrics::{metrics_handler, metrics_json_handler};
pub use playground::{PlaygroundState, playground_handler};
pub use realtime::{BroadcastState, broadcast_handler};
pub use subscriptions::{SubscriptionState, subscription_handler, subscription_metrics};
pub use well_known::security_txt_handler;

#[cfg(test)]
mod tests;
