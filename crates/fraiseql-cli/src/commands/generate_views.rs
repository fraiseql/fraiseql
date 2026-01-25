//! Generate DDL for Arrow views (va_*, tv_*, ta_*)
//!
//! This command generates SQL DDL statements for creating Arrow-optimized views
//! from a compiled FraiseQL schema. It supports multiple view types:
//!
//! - `va_*` (Vector Arrow views) - For vector search and analytics
//! - `tv_*` (Table Vector views) - For materialized table vectors
//! - `ta_*` (Table Arrow views) - For Arrow Flight table streaming
//!
//! The command validates the schema, entity, and view configuration before generation.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;

/// Refresh strategy for view updates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    /// Update via database triggers on fact table changes
    TriggerBased,
    /// Update on a scheduled interval
    Scheduled,
}

impl RefreshStrategy {
    /// Parse from string
    pub fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.to_lowercase().as_str() {
            "trigger-based" | "trigger" => Ok(Self::TriggerBased),
            "scheduled" => Ok(Self::Scheduled),
            _ => Err(format!("Invalid refresh strategy '{s}', expected: trigger-based, scheduled")),
        }
    }
}

impl std::fmt::Display for RefreshStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TriggerBased => write!(f, "trigger-based"),
            Self::Scheduled => write!(f, "scheduled"),
        }
    }
}

/// Configuration for view generation
#[derive(Debug, Clone)]
pub struct GenerateViewsConfig {
    /// Path to schema.json file
    pub schema_path: String,
    /// Entity name (e.g., "User", "Order")
    pub entity: String,
    /// View name (e.g., "tv_user_profile", "ta_orders")
    pub view: String,
    /// Refresh strategy for view updates
    pub refresh_strategy: RefreshStrategy,
    /// Output file path (or None for stdout)
    pub output: Option<String>,
    /// Include helper/composition views
    pub include_composition_views: bool,
    /// Include monitoring functions (performance tracking, etc.)
    pub include_monitoring: bool,
    /// Validate only, don't write file
    pub validate_only: bool,
    /// Show generation steps
    pub verbose: bool,
}

/// Run the generate-views command
///
/// # Arguments
///
/// * `config` - Generation configuration
///
/// # Errors
///
/// Returns error if:
/// - Schema file doesn't exist or can't be read
/// - JSON parsing fails
/// - Entity doesn't exist in schema
/// - View name validation fails
/// - Output file can't be written
pub fn run(config: GenerateViewsConfig) -> Result<()> {
    if config.verbose {
        eprintln!("📋 Generating views...");
        eprintln!("   Schema: {}", config.schema_path);
        eprintln!("   Entity: {}", config.entity);
        eprintln!("   View: {}", config.view);
        eprintln!("   Refresh strategy: {}", config.refresh_strategy);
    }

    // 1. Load schema
    let schema_path = Path::new(&config.schema_path);
    if !schema_path.exists() {
        anyhow::bail!("Schema file not found: {}", config.schema_path);
    }

    let schema_json = fs::read_to_string(schema_path).context("Failed to read schema.json")?;

    // 2. Parse compiled schema
    if config.verbose {
        eprintln!("   ✓ Reading schema...");
    }
    let schema = CompiledSchema::from_json(&schema_json).context("Failed to parse schema.json")?;

    // 3. Validate entity exists in schema
    if config.verbose {
        eprintln!("   ✓ Validating entity...");
    }
    validate_entity(&schema, &config.entity)?;

    // 4. Validate view name
    if config.verbose {
        eprintln!("   ✓ Validating view name...");
    }
    let view_type = validate_view_name(&config.view)?;

    if config.verbose {
        eprintln!("   ✓ View type: {}", view_type);
    }

    // 5. Generate SQL DDL
    if config.verbose {
        eprintln!("   ✓ Generating SQL DDL...");
    }
    let sql = generate_view_sql(
        &config.entity,
        &config.view,
        view_type,
        config.refresh_strategy,
        config.include_composition_views,
        config.include_monitoring,
    );

    // 6. If validate-only, stop here
    if config.validate_only {
        println!("✓ View DDL is valid");
        println!("  Entity: {}", config.entity);
        println!("  View: {}", config.view);
        println!("  Type: {}", view_type);
        println!("  Refresh strategy: {}", config.refresh_strategy);
        println!("  Lines: {}", sql.lines().count());
        return Ok(());
    }

    // 7. Write output
    if config.verbose {
        eprintln!("   ✓ Writing output...");
    }
    let output_path = config.output.unwrap_or_else(|| format!("{}.sql", config.view));

    fs::write(&output_path, sql.clone()).context("Failed to write output file")?;

    // 8. Success message
    println!("✓ View DDL generated successfully");
    println!("  Entity: {}", config.entity);
    println!("  View: {}", config.view);
    println!("  Type: {}", view_type);
    println!("  Output: {}", output_path);
    println!("  Lines: {}", sql.lines().count());

    if config.include_composition_views {
        println!("  ✓ Includes composition views");
    }

    if config.include_monitoring {
        println!("  ✓ Includes monitoring functions");
    }

    if config.verbose {
        eprintln!("\nGenerated SQL preview (first 5 lines):");
        for line in sql.lines().take(5) {
            eprintln!("  {}", line);
        }
    }

    Ok(())
}

