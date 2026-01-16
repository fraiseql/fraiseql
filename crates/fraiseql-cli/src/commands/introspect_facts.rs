//! Introspect database for fact tables and output suggestions.
//!
//! This command discovers `tf_*` tables in the database and outputs:
//! - Python decorator suggestions (@`fraiseql.fact_table`)
//! - JSON metadata for manual review
//!
//! **Purpose**: Help developers discover and declare fact tables.
//! **Does NOT auto-modify schema** - outputs suggestions only.

use anyhow::Result;
use fraiseql_core::compiler::fact_table::FactTableMetadata;
use serde_json::json;

/// Output format for introspection results.
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// Python decorator format
    Python,
    /// JSON format
    Json,
}

impl OutputFormat {
    /// Parse from string
    pub fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Self::Python),
            "json" => Ok(Self::Json),
            _ => Err(format!("Invalid format '{s}', expected: python, json")),
        }
    }
}

/// Introspect database for fact tables and output suggestions.
///
/// # Arguments
///
/// * `database_url` - Database connection string (e.g., "postgresql://...")
/// * `format` - Output format (Python or JSON)
///
/// # Returns
///
/// Success or error
///
/// # Example
///
/// ```bash
/// fraiseql introspect facts --database postgresql://localhost/mydb --format python
/// ```
#[allow(clippy::unused_async)] // Will be async when database introspection is implemented
pub async fn run(database_url: &str, format: OutputFormat) -> Result<()> {
    // For now, return a stub implementation
    // Full implementation requires async database adapter creation

    eprintln!("🔍 Introspecting database for fact tables...");
    eprintln!("   Database: {database_url}");

    // TODO: Implement actual database introspection
    // This requires:
    // 1. Create database adapter from URL
    // 2. Use FactTableDetector::list_fact_tables()
    // 3. For each table, call FactTableDetector::introspect()
    // 4. Format output based on `format` parameter

    eprintln!("\n⚠️  Note: Full introspection not yet implemented (Phase 8B in progress)");
    eprintln!("   This will scan for tf_* tables and suggest @fraiseql.fact_table decorators");

    // Mock output for now
    match format {
        OutputFormat::Python => {
            println!("\n# Suggested fact table decorators:");
            println!("# (Copy and paste into your Python schema)");
            println!();
            println!("@fraiseql.fact_table(");
            println!("    measures=['revenue', 'quantity', 'cost'],");
            println!("    dimensions='data',");
            println!("    filters=['customer_id', 'occurred_at']");
            println!(")");
            println!("class Sales:");
            println!("    \"\"\"Sales fact table (tf_sales)\"\"\"");
            println!("    pass");
        }
        OutputFormat::Json => {
            let mock_metadata = json!({
                "tf_sales": {
                    "table_name": "tf_sales",
                    "measures": [
                        {"name": "revenue", "sql_type": "Decimal", "nullable": false},
                        {"name": "quantity", "sql_type": "Int", "nullable": false}
                    ],
                    "dimensions": {
                        "name": "data",
                        "paths": []
                    },
                    "denormalized_filters": [
                        {"name": "customer_id", "sql_type": "Uuid", "indexed": true},
                        {"name": "occurred_at", "sql_type": "Timestamp", "indexed": true}
                    ]
                }
            });
            println!("{}", serde_json::to_string_pretty(&mock_metadata)?);
        }
    }

    eprintln!("\n✅ Introspection complete");
    eprintln!("   Review suggestions above and add to your schema");

    Ok(())
}

/// List fact tables in the database (helper for introspection).
///
/// Returns a list of table names that match the `tf_*` pattern.
#[allow(dead_code)]
#[allow(clippy::unused_async)] // Will be async when database query is implemented
pub async fn list_fact_tables(database_url: &str) -> Result<Vec<String>> {
    // TODO: Implement actual database query
    // For now, return stub
    eprintln!("Scanning database for tf_* tables...");
    eprintln!("Database: {database_url}");

    // Mock result
    Ok(vec![
        "tf_sales".to_string(),
        "tf_orders".to_string(),
    ])
}

/// Format metadata as Python decorator.
#[allow(dead_code)]
fn format_as_python(metadata: &FactTableMetadata) -> String {
    let mut output = String::new();

    // Extract measure names
    let measures: Vec<String> = metadata.measures.iter()
        .map(|m| format!("'{}'", m.name))
        .collect();

    // Extract filter names
    let filters: Vec<String> = metadata.denormalized_filters.iter()
        .map(|f| format!("'{}'", f.name))
        .collect();

    // Format decorator
    output.push_str(&format!("\n# Suggested decorator for {}\n", metadata.table_name));
    output.push_str("@fraiseql.fact_table(\n");
    output.push_str(&format!("    measures=[{}],\n", measures.join(", ")));
    output.push_str(&format!("    dimensions='{}',\n", metadata.dimensions.name));
    output.push_str(&format!("    filters=[{}]\n", filters.join(", ")));
    output.push_str(")\n");

    // Extract class name from table name (tf_sales -> Sales)
    let class_name = metadata.table_name
        .strip_prefix("tf_")
        .unwrap_or(&metadata.table_name)
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>();

    output.push_str(&format!("class {class_name}:\n"));
    output.push_str(&format!("    \"\"\"Fact table: {}\"\"\"", metadata.table_name));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_from_str() {
        assert!(matches!(OutputFormat::from_str("python"), Ok(OutputFormat::Python)));
        assert!(matches!(OutputFormat::from_str("json"), Ok(OutputFormat::Json)));
        assert!(OutputFormat::from_str("invalid").is_err());
    }
}
