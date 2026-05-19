//! REST resource validation: CQRS checks, field type audits, and conflict detection.

use std::collections::HashMap;

use fraiseql_core::schema::{
    FieldType, MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
};

use super::{Diagnostic, DiagnosticLevel, HttpMethod, RestResource, RouteSource};

/// Check if a query should be skipped (aggregate, window, or scalar return).
pub(super) fn should_skip_query(q: &QueryDefinition) -> bool {
    q.name.ends_with("_aggregate") || q.name.ends_with("_window")
}

/// Check if an operation name is filtered out by include/exclude lists.
pub(super) fn is_filtered_out(name: &str, config: &RestConfig) -> bool {
    if !config.include.is_empty() && !config.include.iter().any(|i| i == name) {
        return true;
    }
    config.exclude.iter().any(|e| e == name)
}

/// Validate CQRS naming: queries should read from `v_*` or `tv_*`.
pub(super) fn validate_cqrs_query(
    sql_source: &str,
    query_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if sql_source.starts_with("tb_") {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            message: format!(
                "Query '{query_name}' reads from write table '{sql_source}' \
                 — expected `v_` or `tv_` prefix. This may indicate a CQRS violation."
            ),
        });
    }
}

/// Validate CQRS naming: mutations should write to `tb_*`.
pub(super) fn validate_cqrs_mutation(
    op: &MutationOperation,
    mutation_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let table = match op {
        MutationOperation::Insert { table }
        | MutationOperation::Update { table }
        | MutationOperation::Delete { table } => table.as_str(),
        // Reason: MutationOperation is #[non_exhaustive]; Custom and unknown variants are skipped.
        _ => return,
    };

    if table.starts_with("v_") || table.starts_with("tv_") {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            message: format!(
                "Mutation '{mutation_name}' writes to view '{table}' — expected `tb_` prefix"
            ),
        });
    }
}

/// Validate pk_*/fk_*/id field types.
pub(super) fn validate_field_types(type_def: &TypeDefinition, diagnostics: &mut Vec<Diagnostic>) {
    for field in &type_def.fields {
        let name: &str = field.name.as_str();
        if name.starts_with("pk_") || name.starts_with("fk_") {
            if !matches!(field.field_type, FieldType::Int | FieldType::Id) {
                diagnostics.push(Diagnostic {
                    level: DiagnosticLevel::Warning,
                    message: format!(
                        "pk_/fk_ field '{name}' is {:?}, expected Int or BigInt",
                        field.field_type
                    ),
                });
            }
        } else if name == "id" && matches!(field.field_type, FieldType::Int) {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                message: format!(
                    "id field on '{}' is Int, expected UUID or ID",
                    type_def.name.as_str()
                ),
            });
        }
    }
}

/// Detect conflicting routes (same method+path from different operations).
pub(super) fn detect_conflicts(
    resources: &[RestResource],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    let mut seen: HashMap<(HttpMethod, String), &str> = HashMap::new();

    for resource in resources {
        for route in &resource.routes {
            let key = (route.method, route.path.clone());
            if let Some(prev_op) = seen.get(&key) {
                let current_op = match &route.source {
                    RouteSource::Query { name } | RouteSource::Mutation { name } => name.as_str(),
                };
                let err = format!(
                    "Route conflict: {} {} is claimed by both '{}' and '{}'. \
                     Use `rest_path` override to resolve.",
                    route.method, route.path, prev_op, current_op
                );
                diagnostics.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    message: err.clone(),
                });
                return Err(err);
            }
            let op_name = match &route.source {
                RouteSource::Query { name } | RouteSource::Mutation { name } => name.as_str(),
            };
            seen.insert(key, op_name);
        }
    }

    Ok(())
}
