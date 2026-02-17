//! SQL template generator - lowers IR to database-specific SQL.
//!
//! # Overview
//!
//! Transforms validated IR into SQL templates for each query/mutation.
//! Supports multiple database backends with dialect-specific generation.
//!
//! # Template Syntax
//!
//! Templates use named placeholders in the format `{param_name}` which are
//! replaced at runtime with actual parameter values:
//!
//! - `{where_clause}` - Optional WHERE clause (empty if no filters)
//! - `{order_by}` - Optional ORDER BY clause
//! - `{limit}` - Optional LIMIT value
//! - `{offset}` - Optional OFFSET value
//! - `{arg_name}` - Query argument placeholders
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::compiler::lowering::{SqlTemplateGenerator, DatabaseTarget};
//!
//! let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);
//! let templates = generator.generate(&ir)?;
//!
//! for template in templates {
//!     println!("{}: {}", template.name, template.template);
//! }
//! ```

use super::ir::{AuthoringIR, IRArgument, IRMutation, IRQuery, MutationOperation};
use crate::error::Result;

/// Database target for SQL generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatabaseTarget {
    /// PostgreSQL database.
    #[default]
    PostgreSQL,
    /// MySQL database.
    MySQL,
    /// SQLite database.
    SQLite,
    /// SQL Server database.
    SQLServer,
}

impl DatabaseTarget {
    /// Get the placeholder format for this database.
    fn placeholder(self, index: usize) -> String {
        match self {
            Self::PostgreSQL => format!("${index}"),
            Self::MySQL | Self::SQLite => "?".to_string(),
            Self::SQLServer => format!("@p{index}"),
        }
    }

    /// Get the identifier quoting character for this database.
    fn quote_identifier(self, name: &str) -> String {
        match self {
            Self::PostgreSQL | Self::SQLite => format!("\"{name}\""),
            Self::MySQL => format!("`{name}`"),
            Self::SQLServer => format!("[{name}]"),
        }
    }

    /// Get the JSONB path extraction syntax for this database.
    fn jsonb_extract(self, column: &str, path: &str) -> String {
        match self {
            Self::PostgreSQL => format!("{column}->'{path}'"),
            Self::MySQL => format!("JSON_EXTRACT({column}, '$.{path}')"),
            Self::SQLite => format!("json_extract({column}, '$.{path}')"),
            Self::SQLServer => format!("JSON_VALUE({column}, '$.{path}')"),
        }
    }

    /// Get the LIMIT/OFFSET syntax for this database.
    fn limit_offset(self, limit: Option<&str>, offset: Option<&str>) -> String {
        match self {
            Self::PostgreSQL | Self::SQLite | Self::MySQL => {
                let mut parts = Vec::new();
                if let Some(lim) = limit {
                    parts.push(format!("LIMIT {lim}"));
                }
                if let Some(off) = offset {
                    parts.push(format!("OFFSET {off}"));
                }
                parts.join(" ")
            },
            Self::SQLServer => {
                // SQL Server uses OFFSET...FETCH
                let mut parts = Vec::new();
                if let Some(off) = offset {
                    parts.push(format!("OFFSET {off} ROWS"));
                    if let Some(lim) = limit {
                        parts.push(format!("FETCH NEXT {lim} ROWS ONLY"));
                    }
                } else if let Some(lim) = limit {
                    // Without OFFSET, use TOP in SELECT
                    parts.push(format!("TOP {lim}"));
                }
                parts.join(" ")
            },
        }
    }
}

/// Template kind for categorizing generated templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    /// SELECT query template.
    Query,
    /// INSERT mutation template.
    Insert,
    /// UPDATE mutation template.
    Update,
    /// DELETE mutation template.
    Delete,
    /// Custom mutation template.
    Custom,
}

