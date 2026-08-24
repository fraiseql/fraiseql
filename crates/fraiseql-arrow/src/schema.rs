//! Arrow schema definitions for FraiseQL data types.
//!
//! This module provides predefined Arrow schemas for common FraiseQL data structures.
//! schemas will be generated dynamically from GraphQL types.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

/// Arrow schema for GraphQL query results.
///
/// ⚠ **Placeholder, and not the shape any code produces.** `do_get` on a
/// `GraphQLQuery` ticket emits either a single `result` column or a schema derived
/// from the result rows — never `{id, data}`. The Flight metadata handlers returned
/// this until #1037 and now refuse instead, so nothing calls it.
///
/// Do not wire it to a metadata RPC: advertising it contradicts the data stream.
///
/// # Schema
///
/// - `id`: UTF-8 string (not nullable)
/// - `data`: UTF-8 string containing JSON (not nullable)
#[must_use]
pub fn graphql_result_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("data", DataType::Utf8, false), // JSON for now
    ]))
}

/// Arrow schema for observer events.
///
/// ⚠ **Disagrees with the payload it claims to describe.** `HistoricalEvent`
/// serializes `id` where this declares `event_id`, `tenant_id` where this declares
/// `org_id`, and a nested JSON object where this types `data` as `Utf8`. The
/// metadata handlers advertised it until #1038 and now refuse instead, so nothing
/// calls it. Reconcile it against `HistoricalEvent` before wiring it anywhere.
///
/// Maps to `EntityEvent` struct from `fraiseql-observers`.
///
/// # Schema
///
/// - `event_id`: Event UUID as UTF-8 string
/// - `event_type`: Event type (e.g., "Order.Created")
/// - `entity_type`: Entity type (e.g., "Order")
/// - `entity_id`: Entity identifier
/// - `timestamp`: Timestamp with microsecond precision in UTC
/// - `data`: Event payload as JSON string
/// - `user_id`: Optional user identifier
/// - `org_id`: Optional organization identifier
#[must_use]
pub fn observer_event_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("entity_id", DataType::Utf8, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC"))),
            false,
        ),
        Field::new("data", DataType::Utf8, false), // JSON string
        Field::new("user_id", DataType::Utf8, true),
        Field::new("org_id", DataType::Utf8, true),
    ]))
}

/// Arrow schema for bulk exports (table rows).
///
/// This is a placeholder schema.
/// schemas will be generated from database table metadata.
///
/// # Schema
///
/// - `id`: 64-bit integer (not nullable)
/// - `data`: UTF-8 string containing JSON (not nullable)
#[must_use]
pub fn bulk_export_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("data", DataType::Utf8, false), // JSON for now
    ]))
}

#[cfg(test)]
mod tests;
