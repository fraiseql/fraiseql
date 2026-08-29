//! Dynamic Arrow schema generation from GraphQL types.
//!
//! This module maps GraphQL scalar types to Apache Arrow data types
//! and generates Arrow schemas from GraphQL query result shapes.

use std::{collections::HashMap, sync::Arc};

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use serde_json::Value;

use crate::error::ArrowFlightError;

/// Map GraphQL scalar types to Arrow types.
///
/// # Arguments
///
/// * `graphql_type` - GraphQL type name (e.g., "String", "Int", "`DateTime`")
/// * `nullable` - Whether the field is nullable
///
/// # Returns
///
/// The corresponding Arrow `DataType`
///
/// # Example
///
/// ```
/// use fraiseql_arrow::schema_gen::graphql_type_to_arrow;
/// use arrow::datatypes::DataType;
///
/// let arrow_type = graphql_type_to_arrow("String", false);
/// assert_eq!(arrow_type, DataType::Utf8);
/// ```
#[must_use]
pub fn graphql_type_to_arrow(graphql_type: &str, _nullable: bool) -> DataType {
    match graphql_type {
        // GraphQL scalars
        "String" => DataType::Utf8,
        "Int" => DataType::Int32,
        "Float" => DataType::Float64,
        "Boolean" => DataType::Boolean,
        "ID" => DataType::Utf8,

        // Custom scalars (common extensions)
        "DateTime" => DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
        "Date" => DataType::Date32,
        "Time" => DataType::Time64(TimeUnit::Nanosecond),
        "UUID" => DataType::Utf8,                  // UUIDs as strings
        "JSON" => DataType::Utf8,                  // JSON as string for now
        "Decimal" => DataType::Decimal128(38, 10), // Default precision

        // Unknown types default to JSON strings
        _ => DataType::Utf8,
    }
}

/// Generate Arrow schema from GraphQL query result shape.
///
/// # Arguments
///
/// * `fields` - Vector of (`field_name`, `graphql_type`, nullable) tuples
///
/// # Returns
///
/// Arrow Schema with fields mapped from GraphQL types
///
/// # Example
///
/// ```
/// use fraiseql_arrow::schema_gen::generate_arrow_schema;
///
/// let fields = vec![
///     ("id".to_string(), "ID".to_string(), false),
///     ("name".to_string(), "String".to_string(), true),
///     ("age".to_string(), "Int".to_string(), true),
/// ];
///
/// let schema = generate_arrow_schema(&fields);
/// assert_eq!(schema.fields().len(), 3);
/// ```
#[must_use]
pub fn generate_arrow_schema(fields: &[(String, String, bool)]) -> Arc<Schema> {
    let arrow_fields: Vec<Field> = fields
        .iter()
        .map(|(name, graphql_type, nullable)| {
            let arrow_type = graphql_type_to_arrow(graphql_type, *nullable);
            Field::new(name, arrow_type, *nullable)
        })
        .collect();

    Arc::new(Schema::new(arrow_fields))
}