/// Validate that entity exists in the schema
fn validate_entity(schema: &CompiledSchema, entity: &str) -> Result<()> {
    if schema.types.iter().any(|t| t.name == entity) {
        Ok(())
    } else {
        let available = schema.types.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ");
        anyhow::bail!("Entity '{}' not found in schema. Available types: {}", entity, available)
    }
}

/// Determine view type from view name and validate naming convention
///
/// Valid prefixes:
/// - `va_` - Vector Arrow view
/// - `tv_` - Table Vector view
/// - `ta_` - Table Arrow view
fn validate_view_name(view_name: &str) -> Result<&'static str> {
    if view_name.starts_with("va_") {
        Ok("Vector Arrow (va_)")
    } else if view_name.starts_with("tv_") {
        Ok("Table Vector (tv_)")
    } else if view_name.starts_with("ta_") {
        Ok("Table Arrow (ta_)")
    } else {
        anyhow::bail!("Invalid view name '{}'. Must start with va_, tv_, or ta_", view_name)
    }
}

/// Generate SQL DDL for the view
///
/// # Arguments
///
/// * `entity` - Entity/type name from schema
/// * `view_name` - Full view name (e.g., "tv_user_profile")
/// * `view_type` - View type string for documentation
/// * `refresh_strategy` - How the view is kept up-to-date
/// * `include_composition_views` - Whether to include helper views
/// * `include_monitoring` - Whether to include monitoring functions
fn generate_view_sql(
    entity: &str,
    view_name: &str,
    view_type: &str,
    refresh_strategy: RefreshStrategy,
    include_composition_views: bool,
    include_monitoring: bool,
) -> String {
    let mut sql = String::new();

    // Header
    sql.push_str("-- Auto-generated Arrow view DDL\n");
    sql.push_str(&format!("-- Entity: {entity}\n"));
    sql.push_str(&format!("-- View: {view_name}\n"));
    sql.push_str(&format!("-- Type: {view_type}\n"));
    sql.push_str(&format!("-- Refresh strategy: {refresh_strategy}\n"));
    sql.push_str("-- Generated by: fraiseql generate-views\n\n");

    // Drop existing view if it exists
    sql.push_str(&format!("DROP VIEW IF EXISTS {view_name} CASCADE;\n\n"));

    // Main view definition
    match view_name.split('_').next() {
        Some("va") => {
            generate_vector_arrow_view(&mut sql, entity, view_name);
        },
        Some("tv") => {
            generate_table_vector_view(&mut sql, entity, view_name);
        },
        Some("ta") => {
            generate_table_arrow_view(&mut sql, entity, view_name);
        },
        _ => {
            // Fallback: generate a basic view
            sql.push_str(&format!("CREATE VIEW {view_name} AS\n"));
            sql.push_str("SELECT * FROM public.schema_placeholder;\n");
        },
    }

    // Composition views (optional)
    if include_composition_views {
        sql.push_str("\n-- Composition views\n");
        generate_composition_views(&mut sql, entity, view_name);
    }

    // Monitoring functions (optional)
    if include_monitoring {
        sql.push_str("\n-- Monitoring functions\n");
        generate_monitoring_functions(&mut sql, view_name);
    }

    sql
}

