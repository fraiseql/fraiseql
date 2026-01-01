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

        // Extract WHERE argument if present
        let where_clause =
            if let Some(where_arg) = root_field.arguments.iter().find(|arg| arg.name == "where") {
                where_builder.build_where(where_arg)?
            } else {
                String::new()
            };

        // Extract ORDER BY
        let order_clause = if let Some(order_arg) = root_field
            .arguments
            .iter()
            .find(|arg| arg.name == "order_by" || arg.name == "orderBy")
        {
            Self::build_order_clause(order_arg)
        } else {
            String::new()
        };

        // Extract pagination
        let limit_clause =
            if let Some(limit_arg) = root_field.arguments.iter().find(|arg| arg.name == "limit") {
                Self::build_limit_clause(limit_arg)
            } else {
                "LIMIT 100".to_string() // Default limit
            };

        let offset_clause = if let Some(offset_arg) =
            root_field.arguments.iter().find(|arg| arg.name == "offset")
        {
            Self::build_offset_clause(offset_arg)
        } else {
            String::new()
        };

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

    fn build_limit_clause(limit_arg: &crate::graphql::types::GraphQLArgument) -> String {
        // Extract limit value
        match limit_arg.value_json.parse::<i64>() {
            Ok(limit) => format!("LIMIT {limit}"),
            Err(_) => "LIMIT 100".to_string(),
        }
    }

    fn build_offset_clause(offset_arg: &crate::graphql::types::GraphQLArgument) -> String {
        // Extract offset value
        match offset_arg.value_json.parse::<i64>() {
            Ok(offset) => format!("OFFSET {offset}"),
            Err(_) => String::new(),
        }
    }
}
