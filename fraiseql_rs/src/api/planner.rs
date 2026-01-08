//! GraphQL Query Planner Layer
//!
//! This module converts parsed GraphQL queries into executable SQL execution plans.
//! Responsibilities:
//! - Take ParsedQuery from parser layer
//! - Access schema to resolve field types and mappings
//! - Build SQL queries for nested selections
//! - Handle aliases and argument transformations
//! - Return ExecutionPlan with SQL information

use crate::api::error::ApiError;
use crate::api::parser::{FieldSelection, OperationType, ParsedQuery};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a SQL query to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlQuery {
    /// The actual SQL to execute
    pub sql: String,

    /// Parameters to bind to the query
    pub parameters: Vec<serde_json::Value>,

    /// Root field this query serves (e.g., "users", "posts")
    pub root_field: String,

    /// Whether this query returns a list or single result
    pub is_list: bool,
}

/// Maps SQL columns to GraphQL fields in the response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultMapping {
    /// How to transform SQL column names to GraphQL field names
    pub column_to_field: HashMap<String, String>,

    /// Which columns to include in the result
    pub selected_columns: Vec<String>,

    /// Nested query plans for nested selections
    pub nested_plans: HashMap<String, Box<ExecutionPlan>>,
}

/// Metadata for transforming SQL results into GraphQL response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    /// Return type (e.g., "User", "[Post]")
    pub return_type: String,

    /// Whether to include __typename field
    pub include_typename: bool,

    /// Field aliases (map original names to aliases in response)
    pub aliases: HashMap<String, String>,
}

/// Complete execution plan for a GraphQL query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// SQL queries to execute (in order)
    pub sql_queries: Vec<SqlQuery>,

    /// How to map SQL results to GraphQL response
    pub result_mapping: ResultMapping,

    /// Metadata for response transformation
    pub response_metadata: ResponseMetadata,
}

/// Planner for converting ParsedQuery to ExecutionPlan
pub struct Planner {
    /// Schema information (would be filled from actual schema registry)
    /// For Phase 2, this is minimal - just field name tracking
    schema: PlannerSchema,
}

/// Minimal schema information for planning
#[derive(Debug, Clone)]
struct PlannerSchema {
    /// Mapping of field names to table/column info
    field_mappings: HashMap<String, FieldMapping>,
}

/// Information about how a field maps to database
#[derive(Debug, Clone)]
struct FieldMapping {
    /// Database table name
    pub table: String,

    /// Database column name
    pub column: String,

    /// Whether this field returns a list
    pub is_list: bool,
}

impl Planner {
    /// Create a new Planner (Phase 2: basic schema)
    pub fn new() -> Self {
        // Phase 2: Hardcode common field mappings
        // Phase 3: Would read from actual SchemaRegistry
        let mut field_mappings = HashMap::new();

        // Common field mappings for demonstration
        field_mappings.insert(
            "users".to_string(),
            FieldMapping {
                table: "users".to_string(),
                column: "*".to_string(),
                is_list: true,
            },
        );
        field_mappings.insert(
            "user".to_string(),
            FieldMapping {
                table: "users".to_string(),
                column: "*".to_string(),
                is_list: false,
            },
        );
        field_mappings.insert(
            "posts".to_string(),
            FieldMapping {
                table: "posts".to_string(),
                column: "*".to_string(),
                is_list: true,
            },
        );
        field_mappings.insert(
            "post".to_string(),
            FieldMapping {
                table: "posts".to_string(),
                column: "*".to_string(),
                is_list: false,
            },
        );

        Planner {
            schema: PlannerSchema { field_mappings },
        }
    }

    /// Plan a query from ParsedQuery into ExecutionPlan
    ///
    /// # Arguments
    /// * `parsed` - The parsed query from the parser layer
    ///
    /// # Returns
    /// * `Result<ExecutionPlan, ApiError>` - Execution plan or error
    pub fn plan_query(&self, parsed: ParsedQuery) -> Result<ExecutionPlan, ApiError> {
        if parsed.operation_type != OperationType::Query {
            return Err(ApiError::QueryError(
                "Planner received non-query operation".to_string(),
            ));
        }

        self.plan_operation(parsed)
    }

