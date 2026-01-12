//! Schema Optimizer
//!
//! Analyzes compiled schemas and adds SQL generation hints for runtime optimization.
//! This runs during compilation to precompute optimization strategies.

use anyhow::Result;
use fraiseql_core::schema::{CompiledSchema, QueryDefinition};
use tracing::{debug, info};

/// Schema optimizer that analyzes queries and adds SQL hints
pub struct SchemaOptimizer;

impl SchemaOptimizer {
    /// Optimize a compiled schema
    ///
    /// This analyzes queries and adds optimization hints like:
    /// - Index suggestions
    /// - Join order hints
    /// - Projection optimization
    /// - Predicate pushdown opportunities
    pub fn optimize(schema: &mut CompiledSchema) -> Result<OptimizationReport> {
        info!("Optimizing compiled schema");

        let mut report = OptimizationReport::default();

        // Analyze queries for optimization opportunities
        for query in &schema.queries {
            Self::analyze_query(query, &mut report);
        }

        // Analyze types for field access patterns
        Self::analyze_types(schema, &mut report);

        info!(
            "Schema optimization complete: {} hints generated",
            report.total_hints()
        );

        Ok(report)
    }

    /// Analyze a single query for optimization opportunities
    fn analyze_query(query: &QueryDefinition, report: &mut OptimizationReport) {
        debug!("Analyzing query: {}", query.name);

        // Check if query would benefit from indexes
        if query.returns_list && !query.arguments.is_empty() {
            report.index_hints.push(IndexHint {
                query_name: query.name.clone(),
                reason: "List query with arguments benefits from index".to_string(),
                suggested_columns: query
                    .arguments
                    .iter()
                    .map(|arg| arg.name.clone())
                    .collect(),
            });
        }

        // Check for auto-params that suggest filtering
        if query.auto_params.has_where {
            report.optimization_notes.push(format!(
                "Query '{}' supports WHERE filtering - ensure denormalized filter columns exist",
                query.name
            ));
        }

        // Check for pagination
        if query.auto_params.has_limit || query.auto_params.has_offset {
            report.optimization_notes.push(format!(
                "Query '{}' supports pagination - consider adding ORDER BY for deterministic results",
                query.name
            ));
        }
    }

    /// Analyze types for field access patterns
    fn analyze_types(schema: &CompiledSchema, report: &mut OptimizationReport) {
        for type_def in &schema.types {
            // Check for large number of fields (potential over-fetching)
            if type_def.fields.len() > 20 {
                report.optimization_notes.push(format!(
                    "Type '{}' has {} fields - consider field selection optimization",
                    type_def.name,
                    type_def.fields.len()
                ));
            }

            // Check for JSONB columns
            if !type_def.jsonb_column.is_empty() {
                report.optimization_notes.push(format!(
                    "Type '{}' uses JSONB column '{}' - ensure GIN index exists for performance",
                    type_def.name, type_def.jsonb_column
                ));
            }
        }
    }
}

/// Optimization report generated during compilation
#[derive(Debug, Default)]
pub struct OptimizationReport {
    /// Index suggestions for query performance
    pub index_hints: Vec<IndexHint>,
    /// General optimization notes
    pub optimization_notes: Vec<String>,
}

impl OptimizationReport {
    /// Get total number of optimization hints
    pub fn total_hints(&self) -> usize {
        self.index_hints.len() + self.optimization_notes.len()
    }

    /// Check if there are any optimization suggestions
    pub fn has_suggestions(&self) -> bool {
        !self.index_hints.is_empty() || !self.optimization_notes.is_empty()
    }

    /// Print report to stdout
    pub fn print(&self) {
        if !self.has_suggestions() {
            return;
        }

        println!("\n📊 Optimization Suggestions:");

        if !self.index_hints.is_empty() {
            println!("\n  Indexes:");
            for hint in &self.index_hints {
                println!("  • Query '{}': {}", hint.query_name, hint.reason);
                println!(
                    "    Columns: {}",
                    hint.suggested_columns.join(", ")
                );
            }
        }

        if !self.optimization_notes.is_empty() {
            println!("\n  Notes:");
            for note in &self.optimization_notes {
                println!("  • {note}");
            }
        }

        println!();
    }
}

/// Index hint for query optimization
#[derive(Debug, Clone)]
pub struct IndexHint {
    /// Query name that would benefit from index
    pub query_name: String,
    /// Reason for the suggestion
    pub reason: String,
    /// Suggested columns to index
    pub suggested_columns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fraiseql_core::schema::{
        ArgumentDefinition, AutoParams, FieldDefinition, FieldType, MutationOperation,
        TypeDefinition,
    };

    #[test]
    fn test_optimize_empty_schema() {
        let mut schema = CompiledSchema {
            types: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert_eq!(report.total_hints(), 0);
    }

    #[test]
    fn test_index_hint_for_list_query() {
        let mut schema = CompiledSchema {
            types: vec![],
            queries: vec![QueryDefinition {
                name: "users".to_string(),
                return_type: "User".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![ArgumentDefinition {
                    name: "status".to_string(),
                    arg_type: FieldType::String,
                    nullable: false,
                    default_value: None,
                    description: None,
                }],
                sql_source: Some("users".to_string()),
                description: None,
                auto_params: AutoParams::default(),
            }],
            mutations: vec![],
            subscriptions: vec![],
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report.total_hints() > 0);
        assert!(!report.index_hints.is_empty());
        assert_eq!(report.index_hints[0].query_name, "users");
    }

    #[test]
    fn test_pagination_note() {
        let mut schema = CompiledSchema {
            types: vec![],
            queries: vec![QueryDefinition {
                name: "products".to_string(),
                return_type: "Product".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                sql_source: Some("products".to_string()),
                description: None,
                auto_params: AutoParams {
                    has_where: false,
                    has_order_by: false,
                    has_limit: true,
                    has_offset: true,
                },
            }],
            mutations: vec![],
            subscriptions: vec![],
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report.optimization_notes.iter().any(|note| note.contains("pagination")));
    }

    #[test]
    fn test_large_type_warning() {
        let mut schema = CompiledSchema {
            types: vec![TypeDefinition {
                name: "BigType".to_string(),
                sql_source: String::new(),
                jsonb_column: String::new(),
                fields: (0..25)
                    .map(|i| FieldDefinition {
                        name: format!("field{i}"),
                        field_type: FieldType::String,
                        nullable: false,
                        default_value: None,
                        description: None,
                        vector_config: None,
                    })
                    .collect(),
                description: None,
            }],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report
            .optimization_notes
            .iter()
            .any(|note| note.contains("25 fields")));
    }
}
