//! Fact Table Introspection Module
//!
//! This module provides functionality to detect and introspect fact tables following
//! FraiseQL's analytics architecture:
//!
//! # Fact Table Pattern
//!
//! - **Table naming**: `tf_*` prefix (table fact)
//! - **Measures**: SQL columns with numeric types (INT, BIGINT, DECIMAL, FLOAT) - for fast aggregation
//! - **Dimensions**: JSONB `data` column - for flexible GROUP BY
//! - **Denormalized filters**: Indexed SQL columns (customer_id, occurred_at) - for fast WHERE
//!
//! # No Joins Principle
//!
//! FraiseQL does NOT support joins. All dimensional data must be denormalized into the
//! `data` JSONB column at ETL time (managed by DBA/data team, not FraiseQL).
//!
//! # Example Fact Table
//!
//! ```sql
//! CREATE TABLE tf_sales (
//!     id BIGSERIAL PRIMARY KEY,
//!     -- Measures (SQL columns for fast aggregation)
//!     revenue DECIMAL(10,2) NOT NULL,
//!     quantity INT NOT NULL,
//!     cost DECIMAL(10,2) NOT NULL,
//!     -- Dimensions (JSONB for flexible grouping)
//!     data JSONB NOT NULL,
//!     -- Denormalized filters (indexed for fast WHERE)
//!     customer_id UUID NOT NULL,
//!     product_id UUID NOT NULL,
//!     occurred_at TIMESTAMPTZ NOT NULL,
//!     created_at TIMESTAMPTZ DEFAULT NOW()
//! );
//! ```

use crate::error::{FraiseQLError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Database introspection trait for querying table metadata
#[async_trait]
pub trait DatabaseIntrospector: Send + Sync {
    /// Query column information for a table
    ///
    /// Returns: Vec of (column_name, data_type, is_nullable)
    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>>;

    /// Query indexes for a table
    ///
    /// Returns: Vec of column names that have indexes
    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>>;

    /// Get database type (for SQL type parsing)
    fn database_type(&self) -> DatabaseType;
}

/// Database type enum for SQL type parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    SQLServer,
}

/// Detects and introspects fact tables
pub struct FactTableDetector;

/// Metadata about a fact table structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactTableMetadata {
    /// Table name (e.g., "tf_sales")
    pub table_name: String,
    /// Measures (aggregatable numeric columns)
    pub measures: Vec<MeasureColumn>,
    /// Dimension column (JSONB)
    pub dimensions: DimensionColumn,
    /// Denormalized filter columns
    pub denormalized_filters: Vec<FilterColumn>,
}

/// A measure column (aggregatable numeric type)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasureColumn {
    /// Column name (e.g., "revenue")
    pub name: String,
    /// SQL data type
    pub sql_type: SqlType,
    /// Is nullable
    pub nullable: bool,
}

/// SQL data types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlType {
    /// SMALLINT, INT, INTEGER
    Int,
    /// BIGINT
    BigInt,
    /// DECIMAL, NUMERIC
    Decimal,
    /// REAL, FLOAT, DOUBLE PRECISION
    Float,
    /// JSONB (PostgreSQL)
    Jsonb,
    /// JSON (MySQL, SQL Server)
    Json,
    /// TEXT, VARCHAR
    Text,
    /// UUID
    Uuid,
    /// TIMESTAMP, TIMESTAMPTZ
    Timestamp,
    /// DATE
    Date,
    /// BOOLEAN
    Boolean,
    /// Other types
    Other(String),
}

/// Dimension column (JSONB)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionColumn {
    /// Column name (default: "data")
    pub name: String,
    /// Detected dimension paths (optional, extracted from sample data)
    pub paths: Vec<DimensionPath>,
}

/// A dimension path within the JSONB column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionPath {
    /// Path name (e.g., "category")
    pub name: String,
    /// JSON path (e.g., "data->>'category'" for PostgreSQL)
    pub json_path: String,
    /// Data type hint
    pub data_type: String,
}

/// A denormalized filter column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterColumn {
    /// Column name (e.g., "customer_id")
    pub name: String,
    /// SQL data type
    pub sql_type: SqlType,
    /// Is indexed (for performance)
    pub indexed: bool,
}

