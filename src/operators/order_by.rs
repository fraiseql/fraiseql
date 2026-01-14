//! ORDER BY clause specification
//!
//! Type-safe representation of ORDER BY clauses with support for
//! collation, NULLS FIRST/LAST, and mixed JSONB/direct column ordering.

use std::fmt;

/// Represents a complete ORDER BY clause
///
/// Supports:
/// - Both JSONB fields and direct columns
/// - PostgreSQL collations
/// - NULLS FIRST/LAST handling
/// - Mixed multi-field ordering
///
/// # Examples
///
/// ```ignore
/// // Order by JSONB field with collation
/// OrderByClause {
///     field: "name".to_string(),
///     field_source: FieldSource::JsonbPayload,
///     direction: SortOrder::Asc,
///     collation: Some("en-US".to_string()),
///     nulls_handling: None,
/// }
///
/// // Order by direct column with NULLS LAST
/// OrderByClause {
///     field: "created_at".to_string(),
///     field_source: FieldSource::DirectColumn,
///     direction: SortOrder::Desc,
///     collation: None,
///     nulls_handling: Some(NullsHandling::Last),
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OrderByClause {
    /// Field name (validated separately based on field_source)
    pub field: String,

    /// Where the field comes from
    pub field_source: FieldSource,

    /// Sort direction
    pub direction: SortOrder,

    /// Optional collation name (e.g., "en-US", "C", "de_DE.UTF-8")
    ///
    /// When specified, generates: `field COLLATE "collation_name"`
    pub collation: Option<String>,

    /// Optional NULLS handling
    pub nulls_handling: Option<NullsHandling>,
}

impl OrderByClause {
    /// Create a new ORDER BY clause for a JSONB field
    pub fn jsonb_field(field: impl Into<String>, direction: SortOrder) -> Self {
        Self {
            field: field.into(),
            field_source: FieldSource::JsonbPayload,
            direction,
            collation: None,
            nulls_handling: None,
        }
    }

    /// Create a new ORDER BY clause for a direct column
    pub fn direct_column(field: impl Into<String>, direction: SortOrder) -> Self {
        Self {
            field: field.into(),
            field_source: FieldSource::DirectColumn,
            direction,
            collation: None,
            nulls_handling: None,
        }
    }

    /// Add collation to this clause
    pub fn with_collation(mut self, collation: impl Into<String>) -> Self {
        self.collation = Some(collation.into());
        self
    }

    /// Add NULLS handling to this clause
    pub fn with_nulls(mut self, handling: NullsHandling) -> Self {
        self.nulls_handling = Some(handling);
        self
    }

    /// Validate field name to prevent SQL injection
    pub fn validate(&self) -> Result<(), String> {
        if self.field.is_empty() {
            return Err("Field name cannot be empty".to_string());
        }

        // Validate field name: alphanumeric + underscore
        if !self
            .field
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
            || self
                .field
                .chars()
                .next()
                .map(|c| !c.is_alphabetic() && c != '_')
                .unwrap_or(false)
        {
            return Err(format!("Invalid field name: {}", self.field));
        }

        // Validate collation name if provided
        if let Some(ref collation) = self.collation {
            if collation.is_empty() {
                return Err("Collation name cannot be empty".to_string());
            }
            // Allow common PostgreSQL collation patterns
            // Format: language_REGION.encoding (e.g., en_US.UTF-8, C.UTF-8)
            // or simple names like "C", custom names
            // We validate format loosely - PostgreSQL will validate at execution time
            if !collation
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '@')
            {
                return Err(format!("Invalid collation name: {}", collation));
            }
        }

        Ok(())
    }

    /// Generate SQL for this clause
    ///
    /// # Examples
    ///
    /// - JSONB with collation: `(data->'name') COLLATE "en-US" ASC`
    /// - Direct column: `created_at DESC`
    /// - With NULLS: `status ASC NULLS LAST`
    pub fn to_sql(&self) -> Result<String, String> {
        self.validate()?;

        let field_expr = match self.field_source {
            FieldSource::JsonbPayload => format!("(data->'{}')", self.field),
            FieldSource::DirectColumn => self.field.clone(),
        };

        let mut sql = field_expr;

        // Add collation if specified
        if let Some(ref collation) = self.collation {
            sql.push_str(&format!(" COLLATE \"{}\"", collation));
        }

        // Add direction
        let direction = match self.direction {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        sql.push(' ');
        sql.push_str(direction);

        // Add NULLS handling
        if let Some(nulls) = self.nulls_handling {
            let nulls_str = match nulls {
                NullsHandling::First => "NULLS FIRST",
                NullsHandling::Last => "NULLS LAST",
            };
            sql.push(' ');
            sql.push_str(nulls_str);
        }

        Ok(sql)
    }
}

impl fmt::Display for OrderByClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_sql() {
            Ok(sql) => write!(f, "{}", sql),
            Err(e) => write!(f, "ERROR: {}", e),
        }
    }
}

