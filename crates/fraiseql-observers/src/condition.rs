//! Condition DSL parser and evaluator for conditional observer actions.
//!
//! This module implements a lightweight DSL for evaluating conditions against events.
//! Supported conditions:
//! - Field comparisons: `field == "value"`, `field != "value"`, `field > 10`, `field < 20`
//! - Field existence: `has_field('name')`
//! - Field changes: `field_changed('status')`, `field_changed_to('status', 'shipped')`
//! - Logical operators: `&&` (AND), `||` (OR)
//! - Grouping: `(condition1) && (condition2)`
//!
//! # Examples
//!
//! ```ignore
//! let evaluator = ConditionParser::new();
//! let ast = evaluator.parse("total > 100 && status_changed_to('shipped')")?;
//! let result = evaluator.evaluate(&ast, &event)?;
//! ```

use std::fmt;

use regex::Regex;
use serde_json::Value;

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

/// Abstract Syntax Tree for conditions
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
pub struct ConditionParser {
    // Regex patterns for tokenization (marked for future advanced parsing)
    #[allow(dead_code)]
    comparison_re: Regex,
    #[allow(dead_code)]
    function_re:   Regex,
    #[allow(dead_code)]
    identifier_re: Regex,
}

impl ConditionParser {
    /// Create a new condition parser
    #[must_use]
    pub fn new() -> Self {
        Self {
            comparison_re: Regex::new(r"(\w+)\s*(==|!=|>|<|>=|<=)\s*('([^']*)')")
                .expect("Invalid regex"),
            function_re:   Regex::new(r"(\w+)\s*\(\s*'([^']*)'\s*(?:,\s*'([^']*)'\s*)?\)")
                .expect("Invalid regex"),
            identifier_re: Regex::new(r"^[a-zA-Z_]\w*$").expect("Invalid regex"),
        }
    }

