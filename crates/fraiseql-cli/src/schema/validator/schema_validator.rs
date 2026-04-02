//! `SchemaValidator` — validates an `IntermediateSchema` with detailed error reporting.

use std::collections::HashSet;

use anyhow::Result;
use tracing::{debug, info};

use super::{
    sql_identifier::validate_sql_identifier,
    types::{ErrorSeverity, ValidationError, ValidationReport},
};
use crate::schema::intermediate::IntermediateSchema;

/// Strip GraphQL type modifiers (`!`, `[]`) to extract the base type name.
///
/// Examples: `"UUID!"` → `"UUID"`, `"[User!]!"` → `"User"`, `"String"` → `"String"`
fn extract_base_type(type_str: &str) -> &str {
    let s = type_str.trim();
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let s = s.trim_end_matches('!').trim_start_matches('!');
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let s = s.trim_end_matches('!');
    s.trim()
}

/// Enhanced schema validator
pub struct SchemaValidator;

impl SchemaValidator {
    /// Validate an intermediate schema with detailed error reporting
    ///
    /// # Errors
    ///
    /// Currently infallible; always returns `Ok` containing the report.
    /// The `Result` return type is reserved for future validation that may
    /// require fallible I/O.
    #[allow(clippy::cognitive_complexity)] // Reason: comprehensive schema validation with many cross-field constraint checks
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

            // Validate return type exists (strip ! and [] modifiers)
            let base_return = extract_base_type(&query.return_type);
            if !type_names.contains(base_return) {
                report.errors.push(ValidationError {
                    message:    format!(
                        "Query '{}' references unknown type '{}'",
                        query.name, base_return
                    ),
                    path:       format!("queries[{idx}].return_type"),
                    severity:   ErrorSeverity::Error,
                    suggestion: Some(format!(
                        "Available types: {}",
                        Self::suggest_similar_type(base_return, &type_names)
                    )),
                });
            }

            // Validate argument types
            for (arg_idx, arg) in query.arguments.iter().enumerate() {
                let base_arg = extract_base_type(&arg.arg_type);
                if !type_names.contains(base_arg) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Query '{}' argument '{}' references unknown type '{}'",
                            query.name, arg.name, base_arg
                        ),
                        path:       format!("queries[{idx}].arguments[{arg_idx}].type"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(base_arg, &type_names)
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

            // Validate return type exists (strip ! and [] modifiers)
            let base_return = extract_base_type(&mutation.return_type);
            if !type_names.contains(base_return) {
                report.errors.push(ValidationError {
                    message:    format!(
                        "Mutation '{}' references unknown type '{}'",
                        mutation.name, base_return
                    ),
                    path:       format!("mutations[{idx}].return_type"),
                    severity:   ErrorSeverity::Error,
                    suggestion: Some(format!(
                        "Available types: {}",
                        Self::suggest_similar_type(base_return, &type_names)
                    )),
                });
            }

            // Validate argument types
            for (arg_idx, arg) in mutation.arguments.iter().enumerate() {
                let base_arg = extract_base_type(&arg.arg_type);
                if !type_names.contains(base_arg) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Mutation '{}' argument '{}' references unknown type '{}'",
                            mutation.name, arg.name, base_arg
                        ),
                        path:       format!("mutations[{idx}].arguments[{arg_idx}].type"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(format!(
                            "Available types: {}",
                            Self::suggest_similar_type(base_arg, &type_names)
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
                let inject_names: Vec<&str> = mutation.inject.keys().map(String::as_str).collect();
                let fn_name = mutation.sql_source.as_deref().unwrap_or("<unknown>");
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

    /// Suggest similar type names for typos.
    ///
    /// # Panics
    ///
    /// Panics if `typo` is empty (cannot slice first character).
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

#[cfg(test)]
mod tests {
    #![allow(missing_docs)]
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::schema::intermediate::{
        IntermediateSchema,
        operations::{IntermediateArgument, IntermediateMutation, IntermediateQuery},
        types::{IntermediateField, IntermediateType},
    };

    fn field(name: &str, ty: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     ty.to_string(),
            nullable:       false,
            description:    None,
            directives:     None,
            requires_scope: None,
            on_deny:        None,
        }
    }

    fn arg(name: &str, ty: &str) -> IntermediateArgument {
        IntermediateArgument {
            name:       name.to_string(),
            arg_type:   ty.to_string(),
            nullable:   false,
            default:    None,
            deprecated: None,
        }
    }

    fn minimal_schema() -> IntermediateSchema {
        let mut schema = IntermediateSchema::default();
        schema.types.push(IntermediateType {
            name: "Item".to_string(),
            fields: vec![field("id", "UUID")],
            ..Default::default()
        });
        schema
    }

    // ── extract_base_type unit tests ────────────────────────────────

    #[test]
    fn extract_base_type_strips_non_null_suffix() {
        assert_eq!(extract_base_type("Item!"), "Item");
        assert_eq!(extract_base_type("String!"), "String");
        assert_eq!(extract_base_type("Json!"), "Json");
    }

    #[test]
    fn extract_base_type_strips_list_brackets() {
        assert_eq!(extract_base_type("[User]"), "User");
        assert_eq!(extract_base_type("[User!]!"), "User");
        assert_eq!(extract_base_type("[String!]"), "String");
    }

    #[test]
    fn extract_base_type_passthrough() {
        assert_eq!(extract_base_type("String"), "String");
        assert_eq!(extract_base_type("Item"), "Item");
    }

    // ── Issue #151: ! suffix accepted in queries ────────────────────

    #[test]
    fn query_with_bang_suffixed_return_type_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "Item!".to_string(),
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "Item! should resolve to Item: {errors:?}");
    }

    #[test]
    fn query_arg_with_bang_suffix_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "Item".to_string(),
            arguments: vec![arg("id", "String!")],
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "String! should resolve to String: {errors:?}");
    }

    #[test]
    fn mutation_with_bang_suffixed_types_is_valid() {
        let mut schema = minimal_schema();
        schema.mutations.push(IntermediateMutation {
            name: "createItem".to_string(),
            return_type: "Item!".to_string(),
            arguments: vec![arg("name", "String!")],
            sql_source: Some("fn_create_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "Item! and String! should be valid: {errors:?}");
    }

    #[test]
    fn list_type_with_bang_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "items".to_string(),
            return_type: "[Item!]!".to_string(),
            returns_list: true,
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "[Item!]! should resolve to Item: {errors:?}");
    }

    // ── Truly unknown types are still rejected ──────────────────────

    #[test]
    fn truly_unknown_type_still_rejected() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "NonExistent!".to_string(),
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(!errors.is_empty(), "NonExistent should still be rejected");
        assert!(
            errors[0].message.contains("NonExistent"),
            "error should name the base type, not 'NonExistent!': {}",
            errors[0].message
        );
        // Error message should show the base type, not the raw "NonExistent!"
        assert!(
            !errors[0].message.contains("NonExistent!"),
            "error should strip ! from type name: {}",
            errors[0].message
        );
    }
}
