//! HTTP routes.

pub mod graphql;
pub mod health;
pub mod introspection;

pub use graphql::graphql_handler;
pub use health::health_handler;
pub use introspection::introspection_handler;