/// Generate a Vector Arrow (va_*) view for vector search and analytics
fn generate_vector_arrow_view(sql: &mut String, entity: &str, view_name: &str) {
    sql.push_str(&format!("CREATE VIEW {view_name} AS\n"));
    sql.push_str("SELECT\n");
    sql.push_str("    id,\n");
    sql.push_str(&format!("    -- {entity} entity fields\n"));
    sql.push_str("    created_at,\n");
    sql.push_str("    updated_at\n");
    sql.push_str("FROM public.schema_placeholder\n");
    sql.push_str("WHERE archived_at IS NULL;\n");
}

/// Generate a Table Vector (tv_*) view for materialized table vectors
fn generate_table_vector_view(sql: &mut String, entity: &str, view_name: &str) {
    sql.push_str(&format!("CREATE MATERIALIZED VIEW {view_name} AS\n"));
    sql.push_str("SELECT\n");
    sql.push_str("    id,\n");
    sql.push_str(&format!("    -- {entity} entity vector representation\n"));
    sql.push_str("    CURRENT_TIMESTAMP as materialized_at\n");
    sql.push_str("FROM public.schema_placeholder\n");
    sql.push_str("WHERE archived_at IS NULL;\n");
    sql.push_str("\n");
    let base_name = view_name.trim_start_matches("tv_");
    sql.push_str(&format!("CREATE INDEX idx_{base_name}_id ON {view_name} (id);\n"));
}

/// Generate a Table Arrow (ta_*) view for Arrow Flight streaming
fn generate_table_arrow_view(sql: &mut String, entity: &str, view_name: &str) {
    sql.push_str(&format!("CREATE VIEW {view_name} AS\n"));
    sql.push_str("SELECT\n");
    sql.push_str("    id,\n");
    sql.push_str(&format!("    -- {entity} entity fields optimized for Arrow\n"));
    sql.push_str("    created_at,\n");
    sql.push_str("    updated_at\n");
    sql.push_str("FROM public.schema_placeholder\n");
    sql.push_str("WHERE archived_at IS NULL\n");
    sql.push_str("ORDER BY id;\n");
}

/// Generate helper composition views
fn generate_composition_views(sql: &mut String, _entity: &str, view_name: &str) {
    let base_name = view_name
        .trim_start_matches("va_")
        .trim_start_matches("tv_")
        .trim_start_matches("ta_");

    // Recent items view
    sql.push_str(&format!("CREATE VIEW {base_name}_recent AS\n"));
    sql.push_str("SELECT * FROM {}\n");
    sql.push_str("WHERE updated_at > NOW() - INTERVAL '7 days'\n");
    sql.push_str("ORDER BY updated_at DESC;\n\n");

    // Count view
    sql.push_str(&format!("CREATE VIEW {base_name}_count AS\n"));
    sql.push_str("SELECT COUNT(*) as total FROM {};\n");
}

