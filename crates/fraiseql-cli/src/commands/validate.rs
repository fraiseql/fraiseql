//! Schema validation command
//!
//! Validates schema.json with comprehensive checks including:
//! - JSON structure validation
//! - Type reference validation
//! - Circular dependency detection
//! - Unused type detection

use std::fs;

use anyhow::Result;
use fraiseql_core::schema::{CompiledSchema, SchemaDependencyGraph};
use serde::Serialize;

use crate::output::CommandResult;

/// Options for schema validation
#[derive(Debug, Clone, Default)]
pub struct ValidateOptions {
    /// Check for circular dependencies between types
    pub check_cycles: bool,

    /// Check for unused types (types with no incoming references)
    pub check_unused: bool,

    /// Strict mode: treat warnings as errors
    pub strict: bool,

    /// Filter to specific types (empty = all types)
    pub filter_types: Vec<String>,
}

/// Detailed validation result
#[derive(Debug, Serialize)]
pub struct ValidationResult {
    /// Schema file path
    pub schema_path: String,

    /// Whether validation passed
    pub valid: bool,

    /// Number of types in schema
    pub type_count: usize,

    /// Number of queries
    pub query_count: usize,

    /// Number of mutations
    pub mutation_count: usize,

    /// Circular dependencies found (errors)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<CycleError>,

    /// Unused types found (warnings or errors in strict mode)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unused_types: Vec<String>,

    /// Type-specific analysis (when --types filter is used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_analysis: Option<Vec<TypeAnalysis>>,
}

/// Information about a circular dependency
#[derive(Debug, Serialize)]
pub struct CycleError {
    /// Types involved in the cycle
    pub types: Vec<String>,
    /// Human-readable path
    pub path: String,
}

/// Analysis of a specific type
#[derive(Debug, Serialize)]
pub struct TypeAnalysis {
    /// Type name
    pub name: String,
    /// Types this type depends on
    pub dependencies: Vec<String>,
    /// Types that depend on this type
    pub dependents: Vec<String>,
    /// Transitive dependencies (all types reachable)
    pub transitive_dependencies: Vec<String>,
}

/// Run the validate command (legacy interface)
///
/// This is just a wrapper around compile with check=true
#[allow(dead_code)]
pub async fn run(input: &str) -> Result<()> {
    // Validate is just compile --check (no database validation)
    super::compile::run(
        input,
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        "unused",
        true,
        None,
    )
    .await
}