    /// Plan a mutation from ParsedQuery into ExecutionPlan
    ///
    /// # Arguments
    /// * `parsed` - The parsed mutation from the parser layer
    ///
    /// # Returns
    /// * `Result<ExecutionPlan, ApiError>` - Execution plan or error
    pub fn plan_mutation(&self, parsed: ParsedQuery) -> Result<ExecutionPlan, ApiError> {
        if parsed.operation_type != OperationType::Mutation {
            return Err(ApiError::QueryError(
                "Planner received non-mutation operation".to_string(),
            ));
        }

        self.plan_operation(parsed)
    }

    /// Internal: Plan a query or mutation operation
    fn plan_operation(&self, parsed: ParsedQuery) -> Result<ExecutionPlan, ApiError> {
        let mut sql_queries = Vec::new();
        let nested_plans = HashMap::new();
        let mut column_to_field = HashMap::new();
        let mut selected_columns = Vec::new();

        // Build SQL query for each root field
        for root_field in &parsed.root_fields {
            // Get field mapping from schema
            let field_mapping = self
                .schema
                .field_mappings
                .get(&root_field.name)
                .ok_or_else(|| {
                    ApiError::QueryError(format!("Unknown field: {}", root_field.name))
                })?;

            // Build WHERE clause from field arguments
            let where_clause = self.build_where_clause(&root_field.arguments)?;

            // Build SELECT clause from nested selections
            let (select_list, column_map) =
                self.build_select_list(&root_field.nested_selections)?;
            column_to_field.extend(column_map);
            selected_columns.extend(select_list.iter().cloned());

            // Build SQL query
            let sql = if where_clause.is_empty() {
                format!(
                    "SELECT {} FROM {}",
                    select_list.join(", "),
                    field_mapping.table
                )
            } else {
                format!(
                    "SELECT {} FROM {} WHERE {}",
                    select_list.join(", "),
                    field_mapping.table,
                    where_clause
                )
            };

            sql_queries.push(SqlQuery {
                sql,
                parameters: vec![], // TODO: Extract from arguments
                root_field: root_field.name.clone(),
                is_list: field_mapping.is_list,
            });

            // Plan nested selections (would recursively handle nested queries)
            if !root_field.nested_selections.is_empty() {
                // For Phase 2, nested queries are flattened into the main query
                // Phase 3 would handle separate nested queries with joins
            }
        }

        // Determine return type
        let return_type = if !parsed.root_fields.is_empty() {
            parsed.root_fields[0].name.clone()
        } else {
            "unknown".to_string()
        };

        Ok(ExecutionPlan {
            sql_queries,
            result_mapping: ResultMapping {
                column_to_field,
                selected_columns,
                nested_plans,
            },
            response_metadata: ResponseMetadata {
                return_type,
                include_typename: false,
                aliases: parsed
                    .root_fields
                    .iter()
                    .filter_map(|f| f.alias.as_ref().map(|a| (f.name.clone(), a.clone())))
                    .collect(),
            },
        })
    }

    /// Build WHERE clause from field arguments
    fn build_where_clause(
        &self,
        arguments: &std::collections::HashMap<String, crate::api::parser::ArgumentValue>,
    ) -> Result<String, ApiError> {
        if arguments.is_empty() {
            return Ok(String::new());
        }

        let mut conditions = Vec::new();
        for (name, value) in arguments {
            // For Phase 2, simple string representation
            // Phase 3 would handle proper parameterization
            let condition = format!("{} = '{}'", name, value_to_string(value)?);
            conditions.push(condition);
        }

        Ok(conditions.join(" AND "))
    }

    /// Build SELECT clause from nested field selections
    fn build_select_list(
        &self,
        selections: &[FieldSelection],
    ) -> Result<(Vec<String>, HashMap<String, String>), ApiError> {
        let mut columns = Vec::new();
        let mut column_map = HashMap::new();

        if selections.is_empty() {
            // If no fields specified, select all
            columns.push("*".to_string());
        } else {
            for selection in selections {
                columns.push(selection.name.clone());
                if let Some(alias) = &selection.alias {
                    column_map.insert(selection.name.clone(), alias.clone());
                }
            }
        }

        Ok((columns, column_map))
    }
}