/// SQL template for a query/mutation.
#[derive(Debug, Clone)]
pub struct SqlTemplate {
    /// Template name (query/mutation name).
    pub name:                String,
    /// Template kind (query, insert, update, delete).
    pub kind:                TemplateKind,
    /// SQL template with placeholders.
    pub template:            String,
    /// Parameter names in order of appearance.
    pub parameters:          Vec<String>,
    /// Whether this template supports dynamic WHERE clauses.
    pub supports_where:      bool,
    /// Whether this template supports dynamic ORDER BY.
    pub supports_order_by:   bool,
    /// Whether this template supports LIMIT/OFFSET.
    pub supports_pagination: bool,
}

impl SqlTemplate {
    /// Create a new query template.
    fn query(name: String, template: String, parameters: Vec<String>) -> Self {
        Self {
            name,
            kind: TemplateKind::Query,
            template,
            parameters,
            supports_where: false,
            supports_order_by: false,
            supports_pagination: false,
        }
    }

    /// Enable WHERE clause support.
    #[must_use]
    pub fn with_where(mut self) -> Self {
        self.supports_where = true;
        self
    }

    /// Enable ORDER BY support.
    #[must_use]
    pub fn with_order_by(mut self) -> Self {
        self.supports_order_by = true;
        self
    }

    /// Enable pagination support.
    #[must_use]
    pub fn with_pagination(mut self) -> Self {
        self.supports_pagination = true;
        self
    }
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

        // Generate SQL templates for each query
        for query in &ir.queries {
            templates.push(self.generate_query_template(query));
        }

        // Generate SQL templates for each mutation
        for mutation in &ir.mutations {
            templates.push(self.generate_mutation_template(mutation));
        }

