//! Condition DSL parser and evaluator for conditional observer actions.
//!
//! This module implements a lightweight DSL for evaluating conditions against events.
//! Supported conditions:
//! - Field comparisons: `field == 'value'`, `field != 'value'`, `field > 10`, `field < 20`
//! - Field existence: `has_field('name')`
//! - Field changes: `field_changed('status')`, `field_changed_to('status', 'shipped')`,
//!   `field_changed_from('status', 'pending')`
//! - Logical operators: `&&` (AND), `||` (OR)
//! - Grouping: `(condition1) && (condition2)`
//!
//! # Equality semantics (#843)
//!
//! `==` / `!=` compare numbers **numerically**, exactly like the ordered
//! operators: a PostgreSQL `numeric(10,2)` value arriving as `100.00` equals
//! the literal `100`. Integer comparisons are exact (no rounding through `f64`
//! above 2^53). Quoting decides the literal's type: `total == 100` compares
//! numerically, `code == '100'` compares as a string — a number never equals a
//! string.
//!
//! # `field_changed*` requires a pre-image (#845)
//!
//! Change detection needs the row's before-image, which the change log records
//! only for mutations that opt into `changelog_pre_image = true` (off by
//! default). Evaluating any `field_changed*` condition against an UPDATE that
//! carries no pre-image is an **error** (logged loudly, action skipped), not
//! `false` — silently-never-fires is how a misconfigured shipping-confirmation
//! observer looks "correctly configured but not matching". INSERT/DELETE
//! events have no diff by definition and evaluate to `false` without error.
//!
//! # Examples
//!
//! ```rust
//! use fraiseql_observers::{ConditionParser, event::{EntityEvent, EventKind}};
//! use uuid::Uuid;
//! use serde_json::json;
//!
//! let evaluator = ConditionParser::new();
//! let ast = evaluator.parse("total > 100").expect("valid condition");
//! let event = EntityEvent::new(
//!     EventKind::Updated,
//!     "Order".to_string(),
//!     Uuid::new_v4(),
//!     json!({"total": 150}),
//! );
//! let result = evaluator.evaluate(&ast, &event).expect("evaluation succeeded");
//! assert!(result);
//! ```

use std::fmt;

use serde_json::Value;

use crate::{error::Result, event::EntityEvent};

mod evaluator;
mod lexer;
mod parser;

#[cfg(test)]
mod tests;

// Bring Token into scope for this module and allow sub-modules to use `super::Token`.
use lexer::Token;

/// Abstract Syntax Tree for conditions
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionAst {
    /// Comparison: field op value
    Comparison {
        /// Field name or path
        field: String,
        /// Operator: ==, !=, >, <, >=, <=
        op:    String,
        /// Value to compare against
        value: String,
    },
    /// Check if field exists
    HasField {
        /// Field name
        field: String,
    },
    /// Check if field changed
    FieldChanged {
        /// Field name
        field: String,
    },
    /// Check if field changed to specific value
    FieldChangedTo {
        /// Field name
        field: String,
        /// Expected new value
        value: String,
    },
    /// Check if field changed from specific value
    FieldChangedFrom {
        /// Field name
        field: String,
        /// Expected old value
        value: String,
    },
    /// Logical AND
    And {
        /// Left operand
        left:  Box<ConditionAst>,
        /// Right operand
        right: Box<ConditionAst>,
    },
    /// Logical OR
    Or {
        /// Left operand
        left:  Box<ConditionAst>,
        /// Right operand
        right: Box<ConditionAst>,
    },
    /// Logical NOT
    Not {
        /// Operand
        expr: Box<ConditionAst>,
    },
}

impl fmt::Display for ConditionAst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConditionAst::Comparison { field, op, value } => {
                write!(f, "{field} {op} {value}")
            },
            ConditionAst::HasField { field } => write!(f, "has_field('{field}')"),
            ConditionAst::FieldChanged { field } => write!(f, "field_changed('{field}')"),
            ConditionAst::FieldChangedTo { field, value } => {
                write!(f, "field_changed_to('{field}', '{value}')")
            },
            ConditionAst::FieldChangedFrom { field, value } => {
                write!(f, "field_changed_from('{field}', '{value}')")
            },
            ConditionAst::And { left, right } => write!(f, "({left}) && ({right})"),
            ConditionAst::Or { left, right } => write!(f, "({left}) || ({right})"),
            ConditionAst::Not { expr } => write!(f, "!({expr})"),
        }
    }
}