/// Infer Arrow schema from raw database rows (JSON objects).
///
/// Field names come from the first row. Field **types** are unified across every
/// row, and the field list is **sorted**, so the same column set always produces
/// the same schema. All fields are nullable.
///
/// Two properties this guarantees, each of which was previously broken:
///
/// - **Deterministic order** (#1002). The field list used to come from `HashMap::iter()`, whose
///   order is unspecified and differs between map instances. `Schema` equality is order-sensitive,
///   so the heterogeneous-schema guard on batched queries (#717) rejected identically-shaped
///   queries at random. Sorting is the only stable order available here: `execute_raw_query`
///   returns `HashMap`, so the SELECT order is already lost by this point.
///
/// - **Types that hold for every row** (#1042). Typing from row 0 alone meant a leading `null`
///   silently retyped a numeric column as `Utf8` and stringified every later value, while a leading
///   whole number typed a column `Int64` and the first fractional value failed the entire request.
///   Both outcomes flipped between otherwise-identical requests depending on which row came back
///   first.
///
/// The field *set* is still taken from the first row, so a column absent there is
/// still dropped; that is tracked separately.
///
/// # Arguments
///
/// * `rows` - Vector of `HashMap` representing database rows
///
/// # Returns
///
/// Arrow Schema inferred from the rows
///
/// # Errors
///
/// Returns error if rows are empty or if schema inference fails
pub fn infer_schema_from_rows(
    rows: &[HashMap<String, Value>],
) -> Result<Arc<Schema>, ArrowFlightError> {
    if rows.is_empty() {
        return Err(ArrowFlightError::SchemaNotFound(
            "Cannot infer schema from empty rows".to_string(),
        ));
    }

    // #1180: the field *set* is the union of every row's keys, as the column
    // *types* have been since #1042. It used to be `rows[0].keys()`, and
    // `convert_db_rows_to_arrow` looks columns up **by name** — so a key that
    // never became a field was never read. Its values did not reach the client
    // and nothing was logged: the stream was self-consistent, so no error
    // surfaced and the data was simply absent.
    //
    // Sorted and deduped, so the field order is a property of the data rather
    // than of `HashMap` iteration order, and a key seen in several rows is one
    // column.
    let mut names: Vec<&String> = rows.iter().flat_map(HashMap::keys).collect();
    names.sort_unstable();
    names.dedup();

    // `None` = no non-null value seen yet for that column.
    let mut inferred: Vec<Option<DataType>> = vec![None; names.len()];

    for row in rows {
        for (slot, name) in inferred.iter_mut().zip(&names) {
            // `Utf8` is the top of the lattice; nothing can widen it further.
            if slot.as_ref() == Some(&DataType::Utf8) {
                continue;
            }
            let Some(value) = row.get(*name) else {
                continue;
            };
            // JSON null carries no type information: a column that is null in one
            // row and numeric in another is numeric.
            if value.is_null() {
                continue;
            }
            let observed = json_value_to_arrow_type(value);
            *slot = Some(match slot.take() {
                Some(current) => unify_arrow_types(&current, &observed),
                None => observed,
            });
        }
    }

    let arrow_fields: Vec<Field> = names
        .into_iter()
        .zip(inferred)
        .map(|(name, data_type)| {
            // An all-null column becomes `Utf8`, never `DataType::Null`: the array
            // converters reject `Null`, so such a column poisoned the whole result
            // (H37).
            Field::new(name.clone(), data_type.unwrap_or(DataType::Utf8), true)
        })
        .collect();

    Ok(Arc::new(Schema::new(arrow_fields)))
}

/// Least common Arrow type of two values observed in the same column.
fn unify_arrow_types(current: &DataType, observed: &DataType) -> DataType {
    if current == observed {
        return current.clone();
    }

    match (current, observed) {
        // A whole number and a fractional one are both `Float64`. The `Float64`
        // converter accepts integer JSON numbers, whereas the `Int64` one rejects
        // any fraction outright and fails the request.
        (DataType::Int64, DataType::Float64) | (DataType::Float64, DataType::Int64) => {
            DataType::Float64
        },
        // Anything else genuinely mixed renders as text, which every JSON value
        // can do, rather than failing the request.
        _ => DataType::Utf8,
    }
}

/// Infer an Arrow [`DataType`] from a JSON value.
///
/// This is the single source of truth for JSON-row → Arrow type inference,
/// shared by `schema_gen` (this module) and [`crate::metadata`] so the two
/// cannot drift.
///
/// JSON `null` maps to [`DataType::Utf8`] — a nullable string column — **not**
/// [`DataType::Null`]. The array converters reject `DataType::Null`, so a column
/// whose first row happened to be `null` previously poisoned the entire result
/// (H37). A whole-number is `Int64`, any other number `Float64`; arrays and
/// objects are carried as their JSON string form.
pub(crate) fn json_value_to_arrow_type(value: &Value) -> DataType {
    match value {
        Value::Null => DataType::Utf8,
        Value::Bool(_) => DataType::Boolean,
        Value::Number(n) => {
            if n.is_i64() {
                DataType::Int64
            } else {
                DataType::Float64
            }
        },
        Value::String(_) => DataType::Utf8,
        Value::Array(_) => DataType::Utf8, // JSON arrays as strings
        Value::Object(_) => DataType::Utf8, // JSON objects as strings
    }
}

#[cfg(test)]
mod tests;