/// Run validation with options and return structured result
pub fn run_with_options(input: &str, opts: ValidateOptions) -> Result<CommandResult> {
    // Load and parse schema
    let schema_content = fs::read_to_string(input)?;
    let schema: CompiledSchema = serde_json::from_str(&schema_content)?;

    // Build dependency graph
    let graph = SchemaDependencyGraph::build(&schema);

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut cycles: Vec<CycleError> = Vec::new();
    let mut unused_types: Vec<String> = Vec::new();

    // Check for circular dependencies
    if opts.check_cycles {
        let detected_cycles = graph.find_cycles();
        for cycle in detected_cycles {
            let cycle_error = CycleError {
                types: cycle.nodes.clone(),
                path:  cycle.path_string(),
            };
            errors.push(format!("Circular dependency: {}", cycle.path_string()));
            cycles.push(cycle_error);
        }
    }

    // Check for unused types
    if opts.check_unused {
        let detected_unused = graph.find_unused();
        for type_name in detected_unused {
            if opts.strict {
                errors.push(format!("Unused type: '{type_name}' has no incoming references"));
            } else {
                warnings.push(format!("Unused type: '{type_name}' has no incoming references"));
            }
            unused_types.push(type_name);
        }
    }

    // Type-specific analysis
    let type_analysis = if opts.filter_types.is_empty() {
        None
    } else {
        let mut analyses = Vec::new();
        for type_name in &opts.filter_types {
            if graph.has_type(type_name) {
                let deps = graph.dependencies_of(type_name);
                let refs = graph.dependents_of(type_name);
                let transitive = graph.transitive_dependencies(type_name);

                analyses.push(TypeAnalysis {
                    name:                     type_name.clone(),
                    dependencies:             deps,
                    dependents:               refs,
                    transitive_dependencies:  transitive.into_iter().collect(),
                });
            } else {
                warnings.push(format!("Type '{type_name}' not found in schema"));
            }
        }
        Some(analyses)
    };

    // Build result
    let result = ValidationResult {
        schema_path:    input.to_string(),
        valid:          errors.is_empty(),
        type_count:     schema.types.len(),
        query_count:    schema.queries.len(),
        mutation_count: schema.mutations.len(),
        cycles,
        unused_types,
        type_analysis,
    };

    let data = serde_json::to_value(&result)?;

    if !errors.is_empty() {
        Ok(CommandResult {
            status:    "validation-failed".to_string(),
            command:   "validate".to_string(),
            data:      Some(data),
            message:   Some(format!("{} validation error(s) found", errors.len())),
            code:      Some("VALIDATION_FAILED".to_string()),
            errors,
            warnings,
            exit_code: 2,
        })
    } else if !warnings.is_empty() {
        Ok(CommandResult::success_with_warnings("validate", data, warnings))
    } else {
        Ok(CommandResult::success("validate", data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_valid_schema() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "sql_source": "v_user",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "profile", "field_type": {"Object": "Profile"}, "nullable": true}
                    ],
                    "implements": []
                },
                {
                    "name": "Profile",
                    "sql_source": "v_profile",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "bio", "field_type": "String", "nullable": true}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "users",
                    "sql_source": "v_user",
                    "return_type": "[User]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    fn create_schema_with_cycle() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "A",
                    "sql_source": "v_a",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "b", "field_type": {"Object": "B"}}
                    ],
                    "implements": []
                },
                {
                    "name": "B",
                    "sql_source": "v_b",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "a", "field_type": {"Object": "A"}}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "items",
                    "sql_source": "v_a",
                    "return_type": "[A]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    fn create_schema_with_unused() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "sql_source": "v_user",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"}
                    ],
                    "implements": []
                },
                {
                    "name": "OrphanType",
                    "sql_source": "v_orphan",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "data", "field_type": "String"}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "users",
                    "sql_source": "v_user",
                    "return_type": "[User]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    #[test]
    fn test_validate_valid_schema() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_detects_cycles() {
        let schema = create_schema_with_cycle();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("Circular")));
    }

    #[test]
    fn test_validate_cycles_disabled() {
        let schema = create_schema_with_cycle();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: false,
            check_unused: false,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        // Should pass because cycle checking is disabled
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_unused_as_warning() {
        let schema = create_schema_with_unused();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        // Should succeed with warnings
        assert_eq!(result.status, "success");
        assert!(!result.warnings.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("OrphanType")));
    }

    #[test]
    fn test_validate_strict_mode() {
        let schema = create_schema_with_unused();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       true,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        // Should fail in strict mode
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("OrphanType")));
    }

    #[test]
    fn test_validate_type_filter() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec!["User".to_string()],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        assert_eq!(result.status, "success");
        let data = result.data.unwrap();
        let type_analysis = data.get("type_analysis").unwrap().as_array().unwrap();
        assert_eq!(type_analysis.len(), 1);
        assert_eq!(type_analysis[0]["name"], "User");
    }

    #[test]
    fn test_validate_type_filter_not_found() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec!["NonExistent".to_string()],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        // Should succeed with warning about missing type
        assert_eq!(result.status, "success");
        assert!(result.warnings.iter().any(|w| w.contains("NonExistent")));
    }

    #[test]
    fn test_validate_result_structure() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions::default();

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        let data = result.data.unwrap();
        assert!(data.get("schema_path").is_some());
        assert!(data.get("valid").is_some());
        assert!(data.get("type_count").is_some());
        assert!(data.get("query_count").is_some());
        assert!(data.get("mutation_count").is_some());
    }
}