/// Condition parser and evaluator
pub struct ConditionParser {}

impl ConditionParser {
    /// Create a new condition parser
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Parse a condition string into an AST
    ///
    /// # Arguments
    /// * `condition` - The condition string to parse
    ///
    /// # Returns
    /// Result with `ConditionAst` on success
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ObserverError::InvalidCondition`] if the condition
    /// string contains invalid syntax or unknown tokens.
    pub fn parse(&self, condition: &str) -> Result<ConditionAst> {
        let tokens = self.tokenize(condition)?;
        self.parse_tokens(&tokens)
    }

    /// Evaluate a parsed condition against an event
    ///
    /// # Arguments
    /// * `ast` - The parsed condition AST
    /// * `event` - The event to evaluate against
    ///
    /// # Returns
    /// true if condition is satisfied, false otherwise
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ObserverError::InvalidCondition`] when a
    /// comparison references a field that is not present in the event data, or
    /// when a numeric comparison is applied to a non-numeric field value.
    pub fn evaluate(&self, ast: &ConditionAst, event: &EntityEvent) -> Result<bool> {
        match ast {
            ConditionAst::Comparison { field, op, value } => {
                self.eval_comparison(field, op, value, event)
            },
            ConditionAst::HasField { field } => Ok(self.eval_has_field(field, event)),
            ConditionAst::FieldChanged { field } => {
                Self::require_change_tracking(event)?;
                Ok(event.field_changed(field))
            },
            ConditionAst::FieldChangedTo { field, value } => {
                Self::require_change_tracking(event)?;
                let parsed_value = serde_json::from_str::<Value>(value)
                    .unwrap_or_else(|_| Value::String(value.clone()));
                Ok(event.field_changed_to(field, &parsed_value))
            },
            ConditionAst::FieldChangedFrom { field, value } => {
                Self::require_change_tracking(event)?;
                let parsed_value = serde_json::from_str::<Value>(value)
                    .unwrap_or_else(|_| Value::String(value.clone()));
                Ok(event.field_changed_from(field, &parsed_value))
            },
            ConditionAst::And { left, right } => {
                Ok(self.evaluate(left, event)? && self.evaluate(right, event)?)
            },
            ConditionAst::Or { left, right } => {
                Ok(self.evaluate(left, event)? || self.evaluate(right, event)?)
            },
            ConditionAst::Not { expr } => Ok(!self.evaluate(expr, event)?),
        }
    }

    /// Parse a condition string and evaluate in one step
    ///
    /// # Errors
    ///
    /// Returns the first error from [`Self::parse`] or [`Self::evaluate`].
    pub fn parse_and_evaluate(&self, condition: &str, event: &EntityEvent) -> Result<bool> {
        let ast = self.parse(condition)?;
        self.evaluate(&ast, event)
    }

    /// Refuse to evaluate `field_changed*` when change tracking is unavailable
    /// (#845).
    ///
    /// An UPDATE whose producer did not opt into `changelog_pre_image` carries
    /// no pre-image, so `EntityEvent.changes` is `None` and "did this field
    /// change?" is **unanswerable** — not "no". Before #845 these conditions
    /// silently evaluated false, so a documented condition family never fired
    /// in the default configuration and the observer merely looked
    /// "not matching". `changes: Some(empty)` (pre-image recorded, nothing
    /// changed) and INSERT/DELETE events (no diff exists by definition) still
    /// evaluate normally.
    fn require_change_tracking(event: &EntityEvent) -> Result<()> {
        if event.event_type == crate::event::EventKind::Updated && event.changes.is_none() {
            return Err(crate::error::ObserverError::InvalidCondition {
                reason: format!(
                    "field_changed* cannot evaluate for {} {}: the UPDATE carries no pre-image \
                     (the producing mutation has changelog_pre_image = false), so change \
                     tracking is unavailable. Enable changelog_pre_image on the mutation, or \
                     remove the field_changed* condition from this observer.",
                    event.entity_type, event.entity_id
                ),
            });
        }
        Ok(())
    }
}

impl Default for ConditionParser {
    fn default() -> Self {
        Self::new()
    }
}
