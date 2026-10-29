//! The rows of a raw read, with each column as PostgreSQL described it.

use std::collections::HashMap;

/// The rows a raw read returned, and each of its columns as PostgreSQL described it.
///
/// A reader that types a column from its values cannot tell a `numeric` from a float: the
/// JSON a `numeric` decodes to is a number, rounded through an `f64`. The description says
/// what the column is, and a `numeric` cell is a string of the digits PostgreSQL printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescribedRows {
    /// The result's columns, in the order the statement returns them; empty when the rows
    /// were not described.
    pub columns: Vec<DescribedColumn>,
    /// Each row, keyed by column name.
    pub rows:    Vec<HashMap<String, serde_json::Value>>,
}

impl DescribedRows {
    /// Rows with no description, each column typed by its values.
    #[must_use]
    pub const fn undescribed(rows: Vec<HashMap<String, serde_json::Value>>) -> Self {
        Self {
            columns: Vec::new(),
            rows,
        }
    }

    /// The `numeric` columns, by name.
    pub fn numeric_columns(&self) -> impl Iterator<Item = (&str, NumericType)> {
        self.columns
            .iter()
            .filter_map(|column| column.numeric.map(|numeric| (column.name.as_str(), numeric)))
    }
}

/// One column of a [`DescribedRows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribedColumn {
    /// The column's name.
    pub name:    String,
    /// Its precision and scale when it is a `numeric`; `None` for any other type.
    pub numeric: Option<NumericType>,
}

/// What a `numeric` column declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericType {
    /// `numeric(precision, scale)`. Since PostgreSQL 15 the scale may be negative.
    Declared {
        /// The total number of significant digits.
        precision: u16,
        /// The number of digits after the decimal point.
        scale:     i16,
    },
    /// A `numeric` with no declared precision: any value, at any scale.
    Unconstrained,
}

impl NumericType {
    /// Decode the type modifier PostgreSQL reports for a `numeric` column.
    ///
    /// The modifier is `((precision << 16) | scale) + 4`, the scale an 11-bit signed field;
    /// any value below 4 (PostgreSQL sends `-1`) declares no precision.
    #[must_use]
    pub fn from_type_modifier(type_modifier: i32) -> Self {
        let Some(packed) = type_modifier.checked_sub(4).filter(|packed| *packed >= 0) else {
            return Self::Unconstrained;
        };
        let precision = u16::try_from(packed >> 16).ok();
        let scale = i16::try_from(((packed & 0x7ff) ^ 0x400) - 0x400).ok();
        match (precision, scale) {
            (Some(precision), Some(scale)) => Self::Declared { precision, scale },
            _ => Self::Unconstrained,
        }
    }
}

#[cfg(test)]
mod tests;