impl FactTableDetector {
    /// Detect if a table name follows the fact table pattern
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::compiler::fact_table::FactTableDetector;
    ///
    /// assert!(FactTableDetector::is_fact_table("tf_sales"));
    /// assert!(FactTableDetector::is_fact_table("tf_events"));
    /// assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
    /// assert!(!FactTableDetector::is_fact_table("v_user"));
    /// ```
    pub fn is_fact_table(table_name: &str) -> bool {
        table_name.starts_with("tf_")
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
                path: None,
            });
        }

        // Query column information
        let columns = introspector.get_columns(table_name).await?;
        if columns.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("Table '{}' not found or has no columns", table_name),
                path: None,
            });
        }

        // Query indexed columns
        let indexed_columns = introspector.get_indexed_columns(table_name).await?;
        let indexed_set: std::collections::HashSet<String> =
            indexed_columns.into_iter().collect();

        // Parse SQL types based on database
        let db_type = introspector.database_type();

        let mut measures = Vec::new();
        let mut dimension_column: Option<DimensionColumn> = None;
        let mut filters = Vec::new();

        for (name, data_type, is_nullable) in columns {
            let sql_type = Self::parse_sql_type(&data_type, db_type);

            match sql_type {
                SqlType::Jsonb | SqlType::Json => {
                    // This is the dimension column
                    dimension_column = Some(DimensionColumn {
                        name: name.clone(),
                        paths: Vec::new(), // TODO: Extract paths from sample data
                    });
                }
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float => {
                    // Skip common non-measure columns
                    if name != "id" && !name.ends_with("_id") {
                        measures.push(MeasureColumn {
                            name: name.clone(),
                            sql_type: sql_type.clone(),
                            nullable: is_nullable,
                        });
                    }

                    // Check if it's a denormalized filter
                    if name.ends_with("_id") && indexed_set.contains(&name) {
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type: sql_type.clone(),
                            indexed: true,
                        });
                    }
                }
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
                            indexed: indexed_set.contains(&name),
                        });
                    } else if (name == "occurred_at" || name == "created_at")
                        && indexed_set.contains(&name)
                    {
                        // Timestamp columns are important filters if indexed
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type,
                            indexed: true,
                        });
                    }
                }
            }
        }

        let metadata = FactTableMetadata {
            table_name: table_name.to_string(),
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name: "data".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
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
                path: None,
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
                    path: None,
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
                path: None,
            });
        }

        Ok(())
    }

    /// Check if SQL type is numeric (suitable for aggregation)
    fn is_numeric_type(sql_type: &SqlType) -> bool {
        matches!(
            sql_type,
            SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float
        )
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
                        name: name.to_string(),
                        paths: Vec::new(),
                    });
                }
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
                }
                _ => {
                    // This might be a filter column (if not id/created_at/updated_at)
                    if name != "id" && name != "created_at" && name != "updated_at" {
                        filters.push(FilterColumn {
                            name: name.to_string(),
                            sql_type,
                            indexed: false, // Would need to query indexes to determine
                        });
                    }
                }
            }
        }

        let metadata = FactTableMetadata {
            table_name,
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name: "data".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
        };

        Self::validate(&metadata)?;
        Ok(metadata)
    }
}