        Ok(templates)
    }

    /// Generate a query template.
    fn generate_query_template(&self, query: &IRQuery) -> SqlTemplate {
        let table = query.sql_source.as_deref().unwrap_or(&query.return_type);
        let quoted_table = self.target.quote_identifier(table);

        // Build parameter list from arguments
        let parameters: Vec<String> = query.arguments.iter().map(|a| a.name.clone()).collect();

        // Build the base SELECT template
        let mut template = format!("SELECT data FROM {quoted_table}");

        // Add placeholder for dynamic WHERE if supported
        if query.auto_params.has_where || !query.arguments.is_empty() {
            template.push_str(" {{where_clause}}");
        }

        // Add placeholder for ORDER BY if supported
        if query.auto_params.has_order_by {
            template.push_str(" {{order_by}}");
        }

        // Add placeholder for LIMIT/OFFSET if supported
        if query.auto_params.has_limit || query.auto_params.has_offset {
            template.push_str(" {{pagination}}");
        }

        let mut sql_template = SqlTemplate::query(query.name.clone(), template, parameters);

        if query.auto_params.has_where || !query.arguments.is_empty() {
            sql_template = sql_template.with_where();
        }
        if query.auto_params.has_order_by {
            sql_template = sql_template.with_order_by();
        }
        if query.auto_params.has_limit || query.auto_params.has_offset {
            sql_template = sql_template.with_pagination();
        }

        sql_template
    }

    /// Generate a mutation template.
    fn generate_mutation_template(&self, mutation: &IRMutation) -> SqlTemplate {
        // Infer table name from return type (lowercase)
        let table = mutation.return_type.to_lowercase();
        let quoted_table = self.target.quote_identifier(&table);

        // Build parameter list from arguments
        let parameters: Vec<String> = mutation.arguments.iter().map(|a| a.name.clone()).collect();

        let (template, kind) = match mutation.operation {
            MutationOperation::Create => {
                self.generate_insert_template(&quoted_table, &mutation.arguments)
            },
            MutationOperation::Update => {
                self.generate_update_template(&quoted_table, &mutation.arguments)
            },
            MutationOperation::Delete => {
                self.generate_delete_template(&quoted_table, &mutation.arguments)
            },
            MutationOperation::Custom => {
                (format!("-- Custom mutation: {}", mutation.name), TemplateKind::Custom)
            },
        };

        SqlTemplate {
            name: mutation.name.clone(),
            kind,
            template,
            parameters,
            supports_where: false,
            supports_order_by: false,
            supports_pagination: false,
        }
    }

    /// Generate an INSERT template.
    fn generate_insert_template(
        &self,
        quoted_table: &str,
        arguments: &[IRArgument],
    ) -> (String, TemplateKind) {
        if arguments.is_empty() {
            return (
                format!("INSERT INTO {quoted_table} (data) VALUES ({{data}}) RETURNING data"),
                TemplateKind::Insert,
            );
        }

        // For mutations with arguments, we expect an "input" argument containing the data
        let has_input = arguments.iter().any(|a| a.name == "input");

        if has_input {
            // Standard input pattern
            (
                format!("INSERT INTO {quoted_table} (data) VALUES ({{input}}) RETURNING data"),
                TemplateKind::Insert,
            )
        } else {
            // Build column list from arguments
            let columns: Vec<&str> = arguments.iter().map(|a| a.name.as_str()).collect();
            let placeholders: Vec<String> =
                (1..=columns.len()).map(|i| self.target.placeholder(i)).collect();

            (
                format!(
                    "INSERT INTO {quoted_table} ({}) VALUES ({}) RETURNING data",
                    columns.join(", "),
                    placeholders.join(", ")
                ),
                TemplateKind::Insert,
            )
        }
    }

    /// Generate an UPDATE template.
    fn generate_update_template(
        &self,
        quoted_table: &str,
        arguments: &[IRArgument],
    ) -> (String, TemplateKind) {
        // Find the id argument (usually "id" or "where")
        let id_arg = arguments.iter().find(|a| a.name == "id" || a.name == "where");

        // Find the input argument
        let input_arg = arguments.iter().find(|a| a.name == "input" || a.name == "data");

        match (id_arg, input_arg) {
            (Some(id), Some(_)) => (
                format!(
                    "UPDATE {quoted_table} SET data = {{input}} WHERE {} = {{{}}} RETURNING data",
                    self.target.jsonb_extract("data", "id"),
                    id.name
                ),
                TemplateKind::Update,
            ),
            (Some(id), None) => {
                // Update with individual fields
                let set_clauses: Vec<String> = arguments
                    .iter()
                    .filter(|a| a.name != "id" && a.name != "where")
                    .map(|a| {
                        format!("data = jsonb_set(data, '{{{{{}}}}}', {{{}}})", a.name, a.name)
                    })
                    .collect();

                if set_clauses.is_empty() {
                    (
                        format!(
                            "UPDATE {quoted_table} SET data = {{data}} WHERE {} = {{{}}} RETURNING data",
                            self.target.jsonb_extract("data", "id"),
                            id.name
                        ),
                        TemplateKind::Update,
                    )
                } else {
                    (
                        format!(
                            "UPDATE {quoted_table} SET {} WHERE {} = {{{}}} RETURNING data",
                            set_clauses.join(", "),
                            self.target.jsonb_extract("data", "id"),
                            id.name
                        ),
                        TemplateKind::Update,
                    )
                }
            },
            _ => (
                format!(
                    "UPDATE {quoted_table} SET data = {{data}} WHERE {{where_clause}} RETURNING data"
                ),
                TemplateKind::Update,
            ),
        }
    }

    /// Generate a DELETE template.
    fn generate_delete_template(
        &self,
        quoted_table: &str,
        arguments: &[IRArgument],
    ) -> (String, TemplateKind) {
        // Find the id argument
        let id_arg = arguments.iter().find(|a| a.name == "id" || a.name == "where");

        if let Some(id) = id_arg {
            (
                format!(
                    "DELETE FROM {quoted_table} WHERE {} = {{{}}} RETURNING data",
                    self.target.jsonb_extract("data", "id"),
                    id.name
                ),
                TemplateKind::Delete,
            )
        } else {
            (
                format!("DELETE FROM {quoted_table} WHERE {{where_clause}} RETURNING data"),
                TemplateKind::Delete,
            )
        }
    }

    /// Get database target.
    #[must_use]
    pub const fn target(&self) -> DatabaseTarget {
        self.target
    }

    /// Expand a template with actual parameter values.
    ///
    /// This method replaces template placeholders with actual values:
    /// - `{param}` -> actual parameter value
    /// - `{where_clause}` -> WHERE clause or empty
    /// - `{order_by}` -> ORDER BY clause or empty
    /// - `{pagination}` -> LIMIT/OFFSET or empty
    pub fn expand_template(
        &self,
        template: &SqlTemplate,
        params: &std::collections::HashMap<String, serde_json::Value>,
        where_clause: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> String {
        let mut sql = template.template.clone();

        // Replace parameter placeholders
        for (i, param_name) in template.parameters.iter().enumerate() {
            if let Some(value) = params.get(param_name) {
                let placeholder = format!("{{{param_name}}}");
                let replacement = self.value_to_sql(value);
                sql = sql.replace(&placeholder, &replacement);
            } else {
                // Replace with positional placeholder for unbound params
                let placeholder = format!("{{{param_name}}}");
                sql = sql.replace(&placeholder, &self.target.placeholder(i + 1));
            }
        }

        // Replace WHERE clause placeholder
        if template.supports_where {
            let where_sql = where_clause.map(|w| format!("WHERE {w}")).unwrap_or_default();
            sql = sql.replace("{{where_clause}}", &where_sql);
        }

        // Replace ORDER BY placeholder
        if template.supports_order_by {
            let order_sql = order_by.map(|o| format!("ORDER BY {o}")).unwrap_or_default();
            sql = sql.replace("{{order_by}}", &order_sql);
        }

        // Replace pagination placeholder
        if template.supports_pagination {
            let limit_str = limit.map(|l| l.to_string());
            let offset_str = offset.map(|o| o.to_string());
            let pagination_sql =
                self.target.limit_offset(limit_str.as_deref(), offset_str.as_deref());
            sql = sql.replace("{{pagination}}", &pagination_sql);
        }

        // Clean up any remaining empty placeholders
        sql = sql.replace("{{where_clause}}", "");
        sql = sql.replace("{{order_by}}", "");
        sql = sql.replace("{{pagination}}", "");

        // Clean up extra whitespace
        sql.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Convert a JSON value to SQL literal.
    fn value_to_sql(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "NULL".to_string(),
            serde_json::Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                format!("'{}'", value.to_string().replace('\'', "''"))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::ir::{AutoParams, IRArgument},
        *,
    };

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

    #[test]
    fn test_database_target_placeholder() {
        assert_eq!(DatabaseTarget::PostgreSQL.placeholder(1), "$1");
        assert_eq!(DatabaseTarget::PostgreSQL.placeholder(5), "$5");
        assert_eq!(DatabaseTarget::MySQL.placeholder(1), "?");
        assert_eq!(DatabaseTarget::SQLite.placeholder(1), "?");
        assert_eq!(DatabaseTarget::SQLServer.placeholder(1), "@p1");
    }

    #[test]
    fn test_database_target_quote_identifier() {
        assert_eq!(DatabaseTarget::PostgreSQL.quote_identifier("users"), "\"users\"");
        assert_eq!(DatabaseTarget::MySQL.quote_identifier("users"), "`users`");
        assert_eq!(DatabaseTarget::SQLite.quote_identifier("users"), "\"users\"");
        assert_eq!(DatabaseTarget::SQLServer.quote_identifier("users"), "[users]");
    }

    #[test]
    fn test_database_target_jsonb_extract() {
        assert_eq!(DatabaseTarget::PostgreSQL.jsonb_extract("data", "name"), "data->'name'");
        assert_eq!(
            DatabaseTarget::MySQL.jsonb_extract("data", "name"),
            "JSON_EXTRACT(data, '$.name')"
        );
        assert_eq!(
            DatabaseTarget::SQLite.jsonb_extract("data", "name"),
            "json_extract(data, '$.name')"
        );
        assert_eq!(
            DatabaseTarget::SQLServer.jsonb_extract("data", "name"),
            "JSON_VALUE(data, '$.name')"
        );
    }

    #[test]
    fn test_database_target_limit_offset() {
        // PostgreSQL/MySQL/SQLite
        assert_eq!(DatabaseTarget::PostgreSQL.limit_offset(Some("10"), None), "LIMIT 10");
        assert_eq!(
            DatabaseTarget::PostgreSQL.limit_offset(Some("10"), Some("5")),
            "LIMIT 10 OFFSET 5"
        );
        assert_eq!(DatabaseTarget::PostgreSQL.limit_offset(None, Some("5")), "OFFSET 5");

        // SQL Server
        assert_eq!(DatabaseTarget::SQLServer.limit_offset(Some("10"), None), "TOP 10");
        assert_eq!(
            DatabaseTarget::SQLServer.limit_offset(Some("10"), Some("5")),
            "OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }

    #[test]
    fn test_generate_query_template_basic() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        };

        let template = generator.generate_query_template(&query);

        assert_eq!(template.name, "users");
        assert_eq!(template.kind, TemplateKind::Query);
        assert_eq!(template.template, "SELECT data FROM \"v_user\"");
        assert!(!template.supports_where);
        assert!(!template.supports_order_by);
        assert!(!template.supports_pagination);
    }

    #[test]
    fn test_generate_query_template_with_auto_params() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams {
                has_where:    true,
                has_order_by: true,
                has_limit:    true,
                has_offset:   true,
            },
        };

        let template = generator.generate_query_template(&query);

        assert!(template.template.contains("{{where_clause}}"));
        assert!(template.template.contains("{{order_by}}"));
        assert!(template.template.contains("{{pagination}}"));
        assert!(template.supports_where);
        assert!(template.supports_order_by);
        assert!(template.supports_pagination);
    }

    #[test]
    fn test_generate_query_template_with_arguments() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let query = IRQuery {
            name:         "user".to_string(),
            return_type:  "User".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![IRArgument {
                name:          "id".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        };

        let template = generator.generate_query_template(&query);

        assert_eq!(template.parameters, vec!["id"]);
        assert!(template.supports_where); // Arguments imply WHERE support
    }

    #[test]
    fn test_generate_mutation_template_insert() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let mutation = IRMutation {
            name:        "createUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![IRArgument {
                name:          "input".to_string(),
                arg_type:      "CreateUserInput!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: None,
            operation:   MutationOperation::Create,
        };

        let template = generator.generate_mutation_template(&mutation);

        assert_eq!(template.name, "createUser");
        assert_eq!(template.kind, TemplateKind::Insert);
        assert!(template.template.contains("INSERT INTO"));
        assert!(template.template.contains("{input}"));
        assert!(template.template.contains("RETURNING data"));
    }

    #[test]
    fn test_generate_mutation_template_update() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let mutation = IRMutation {
            name:        "updateUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![
                IRArgument {
                    name:          "id".to_string(),
                    arg_type:      "ID!".to_string(),
                    nullable:      false,
                    default_value: None,
                    description:   None,
                },
                IRArgument {
                    name:          "input".to_string(),
                    arg_type:      "UpdateUserInput!".to_string(),
                    nullable:      false,
                    default_value: None,
                    description:   None,
                },
            ],
            description: None,
            operation:   MutationOperation::Update,
        };

        let template = generator.generate_mutation_template(&mutation);

        assert_eq!(template.name, "updateUser");
        assert_eq!(template.kind, TemplateKind::Update);
        assert!(template.template.contains("UPDATE"));
        assert!(template.template.contains("{id}"));
        assert!(template.template.contains("RETURNING data"));
    }

    #[test]
    fn test_generate_mutation_template_delete() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let mutation = IRMutation {
            name:        "deleteUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![IRArgument {
                name:          "id".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: None,
            operation:   MutationOperation::Delete,
        };

        let template = generator.generate_mutation_template(&mutation);

        assert_eq!(template.name, "deleteUser");
        assert_eq!(template.kind, TemplateKind::Delete);
        assert!(template.template.contains("DELETE FROM"));
        assert!(template.template.contains("{id}"));
    }

    #[test]
    fn test_generate_all_templates() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);
        let mut ir = AuthoringIR::new();

        ir.queries.push(IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        });

        ir.mutations.push(IRMutation {
            name:        "createUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![],
            description: None,
            operation:   MutationOperation::Create,
        });

        let templates = generator.generate(&ir).unwrap();

        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].name, "users");
        assert_eq!(templates[1].name, "createUser");
    }

    #[test]
    fn test_expand_template_basic() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let template = SqlTemplate::query(
            "users".to_string(),
            "SELECT data FROM \"v_user\"".to_string(),
            vec![],
        );

        let params = std::collections::HashMap::new();
        let sql = generator.expand_template(&template, &params, None, None, None, None);

        assert_eq!(sql, "SELECT data FROM \"v_user\"");
    }

    #[test]
    fn test_expand_template_with_where() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let template = SqlTemplate::query(
            "users".to_string(),
            "SELECT data FROM \"v_user\" {{where_clause}}".to_string(),
            vec![],
        )
        .with_where();

        let params = std::collections::HashMap::new();
        let sql = generator.expand_template(
            &template,
            &params,
            Some("data->>'status' = 'active'"),
            None,
            None,
            None,
        );

        assert_eq!(sql, "SELECT data FROM \"v_user\" WHERE data->>'status' = 'active'");
    }

    #[test]
    fn test_expand_template_with_pagination() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let template = SqlTemplate::query(
            "users".to_string(),
            "SELECT data FROM \"v_user\" {{pagination}}".to_string(),
            vec![],
        )
        .with_pagination();

        let params = std::collections::HashMap::new();
        let sql = generator.expand_template(&template, &params, None, None, Some(10), Some(5));

        assert_eq!(sql, "SELECT data FROM \"v_user\" LIMIT 10 OFFSET 5");
    }

    #[test]
    fn test_expand_template_with_order_by() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        let template = SqlTemplate::query(
            "users".to_string(),
            "SELECT data FROM \"v_user\" {{order_by}}".to_string(),
            vec![],
        )
        .with_order_by();

        let params = std::collections::HashMap::new();
        let sql = generator.expand_template(
            &template,
            &params,
            None,
            Some("data->>'name' ASC"),
            None,
            None,
        );

        assert_eq!(sql, "SELECT data FROM \"v_user\" ORDER BY data->>'name' ASC");
    }

    #[test]
    fn test_value_to_sql() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::PostgreSQL);

        assert_eq!(generator.value_to_sql(&serde_json::Value::Null), "NULL");
        assert_eq!(generator.value_to_sql(&serde_json::json!(true)), "TRUE");
        assert_eq!(generator.value_to_sql(&serde_json::json!(false)), "FALSE");
        assert_eq!(generator.value_to_sql(&serde_json::json!(42)), "42");
        assert_eq!(generator.value_to_sql(&serde_json::json!(1.5)), "1.5");
        assert_eq!(generator.value_to_sql(&serde_json::json!("hello")), "'hello'");
        assert_eq!(generator.value_to_sql(&serde_json::json!("it's")), "'it''s'"); // SQL escaping
    }

    #[test]
    fn test_mysql_query_template() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::MySQL);

        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        };

        let template = generator.generate_query_template(&query);

        // MySQL uses backticks for identifiers
        assert!(template.template.contains("`v_user`"));
    }

    #[test]
    fn test_sqlserver_query_template() {
        let generator = SqlTemplateGenerator::new(DatabaseTarget::SQLServer);

        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        };

        let template = generator.generate_query_template(&query);

        // SQL Server uses square brackets for identifiers
        assert!(template.template.contains("[v_user]"));
    }
}
