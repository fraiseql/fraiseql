//! Declared scalar types for the fields a WHERE clause filters on.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::types::sql_hints::ScalarFieldType;

/// The declared scalar type of every field a WHERE clause may reference,
/// keyed by the dotted snake_case storage path (`"created_at"`,
/// `"machine.serial_number"`).
///
/// A JSON extraction is always `text`, so the generator has to cast before the
/// database can compare it as a number, an instant or a boolean. Which cast is
/// correct is a property of the *field*, not of the operator — deciding it from
/// the operator is what made every date, UUID and string range filter a hard
/// SQL error (#798) and made `in: [19.9]` miss rows `eq: 19.9` matched (#800).
///
/// A path that is absent from the map has no declared type; the generator then
/// falls back to the shape of the supplied JSON value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldTypeMap(HashMap<String, ScalarFieldType>);

impl FieldTypeMap {
    /// Build a map from `(dotted snake_case path, type)` pairs.
    #[must_use]
    pub fn from_pairs<I, S>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (S, ScalarFieldType)>,
        S: Into<String>,
    {
        Self(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Declared type of `path`, or `None` when the field is not declared.
    #[must_use]
    pub fn get(&self, path: &[String]) -> Option<ScalarFieldType> {
        if path.len() == 1 {
            // Reason: length checked on the line above.
            return self.0.get(path.first()?).copied();
        }
        self.0.get(&path.join(".")).copied()
    }

    /// `true` when no field carries a declared type.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of declared fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<(String, ScalarFieldType)> for FieldTypeMap {
    fn from_iter<I: IntoIterator<Item = (String, ScalarFieldType)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Shared handle to a [`FieldTypeMap`], as carried by [`WhereClause::Typed`].
///
/// [`WhereClause::Typed`]: crate::WhereClause::Typed
pub type SharedFieldTypes = Arc<FieldTypeMap>;

/// Resolve the SQL type a comparison against `path` must be made in.
///
/// The declared type wins whenever the compiled schema names the field: it is
/// the only source that knows a JSON string is an instant rather than text.
/// When the field is not declared — a nested relation path, an RLS-composed
/// condition, a library embedder with no schema — the shape of the supplied
/// JSON value is the best available signal.
///
/// For a containment operand (`in` / `nin`) the array's *elements* carry the
/// shape; a mixed array falls back to text, which keeps the comparison
/// well-typed rather than erroring on the first element.
///
/// Both WHERE generators call this, so the parameterised path and the
/// raw-SQL wire path cannot disagree about how a field is typed.
#[must_use]
pub fn operand_type(
    types: Option<&FieldTypeMap>,
    path: &[String],
    value: &serde_json::Value,
) -> ScalarFieldType {
    if let Some(declared) = types.and_then(|t| t.get(path)) {
        return declared;
    }
    match value.as_array() {
        Some(items) => {
            let first = items.first().map_or(ScalarFieldType::Text, value_shape);
            if items.iter().all(|v| value_shape(v) == first) {
                first
            } else {
                ScalarFieldType::Text
            }
        },
        None => value_shape(value),
    }
}

/// Best-effort scalar type for an operand that has no declared type.
///
/// Only the JSON shape is available, so a date is indistinguishable from any
/// other string and compares as text. A declared type always wins over this.
#[must_use]
pub fn value_shape(value: &serde_json::Value) -> ScalarFieldType {
    if value.is_number() {
        ScalarFieldType::Numeric
    } else if value.is_boolean() {
        ScalarFieldType::Boolean
    } else {
        ScalarFieldType::Text
    }
}
