//! GraphQL HTTP endpoint.
//!
//! Supports both POST and GET requests per the GraphQL over HTTP spec:
//! - POST: JSON body with `query`, `variables`, `operationName`
//! - GET: Query parameters `query`, `variables` (JSON-encoded), `operationName`

pub mod app_state;
pub mod handler;
pub mod request;

#[cfg(test)]
mod tests;

pub use app_state::AppState;
pub use handler::{graphql_get_handler, graphql_handler};
pub use request::{GraphQLGetParams, GraphQLRequest, GraphQLResponse};
