//! Cost analysis design rules (FraiseQL-calibrated)
//!
//! **FraiseQL Philosophy**: Cost rules check if queries will compile to
//! **deterministic, predictable SQL** at build time.
//!
//! FraiseQL compiles queries to deterministic SQL plans at schema time, not query time.
//! This means costs must be calculable without knowing query values:
//!
//! - **Compiled Query Determinism**: Will the SQL execute in constant time or does it degrade?
//! - **JSONB Nesting Cost**: Multiple nested JSONB aggregations = exponential cardinality
//! - **Bounded Pagination**: Compiler needs default limits to pre-compute costs
//! - **Field Multiplier Chains**: lists[].items[].details[] = O(n³) JSONB size
//!
//! Rules detect patterns that force runtime decisions or defy compile-time cost calculation

use super::{CostWarning, DesignAudit, IssueSeverity};
use serde_json::Value;

/// Analyze query cost patterns in the schema
pub fn analyze(schema: &Value, audit: &mut DesignAudit) {
    check_worst_case_complexity(schema, audit);
    check_unbounded_pagination(schema, audit);
    check_field_multipliers(schema, audit);
}

/// Detect worst-case complexity scenarios
fn check_worst_case_complexity(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        // Calculate compound multipliers for nested types
        let mut type_multipliers: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        // First pass: collect direct multipliers
        for type_def in types {
            let type_name = type_def
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(multiplier) = field.get("complexity_multiplier").and_then(|v| v.as_u64()) {
                        let current = type_multipliers.entry(type_name.clone()).or_insert(0);
                        *current = (u64::from(*current) + multiplier).min(10000) as u32;
                    }
                }
            }
        }

        // Second pass: detect nested multipliers (lists within lists)
        for type_def in types {
            let type_name = type_def
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        if field_type.contains("[") && field_type.contains("]") {
                            // Extract inner type
                            let inner_type = field_type
                                .trim_start_matches('[')
                                .trim_end_matches(']')
                                .trim_matches('!')
                                .to_string();

                            // Calculate compound complexity
                            let base_multiplier = field.get("complexity_multiplier").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let inner_multiplier = type_multipliers.get(&inner_type).copied().unwrap_or(0);
                            let compound = u32::try_from((u64::from(base_multiplier) * u64::from(inner_multiplier)).min(10000)).unwrap_or(10000);

                            if compound > 1000 || base_multiplier > 50 {
                                let severity = if compound > 5000 {
                                    IssueSeverity::Critical
                                } else {
                                    IssueSeverity::Warning
                                };

                                let max_complexity = compound.max(base_multiplier);
                                audit.cost_warnings.push(CostWarning {
                                    severity,
                                    message: format!(
                                        "Compiled JSONB cost: {}.{} can reach {} cardinality - May not compile to deterministic SQL",
                                        type_name,
                                        field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                        max_complexity
                                    ),
                                    suggestion: "Reduce JSONB nesting depth or add pagination limits so compiler can guarantee constant-time execution".to_string(),
                                    worst_case_complexity: Some(max_complexity),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Detect unbounded pagination
fn check_unbounded_pagination(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        for type_def in types {
            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    // Check if field is a list type
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        if field_type.contains("[") && field_type.contains("]") {
                            // Check if it has a non-null default limit
                            // The key is that we must have an explicit, non-null default_limit
                            let has_default_limit = field
                                .get("default_limit")
                                .map(|v| !v.is_null())
                                .unwrap_or(false);

                            if !has_default_limit {
                                let field_name = field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");

                                audit.cost_warnings.push(CostWarning {
                                    severity: IssueSeverity::Warning,
                                    message: format!(
                                        "Unbounded pagination on {}: No default limit - Compiler can't pre-compute worst-case JSONB cardinality",
                                        field_name
                                    ),
                                    suggestion: "Add defaultLimit so compiler can guarantee deterministic query cost at compile time".to_string(),
                                    worst_case_complexity: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Detect field multiplier patterns (lists within lists)
fn check_field_multipliers(schema: &Value, audit: &mut DesignAudit) {
    if let Some(types) = schema.get("types").and_then(|v| v.as_array()) {
        let mut type_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        // First pass: identify list fields
        for type_def in types {
            let type_name = type_def
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        if field_type.contains("[") {
                            type_map
                                .entry(type_name.clone())
                                .or_insert_with(Vec::new)
                                .push(field_type.to_string());
                        }
                    }
                }
            }
        }

        // Second pass: detect multipliers (list field pointing to type with list fields)
        for type_def in types {
            let type_name = type_def
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if let Some(fields) = type_def.get("fields").and_then(|v| v.as_array()) {
                for field in fields {
                    if let Some(field_type) = field.get("type").and_then(|v| v.as_str()) {
                        if field_type.contains("[") {
                            // Extract the inner type name
                            let inner_type = field_type
                                .trim_matches('[')
                                .trim_matches(']')
                                .trim_matches('!')
                                .to_string();

                            // Check if inner type has list fields
                            if let Some(nested_lists) = type_map.get(&inner_type) {
                                if !nested_lists.is_empty() {
                                    audit.cost_warnings.push(CostWarning {
                                        severity: IssueSeverity::Warning,
                                        message: format!(
                                            "JSONB multiplier chain: {}.{} lists {} which has {} nested lists - O(n²) cardinality",
                                            type_name,
                                            field.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                            inner_type,
                                            nested_lists.len()
                                        ),
                                        suggestion: "Limit pagination on both levels to keep JSONB cardinality bounded. E.g., paginate inner lists separately.".to_string(),
                                        worst_case_complexity: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}
