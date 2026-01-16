//! Enhanced Schema Validation
//!
//! Provides detailed validation error reporting with line numbers and context.

use super::intermediate::IntermediateSchema;
use anyhow::Result;
use std::collections::HashSet;
use tracing::{debug, info};

/// Detailed validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message
    pub message: String,
    /// JSON path to the error (e.g., "queries[0].`return_type`")
    pub path: String,
    /// Severity level
    pub severity: ErrorSeverity,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Critical error - schema is invalid
    Error,
    /// Warning - schema is valid but may have issues
    Warning,
}

/// Enhanced schema validator
pub struct SchemaValidator;

impl SchemaValidator {
    /// Validate an intermediate schema with detailed error reporting
    pub fn validate(schema: &IntermediateSchema) -> Result<ValidationReport> {
        info!("Validating schema structure");

        let mut report = ValidationReport::default();

        // Build type registry
        let mut type_names = HashSet::new();
        for type_def in &schema.types {
            if type_names.contains(&type_def.name) {
                report.errors.push(ValidationError {
                    message: format!("Duplicate type name: '{}'", type_def.name),
                    path: format!("types[{}].name", type_names.len()),
                    severity: ErrorSeverity::Error,
                    suggestion: Some("Type names must be unique".to_string()),
                });
            }
            type_names.insert(type_def.name.clone());
        }

        // Add built-in scalars
        type_names.insert("Int".to_string());
        type_names.insert("Float".to_string());
        type_names.insert("String".to_string());
        type_names.insert("Boolean".to_string());
        type_names.insert("ID".to_string());

        // Validate queries
        let mut query_names = HashSet::new();
        for (idx, query) in schema.queries.iter().enumerate() {
            debug!("Validating query: {}", query.name);

            // Check for duplicate query names
            if query_names.contains(&query.name) {
                report.errors.push(ValidationError {
                    message: format!("Duplicate query name: '{}'", query.name),
                    path: format!("queries[{idx}].name"),
                    severity: ErrorSeverity::Error,
                    suggestion: Some("Query names must be unique".to_string()),
                });
            }
            query_names.insert(query.name.clone());

            // Validate return type exists
            if !type_names.contains(&query.return_type) {
                report.errors.push(ValidationError {
                    message: format!(
                        "Query '{}' references unknown type '{}'",
                        query.name, query.return_type
                    ),
                    path: format!("queries[{idx}].return_type"),
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!(
                        "Available types: {}",
                        Self::suggest_similar_type(&query.return_type, &type_names)
                    )),
                });
            }

            // Validate argument types
            for (arg_idx, arg) in query.arguments.iter().enumerate() {
                if !type_names.contains(&arg.arg_type) {
                    report.errors.push(ValidationError {
                        message: format!(
                            "Query '{}' argument '{}' references unknown type '{}'",
                            query.name, arg.name, arg.arg_type
                        ),
                        path: format!("queries[{idx}].arguments[{arg_idx}].type"),
                        severity: ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(&arg.arg_type, &type_names)
                        )),
                    });
                }
            }

            // Warning for queries without SQL source
            if query.sql_source.is_none() && query.returns_list {
                report.errors.push(ValidationError {
                    message: format!(
                        "Query '{}' returns a list but has no sql_source",
                        query.name
                    ),
                    path: format!("queries[{idx}]"),
                    severity: ErrorSeverity::Warning,
                    suggestion: Some("Add sql_source for SQL-backed queries".to_string()),
                });
            }
        }

        // Validate mutations
        let mut mutation_names = HashSet::new();
        for (idx, mutation) in schema.mutations.iter().enumerate() {
            debug!("Validating mutation: {}", mutation.name);

            // Check for duplicate mutation names
            if mutation_names.contains(&mutation.name) {
                report.errors.push(ValidationError {
                    message: format!("Duplicate mutation name: '{}'", mutation.name),
                    path: format!("mutations[{idx}].name"),
                    severity: ErrorSeverity::Error,
                    suggestion: Some("Mutation names must be unique".to_string()),
                });
            }
            mutation_names.insert(mutation.name.clone());

            // Validate return type exists
            if !type_names.contains(&mutation.return_type) {
                report.errors.push(ValidationError {
                    message: format!(
                        "Mutation '{}' references unknown type '{}'",
                        mutation.name, mutation.return_type
                    ),
                    path: format!("mutations[{idx}].return_type"),
                    severity: ErrorSeverity::Error,
                    suggestion: Some(format!(
                        "Available types: {}",
                        Self::suggest_similar_type(&mutation.return_type, &type_names)
                    )),
                });
            }

            // Validate argument types
            for (arg_idx, arg) in mutation.arguments.iter().enumerate() {
                if !type_names.contains(&arg.arg_type) {
                    report.errors.push(ValidationError {
                        message: format!(
                            "Mutation '{}' argument '{}' references unknown type '{}'",
                            mutation.name, arg.name, arg.arg_type
                        ),
                        path: format!("mutations[{idx}].arguments[{arg_idx}].type"),
                        severity: ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(&arg.arg_type, &type_names)
                        )),
                    });
                }
            }
        }

        info!(
            "Validation complete: {} errors, {} warnings",
            report.error_count(),
            report.warning_count()
        );

        Ok(report)
    }

    /// Suggest similar type names for typos
    fn suggest_similar_type(typo: &str, available: &HashSet<String>) -> String {
        // Simple Levenshtein-style similarity (first letter match)
        let similar: Vec<&String> = available
            .iter()
            .filter(|name| {
                name.to_lowercase().starts_with(&typo[0..1].to_lowercase())
                    || typo.to_lowercase().starts_with(&name[0..1].to_lowercase())
            })
            .take(3)
            .collect();

        if similar.is_empty() {
            available.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        } else {
            similar.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        }
    }
}

