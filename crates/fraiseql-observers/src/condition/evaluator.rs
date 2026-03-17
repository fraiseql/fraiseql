//! Evaluator for the condition DSL — interprets a `ConditionAst` against an event.

use std::cmp::Ordering;

use serde_json::Value;
use tracing::warn;

use super::ConditionParser;
use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

impl ConditionParser {
    pub(super) fn eval_comparison(
        &self,
        field: &str,
        op: &str,
        value: &str,
        event: &EntityEvent,
    ) -> Result<bool> {
        let event_value = event.data.get(field).ok_or(ObserverError::InvalidCondition {
            reason: format!("Field not found: {field}"),
        })?;

        // Try to parse value as number first, then as string.
        // Warn when falling back to string: numeric operators (>, <, >=, <=) will
        // subsequently fail with a type mismatch, and == / != may compare
        // a numeric field against a string literal silently.
        let value_parsed: Value = serde_json::from_str(value).unwrap_or_else(|_| {
            warn!(
                condition_value = %value,
                "Condition DSL: value is not valid JSON; treating as string literal. \
                 Numeric comparisons (>, <, >=, <=) will fail with a type mismatch."
            );
            Value::String(value.to_string())
        });

        match op {
            "==" => Ok(event_value == &value_parsed),
            "!=" => Ok(event_value != &value_parsed),
            ">" => self.compare_ordered(event_value, &value_parsed, Ordering::is_gt),
            "<" => self.compare_ordered(event_value, &value_parsed, Ordering::is_lt),
            ">=" => self.compare_ordered(event_value, &value_parsed, Ordering::is_ge),
            "<=" => self.compare_ordered(event_value, &value_parsed, Ordering::is_le),
            _ => Err(ObserverError::InvalidCondition {
                reason: format!("Unknown operator: {op}"),
            }),
        }
    }

    /// Check whether `field` is present in the event data.
    ///
    /// # Errors
    ///
    /// This function currently always returns `Ok`; the `Result` return type is
    /// reserved for future validation.
    pub(super) fn eval_has_field(&self, field: &str, event: &EntityEvent) -> Result<bool> {
        Ok(event.data.get(field).is_some())
    }

    /// Compare two JSON numeric values with exact integer semantics when possible.
    ///
    /// JSON integers are represented as `i64`/`u64` internally; converting them to
    /// `f64` for comparison loses precision for values above 2^53 (e.g.
    /// `9007199254740993` rounds to `9007199254740992.0`).  This method attempts
    /// an exact `i64` comparison first and falls back to `f64` only when one of the
    /// values is a JSON float (or does not fit in `i64`).
    ///
    /// `f` receives the `Ordering` and returns the Boolean result, e.g.
    /// `|o| o.is_gt()` for `>`.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidCondition`] if either operand is not a number,
    /// or if `partial_cmp` produces `NaN`.
    pub(super) fn compare_ordered<F>(&self, left: &Value, right: &Value, f: F) -> Result<bool>
    where
        F: Fn(Ordering) -> bool,
    {
        // Try exact integer comparison first.
        if let (Some(l), Some(r)) = (left.as_i64(), right.as_i64()) {
            return Ok(f(l.cmp(&r)));
        }

        // Fall back to f64 for floats and numbers that don't fit in i64.
        let l = left.as_f64().ok_or(ObserverError::InvalidCondition {
            reason: "Left value is not a number".to_string(),
        })?;
        let r = right.as_f64().ok_or(ObserverError::InvalidCondition {
            reason: "Right value is not a number".to_string(),
        })?;
        let ord = l.partial_cmp(&r).ok_or(ObserverError::InvalidCondition {
            reason: "Cannot compare NaN values".to_string(),
        })?;
        Ok(f(ord))
    }
}
