//! GraphQL schema generator for filter and orderby types.
//!
//! This module exports complete filter and orderby schemas that can be used
//! by Python to generate GraphQL input types without runtime reflection.
//!
//! Phase A.1: Schema generation moved to Rust for better performance and clarity.

use serde_json::{json, Value};

/// Export complete schema for filter and orderby input types.
///
/// Returns a JSON string containing:
/// - `filter_schemas`: Filter types for all base and custom types
/// - `order_by_schemas`: Order by configurations
/// - `version`: Schema version for compatibility checking
///
/// This schema is used by Python to generate GraphQL `WhereInput` and `OrderByInput` types
/// at schema-building time, eliminating the need for runtime type introspection.
#[must_use]
pub fn export_schema_generators() -> Value {
    json!({
        "version": "1.0",
        "filter_schemas": {
            "String": {
                "fields": {
                    "eq": {"type": "String", "nullable": true},
                    "neq": {"type": "String", "nullable": true},
                    "contains": {"type": "String", "nullable": true},
                    "icontains": {"type": "String", "nullable": true},
                    "startswith": {"type": "String", "nullable": true},
                    "istartswith": {"type": "String", "nullable": true},
                    "endswith": {"type": "String", "nullable": true},
                    "iendswith": {"type": "String", "nullable": true},
                    "like": {"type": "String", "nullable": true},
                    "ilike": {"type": "String", "nullable": true},
                    "matches": {"type": "String", "nullable": true},
                    "imatches": {"type": "String", "nullable": true},
                    "not_matches": {"type": "String", "nullable": true},
                    "in": {"type": "[String!]", "nullable": true},
                    "nin": {"type": "[String!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "Int": {
                "fields": {
                    "eq": {"type": "Int", "nullable": true},
                    "neq": {"type": "Int", "nullable": true},
                    "lt": {"type": "Int", "nullable": true},
                    "lte": {"type": "Int", "nullable": true},
                    "gt": {"type": "Int", "nullable": true},
                    "gte": {"type": "Int", "nullable": true},
                    "in": {"type": "[Int!]", "nullable": true},
                    "nin": {"type": "[Int!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "Float": {
                "fields": {
                    "eq": {"type": "Float", "nullable": true},
                    "neq": {"type": "Float", "nullable": true},
                    "lt": {"type": "Float", "nullable": true},
                    "lte": {"type": "Float", "nullable": true},
                    "gt": {"type": "Float", "nullable": true},
                    "gte": {"type": "Float", "nullable": true},
                    "in": {"type": "[Float!]", "nullable": true},
                    "nin": {"type": "[Float!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "Boolean": {
                "fields": {
                    "eq": {"type": "Boolean", "nullable": true},
                    "neq": {"type": "Boolean", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "ID": {
                "fields": {
                    "eq": {"type": "ID", "nullable": true},
                    "neq": {"type": "ID", "nullable": true},
                    "in": {"type": "[ID!]", "nullable": true},
                    "nin": {"type": "[ID!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "Date": {
                "fields": {
                    "eq": {"type": "Date", "nullable": true},
                    "neq": {"type": "Date", "nullable": true},
                    "lt": {"type": "Date", "nullable": true},
                    "lte": {"type": "Date", "nullable": true},
                    "gt": {"type": "Date", "nullable": true},
                    "gte": {"type": "Date", "nullable": true},
                    "in": {"type": "[Date!]", "nullable": true},
                    "nin": {"type": "[Date!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "DateTime": {
                "fields": {
                    "eq": {"type": "DateTime", "nullable": true},
                    "neq": {"type": "DateTime", "nullable": true},
                    "lt": {"type": "DateTime", "nullable": true},
                    "lte": {"type": "DateTime", "nullable": true},
                    "gt": {"type": "DateTime", "nullable": true},
                    "gte": {"type": "DateTime", "nullable": true},
                    "in": {"type": "[DateTime!]", "nullable": true},
                    "nin": {"type": "[DateTime!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "UUID": {
                "fields": {
                    "eq": {"type": "UUID", "nullable": true},
                    "neq": {"type": "UUID", "nullable": true},
                    "in": {"type": "[UUID!]", "nullable": true},
                    "nin": {"type": "[UUID!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
            "Decimal": {
                "fields": {
                    "eq": {"type": "Decimal", "nullable": true},
                    "neq": {"type": "Decimal", "nullable": true},
                    "lt": {"type": "Decimal", "nullable": true},
                    "lte": {"type": "Decimal", "nullable": true},
                    "gt": {"type": "Decimal", "nullable": true},
                    "gte": {"type": "Decimal", "nullable": true},
                    "in": {"type": "[Decimal!]", "nullable": true},
                    "nin": {"type": "[Decimal!]", "nullable": true},
                    "isnull": {"type": "Boolean", "nullable": true},
                }
            },
        },
        "order_by_schemas": {
            "directions": {
                "ASC": {"value": "ASC"},
                "DESC": {"value": "DESC"},
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_export_structure() {
        let schema = export_schema_generators();
        assert!(schema.get("version").is_some());
        assert!(schema.get("filter_schemas").is_some());
        assert!(schema.get("order_by_schemas").is_some());
    }

    #[test]
    fn test_string_filter_operators() {
        let schema = export_schema_generators();
        let string_filter = schema["filter_schemas"]["String"].as_object().unwrap();
        assert!(string_filter.contains_key("fields"));

        let fields = string_filter["fields"].as_object().unwrap();
        assert!(fields.contains_key("eq"));
        assert!(fields.contains_key("contains"));
        assert!(fields.contains_key("in"));
    }
}