/// Validation report
#[derive(Debug, Default)]
pub struct ValidationReport {
    /// Validation errors and warnings
    pub errors: Vec<ValidationError>,
}

impl ValidationReport {
    /// Check if validation passed (no errors, warnings OK)
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.errors
            .iter()
            .any(|e| e.severity == ErrorSeverity::Error)
    }

    /// Count errors
    pub fn error_count(&self) -> usize {
        self.errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Error)
            .count()
    }

    /// Count warnings
    pub fn warning_count(&self) -> usize {
        self.errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Warning)
            .count()
    }

    /// Print formatted report
    pub fn print(&self) {
        if self.errors.is_empty() {
            return;
        }

        println!("\n📋 Validation Report:");

        let errors: Vec<_> = self
            .errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Error)
            .collect();

        let warnings: Vec<_> = self
            .errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Warning)
            .collect();

        if !errors.is_empty() {
            println!("\n  ❌ Errors ({}):", errors.len());
            for error in errors {
                println!("     {}", error.message);
                println!("     at: {}", error.path);
                if let Some(suggestion) = &error.suggestion {
                    println!("     💡 {suggestion}");
                }
                println!();
            }
        }

        if !warnings.is_empty() {
            println!("\n  ⚠️  Warnings ({}):", warnings.len());
            for warning in warnings {
                println!("     {}", warning.message);
                println!("     at: {}", warning.path);
                if let Some(suggestion) = &warning.suggestion {
                    println!("     💡 {suggestion}");
                }
                println!();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::intermediate::{
        IntermediateQuery,
        IntermediateType,
    };

    #[test]
    fn test_validate_empty_schema() {
        let schema = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            queries: vec![],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(report.is_valid());
    }

    #[test]
    fn test_detect_unknown_return_type() {
        let schema = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![],
            queries: vec![IntermediateQuery {
                name: "users".to_string(),
                return_type: "UnknownType".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                description: None,
                sql_source: Some("users".to_string()),
                auto_params: None,
            }],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert_eq!(report.error_count(), 1);
        assert!(report.errors[0].message.contains("unknown type 'UnknownType'"));
    }

    #[test]
    fn test_detect_duplicate_query_names() {
        let schema = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![],
                description: None,
            }],
            queries: vec![
                IntermediateQuery {
                    name: "users".to_string(),
                    return_type: "User".to_string(),
                    returns_list: true,
                    nullable: false,
                    arguments: vec![],
                    description: None,
                    sql_source: Some("users".to_string()),
                    auto_params: None,
                },
                IntermediateQuery {
                    name: "users".to_string(), // Duplicate!
                    return_type: "User".to_string(),
                    returns_list: true,
                    nullable: false,
                    arguments: vec![],
                    description: None,
                    sql_source: Some("users".to_string()),
                    auto_params: None,
                },
            ],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("Duplicate query name")));
    }

    #[test]
    fn test_warning_for_query_without_sql_source() {
        let schema = IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name: "User".to_string(),
                fields: vec![],
                description: None,
            }],
            queries: vec![IntermediateQuery {
                name: "users".to_string(),
                return_type: "User".to_string(),
                returns_list: true,
                nullable: false,
                arguments: vec![],
                description: None,
                sql_source: None, // Missing SQL source
                auto_params: None,
            }],
            mutations: vec![],
            fact_tables: None,
            aggregate_queries: None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(report.is_valid()); // Still valid, just a warning
        assert_eq!(report.warning_count(), 1);
        assert!(report.errors[0].message.contains("no sql_source"));
    }
}
