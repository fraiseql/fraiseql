//! Evaluator for the condition DSL — interprets a `ConditionAst` against an event.

use serde_json::Value;

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

use super::ConditionParser;

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

        // Try to parse value as number first, then as string
        let value_parsed: Value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));

        match op {
            "==" => Ok(event_value == &value_parsed),
            "!=" => Ok(event_value != &value_parsed),
            ">" => self.compare_numeric(event_value, &value_parsed, |a, b| a > b),
            "<" => self.compare_numeric(event_value, &value_parsed, |a, b| a < b),
            ">=" => self.compare_numeric(event_value, &value_parsed, |a, b| a >= b),
            "<=" => self.compare_numeric(event_value, &value_parsed, |a, b| a <= b),
            _ => Err(ObserverError::InvalidCondition {
                reason: format!("Unknown operator: {op}"),
            }),
        }
    }

    pub(super) fn eval_has_field(&self, field: &str, event: &EntityEvent) -> Result<bool> {
        Ok(event.data.get(field).is_some())
    }

    pub(super) fn compare_numeric<F>(&self, left: &Value, right: &Value, f: F) -> Result<bool>
    where
        F: Fn(f64, f64) -> bool,
    {
        let left_num = left.as_f64().ok_or(ObserverError::InvalidCondition {
            reason: "Left value is not a number".to_string(),
        })?;

        let right_num = right.as_f64().ok_or(ObserverError::InvalidCondition {
            reason: "Right value is not a number".to_string(),
        })?;

        Ok(f(left_num, right_num))
    }
}