/// Specifies where a field comes from in ORDER BY
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSource {
    /// Field is inside the JSONB `data` column: `data->>'field'`
    JsonbPayload,

    /// Field is a direct database column: `column_name`
    DirectColumn,
}

impl fmt::Display for FieldSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldSource::JsonbPayload => write!(f, "JSONB"),
            FieldSource::DirectColumn => write!(f, "DIRECT_COLUMN"),
        }
    }
}

/// Sort direction for ORDER BY
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Ascending order (default)
    Asc,

    /// Descending order
    Desc,
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortOrder::Asc => write!(f, "ASC"),
            SortOrder::Desc => write!(f, "DESC"),
        }
    }
}

/// NULL handling in ORDER BY
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullsHandling {
    /// NULL values come first
    First,

    /// NULL values come last
    Last,
}

impl fmt::Display for NullsHandling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NullsHandling::First => write!(f, "NULLS FIRST"),
            NullsHandling::Last => write!(f, "NULLS LAST"),
        }
    }
}

/// PostgreSQL collation specifications
///
/// Supports common collations and custom names.
/// When used, generates: `field COLLATE "collation_name"`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Collation {
    /// Binary collation (fast, deterministic)
    C,

    /// UTF-8 binary collation
    Utf8,

    /// Custom collation name (e.g., "en-US", "de_DE.UTF-8")
    Custom(String),
}

impl Collation {
    /// Get the PostgreSQL collation name as a string
    pub fn as_str(&self) -> &str {
        match self {
            Collation::C => "C",
            Collation::Utf8 => "C.UTF-8",
            Collation::Custom(name) => name,
        }
    }
}

impl fmt::Display for Collation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonb_field_ordering() {
        let clause = OrderByClause::jsonb_field("name", SortOrder::Asc);
        let sql = clause.to_sql().unwrap();
        assert_eq!(sql, "(data->'name') ASC");
    }

    #[test]
    fn test_direct_column_ordering() {
        let clause = OrderByClause::direct_column("created_at", SortOrder::Desc);
        let sql = clause.to_sql().unwrap();
        assert_eq!(sql, "created_at DESC");
    }

    #[test]
    fn test_ordering_with_collation() {
        let clause = OrderByClause::jsonb_field("name", SortOrder::Asc)
            .with_collation("en-US");
        let sql = clause.to_sql().unwrap();
        assert_eq!(sql, "(data->'name') COLLATE \"en-US\" ASC");
    }

    #[test]
    fn test_ordering_with_nulls_last() {
        let clause = OrderByClause::direct_column("status", SortOrder::Asc)
            .with_nulls(NullsHandling::Last);
        let sql = clause.to_sql().unwrap();
        assert_eq!(sql, "status ASC NULLS LAST");
    }

    #[test]
    fn test_ordering_with_collation_and_nulls() {
        let clause = OrderByClause::jsonb_field("email", SortOrder::Desc)
            .with_collation("C")
            .with_nulls(NullsHandling::First);
        let sql = clause.to_sql().unwrap();
        assert_eq!(sql, "(data->'email') COLLATE \"C\" DESC NULLS FIRST");
    }

    #[test]
    fn test_field_validation() {
        assert!(OrderByClause::jsonb_field("valid_name", SortOrder::Asc)
            .validate()
            .is_ok());
        assert!(OrderByClause::jsonb_field("123invalid", SortOrder::Asc)
            .validate()
            .is_err());
        assert!(OrderByClause::jsonb_field("bad-name", SortOrder::Asc)
            .validate()
            .is_err());
    }

    #[test]
    fn test_collation_validation() {
        let clause = OrderByClause::jsonb_field("name", SortOrder::Asc)
            .with_collation("en-US");
        assert!(clause.validate().is_ok());

        let clause = OrderByClause::jsonb_field("name", SortOrder::Asc)
            .with_collation("C.UTF-8");
        assert!(clause.validate().is_ok());

        let clause = OrderByClause::jsonb_field("name", SortOrder::Asc)
            .with_collation("invalid!!!special");
        assert!(clause.validate().is_err());
    }

    #[test]
    fn test_sort_order_display() {
        assert_eq!(SortOrder::Asc.to_string(), "ASC");
        assert_eq!(SortOrder::Desc.to_string(), "DESC");
    }

    #[test]
    fn test_field_source_display() {
        assert_eq!(FieldSource::JsonbPayload.to_string(), "JSONB");
        assert_eq!(FieldSource::DirectColumn.to_string(), "DIRECT_COLUMN");
    }

    #[test]
    fn test_collation_enum() {
        assert_eq!(Collation::C.as_str(), "C");
        assert_eq!(Collation::Utf8.as_str(), "C.UTF-8");
        assert_eq!(Collation::Custom("de-DE".to_string()).as_str(), "de-DE");
    }
}
