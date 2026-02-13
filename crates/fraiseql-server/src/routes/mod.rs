//! HTTP routes.

pub mod api;
pub mod graphql;
pub mod health;
pub mod introspection;
pub mod metrics;
pub mod playground;
pub mod subscriptions;

pub use graphql::{graphql_get_handler, graphql_handler};
pub use health::{federation_health_handler, health_handler};
pub use introspection::introspection_handler;
pub use metrics::{metrics_handler, metrics_json_handler};
pub use playground::{PlaygroundState, playground_handler};
pub use subscriptions::{SubscriptionState, subscription_handler};