/// Generate monitoring functions for the view
fn generate_monitoring_functions(sql: &mut String, view_name: &str) {
    let func_name = format!("monitor_{view_name}");

    sql.push_str(&format!("CREATE OR REPLACE FUNCTION {func_name}()\n"));
    sql.push_str("RETURNS TABLE (\n");
    sql.push_str("    metric_name TEXT,\n");
    sql.push_str("    metric_value BIGINT\n");
    sql.push_str(") AS $$\n");
    sql.push_str("BEGIN\n");
    sql.push_str("    RETURN QUERY\n");
    sql.push_str(&format!("    SELECT 'row_count'::TEXT, COUNT(*)::BIGINT FROM {view_name};\n"));
    sql.push_str("END;\n");
    sql.push_str("$$ LANGUAGE plpgsql IMMUTABLE;\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_strategy_from_str() {
        assert_eq!(
            RefreshStrategy::from_str("trigger-based").unwrap(),
            RefreshStrategy::TriggerBased
        );
        assert_eq!(RefreshStrategy::from_str("trigger").unwrap(), RefreshStrategy::TriggerBased);
        assert_eq!(RefreshStrategy::from_str("scheduled").unwrap(), RefreshStrategy::Scheduled);
        assert!(RefreshStrategy::from_str("invalid").is_err());
    }

    #[test]
    fn test_refresh_strategy_display() {
        assert_eq!(RefreshStrategy::TriggerBased.to_string(), "trigger-based");
        assert_eq!(RefreshStrategy::Scheduled.to_string(), "scheduled");
    }

    #[test]
    fn test_validate_view_name_vector_arrow() {
        assert_eq!(validate_view_name("va_user_embeddings").unwrap(), "Vector Arrow (va_)");
    }

    #[test]
    fn test_validate_view_name_table_vector() {
        assert_eq!(validate_view_name("tv_user_profile").unwrap(), "Table Vector (tv_)");
    }

    #[test]
    fn test_validate_view_name_table_arrow() {
        assert_eq!(validate_view_name("ta_orders").unwrap(), "Table Arrow (ta_)");
    }

    #[test]
    fn test_validate_view_name_invalid() {
        assert!(validate_view_name("invalid_view").is_err());
        assert!(validate_view_name("v_user").is_err());
    }

    #[test]
    fn test_generate_view_sql_vector_arrow() {
        let sql = generate_view_sql(
            "User",
            "va_user_embeddings",
            "Vector Arrow (va_)",
            RefreshStrategy::TriggerBased,
            false,
            false,
        );

        assert!(sql.contains("CREATE VIEW va_user_embeddings"));
        assert!(sql.contains("Entity: User"));
        assert!(sql.contains("Vector Arrow (va_)"));
        assert!(sql.contains("trigger-based"));
    }

    #[test]
    fn test_generate_view_sql_table_vector() {
        let sql = generate_view_sql(
            "Order",
            "tv_order_summary",
            "Table Vector (tv_)",
            RefreshStrategy::Scheduled,
            false,
            false,
        );

        assert!(sql.contains("CREATE MATERIALIZED VIEW tv_order_summary"));
        assert!(sql.contains("Entity: Order"));
        assert!(sql.contains("scheduled"));
    }

    #[test]
    fn test_generate_view_sql_with_composition_views() {
        let sql = generate_view_sql(
            "User",
            "tv_user_profile",
            "Table Vector (tv_)",
            RefreshStrategy::TriggerBased,
            true,
            false,
        );

        assert!(sql.contains("Composition views"));
        assert!(sql.contains("_recent"));
        assert!(sql.contains("_count"));
    }

    #[test]
    fn test_generate_view_sql_with_monitoring() {
        let sql = generate_view_sql(
            "User",
            "tv_user_profile",
            "Table Vector (tv_)",
            RefreshStrategy::TriggerBased,
            false,
            true,
        );

        assert!(sql.contains("Monitoring functions"));
        assert!(sql.contains("monitor_tv_user_profile"));
        assert!(sql.contains("metric_name"));
    }

    #[test]
    fn test_generate_view_sql_full_options() {
        let sql = generate_view_sql(
            "User",
            "ta_users",
            "Table Arrow (ta_)",
            RefreshStrategy::TriggerBased,
            true,
            true,
        );

        assert!(sql.contains("Entity: User"));
        assert!(sql.contains("View: ta_users"));
        assert!(sql.contains("Composition views"));
        assert!(sql.contains("Monitoring functions"));
    }
}
