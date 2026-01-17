//! HTTP routes.

pub mod graphql;
pub mod health;
pub mod introspection;
pub mod metrics;
pub mod playground;
pub mod subscriptions;

pub use graphql::{graphql_get_handler, graphql_handler};
pub use health::health_handler;
pub use introspection::introspection_handler;
pub use metrics::{metrics_handler, metrics_json_handler};
pub use playground::{playground_handler, PlaygroundState};
pub use subscriptions::{subscription_handler, SubscriptionState};
