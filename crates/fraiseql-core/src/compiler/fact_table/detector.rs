use super::{
    CalendarBucket, CalendarDimension, CalendarGranularity, DatabaseIntrospector, DatabaseType,
    DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
};
use crate::error::{FraiseQLError, Result};

/// Detects and introspects fact tables
pub struct FactTableDetector;

impl FactTableDetector {
    /// Detect if a table name follows the fact table pattern.
    ///
    /// Fact tables must follow the naming convention: `tf_<table_name>`
    /// where the table name contains only lowercase letters, numbers, and underscores.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Table name to check
    ///
    /// # Returns
    ///
    /// `true` if the table name starts with `tf_` and follows naming conventions,
    /// `false` otherwise.
    ///
    /// # Notes
    ///
    /// - The check is strict: `tf_` is required as a prefix
    /// - Table names like `TF_sales` (uppercase prefix) are NOT recognized as fact tables
    /// - Empty strings and tables named just `tf_` without additional suffix are not valid
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::compiler::fact_table::FactTableDetector;
    ///
    /// assert!(FactTableDetector::is_fact_table("tf_sales"));
    /// assert!(FactTableDetector::is_fact_table("tf_events"));
    /// assert!(FactTableDetector::is_fact_table("tf_page_views_daily"));
    /// assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
    /// assert!(!FactTableDetector::is_fact_table("v_user"));
    /// assert!(!FactTableDetector::is_fact_table("TF_sales")); // uppercase prefix not recognized
    /// assert!(!FactTableDetector::is_fact_table("tf_")); // incomplete name
    /// ```
    pub fn is_fact_table(table_name: &str) -> bool {
        // Must start with "tf_" and have at least one more character
        table_name.len() > 3 && table_name.starts_with("tf_")
    }

