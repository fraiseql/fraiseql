//! SQL template generator - lowers IR to database-specific SQL.
//!
//! # Overview
//!
//! Transforms validated IR into SQL templates for each query/mutation.
//! Supports multiple database backends with dialect-specific generation.

use crate::error::Result;
use super::ir::AuthoringIR;

/// Database target for SQL generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseTarget {
    /// PostgreSQL database.
    PostgreSQL,
    /// MySQL database.
    MySQL,
    /// SQLite database.
    SQLite,
    /// SQL Server database.
    SQLServer,
}

/// SQL template for a query/mutation.
#[derive(Debug, Clone)]
pub struct SqlTemplate {
    /// Template name (query/mutation name).
    pub name: String,
    /// SQL template with placeholders.
    pub template: String,
    /// Parameter names.
    pub parameters: Vec<String>,
}

/// SQL template generator.
pub struct SqlTemplateGenerator {
    target: DatabaseTarget,
}

impl SqlTemplateGenerator {
    /// Create new SQL template generator.
    #[must_use]
    pub fn new(target: DatabaseTarget) -> Self {
        Self { target }
    }

    /// Generate SQL templates from IR.
    ///
    /// # Arguments
    ///
    /// * `ir` - Validated IR
    ///
    /// # Returns
    ///
    /// SQL templates for all queries/mutations
    ///
    /// # Errors
    ///
    /// Returns error if SQL generation fails.
    pub fn generate(&self, ir: &AuthoringIR) -> Result<Vec<SqlTemplate>> {
        let mut templates = Vec::new();

        // TODO: Generate SQL templates for each query
        for query in &ir.queries {
            if let Some(sql_source) = &query.sql_source {
                let template = SqlTemplate {
                    name: query.name.clone(),
                    template: format!("SELECT data FROM {sql_source}"),
                    parameters: Vec::new(),
                };
                templates.push(template);
            }
        }

        // TODO: Generate SQL templates for each mutation
        for mutation in &ir.mutations {
            let template = SqlTemplate {
                name: mutation.name.clone(),
                template: "-- TODO: Mutation SQL".to_string(),
                parameters: Vec::new(),
            };
            templates.push(template);
        }

        Ok(templates)
    }

    /// Get database target.
    #[must_use]
    pub const fn target(&self) -> DatabaseTarget {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_template_generator_new() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);
        assert_eq!(generator.target(), DatabaseTarget::PostgreSQL);
    }

    #[test]
    fn test_database_target_equality() {
        assert_eq!(DatabaseTarget::PostgreSQL, DatabaseTarget::PostgreSQL);
        assert_ne!(DatabaseTarget::PostgreSQL, DatabaseTarget::MySQL);
    }
}
