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
/// ```rust
/// use fraiseql_wire::operators::{OrderByClause, FieldSource, SortOrder, NullsHandling};
///
/// // Order by JSONB field with collation
/// let _ = OrderByClause {
///     field: "name".to_string(),
///     field_source: FieldSource::JsonbPayload,
///     direction: SortOrder::Asc,
///     collation: Some("en-US".to_string()),
///     nulls_handling: None,
/// };
///
/// // Order by direct column with NULLS LAST
/// let _ = OrderByClause {
///     field: "created_at".to_string(),
///     field_source: FieldSource::DirectColumn,
///     direction: SortOrder::Desc,
///     collation: None,
///     nulls_handling: Some(NullsHandling::Last),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct OrderByClause {
    /// Field name (validated separately based on `field_source`)
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
    pub const fn with_nulls(mut self, handling: NullsHandling) -> Self {
        self.nulls_handling = Some(handling);
        self
    }

    /// Validate field name to prevent SQL injection
    ///
    /// # Errors
    ///
    /// Returns an error string if the field name is empty, contains invalid characters,
    /// or the collation name (if set) contains invalid characters.
    pub fn validate(&self) -> Result<(), String> {
        if self.field.is_empty() {
            return Err("Field name cannot be empty".to_string());
        }

        // Validate field name: alphanumeric + underscore
        if !self.field.chars().all(|c| c.is_alphanumeric() || c == '_')
            || self
                .field
                .chars()
                .next()
                .is_some_and(|c| !c.is_alphabetic() && c != '_')
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
    ///
    /// # Errors
    ///
    /// Returns an error string if the clause fails validation (see [`Self::validate`]).
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
mod tests;
