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
pub(crate) fn extract_base_type(type_str: &str) -> &str {
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

        // Build type registry.
        //
        // `enumerate()` rather than `type_names.len()` for the duplicate path: the count of
        // unique names seen so far is not the offending element's index, and the two diverge
        // exactly when there are duplicates — the one case this diagnostic exists for. With
        // `[A, B, A, A]` the count is stuck at 2, so the old path reported `types[2]` twice
        // and never named element 3 (#724 item 3).
        let mut type_names = HashSet::new();
        for (idx, type_def) in schema.types.iter().enumerate() {
            if type_names.contains(&type_def.name) {
                report.errors.push(ValidationError {
                    message:    format!("Duplicate type name: '{}'", type_def.name),
                    path:       format!("types[{idx}].name"),
                    severity:   ErrorSeverity::Error,
                    suggestion: Some("Type names must be unique".to_string()),
                });
            }
            type_names.insert(type_def.name.clone());
        }

        // Add input types — valid as mutation argument types (fraiseql/fraiseql#190)
        for input_type in &schema.input_types {
            type_names.insert(input_type.name.clone());
        }

        // Add union type names — valid as mutation/query return types
        for union_def in &schema.unions {
            type_names.insert(union_def.name.clone());
        }

        // Add interfaces — valid as field and return types (GraphQL spec §3.7)
        for interface in &schema.interfaces {
            type_names.insert(interface.name.clone());
        }

        // Add enums — valid as field and argument types
        for enum_def in &schema.enums {
            type_names.insert(enum_def.name.clone());
        }

        // Add built-in scalars
        for scalar in crate::schema::BUILTIN_SCALAR_NAMES {
            type_names.insert((*scalar).to_string());
        }

        // Add **declared** custom scalars.
        //
        // This used to register every type name appearing in any object field as an implicit
        // custom scalar, which made field-type typos invisible: `"type": "Strng"` validated
        // cleanly, and the typo then also legalized `Strng` as a query return type, so one
        // mistake propagated instead of being caught (#724 item 2). Only names the author
        // actually declared are scalars now; an undeclared field type is reported below.
        for scalar in schema.custom_scalars.iter().flatten() {
            type_names.insert(scalar.name.clone());
        }

        // Report field types that resolve to nothing declared.
        //
        // A warning rather than an error: the implicit registration it replaces was there to
        // keep custom-scalar authoring frictionless, and there is no way to distinguish a
        // typo from a scalar the author has declared elsewhere in a workflow this validator
        // cannot see. Naming it is what was missing — silence was the defect.
        for (type_idx, type_def) in schema.types.iter().enumerate() {
            for (field_idx, field) in type_def.fields.iter().enumerate() {
                let base = extract_base_type(&field.field_type);
                if base.is_empty() || type_names.contains(base) {
                    continue;
                }
                report.errors.push(ValidationError {
                    message:    format!(
                        "Field '{}.{}' has type '{base}', which is not a declared type, enum, \
                         interface, union, input type, built-in scalar or custom scalar",
                        type_def.name, field.name
                    ),
                    path:       format!("types[{type_idx}].fields[{field_idx}].type"),
                    severity:   ErrorSeverity::Warning,
                    suggestion: Some(format!(
                        "{} If '{base}' is a custom scalar, declare it in `custom_scalars`.",
                        Self::suggest_similar_type(base, &type_names)
                    )),
                });
            }
        }

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
                    suggestion: Some(Self::suggest_similar_type(base_return, &type_names)),
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
                        suggestion: Some(Self::suggest_similar_type(base_arg, &type_names)),
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
                    suggestion: Some(Self::suggest_similar_type(base_return, &type_names)),
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
                        suggestion: Some(Self::suggest_similar_type(base_arg, &type_names)),
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
                        suggestion: Some(Self::suggest_similar_type(&observer.entity, &type_names)),
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

        // Validate sources (#573 scheduled ingress — the dual of observers).
        if let Some(sources) = &schema.sources {
            let mut source_names = HashSet::new();
            let mut cursor_names = HashSet::new();
            for (idx, source) in sources.iter().enumerate() {
                // Names key the durable cursor and the advisory lease — they must be unique.
                if !source_names.insert(source.name.clone()) {
                    report.errors.push(ValidationError {
                        message:    format!("Duplicate source name: '{}'", source.name),
                        path:       format!("sources[{idx}].name"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("Source names must be unique".to_string()),
                    });
                }

                // A shared cursor name would let two sources clobber each other's watermark.
                let cursor = source.cursor_name().to_string();
                if !cursor_names.insert(cursor.clone()) {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Source '{}' reuses cursor '{cursor}' already claimed by another source",
                            source.name
                        ),
                        path:       format!("sources[{idx}].cursor"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(
                            "Give each source a distinct cursor (it defaults to the source name)"
                                .to_string(),
                        ),
                    });
                }

                // The schedule must be a 5-field POSIX cron expression (the runtime parses it
                // fully).
                if source.schedule.split_whitespace().count() != 5 {
                    report.errors.push(ValidationError {
                        message:    format!(
                            "Source '{}' has an invalid cron schedule '{}': expected 5 \
                             whitespace-separated fields",
                            source.name, source.schedule
                        ),
                        path:       format!("sources[{idx}].schedule"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some(
                            "Use a 5-field cron expression, e.g. '*/5 * * * *'".to_string(),
                        ),
                    });
                }

                // The bound handler must be named.
                if source.function.trim().is_empty() {
                    report.errors.push(ValidationError {
                        message:    format!("Source '{}' has no function", source.name),
                        path:       format!("sources[{idx}].function"),
                        severity:   ErrorSeverity::Error,
                        suggestion: Some("Set the handler function name".to_string()),
                    });
                }

                // The `run_as` authority ceiling (#573 D6): a blank grant is a config
                // error, and an empty ceiling is fail-closed (the source can write
                // nothing) — valid, but almost always a mistake, so warn.
                if let Some(run_as) = &source.run_as {
                    let has_blank = run_as
                        .roles
                        .iter()
                        .chain(run_as.scopes.iter())
                        .any(|grant| grant.trim().is_empty())
                        || run_as.tenant.as_ref().is_some_and(|t| t.trim().is_empty());
                    if has_blank {
                        report.errors.push(ValidationError {
                            message:    format!(
                                "Source '{}' has a blank role, scope, or tenant in run_as",
                                source.name
                            ),
                            path:       format!("sources[{idx}].run_as"),
                            severity:   ErrorSeverity::Error,
                            suggestion: Some(
                                "Remove empty entries; each role/scope/tenant must be non-blank"
                                    .to_string(),
                            ),
                        });
                    }
                    if run_as.roles.is_empty() && run_as.scopes.is_empty() {
                        report.errors.push(ValidationError {
                            message:    format!(
                                "Source '{}' declares run_as with no authority (no roles or \
                                 scopes): its mutations will be denied (fail-closed)",
                                source.name
                            ),
                            path:       format!("sources[{idx}].run_as"),
                            severity:   ErrorSeverity::Warning,
                            suggestion: Some(
                                "Grant the source the least-privilege roles/scopes its mutations \
                                 need, or drop run_as if it never mutates"
                                    .to_string(),
                            ),
                        });
                    }
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

    /// Suggest similar type names for a typo.
    ///
    /// Delegates ranking to [`fraiseql_core::runtime::suggest_similar`], a real edit-distance
    /// match over `char`s. What this replaces did two things wrong at once:
    ///
    /// * It sliced `&typo[0..1]` and `&name[0..1]` — **byte** ranges — so an empty base type
    ///   (`"return_type": ""`) or any name beginning with a multi-byte character *panicked the CLI
    ///   while it was composing an error message*. The panic was documented in a `# Panics` section
    ///   that no caller honoured (#724 item 1).
    /// * It matched on first letter only, behind a comment calling itself "Levenshtein-style". For
    ///   a typo of `User` it offered `Universe` and `Umbrella`.
    ///
    /// Deterministic output: `available` is a `HashSet`, so candidates are sorted before
    /// ranking and the fallback list is sorted too. Without that, the same schema produced
    /// different error text run to run.
    fn suggest_similar_type(typo: &str, available: &HashSet<String>) -> String {
        let mut candidates: Vec<&str> = available.iter().map(String::as_str).collect();
        candidates.sort_unstable();

        let similar = fraiseql_core::runtime::suggest_similar(typo, &candidates);
        if similar.is_empty() {
            format!("Available types: {}", candidates.join(", "))
        } else {
            format!("Did you mean: {}?", similar.join(", "))
        }
    }
}
