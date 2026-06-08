//! Schema validation command
//!
//! Validates schema.json with comprehensive checks including:
//! - JSON structure validation
//! - Type reference validation
//! - Circular dependency detection
//! - Unused type detection

use std::fs;

use anyhow::{Context, Result};
use fraiseql_core::schema::{CompiledSchema, SchemaDependencyGraph};
use serde::Serialize;

use crate::{
    output::CommandResult,
    schema::{mutation_contract::validate_mutation_contract, pg_catalog::PgCatalog},
};

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
    pub path:  String,
}

/// Analysis of a specific type
#[derive(Debug, Serialize)]
pub struct TypeAnalysis {
    /// Type name
    pub name:                    String,
    /// Types this type depends on
    pub dependencies:            Vec<String>,
    /// Types that depend on this type
    pub dependents:              Vec<String>,
    /// Transitive dependencies (all types reachable)
    pub transitive_dependencies: Vec<String>,
}

/// Run validation with options and return structured result
///
/// # Errors
///
/// Returns an error if the schema file cannot be read, cannot be deserialized as
/// a `CompiledSchema`, or if JSON serialization of the result fails.
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
                    name:                    type_name.clone(),
                    dependencies:            deps,
                    dependents:              refs,
                    transitive_dependencies: transitive.into_iter().collect(),
                });
            } else {
                warnings.push(format!("Type '{type_name}' not found in schema"));
            }
        }
        Some(analyses)
    };

    // Build result
    let result = ValidationResult {
        schema_path: input.to_string(),
        valid: errors.is_empty(),
        type_count: schema.types.len(),
        query_count: schema.queries.len(),
        mutation_count: schema.mutations.len(),
        cycles,
        unused_types,
        type_analysis,
    };

    let data = serde_json::to_value(&result)?;

    if !errors.is_empty() {
        Ok(CommandResult {
            status: "validation-failed".to_string(),
            command: "validate".to_string(),
            data: Some(data),
            message: Some(format!("{} validation error(s) found", errors.len())),
            code: Some("VALIDATION_FAILED".to_string()),
            errors,
            warnings,
        })
    } else if !warnings.is_empty() {
        Ok(CommandResult::success_with_warnings("validate", data, warnings))
    } else {
        Ok(CommandResult::success("validate", data))
    }
}

/// Validate the mutation contract against a live PostgreSQL database (#397).
///
/// Loads the compiled schema at `input`, checks every DB-backed mutation's call
/// binding and response shape against the database, prints a report, and fails
/// when any error-severity violation is found.
///
/// # Errors
///
/// Returns an error if the schema cannot be read or parsed, the database cannot
/// be reached, or any mutation violates the contract.
pub async fn run_against_db(input: &str, db_url: &str, json: bool) -> Result<()> {
    let schema_content = fs::read_to_string(input)
        .with_context(|| format!("failed to read compiled schema `{input}`"))?;
    let schema: CompiledSchema = serde_json::from_str(&schema_content).with_context(|| {
        format!("`{input}` is not a valid compiled schema (run `fraiseql compile` first)")
    })?;

    let catalog = PgCatalog::connect(db_url)?;
    let report = validate_mutation_contract(&schema, &catalog).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report.to_json())?);
    } else {
        report.print_text();
    }

    if report.error_count() > 0 {
        anyhow::bail!("mutation contract validation failed: {} error(s)", report.error_count());
    }
    Ok(())
}
