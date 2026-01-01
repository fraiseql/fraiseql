//! SQL composition for complete queries.

use crate::graphql::types::ParsedQuery;
use crate::query::schema::SchemaMetadata;
use crate::query::where_builder::{ParameterValue, WhereClauseBuilder};
use anyhow::{Context, Result};

/// Composes SQL queries from GraphQL parsed queries
#[derive(Debug)]
pub struct SQLComposer {
    schema: SchemaMetadata,
}

/// Composed SQL query with typed parameters
#[derive(Debug)]
pub struct ComposedSQL {
    /// SQL query string
    pub sql: String,
    /// Query parameters with typed values
    pub parameters: Vec<(String, ParameterValue)>,
}

impl SQLComposer {
    /// Create a new SQL composer with schema metadata
    #[must_use]
    pub const fn new(schema: SchemaMetadata) -> Self {
        Self { schema }
    }

    /// Compose complete SQL query from parsed GraphQL.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Root table/view not found in schema metadata
    /// - WHERE clause building fails (invalid filter syntax)
    /// - ORDER BY clause construction fails
    /// - SQL composition fails
    pub fn compose(&self, parsed_query: &ParsedQuery) -> Result<ComposedSQL> {
        // Get root field
        let root_field = &parsed_query.selections[0];
        let view_name = self
            .schema
            .get_table(&root_field.name)
            .context(format!("Table not found: {}", root_field.name))?
            .view_name
            .clone();

        // Start building WHERE clause
        let mut where_builder =
            WhereClauseBuilder::new(self.schema.clone(), root_field.name.clone());

        // Extract WHERE clause
        // Phase 7.1: Check for pre-compiled WHERE SQL in schema first (pass-through)
        let where_clause = if let Some(table_schema) = self.schema.get_table(&root_field.name) {
            if let Some(ref where_sql) = table_schema.where_sql {
                // Use pre-compiled WHERE SQL from schema (Phase 7.1)
                where_sql.clone()
            } else if let Some(where_arg) =
                root_field.arguments.iter().find(|arg| arg.name == "where")
            {
                // Build WHERE from GraphQL argument (existing behavior)
                where_builder.build_where(where_arg)?
            } else {
                String::new()
            }
        } else if let Some(where_arg) = root_field.arguments.iter().find(|arg| arg.name == "where")
        {
            // Fallback: build WHERE from GraphQL argument
            where_builder.build_where(where_arg)?
        } else {
            String::new()
        };

        // Extract ORDER BY
        // Phase 7.1: Check for ORDER BY in schema first
        let order_clause = self.schema.get_table(&root_field.name).map_or_else(
            || {
                // Fallback: check GraphQL argument
                root_field
                    .arguments
                    .iter()
                    .find(|arg| arg.name == "order_by" || arg.name == "orderBy")
                    .map_or(String::new(), Self::build_order_clause)
            },
            |table_schema| {
                if table_schema.order_by.is_empty() {
                    // Check GraphQL argument
                    root_field
                        .arguments
                        .iter()
                        .find(|arg| arg.name == "order_by" || arg.name == "orderBy")
                        .map_or(String::new(), Self::build_order_clause)
                } else {
                    // Use ORDER BY from schema (Phase 7.1)
                    Self::build_order_from_tuples(&table_schema.order_by)
                }
            },
        );

        // Extract pagination
        let limit_clause = root_field
            .arguments
            .iter()
            .find(|arg| arg.name == "limit")
            .map_or_else(|| "LIMIT 100".to_string(), Self::build_limit_clause);

        let offset_clause = root_field
            .arguments
            .iter()
            .find(|arg| arg.name == "offset")
            .map_or(String::new(), Self::build_offset_clause);

        // Build base SELECT
        let sql = format!(
            "SELECT CAST(row_to_json(t) AS text) AS data FROM {} t {}{}{}{}",
            view_name,
            if where_clause.is_empty() {
                String::new()
            } else {
                format!("WHERE {where_clause}")
            },
            if order_clause.is_empty() {
                String::new()
            } else {
                format!(" {order_clause}")
            },
            if limit_clause.is_empty() {
                String::new()
            } else {
                format!(" {limit_clause}")
            },
            if offset_clause.is_empty() {
                String::new()
            } else {
                format!(" {offset_clause}")
            }
        );

        Ok(ComposedSQL {
            sql,
            parameters: where_builder.get_params(),
        })
    }

    fn build_order_clause(_order_arg: &crate::graphql::types::GraphQLArgument) -> String {
        // Parse ORDER BY argument
        // TODO: Implement proper ORDER BY parsing from GraphQL argument
        "ORDER BY t.id DESC".to_string()
    }

    /// Build ORDER BY clause from tuples (Phase 7.1).
    ///
    /// # Arguments
    ///
    /// * `order_by` - List of (`field_name`, `direction`) tuples
    ///
    /// # Examples
    ///
    /// ```ignore
    /// build_order_from_tuples(&[("created_at".to_string(), "DESC".to_string())])
    /// // Returns: "ORDER BY t.created_at DESC"
    /// ```
    fn build_order_from_tuples(order_by: &[(String, String)]) -> String {
        if order_by.is_empty() {
            return String::new();
        }

        let clauses: Vec<String> = order_by
            .iter()
            .map(|(field, direction)| {
                // Validate direction
                let dir = match direction.to_uppercase().as_str() {
                    "ASC" | "DESC" => direction.to_uppercase(),
                    _ => "ASC".to_string(), // Default to ASC if invalid
                };

                format!("t.{field} {dir}")
            })
            .collect();

        format!("ORDER BY {}", clauses.join(", "))
    }

    fn build_limit_clause(limit_arg: &crate::graphql::types::GraphQLArgument) -> String {
        // Extract limit value
        limit_arg.value_json.parse::<i64>().map_or_else(
            |_| "LIMIT 100".to_string(),
            |limit| format!("LIMIT {limit}"),
        )
    }

    fn build_offset_clause(offset_arg: &crate::graphql::types::GraphQLArgument) -> String {
        // Extract offset value
        offset_arg
            .value_json
            .parse::<i64>()
            .map_or(String::new(), |offset| format!("OFFSET {offset}"))
    }
}
