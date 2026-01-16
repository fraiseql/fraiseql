//! HTTP routes.

pub mod graphql;
pub mod health;
pub mod introspection;
pub mod metrics;

pub use graphql::graphql_handler;
pub use health::health_handler;
pub use introspection::introspection_handler;
pub use metrics::{metrics_handler, metrics_json_handler};
