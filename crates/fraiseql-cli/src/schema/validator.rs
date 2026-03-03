//! Enhanced Schema Validation
//!
//! Provides detailed validation error reporting with line numbers and context.

use std::collections::HashSet;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use tracing::{debug, info};

use super::intermediate::IntermediateSchema;

/// Pattern for safe SQL identifiers: `schema.name` or just `name`.
/// Each part must start with a letter or underscore, followed by alphanumerics/underscores.
static SAFE_IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)?$")
        .expect("static regex is valid")
});

/// Validates that `value` is a safe SQL identifier.
///
/// Accepts `[A-Za-z_][A-Za-z0-9_]*` with an optional single schema dot
/// (e.g. `"v_user"` or `"public.v_user"`). Rejects anything that could be
/// SQL injection or cause a runtime syntax error.
///
/// # Arguments
/// - `value`: The string to validate (e.g. `"v_user"` or `"public.v_user"`)
/// - `field`: The TOML/decorator field name (`"sql_source"`, `"function_name"`)
/// - `path`: Human-readable location for the error (`"Query.users"`, `"Mutation.createPost"`)
///
/// # Errors
///
/// Returns a `ValidationError` if `value` is empty or does not match the safe
/// identifier pattern.
pub fn validate_sql_identifier(
    value: &str,
    field: &str,
    path: &str,
) -> std::result::Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError {
            message:    format!(
                "`{field}` at `{path}` must not be empty. \
                 Provide a view or function name such as \"v_user\" or \"public.v_user\"."
            ),
            path:       path.to_string(),
            severity:   ErrorSeverity::Error,
            suggestion: None,
        });
    }
    if !SAFE_IDENTIFIER.is_match(value) {
        return Err(ValidationError {
            message:    format!(
                "`{field}` value {value:?} at `{path}` is not a valid SQL identifier. \
                 Only ASCII letters, digits, underscores, and an optional schema dot are \
                 allowed. Valid examples: \"v_user\", \"public.v_user\", \"fn_create_post\"."
            ),
            path:       path.to_string(),
            severity:   ErrorSeverity::Error,
            suggestion: Some(
                "Remove semicolons, quotes, dashes, spaces, or any SQL syntax \
                 from the identifier value."
                    .to_string(),
            ),
        });
    }
    Ok(())
}