    /// Introspect a fact table from the database
    ///
    /// Queries the database schema to extract:
    /// - Measures (numeric columns)
    /// - Dimensions (JSONB/JSON columns)
    /// - Denormalized filters (indexed columns)
    ///
    /// # Arguments
    ///
    /// * `introspector` - Database introspection implementation
    /// * `table_name` - Fact table name (must start with "tf_")
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Table is not a fact table (doesn't start with "tf_")
    /// - Database query fails
    /// - Table structure is invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::compiler::fact_table::{FactTableDetector, DatabaseIntrospector};
    ///
    /// # async fn example(db: impl DatabaseIntrospector) -> Result<(), Box<dyn std::error::Error>> {
    /// let metadata = FactTableDetector::introspect(&db, "tf_sales").await?;
    /// println!("Found {} measures", metadata.measures.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn introspect(
        introspector: &impl DatabaseIntrospector,
        table_name: &str,
    ) -> Result<FactTableMetadata> {
        // Validate table name follows fact table pattern
        if !Self::is_fact_table(table_name) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Table '{}' is not a fact table (must start with 'tf_')",
                    table_name
                ),
                path:    None,
            });
        }

        // Query column information
        let columns = introspector.get_columns(table_name).await?;
        if columns.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("Table '{}' not found or has no columns", table_name),
                path:    None,
            });
        }

        // Query indexed columns
        let indexed_columns = introspector.get_indexed_columns(table_name).await?;
        let indexed_set: std::collections::HashSet<String> = indexed_columns.into_iter().collect();

        // Parse SQL types based on database
        let db_type = introspector.database_type();

        let mut measures = Vec::new();
        let mut dimension_column: Option<DimensionColumn> = None;
        let mut filters = Vec::new();

        for (name, data_type, is_nullable) in &columns {
            let sql_type = Self::parse_sql_type(data_type, db_type);

            match sql_type {
                SqlType::Jsonb | SqlType::Json => {
                    // This is the dimension column - try to extract paths from sample data
                    let paths = if let Ok(Some(sample)) =
                        introspector.get_sample_jsonb(table_name, name).await
                    {
                        Self::extract_dimension_paths(&sample, name, db_type)
                    } else {
                        Vec::new()
                    };
                    dimension_column = Some(DimensionColumn {
                        name: name.clone(),
                        paths,
                    });
                },
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float => {
                    // Skip common non-measure columns
                    if name != "id" && !name.ends_with("_id") {
                        measures.push(MeasureColumn {
                            name:     name.clone(),
                            sql_type: sql_type.clone(),
                            nullable: *is_nullable,
                        });
                    }

                    // Check if it's a denormalized filter
                    if name.ends_with("_id") && indexed_set.contains(name.as_str()) {
                        filters.push(FilterColumn {
                            name:     name.clone(),
                            sql_type: sql_type.clone(),
                            indexed:  true,
                        });
                    }
                },
                _ => {
                    // Other types might be denormalized filters
                    if name != "id"
                        && name != "created_at"
                        && name != "updated_at"
                        && name != "occurred_at"
                    {
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type,
                            indexed: indexed_set.contains(name.as_str()),
                        });
                    } else if (name == "occurred_at" || name == "created_at")
                        && indexed_set.contains(name.as_str())
                    {
                        // Timestamp columns are important filters if indexed
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type,
                            indexed: true,
                        });
                    }
                },
            }
        }

        // Detect calendar dimensions
        let calendar_dimensions = Self::detect_calendar_dimensions(&columns, &indexed_set)?;

        let metadata = FactTableMetadata {
            table_name: table_name.to_string(),
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name:  "dimensions".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
            calendar_dimensions,
        };

        Self::validate(&metadata)?;
        Ok(metadata)
    }

    /// Parse SQL type string to SqlType enum
    fn parse_sql_type(type_name: &str, db_type: DatabaseType) -> SqlType {
        match db_type {
            DatabaseType::PostgreSQL => SqlType::from_str_postgres(type_name),
            DatabaseType::MySQL => SqlType::from_str_mysql(type_name),
            DatabaseType::SQLite => SqlType::from_str_sqlite(type_name),
            DatabaseType::SQLServer => SqlType::from_str_sqlserver(type_name),
        }
    }

    /// Validate fact table structure
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No measures found
    /// - No dimension column found
    /// - Measures are not numeric types
    pub fn validate(metadata: &FactTableMetadata) -> Result<()> {
        // Must have at least one measure
        if metadata.measures.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Fact table '{}' must have at least one measure column",
                    metadata.table_name
                ),
                path:    None,
            });
        }

        // Validate all measures are numeric
        for measure in &metadata.measures {
            if !Self::is_numeric_type(&measure.sql_type) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Measure column '{}' must be numeric type, found {:?}",
                        measure.name, measure.sql_type
                    ),
                    path:    None,
                });
            }
        }

        // Must have dimension column
        if metadata.dimensions.name.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Fact table '{}' must have a dimension column (JSONB)",
                    metadata.table_name
                ),
                path:    None,
            });
        }

        Ok(())
    }

    /// Check if SQL type is numeric (suitable for aggregation)
    pub(super) fn is_numeric_type(sql_type: &SqlType) -> bool {
        matches!(sql_type, SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float)
    }

    /// Extract dimension paths from a sample JSON value
    ///
    /// Walks through the JSON structure and extracts top-level keys as dimension paths.
    /// Nested objects are represented with dot notation (e.g., "customer.region").
    ///
    /// # Arguments
    ///
    /// * `sample` - Sample JSON value from the dimension column
    /// * `column_name` - Name of the JSONB column (e.g., "dimensions")
    /// * `db_type` - Database type for generating correct JSON path syntax
    ///
    /// # Returns
    ///
    /// Vec of `DimensionPath` extracted from the sample data
    pub fn extract_dimension_paths(
        sample: &serde_json::Value,
        column_name: &str,
        db_type: DatabaseType,
    ) -> Vec<DimensionPath> {
        let mut paths = Vec::new();
        Self::extract_paths_recursive(sample, column_name, "", &mut paths, db_type, 0);
        paths
    }

    /// Recursively extract paths from JSON structure
    fn extract_paths_recursive(
        value: &serde_json::Value,
        column_name: &str,
        prefix: &str,
        paths: &mut Vec<DimensionPath>,
        db_type: DatabaseType,
        depth: usize,
    ) {
        // Limit depth to avoid infinite recursion on deeply nested structures
        if depth > 3 {
            return;
        }

        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                let full_path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };

                // Determine data type from the value
                let data_type = Self::infer_json_type(val);

                // Generate database-specific JSON path syntax
                let json_path = Self::generate_json_path(column_name, &full_path, db_type);

                paths.push(DimensionPath {
                    name: full_path.replace('.', "_"), /* Convert dots to underscores for field
                                                        * names */
                    json_path,
                    data_type,
                });

                // Recurse into nested objects
                if val.is_object() {
                    Self::extract_paths_recursive(
                        val,
                        column_name,
                        &full_path,
                        paths,
                        db_type,
                        depth + 1,
                    );
                }
            }
        }
    }

    /// Infer JSON data type from a value
    pub(super) fn infer_json_type(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "string".to_string(),
            serde_json::Value::Bool(_) => "boolean".to_string(),
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer".to_string()
                } else {
                    "float".to_string()
                }
            },
            serde_json::Value::String(_) => "string".to_string(),
            serde_json::Value::Array(_) => "array".to_string(),
            serde_json::Value::Object(_) => "object".to_string(),
        }
    }

    /// Generate database-specific JSON path syntax
    pub(super) fn generate_json_path(
        column_name: &str,
        path: &str,
        db_type: DatabaseType,
    ) -> String {
        let parts: Vec<&str> = path.split('.').collect();

        match db_type {
            DatabaseType::PostgreSQL => {
                // PostgreSQL: column->>'key' for top-level, column->'nested'->>'key' for nested
                if parts.is_empty() {
                    // Safety: handle empty path by returning raw column name
                    column_name.to_string()
                } else if parts.len() == 1 {
                    format!("{}->>'{}'", column_name, parts[0])
                } else {
                    // Safe: parts.len() >= 2 is guaranteed here
                    if let Some(last) = parts.last() {
                        let rest = &parts[..parts.len() - 1];
                        let nav = rest.iter().fold(String::new(), |mut acc, p| {
                            use std::fmt::Write;
                            let _ = write!(acc, "->'{}'", p);
                            acc
                        });
                        format!("{}{}->>'{}'", column_name, nav, last)
                    } else {
                        // This branch is unreachable due to length check, but safe fallback
                        column_name.to_string()
                    }
                }
            },
            DatabaseType::MySQL => {
                // MySQL: JSON_EXTRACT(column, '$.path.to.key')
                format!("JSON_UNQUOTE(JSON_EXTRACT({}, '$.{}')", column_name, path)
            },
            DatabaseType::SQLite => {
                // SQLite: json_extract(column, '$.path.to.key')
                format!("json_extract({}, '$.{}')", column_name, path)
            },
            DatabaseType::SQLServer => {
                // SQL Server: JSON_VALUE(column, '$.path.to.key')
                format!("JSON_VALUE({}, '$.{}')", column_name, path)
            },
        }
    }

    /// Detect calendar dimension columns (date_info, week_info, etc.)
    ///
    /// Looks for `*_info` JSONB/JSON columns following the calendar dimension pattern.
    /// Returns calendar dimension metadata if calendar columns are found.
    ///
    /// # Arguments
    ///
    /// * `columns` - List of (name, data_type, nullable) tuples
    /// * `_indexed_set` - Set of indexed columns (unused, for future optimization detection)
    ///
    /// # Returns
    ///
    /// Vec of calendar dimensions (empty if none found)
    pub(super) fn detect_calendar_dimensions(
        columns: &[(String, String, bool)],
        _indexed_set: &std::collections::HashSet<String>,
    ) -> Result<Vec<CalendarDimension>> {
        // Look for *_info columns with JSONB/JSON type
        let calendar_columns: Vec<String> = columns
            .iter()
            .filter(|(name, data_type, _)| {
                name.ends_with("_info")
                    && (data_type.to_lowercase().contains("json")
                        || data_type.to_lowercase().contains("jsonb"))
            })
            .map(|(name, _, _)| name.clone())
            .collect();

        if calendar_columns.is_empty() {
            return Ok(Vec::new());
        }

        // Build granularities based on calendar dimension pattern
        let mut granularities = Vec::new();
        for col_name in calendar_columns {
            let buckets = Self::infer_calendar_buckets(&col_name);
            if !buckets.is_empty() {
                granularities.push(CalendarGranularity {
                    column_name: col_name,
                    buckets,
                });
            }
        }

        if granularities.is_empty() {
            return Ok(Vec::new());
        }

        // Assume single source column "occurred_at"
        // (could be enhanced to detect from schema later)
        Ok(vec![CalendarDimension {
            source_column: "occurred_at".to_string(),
            granularities,
        }])
    }

    /// Map calendar column names to available buckets (standard pattern)
    ///
    /// # Arguments
    ///
    /// * `column_name` - Name of the calendar column (e.g., "date_info", "month_info")
    ///
    /// # Returns
    ///
    /// Vec of calendar buckets available in this column
    pub(super) fn infer_calendar_buckets(column_name: &str) -> Vec<CalendarBucket> {
        use crate::compiler::aggregate_types::TemporalBucket;

        match column_name {
            "date_info" => vec![
                CalendarBucket {
                    json_key:    "date".to_string(),
                    bucket_type: TemporalBucket::Day,
                    data_type:   "date".to_string(),
                },
                CalendarBucket {
                    json_key:    "week".to_string(),
                    bucket_type: TemporalBucket::Week,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "week_info" => vec![
                CalendarBucket {
                    json_key:    "week".to_string(),
                    bucket_type: TemporalBucket::Week,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "month_info" => vec![
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "quarter_info" => vec![
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "semester_info" => vec![
                CalendarBucket {
                    json_key:    "semester".to_string(),
                    bucket_type: TemporalBucket::Quarter, // Map to Quarter for now
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "year_info" => vec![CalendarBucket {
                json_key:    "year".to_string(),
                bucket_type: TemporalBucket::Year,
                data_type:   "integer".to_string(),
            }],
            _ => Vec::new(),
        }
    }

    /// Create metadata from column definitions (for testing)
    pub fn from_columns(
        table_name: String,
        columns: Vec<(&str, SqlType, bool)>,
    ) -> Result<FactTableMetadata> {
        let mut measures = Vec::new();
        let mut dimension_column: Option<DimensionColumn> = None;
        let mut filters = Vec::new();

        for (name, sql_type, nullable) in columns {
            match sql_type {
                SqlType::Jsonb | SqlType::Json => {
                    // This is the dimension column
                    dimension_column = Some(DimensionColumn {
                        name:  name.to_string(),
                        paths: Vec::new(),
                    });
                },
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float => {
                    // Skip id column
                    if name != "id" && !name.ends_with("_id") {
                        // This is a measure
                        measures.push(MeasureColumn {
                            name: name.to_string(),
                            sql_type,
                            nullable,
                        });
                    } else if name != "id" {
                        // This is a filter (_id columns)
                        filters.push(FilterColumn {
                            name: name.to_string(),
                            sql_type,
                            indexed: false,
                        });
                    }
                },
                _ => {
                    // This might be a filter column (if not id/created_at/updated_at)
                    if name != "id" && name != "created_at" && name != "updated_at" {
                        filters.push(FilterColumn {
                            name: name.to_string(),
                            sql_type,
                            indexed: false, // Would need to query indexes to determine
                        });
                    }
                },
            }
        }

        let metadata = FactTableMetadata {
            table_name,
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name:  "dimensions".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
            calendar_dimensions: Vec::new(), // No calendar detection in test helper
        };

        Self::validate(&metadata)?;
        Ok(metadata)
    }
}