    /// Parse a condition string into an AST
    ///
    /// # Arguments
    /// * `condition` - The condition string to parse
    ///
    /// # Returns
    /// Result with `ConditionAst` on success
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
    pub fn evaluate(&self, ast: &ConditionAst, event: &EntityEvent) -> Result<bool> {
        match ast {
            ConditionAst::Comparison { field, op, value } => {
                self.eval_comparison(field, op, value, event)
            },
            ConditionAst::HasField { field } => self.eval_has_field(field, event),
            ConditionAst::FieldChanged { field } => Ok(event.field_changed(field)),
            ConditionAst::FieldChangedTo { field, value } => {
                let parsed_value = serde_json::from_str::<Value>(value)
                    .unwrap_or_else(|_| Value::String(value.clone()));
                Ok(event.field_changed_to(field, &parsed_value))
            },
            ConditionAst::FieldChangedFrom { field, value } => {
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
    pub fn parse_and_evaluate(&self, condition: &str, event: &EntityEvent) -> Result<bool> {
        let ast = self.parse(condition)?;
        self.evaluate(&ast, event)
    }

    // Private helper methods

    fn tokenize(&self, condition: &str) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let mut chars = condition.chars().peekable();

        while let Some(&ch) = chars.peek() {
            match ch {
                // Skip whitespace
                ' ' | '\t' | '\n' | '\r' => {
                    chars.next();
                },
                // Parentheses
                '(' => {
                    tokens.push(Token::LParen);
                    chars.next();
                },
                ')' => {
                    tokens.push(Token::RParen);
                    chars.next();
                },
                // Logical NOT
                '!' => {
                    chars.next();
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        // Extract value for != comparison
                        let mut field = String::new();
                        while let Some(&c) = chars.peek() {
                            if c.is_whitespace() || c == ')' || c == '&' || c == '|' || c == '!' {
                                break;
                            }
                            field.push(c);
                            chars.next();
                        }
                        return Err(ObserverError::InvalidCondition {
                            reason: "!= should be part of comparison, not standalone".to_string(),
                        });
                    }
                    tokens.push(Token::Not);
                },
                // Logical AND
                '&' => {
                    chars.next();
                    if chars.peek() == Some(&'&') {
                        chars.next();
                        tokens.push(Token::And);
                    } else {
                        return Err(ObserverError::InvalidCondition {
                            reason: "Expected '&&', got single '&'".to_string(),
                        });
                    }
                },
                // Logical OR
                '|' => {
                    chars.next();
                    if chars.peek() == Some(&'|') {
                        chars.next();
                        tokens.push(Token::Or);
                    } else {
                        return Err(ObserverError::InvalidCondition {
                            reason: "Expected '||', got single '|'".to_string(),
                        });
                    }
                },
                // Identifier or comparison
                _ if ch.is_alphabetic() || ch == '_' => {
                    let mut ident = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            ident.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Skip whitespace
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }

                    // Check what comes next
                    if chars.peek() == Some(&'(') {
                        // It's a function call
                        chars.next(); // consume '('
                        let mut args = Vec::new();

                        loop {
                            // Skip whitespace
                            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                chars.next();
                            }

                            // Extract quoted string
                            if chars.peek() == Some(&'\'') {
                                chars.next(); // consume opening quote
                                let mut arg = String::new();
                                while let Some(&c) = chars.peek() {
                                    if c == '\'' {
                                        chars.next(); // consume closing quote
                                        break;
                                    }
                                    arg.push(c);
                                    chars.next();
                                }
                                args.push(arg);
                            } else {
                                break;
                            }

                            // Skip whitespace
                            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                chars.next();
                            }

                            // Check for comma or closing paren
                            if chars.peek() == Some(&',') {
                                chars.next(); // consume comma
                            } else if chars.peek() == Some(&')') {
                                chars.next(); // consume closing paren
                                break;
                            } else {
                                break;
                            }
                        }

                        tokens.push(Token::Function { name: ident, args });
                    } else {
                        // It might be a comparison
                        let mut op = String::new();
                        while let Some(&c) = chars.peek() {
                            if c == '=' || c == '!' || c == '>' || c == '<' {
                                op.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        if !op.is_empty()
                            && (op == "=="
                                || op == "!="
                                || op == ">"
                                || op == "<"
                                || op == ">="
                                || op == "<=")
                        {
                            // Skip whitespace
                            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                chars.next();
                            }

                            // Extract value (quoted string or number)
                            let mut value = String::new();
                            if chars.peek() == Some(&'\'') {
                                chars.next(); // consume opening quote
                                while let Some(&c) = chars.peek() {
                                    if c == '\'' {
                                        chars.next(); // consume closing quote
                                        break;
                                    }
                                    value.push(c);
                                    chars.next();
                                }
                            } else {
                                // Extract number or identifier
                                while let Some(&c) = chars.peek() {
                                    if c.is_alphanumeric() || c == '.' || c == '-' {
                                        value.push(c);
                                        chars.next();
                                    } else {
                                        break;
                                    }
                                }
                            }

                            tokens.push(Token::Comparison {
                                field: ident,
                                op,
                                value,
                            });
                        } else {
                            return Err(ObserverError::InvalidCondition {
                                reason: format!("Unknown token: {ident}"),
                            });
                        }
                    }
                },
                _ => {
                    return Err(ObserverError::InvalidCondition {
                        reason: format!("Unexpected character: {ch}"),
                    });
                },
            }
        }