/// Detailed validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message
    pub message:    String,
    /// JSON path to the error (e.g., `"queries[0].return_type"`)
    pub path:       String,
    /// Severity level
    pub severity:   ErrorSeverity,
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
                    message:    format!("Duplicate type name: '{}'", type_def.name),
                    path:       format!("types[{}].name", type_names.len()),
                    severity:   ErrorSeverity::Error,
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
                    message:    format!("Duplicate query name: '{}'", query.name),
                    path:       format!("queries[{idx}].name"),
                    severity:   ErrorSeverity::Error,
                    suggestion: Some("Query names must be unique".to_string()),
                });
            }
            query_names.insert(query.name.clone());

            // Validate return type exists
            if !type_names.contains(&query.return_type) {
                report.errors.push(ValidationError {
                    message:    format!(
                        "Query '{}' references unknown type '{}'",
                        query.name, query.return_type
                    ),
                    path:       format!("queries[{idx}].return_type"),
                    severity:   ErrorSeverity::Error,
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
                        message:    format!(
                            "Query '{}' argument '{}' references unknown type '{}'",
                            query.name, arg.name, arg.arg_type
                        ),
                        path:       format!("queries[{idx}].arguments[{arg_idx}].type"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(&arg.arg_type, &type_names)
                        )),
                    });
                }
            }

            // Validate sql_source is a safe SQL identifier
            if let Some(sql_source) = &query.sql_source {
                if let Err(e) = validate_sql_identifier(
                    sql_source,
                    "sql_source",
                    &format!("Query.{}", query.name),
                ) {
                    report.errors.push(e);
                }
            }

            // Warning for queries without SQL source
            if query.sql_source.is_none() && query.returns_list {
                report.errors.push(ValidationError {
                    message:    format!(
                        "Query '{}' returns a list but has no sql_source",
                        query.name
                    ),
                    path:       format!("queries[{idx}]"),
                    severity:   ErrorSeverity::Warning,
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
                    message:    format!("Duplicate mutation name: '{}'", mutation.name),
                    path:       format!("mutations[{idx}].name"),
                    severity:   ErrorSeverity::Error,
                    suggestion: Some("Mutation names must be unique".to_string()),
                });
            }
            mutation_names.insert(mutation.name.clone());

            // Validate return type exists
            if !type_names.contains(&mutation.return_type) {
                report.errors.push(ValidationError {
                    message:    format!(
                        "Mutation '{}' references unknown type '{}'",
                        mutation.name, mutation.return_type
                    ),
                    path:       format!("mutations[{idx}].return_type"),
                    severity:   ErrorSeverity::Error,
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
                        message:    format!(
                            "Mutation '{}' argument '{}' references unknown type '{}'",
                            mutation.name, arg.name, arg.arg_type
                        ),
                        path:       format!("mutations[{idx}].arguments[{arg_idx}].type"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(&arg.arg_type, &type_names)
                        )),
                    });
                }
            }

            // Validate sql_source is a safe SQL identifier
            if let Some(sql_source) = &mutation.sql_source {
                if let Err(e) = validate_sql_identifier(
                    sql_source,
                    "sql_source",
                    &format!("Mutation.{}", mutation.name),
                ) {
                    report.errors.push(e);
                }
            }

            // Warn about inject_params ordering contract
            if !mutation.inject.is_empty() {
                let inject_names: Vec<&str> =
                    mutation.inject.keys().map(String::as_str).collect();
                let fn_name = mutation
                    .sql_source
                    .as_deref()
                    .unwrap_or("<unknown>");
                report.errors.push(ValidationError {
                    message:    format!(
                        "Mutation '{}' has inject params {:?}. \
                         These are appended as the LAST positional arguments to \
                         `{fn_name}`. Your SQL function MUST declare injected \
                         parameters last, after all client-provided arguments.",
                        mutation.name, inject_names,
                    ),
                    path:       format!("Mutation.{}", mutation.name),
                    severity:   ErrorSeverity::Warning,
                    suggestion: None,
                });
            }
        }

        // Validate observers
        if let Some(observers) = &schema.observers {
            let mut observer_names = HashSet::new();
            for (idx, observer) in observers.iter().enumerate() {
                debug!("Validating observer: {}", observer.name);

                // Check for duplicate observer names
                if observer_names.contains(&observer.name) {
                    report.errors.push(ValidationError {
                        message:    format!("Duplicate observer name: '{}'", observer.name),
                        path:       format!("observers[{idx}].name"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("Observer names must be unique".to_string()),
                    });
                }
                observer_names.insert(observer.name.clone());

                // Validate entity type exists
                if !type_names.contains(&observer.entity) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' references unknown entity '{}'",
                            observer.name, observer.entity
                        ),
                        path:       format!("observers[{idx}].entity"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(&observer.entity, &type_names)
                        )),
                    });
                }

                // Validate event type
                let valid_events = ["INSERT", "UPDATE", "DELETE"];
                if !valid_events.contains(&observer.event.as_str()) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' has invalid event '{}'. Must be INSERT, UPDATE, or DELETE",
                            observer.name, observer.event
                        ),
                        path:       format!("observers[{idx}].event"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("Valid events: INSERT, UPDATE, DELETE".to_string()),
                    });
                }

                // Validate at least one action exists
                if observer.actions.is_empty() {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' must have at least one action",
                            observer.name
                        ),
                        path:       format!("observers[{idx}].actions"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("Add a webhook, slack, or email action".to_string()),
                    });
                }

                // Validate each action
                for (action_idx, action) in observer.actions.iter().enumerate() {
                    if let Some(obj) = action.as_object() {
                        // Check action has a type field
                        if let Some(action_type) = obj.get("type").and_then(|v| v.as_str()) {
                            let valid_action_types = ["webhook", "slack", "email"];
                            if !valid_action_types.contains(&action_type) {
                                report.errors.push(ValidationError {
                                    message:    format!(
                                        "Observer '{}' action {} has invalid type '{}'",
                                        observer.name, action_idx, action_type
                                    ),
                                    path:       format!(
                                        "observers[{idx}].actions[{action_idx}].type"
                                    ),
                                    severity:   ErrorSeverity::Error,
                                    suggestion: Some(
                                        "Valid action types: webhook, slack, email".to_string(),
                                    ),
                                });
                            }

                            // Validate action-specific required fields
                            match action_type {
                                "webhook" => {
                                    let has_url = obj.contains_key("url");
                                    let has_url_env = obj.contains_key("url_env");
                                    if !has_url && !has_url_env {
                                        report.errors.push(ValidationError {
                                            message:    format!(
                                                "Observer '{}' webhook action must have 'url' or 'url_env'",
                                                observer.name
                                            ),
                                            path:       format!("observers[{idx}].actions[{action_idx}]"),
                                            severity:   ErrorSeverity::Error,
                                            suggestion: Some("Add 'url' or 'url_env' field".to_string()),
                                        });
                                    }
                                },
                                "slack" => {
                                    if !obj.contains_key("channel") {
                                        report.errors.push(ValidationError {
                                            message:    format!(
                                                "Observer '{}' slack action must have 'channel' field",
                                                observer.name
                                            ),
                                            path:       format!("observers[{idx}].actions[{action_idx}]"),
                                            severity:   ErrorSeverity::Error,
                                            suggestion: Some("Add 'channel' field (e.g., '#sales')".to_string()),
                                        });
                                    }
                                    if !obj.contains_key("message") {
                                        report.errors.push(ValidationError {
                                            message:    format!(
                                                "Observer '{}' slack action must have 'message' field",
                                                observer.name
                                            ),
                                            path:       format!("observers[{idx}].actions[{action_idx}]"),
                                            severity:   ErrorSeverity::Error,
                                            suggestion: Some("Add 'message' field".to_string()),
                                        });
                                    }
                                },
                                "email" => {
                                    let required_fields = ["to", "subject", "body"];
                                    for field in &required_fields {
                                        if !obj.contains_key(*field) {
                                            report.errors.push(ValidationError {
                                                message:    format!(
                                                    "Observer '{}' email action must have '{}' field",
                                                    observer.name, field
                                                ),
                                                path:       format!("observers[{idx}].actions[{action_idx}]"),
                                                severity:   ErrorSeverity::Error,
                                                suggestion: Some(format!("Add '{field}' field")),
                                            });
                                        }
                                    }
                                },
                                _ => {},
                            }
                        } else {
                            report.errors.push(ValidationError {
                                message:    format!(
                                    "Observer '{}' action {} missing 'type' field",
                                    observer.name, action_idx
                                ),
                                path:       format!("observers[{idx}].actions[{action_idx}]"),
                                severity:   ErrorSeverity::Error,
                                suggestion: Some(
                                    "Add 'type' field (webhook, slack, or email)".to_string(),
                                ),
                            });
                        }
                    } else {
                        report.errors.push(ValidationError {
                            message:    format!(
                                "Observer '{}' action {} must be an object",
                                observer.name, action_idx
                            ),
                            path:       format!("observers[{idx}].actions[{action_idx}]"),
                            severity:   ErrorSeverity::Error,
                            suggestion: None,
                        });
                    }
                }

                // Validate retry config
                let valid_backoff_strategies = ["exponential", "linear", "fixed"];
                if !valid_backoff_strategies.contains(&observer.retry.backoff_strategy.as_str()) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' has invalid backoff_strategy '{}'",
                            observer.name, observer.retry.backoff_strategy
                        ),
                        path:       format!("observers[{idx}].retry.backoff_strategy"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(
                            "Valid strategies: exponential, linear, fixed".to_string(),
                        ),
                    });
                }

                if observer.retry.max_attempts == 0 {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' has max_attempts=0, actions will never execute",
                            observer.name
                        ),
                        path:       format!("observers[{idx}].retry.max_attempts"),
                        severity:   ErrorSeverity::Warning,
                        suggestion: Some("Set max_attempts >= 1".to_string()),
                    });
                }

                if observer.retry.initial_delay_ms == 0 {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' has initial_delay_ms=0, retries will be immediate",
                            observer.name
                        ),
                        path:       format!("observers[{idx}].retry.initial_delay_ms"),
                        severity:   ErrorSeverity::Warning,
                        suggestion: Some("Consider setting initial_delay_ms > 0".to_string()),
                    });
                }

                if observer.retry.max_delay_ms < observer.retry.initial_delay_ms {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Observer '{}' has max_delay_ms < initial_delay_ms",
                            observer.name
                        ),
                        path:       format!("observers[{idx}].retry.max_delay_ms"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("max_delay_ms must be >= initial_delay_ms".to_string()),
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
        self.errors.iter().any(|e| e.severity == ErrorSeverity::Error)
    }

    /// Count errors
    pub fn error_count(&self) -> usize {
        self.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).count()
    }

    /// Count warnings
    pub fn warning_count(&self) -> usize {
        self.errors.iter().filter(|e| e.severity == ErrorSeverity::Warning).count()
    }

    /// Print formatted report
    pub fn print(&self) {
        if self.errors.is_empty() {
            return;
        }

        println!("\n📋 Validation Report:");

        let errors: Vec<_> =
            self.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();

        let warnings: Vec<_> =
            self.errors.iter().filter(|e| e.severity == ErrorSeverity::Warning).collect();

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

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::schema::intermediate::{IntermediateQuery, IntermediateType};

    #[test]
    fn test_validate_empty_schema() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(report.is_valid());
    }

    #[test]
    fn test_detect_unknown_return_type() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![IntermediateQuery {
                name:         "users".to_string(),
                return_type:  "UnknownType".to_string(),
                returns_list: true,
                nullable:     false,
                arguments:    vec![],
                description:  None,
                sql_source:   Some("users".to_string()),
                auto_params:  None,
                deprecated:   None,
                jsonb_column: None,
                relay: false,
                 inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                relay_cursor_type: None,
            }],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert_eq!(report.error_count(), 1);
        assert!(report.errors[0].message.contains("unknown type 'UnknownType'"));
    }

    #[test]
    fn test_detect_duplicate_query_names() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "User".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![
                IntermediateQuery {
                    name:         "users".to_string(),
                    return_type:  "User".to_string(),
                    returns_list: true,
                    nullable:     false,
                    arguments:    vec![],
                    description:  None,
                    sql_source:   Some("users".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
                IntermediateQuery {
                    name:         "users".to_string(), // Duplicate!
                    return_type:  "User".to_string(),
                    returns_list: true,
                    nullable:     false,
                    arguments:    vec![],
                    description:  None,
                    sql_source:   Some("users".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
            ],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("Duplicate query name")));
    }

    #[test]
    fn test_warning_for_query_without_sql_source() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "User".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![IntermediateQuery {
                name:         "users".to_string(),
                return_type:  "User".to_string(),
                returns_list: true,
                nullable:     false,
                arguments:    vec![],
                description:  None,
                sql_source:   None, // Missing SQL source
                auto_params:  None,
                deprecated:   None,
                jsonb_column: None,
                relay: false,
                 inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                relay_cursor_type: None,
            }],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(report.is_valid()); // Still valid, just a warning
        assert_eq!(report.warning_count(), 1);
        assert!(report.errors[0].message.contains("no sql_source"));
    }

    #[test]
    fn test_valid_observer() {
        use serde_json::json;

        use super::super::intermediate::{IntermediateObserver, IntermediateRetryConfig};

        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "Order".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         Some(vec![IntermediateObserver {
                name:      "onOrderCreated".to_string(),
                entity:    "Order".to_string(),
                event:     "INSERT".to_string(),
                actions:   vec![json!({
                    "type": "webhook",
                    "url": "https://example.com/orders"
                })],
                condition: None,
                retry:     IntermediateRetryConfig {
                    max_attempts:     3,
                    backoff_strategy: "exponential".to_string(),
                    initial_delay_ms: 100,
                    max_delay_ms:     60000,
                },
            }]),
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(report.is_valid(), "Valid observer should pass validation");
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn test_observer_with_unknown_entity() {
        use serde_json::json;

        use super::super::intermediate::{IntermediateObserver, IntermediateRetryConfig};

        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         Some(vec![IntermediateObserver {
                name:      "onOrderCreated".to_string(),
                entity:    "UnknownEntity".to_string(),
                event:     "INSERT".to_string(),
                actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
                condition: None,
                retry:     IntermediateRetryConfig {
                    max_attempts:     3,
                    backoff_strategy: "exponential".to_string(),
                    initial_delay_ms: 100,
                    max_delay_ms:     60000,
                },
            }]),
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("unknown entity")));
    }

    #[test]
    fn test_observer_with_invalid_event() {
        use serde_json::json;

        use super::super::intermediate::{IntermediateObserver, IntermediateRetryConfig};

        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "Order".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         Some(vec![IntermediateObserver {
                name:      "onOrderCreated".to_string(),
                entity:    "Order".to_string(),
                event:     "INVALID_EVENT".to_string(),
                actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
                condition: None,
                retry:     IntermediateRetryConfig {
                    max_attempts:     3,
                    backoff_strategy: "exponential".to_string(),
                    initial_delay_ms: 100,
                    max_delay_ms:     60000,
                },
            }]),
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("invalid event")));
    }

    #[test]
    fn test_observer_with_invalid_action_type() {
        use serde_json::json;

        use super::super::intermediate::{IntermediateObserver, IntermediateRetryConfig};

        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "Order".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         Some(vec![IntermediateObserver {
                name:      "onOrderCreated".to_string(),
                entity:    "Order".to_string(),
                event:     "INSERT".to_string(),
                actions:   vec![json!({"type": "invalid_action"})],
                condition: None,
                retry:     IntermediateRetryConfig {
                    max_attempts:     3,
                    backoff_strategy: "exponential".to_string(),
                    initial_delay_ms: 100,
                    max_delay_ms:     60000,
                },
            }]),
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("invalid type")));
    }

    #[test]
    fn test_observer_with_invalid_retry_config() {
        use serde_json::json;

        use super::super::intermediate::{IntermediateObserver, IntermediateRetryConfig};

        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "Order".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         Some(vec![IntermediateObserver {
                name:      "onOrderCreated".to_string(),
                entity:    "Order".to_string(),
                event:     "INSERT".to_string(),
                actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
                condition: None,
                retry:     IntermediateRetryConfig {
                    max_attempts:     3,
                    backoff_strategy: "invalid_strategy".to_string(),
                    initial_delay_ms: 100,
                    max_delay_ms:     60000,
                },
            }]),
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("invalid backoff_strategy")));
    }

    #[test]
    fn test_query_injection_in_sql_source_rejected() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "User".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:       false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![IntermediateQuery {
                name:              "users".to_string(),
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("v_user\"; DROP TABLE users; --".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role: None,
                relay_cursor_type: None,
            }],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        assert!(!report.is_valid());
        assert!(report.errors.iter().any(|e| e.message.contains("valid SQL identifier")));
    }

    #[test]
    fn test_query_schema_qualified_sql_source_passes() {
        let schema = IntermediateSchema {
            security:          None,
            version:           "2.0.0".to_string(),
            types:             vec![IntermediateType {
                name:        "User".to_string(),
                fields:      vec![],
                description: None,
                implements:  vec![],
                requires_role: None,
                is_error:    false,
                relay:       false,
            }],
            enums:             vec![],
            input_types:       vec![],
            interfaces:        vec![],
            unions:            vec![],
            queries:           vec![IntermediateQuery {
                name:              "users".to_string(),
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("public.v_user".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role: None,
                relay_cursor_type: None,
            }],
            mutations:         vec![],
            subscriptions:     vec![],
            fragments:         None,
            directives:        None,
            fact_tables:       None,
            aggregate_queries: None,
            observers:         None,
            custom_scalars:    None,
            observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
            federation_config: None,
            debug_config:      None,
            mcp_config:        None,
            query_defaults:    None,
        };

        let report = SchemaValidator::validate(&schema).unwrap();
        // Should only have the usual "no sql_source" warnings for other queries, not errors
        assert!(report.is_valid(), "Schema-qualified sql_source should be valid");
    }
}
