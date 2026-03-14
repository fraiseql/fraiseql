//! REST transport layer.
//!
//! When enabled via the `rest-transport` Cargo feature, the server mounts HTTP
//! routes for every query and mutation that carries a `rest` annotation in the
//! compiled schema.
//!
//! Each REST route translates an HTTP request into a GraphQL execution without
//! an extra HTTP round-trip: path parameters, query-string parameters, and JSON
//! body fields are mapped to GraphQL arguments, and the result is returned as
//! plain JSON (the `data.<operation>` slice of the GraphQL response).

pub mod router;
pub mod translator;

#[cfg(test)]
mod tests;