        Ok(tokens)
    }

    fn parse_tokens(&self, tokens: &[Token]) -> Result<ConditionAst> {
        let mut pos = 0;
        let ast = self.parse_or(tokens, &mut pos)?;
        if pos < tokens.len() {
            return Err(ObserverError::InvalidCondition {
                reason: "Unexpected tokens after condition".to_string(),
            });
        }
        Ok(ast)
    }

    fn parse_or(&self, tokens: &[Token], pos: &mut usize) -> Result<ConditionAst> {
        let mut left = self.parse_and(tokens, pos)?;

        while *pos < tokens.len() {
            if matches!(tokens[*pos], Token::Or) {
                *pos += 1;
                let right = self.parse_and(tokens, pos)?;
                left = ConditionAst::Or {
                    left:  Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_and(&self, tokens: &[Token], pos: &mut usize) -> Result<ConditionAst> {
        let mut left = self.parse_not(tokens, pos)?;

        while *pos < tokens.len() {
            if matches!(tokens[*pos], Token::And) {
                *pos += 1;
                let right = self.parse_not(tokens, pos)?;
                left = ConditionAst::And {
                    left:  Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_not(&self, tokens: &[Token], pos: &mut usize) -> Result<ConditionAst> {
        if *pos < tokens.len() && matches!(tokens[*pos], Token::Not) {
            *pos += 1;
            let expr = self.parse_not(tokens, pos)?;
            return Ok(ConditionAst::Not {
                expr: Box::new(expr),
            });
        }

        self.parse_primary(tokens, pos)
    }

    fn parse_primary(&self, tokens: &[Token], pos: &mut usize) -> Result<ConditionAst> {
        if *pos >= tokens.len() {
            return Err(ObserverError::InvalidCondition {
                reason: "Unexpected end of condition".to_string(),
            });
        }

        match &tokens[*pos] {
            Token::LParen => {
                *pos += 1;
                let ast = self.parse_or(tokens, pos)?;
                if *pos >= tokens.len() || !matches!(tokens[*pos], Token::RParen) {
                    return Err(ObserverError::InvalidCondition {
                        reason: "Expected closing parenthesis".to_string(),
                    });
                }
                *pos += 1;
                Ok(ast)
            },
            Token::Comparison { field, op, value } => {
                let ast = ConditionAst::Comparison {
                    field: field.clone(),
                    op:    op.clone(),
                    value: value.clone(),
                };
                *pos += 1;
                Ok(ast)
            },
            Token::Function { name, args } => {
                let ast = self.parse_function(name, args)?;
                *pos += 1;
                Ok(ast)
            },
            _ => Err(ObserverError::InvalidCondition {
                reason: format!("Expected expression, got {:?}", tokens[*pos]),
            }),
        }
    }

    fn parse_function(&self, name: &str, args: &[String]) -> Result<ConditionAst> {
        match name {
            "has_field" => {
                if args.len() != 1 {
                    return Err(ObserverError::InvalidCondition {
                        reason: format!("has_field expects 1 argument, got {}", args.len()),
                    });
                }
                Ok(ConditionAst::HasField {
                    field: args[0].clone(),
                })
            },
            "field_changed" => {
                if args.len() != 1 {
                    return Err(ObserverError::InvalidCondition {
                        reason: format!("field_changed expects 1 argument, got {}", args.len()),
                    });
                }
                Ok(ConditionAst::FieldChanged {
                    field: args[0].clone(),
                })
            },
            "field_changed_to" => {
                if args.len() != 2 {
                    return Err(ObserverError::InvalidCondition {
                        reason: format!("field_changed_to expects 2 arguments, got {}", args.len()),
                    });
                }
                Ok(ConditionAst::FieldChangedTo {
                    field: args[0].clone(),
                    value: args[1].clone(),
                })
            },
            "field_changed_from" => {
                if args.len() != 2 {
                    return Err(ObserverError::InvalidCondition {
                        reason: format!(
                            "field_changed_from expects 2 arguments, got {}",
                            args.len()
                        ),
                    });
                }
                Ok(ConditionAst::FieldChangedFrom {
                    field: args[0].clone(),
                    value: args[1].clone(),
                })
            },
            _ => Err(ObserverError::InvalidCondition {
                reason: format!("Unknown function: {name}"),
            }),
        }
    }

    fn eval_comparison(
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

    fn eval_has_field(&self, field: &str, event: &EntityEvent) -> Result<bool> {
        Ok(event.data.get(field).is_some())
    }

    fn compare_numeric<F>(&self, left: &Value, right: &Value, f: F) -> Result<bool>
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

impl Default for ConditionParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Token types for the lexer
#[derive(Debug, Clone)]
enum Token {
    Comparison {
        field: String,
        op:    String,
        value: String,
    },
    Function {
        name: String,
        args: Vec<String>,
    },
    And,
    Or,
    Not,
    LParen,
    RParen,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::event::EventKind;

    #[test]
    fn test_parse_simple_comparison() {
        let parser = ConditionParser::new();
        let ast = parser.parse("total == 100").unwrap();

        match ast {
            ConditionAst::Comparison { field, op, value } => {
                assert_eq!(field, "total");
                assert_eq!(op, "==");
                assert_eq!(value, "100");
            },
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_parse_has_field() {
        let parser = ConditionParser::new();
        let ast = parser.parse("has_field('status')").unwrap();

        match ast {
            ConditionAst::HasField { field } => {
                assert_eq!(field, "status");
            },
            _ => panic!("Expected has_field"),
        }
    }

    #[test]
    fn test_parse_field_changed_to() {
        let parser = ConditionParser::new();
        let ast = parser.parse("field_changed_to('status', 'shipped')").unwrap();

        match ast {
            ConditionAst::FieldChangedTo { field, value } => {
                assert_eq!(field, "status");
                assert_eq!(value, "shipped");
            },
            _ => panic!("Expected field_changed_to"),
        }
    }

    #[test]
    fn test_parse_and_operator() {
        let parser = ConditionParser::new();
        let ast = parser.parse("total > 100 && field_changed_to('status', 'shipped')").unwrap();

        match ast {
            ConditionAst::And { left, right } => {
                assert!(matches!(*left, ConditionAst::Comparison { .. }));
                assert!(matches!(*right, ConditionAst::FieldChangedTo { .. }));
            },
            _ => panic!("Expected AND"),
        }
    }

    #[test]
    fn test_parse_or_operator() {
        let parser = ConditionParser::new();
        let ast = parser.parse("status == 'pending' || status == 'processing'").unwrap();

        match ast {
            ConditionAst::Or { .. } => {},
            _ => panic!("Expected OR"),
        }
    }

    #[test]
    fn test_parse_not_operator() {
        let parser = ConditionParser::new();
        let ast = parser.parse("!has_field('deleted_at')").unwrap();

        match ast {
            ConditionAst::Not { .. } => {},
            _ => panic!("Expected NOT"),
        }
    }

    #[test]
    fn test_parse_parentheses() {
        let parser = ConditionParser::new();
        let ast = parser.parse("(total > 100) && (status == 'shipped')").unwrap();

        match ast {
            ConditionAst::And { .. } => {},
            _ => panic!("Expected AND"),
        }
    }

    #[test]
    fn test_evaluate_simple_comparison() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 150, "status": "pending"}),
        );

        let result = parser.parse_and_evaluate("total > 100", &event).unwrap();
        assert!(result);

        let result = parser.parse_and_evaluate("total < 100", &event).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_equality() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"status": "pending"}),
        );

        let result = parser.parse_and_evaluate("status == 'pending'", &event).unwrap();
        assert!(result);

        let result = parser.parse_and_evaluate("status == 'shipped'", &event).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_has_field() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = parser.parse_and_evaluate("has_field('total')", &event).unwrap();
        assert!(result);

        let result = parser.parse_and_evaluate("has_field('nonexistent')", &event).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_and_operator() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 150, "status": "shipped"}),
        );

        let result =
            parser.parse_and_evaluate("total > 100 && status == 'shipped'", &event).unwrap();
        assert!(result);

        let result =
            parser.parse_and_evaluate("total > 100 && status == 'pending'", &event).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_or_operator() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"status": "shipped"}),
        );

        let result = parser
            .parse_and_evaluate("status == 'pending' || status == 'shipped'", &event)
            .unwrap();
        assert!(result);

        let result = parser
            .parse_and_evaluate("status == 'pending' || status == 'processing'", &event)
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_not_operator() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = parser.parse_and_evaluate("!has_field('deleted_at')", &event).unwrap();
        assert!(result);

        let result = parser.parse_and_evaluate("!has_field('total')", &event).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_complex_condition() {
        let parser = ConditionParser::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 150, "status": "shipped", "priority": "high"}),
        );

        let result = parser
            .parse_and_evaluate(
                "(total > 100 && status == 'shipped') || priority == 'high'",
                &event,
            )
            .unwrap();
        assert!(result);
    }
}