/// Helper: Convert ArgumentValue to string for WHERE clause
fn value_to_string(value: &crate::api::parser::ArgumentValue) -> Result<String, ApiError> {
    use crate::api::parser::ArgumentValue;

    Ok(match value {
        ArgumentValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        ArgumentValue::Int(i) => i.to_string(),
        ArgumentValue::Float(f) => f.to_string(),
        ArgumentValue::Boolean(b) => b.to_string(),
        ArgumentValue::Null => "NULL".to_string(),
        ArgumentValue::Variable(_v) => {
            return Err(ApiError::QueryError(
                "Variables in WHERE clause not yet supported".to_string(),
            ))
        }
        ArgumentValue::List(_) => {
            return Err(ApiError::QueryError(
                "List values in WHERE clause not yet supported".to_string(),
            ))
        }
        ArgumentValue::Object(_) => {
            return Err(ApiError::QueryError(
                "Object values in WHERE clause not yet supported".to_string(),
            ))
        }
    })
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::parser::{parse_graphql_query, ArgumentValue};

    #[test]
    fn test_plan_simple_query() {
        let parsed = parse_graphql_query("{ users { id name } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert_eq!(plan.sql_queries.len(), 1);
        assert!(plan.sql_queries[0].sql.contains("SELECT"));
        assert!(plan.sql_queries[0].sql.contains("FROM users"));
    }

    #[test]
    fn test_plan_query_preserves_root_field() {
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert_eq!(plan.sql_queries[0].root_field, "users");
        assert_eq!(plan.response_metadata.return_type, "users");
    }

    #[test]
    fn test_plan_query_marks_list_queries() {
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert!(plan.sql_queries[0].is_list);
    }

    #[test]
    fn test_plan_query_marks_single_queries() {
        let parsed = parse_graphql_query("{ user { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert!(!plan.sql_queries[0].is_list);
    }

    #[test]
    fn test_plan_with_aliases() {
        let parsed = parse_graphql_query("{ u: user { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert!(plan.response_metadata.aliases.contains_key("user"));
        assert_eq!(plan.response_metadata.aliases["user"], "u");
    }

    #[test]
    fn test_plan_mutation() {
        let parsed = parse_graphql_query("mutation { createUser(name: \"John\") { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_mutation(parsed).unwrap();

        assert_eq!(plan.sql_queries.len(), 1);
    }

    #[test]
    fn test_plan_invalid_field_fails() {
        let parsed = parse_graphql_query("{ unknownField { id } }").unwrap();
        let planner = Planner::new();

        let result = planner.plan_query(parsed);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown field"));
    }

    #[test]
    fn test_plan_multiple_root_fields() {
        let parsed = parse_graphql_query("{ users { id } posts { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        assert_eq!(plan.sql_queries.len(), 2);
        assert_eq!(plan.sql_queries[0].root_field, "users");
        assert_eq!(plan.sql_queries[1].root_field, "posts");
    }

    #[test]
    fn test_plan_with_arguments() {
        let parsed = parse_graphql_query("{ user(id: \"123\") { id } }").unwrap();
        let planner = Planner::new();

        let plan = planner.plan_query(parsed).unwrap();

        // WHERE clause should be present in SQL
        assert!(plan.sql_queries[0].sql.contains("WHERE"));
    }

    #[test]
    fn test_plan_field_mapping_resolution() {
        let planner = Planner::new();

        // Should resolve 'users' field
        assert!(planner.schema.field_mappings.contains_key("users"));

        // Should not resolve unknown field
        assert!(!planner.schema.field_mappings.contains_key("unknown"));
    }

    #[test]
    fn test_build_where_clause_empty() {
        let planner = Planner::new();
        let args = HashMap::new();

        let where_clause = planner.build_where_clause(&args).unwrap();

        assert!(where_clause.is_empty());
    }

    #[test]
    fn test_build_where_clause_single_arg() {
        let planner = Planner::new();
        let mut args = HashMap::new();
        args.insert("id".to_string(), ArgumentValue::String("123".to_string()));

        let where_clause = planner.build_where_clause(&args).unwrap();

        assert!(!where_clause.is_empty());
        assert!(where_clause.contains("id"));
    }

    #[test]
    fn test_value_to_string_string() {
        let result = value_to_string(&ArgumentValue::String("test".to_string())).unwrap();
        assert_eq!(result, "'test'");
    }

    #[test]
    fn test_value_to_string_int() {
        let result = value_to_string(&ArgumentValue::Int(42)).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_value_to_string_null() {
        let result = value_to_string(&ArgumentValue::Null).unwrap();
        assert_eq!(result, "NULL");
    }
}