impl SqlType {
    /// Parse SQL type from string (database-specific)
    pub fn from_str_postgres(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "smallint" | "int" | "integer" | "int2" | "int4" => Self::Int,
            "bigint" | "int8" => Self::BigInt,
            "decimal" | "numeric" => Self::Decimal,
            "real" | "float" | "double precision" | "float4" | "float8" => Self::Float,
            "jsonb" => Self::Jsonb,
            "json" => Self::Json,
            "text" | "varchar" | "character varying" | "char" | "character" => Self::Text,
            "uuid" => Self::Uuid,
            "timestamp" | "timestamptz" | "timestamp with time zone"
            | "timestamp without time zone" => Self::Timestamp,
            "date" => Self::Date,
            "boolean" | "bool" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (MySQL)
    pub fn from_str_mysql(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "tinyint" | "smallint" | "mediumint" | "int" | "integer" => Self::Int,
            "bigint" => Self::BigInt,
            "decimal" | "numeric" => Self::Decimal,
            "float" | "double" | "real" => Self::Float,
            "json" => Self::Json,
            "text" | "varchar" | "char" | "tinytext" | "mediumtext" | "longtext" => Self::Text,
            "timestamp" | "datetime" => Self::Timestamp,
            "date" => Self::Date,
            "boolean" | "bool" | "tinyint(1)" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (SQLite)
    pub fn from_str_sqlite(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "integer" | "int" => Self::BigInt, // SQLite INTEGER is 64-bit
            "real" | "double" | "float" => Self::Float,
            "numeric" | "decimal" => Self::Decimal,
            "text" | "varchar" | "char" => Self::Text,
            "blob" => Self::Other("BLOB".to_string()),
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (SQL Server)
    pub fn from_str_sqlserver(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "tinyint" | "smallint" | "int" => Self::Int,
            "bigint" => Self::BigInt,
            "decimal" | "numeric" | "money" | "smallmoney" => Self::Decimal,
            "float" | "real" => Self::Float,
            "nvarchar" | "varchar" | "char" | "nchar" | "text" | "ntext" => Self::Text,
            "uniqueidentifier" => Self::Uuid,
            "datetime" | "datetime2" | "smalldatetime" | "datetimeoffset" => Self::Timestamp,
            "date" => Self::Date,
            "bit" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fact_table() {
        assert!(FactTableDetector::is_fact_table("tf_sales"));
        assert!(FactTableDetector::is_fact_table("tf_events"));
        assert!(FactTableDetector::is_fact_table("tf_api_requests"));
        assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
        assert!(!FactTableDetector::is_fact_table("td_products"));
        assert!(!FactTableDetector::is_fact_table("v_user"));
        assert!(!FactTableDetector::is_fact_table("tb_user"));
    }

    #[test]
    fn test_validate_valid_fact_table() {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![MeasureColumn {
                name: "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
        };

        assert!(FactTableDetector::validate(&metadata).is_ok());
    }

    #[test]
    fn test_validate_missing_measures() {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
        };

        let result = FactTableDetector::validate(&metadata);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one measure"));
    }

    #[test]
    fn test_validate_non_numeric_measure() {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![MeasureColumn {
                name: "category".to_string(),
                sql_type: SqlType::Text, // Wrong type for measure!
                nullable: false,
            }],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
        };

        let result = FactTableDetector::validate(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be numeric"));
    }

    #[test]
    fn test_from_columns() {
        let columns = vec![
            ("id", SqlType::BigInt, false),
            ("revenue", SqlType::Decimal, false),
            ("quantity", SqlType::Int, false),
            ("data", SqlType::Jsonb, false),
            ("customer_id", SqlType::Uuid, false),
            ("occurred_at", SqlType::Timestamp, false),
        ];

        let metadata =
            FactTableDetector::from_columns("tf_sales".to_string(), columns).unwrap();

        assert_eq!(metadata.measures.len(), 2);
        assert_eq!(metadata.measures[0].name, "revenue");
        assert_eq!(metadata.measures[1].name, "quantity");
        assert_eq!(metadata.dimensions.name, "data");
        assert_eq!(metadata.denormalized_filters.len(), 2);
        assert_eq!(metadata.denormalized_filters[0].name, "customer_id");
        assert_eq!(metadata.denormalized_filters[1].name, "occurred_at");
    }

    #[test]
    fn test_sql_type_from_str_postgres() {
        assert_eq!(SqlType::from_str_postgres("integer"), SqlType::Int);
        assert_eq!(SqlType::from_str_postgres("BIGINT"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_postgres("decimal"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_postgres("FLOAT"), SqlType::Float);
        assert_eq!(SqlType::from_str_postgres("jsonb"), SqlType::Jsonb);
        assert_eq!(SqlType::from_str_postgres("text"), SqlType::Text);
        assert_eq!(SqlType::from_str_postgres("uuid"), SqlType::Uuid);
        assert_eq!(
            SqlType::from_str_postgres("timestamptz"),
            SqlType::Timestamp
        );
    }

    #[test]
    fn test_sql_type_from_str_mysql() {
        assert_eq!(SqlType::from_str_mysql("INT"), SqlType::Int);
        assert_eq!(SqlType::from_str_mysql("bigint"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_mysql("DECIMAL"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_mysql("double"), SqlType::Float);
        assert_eq!(SqlType::from_str_mysql("json"), SqlType::Json);
        assert_eq!(SqlType::from_str_mysql("VARCHAR"), SqlType::Text);
    }

    #[test]
    fn test_sql_type_from_str_sqlite() {
        assert_eq!(SqlType::from_str_sqlite("INTEGER"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_sqlite("real"), SqlType::Float);
        assert_eq!(SqlType::from_str_sqlite("TEXT"), SqlType::Text);
    }

    #[test]
    fn test_sql_type_from_str_sqlserver() {
        assert_eq!(SqlType::from_str_sqlserver("INT"), SqlType::Int);
        assert_eq!(SqlType::from_str_sqlserver("BIGINT"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_sqlserver("decimal"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_sqlserver("float"), SqlType::Float);
        assert_eq!(SqlType::from_str_sqlserver("NVARCHAR"), SqlType::Text);
        assert_eq!(
            SqlType::from_str_sqlserver("uniqueidentifier"),
            SqlType::Uuid
        );
    }

    #[test]
    fn test_is_numeric_type() {
        assert!(FactTableDetector::is_numeric_type(&SqlType::Int));
        assert!(FactTableDetector::is_numeric_type(&SqlType::BigInt));
        assert!(FactTableDetector::is_numeric_type(&SqlType::Decimal));
        assert!(FactTableDetector::is_numeric_type(&SqlType::Float));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Text));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Jsonb));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Uuid));
    }
}
