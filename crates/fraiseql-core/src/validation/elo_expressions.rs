//! Elo expression-based validation rules.
//!
//! This module integrates Elo (<https://elo-lang.org/>), an expression language by Bernard Lambeau,
//! as a validation framework, enabling concise, portable validation rules that can be compiled to
//! multiple targets (Rust, `JavaScript`, SQL).

use serde_json::{Value, json};

use crate::error::{FraiseQLError, Result};

/// Elo expression evaluator for validation rules.
///
/// Supports a subset of Elo syntax optimized for validation:
/// - Comparison operators: <, <=, >, >=, ==, !=
/// - Logical operators: &&, ||, !
/// - Field references: user.email, user.age
/// - Function calls: `today()`, age(field), matches(field, pattern)
/// - Literals: numbers, strings, booleans, dates
#[derive(Debug, Clone)]
pub struct EloExpressionEvaluator {
    /// The ELO expression to evaluate
    expression: String,
}

/// Validation result from ELO expression evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EloValidationResult {
    /// Whether the expression evaluated to true
    pub valid: bool,
    /// Error message if validation failed
    pub error: Option<String>,
}

impl EloExpressionEvaluator {
    /// Create a new ELO expression evaluator.
    #[must_use]
    pub const fn new(expression: String) -> Self {
        Self { expression }
    }

    /// Evaluate the ELO expression against a JSON object.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the expression references an
    /// unknown field, uses an unsupported function, or cannot be parsed.
    pub fn evaluate(&self, context: &Value) -> Result<EloValidationResult> {
        // Parse and evaluate the expression
        self.evaluate_expression(&self.expression, context)
    }

    /// Evaluate a specific expression string against context.
    fn evaluate_expression(&self, expr: &str, context: &Value) -> Result<EloValidationResult> {
        let trimmed = expr.trim();

        // Strip outer parentheses if present
        let expr = if trimmed.starts_with('(') && trimmed.ends_with(')') {
            // Check if these are matching parentheses
            if self.are_matching_parens(trimmed) {
                &trimmed[1..trimmed.len() - 1]
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        let expr = expr.trim();

        // Handle logical operators with proper precedence
        // First, split by || (lowest precedence)
        if let Some(or_idx) = self.find_operator_outside_parens(expr, "||") {
            let left = &expr[..or_idx];
            let right = &expr[or_idx + 2..];

            let left_result = self.evaluate_expression(left, context)?;
            if left_result.valid {
                return Ok(EloValidationResult {
                    valid: true,
                    error: None,
                });
            }

            let right_result = self.evaluate_expression(right, context)?;
            return Ok(right_result);
        }

        // Then, split by && (higher precedence than ||)
        if let Some(and_idx) = self.find_operator_outside_parens(expr, "&&") {
            let left = &expr[..and_idx];
            let right = &expr[and_idx + 2..];

            let left_result = self.evaluate_expression(left, context)?;
            if !left_result.valid {
                return Ok(left_result);
            }

            let right_result = self.evaluate_expression(right, context)?;
            return Ok(right_result);
        }

        // Handle negation (!)
        if let Some(inner) = expr.strip_prefix('!') {
            let inner_result = self.evaluate_expression(inner.trim(), context)?;
            return Ok(EloValidationResult {
                valid: !inner_result.valid,
                error: if inner_result.valid {
                    Some("Negation failed".to_string())
                } else {
                    None
                },
            });
        }

        // Handle comparison operators
        for op in &["==", "!=", "<=", ">=", "<", ">"] {
            if let Some(op_idx) = self.find_operator_outside_parens(expr, op) {
                let left = &expr[..op_idx].trim();
                let right = &expr[op_idx + op.len()..].trim();

                return self.evaluate_comparison(left, op, right, context);
            }
        }

        // Handle function calls
        if expr.contains('(') && expr.ends_with(')') {
            return self.evaluate_function_call(expr, context);
        }

        // Handle field access or literals
        self.evaluate_value(expr, context)
    }

    /// Evaluate a comparison operation.
    fn evaluate_comparison(
        &self,
        left: &str,
        op: &str,
        right: &str,
        context: &Value,
    ) -> Result<EloValidationResult> {
        let left_val = self.get_value(left, context)?;
        let right_val = self.get_value(right, context)?;

        let valid = match op {
            "==" => left_val == right_val,
            "!=" => left_val != right_val,
            "<" => self.compare_values(&left_val, &right_val) == Some(std::cmp::Ordering::Less),
            "<=" => {
                matches!(
                    self.compare_values(&left_val, &right_val),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            },
            ">" => self.compare_values(&left_val, &right_val) == Some(std::cmp::Ordering::Greater),
            ">=" => {
                matches!(
                    self.compare_values(&left_val, &right_val),
                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                )
            },
            _ => false,
        };

        Ok(EloValidationResult {
            valid,
            error: if valid {
                None
            } else {
                Some(format!("Comparison failed: {} {} {}", left_val, op, right_val))
            },
        })
    }

    /// Parse function arguments, respecting quoted strings
    fn parse_function_args(&self, args_str: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut in_string = false;

        for ch in args_str.chars() {
            match ch {
                '"' => {
                    in_string = !in_string;
                    current_arg.push(ch);
                },
                ',' if !in_string => {
                    args.push(current_arg.trim().to_string());
                    current_arg = String::new();
                },
                _ => {
                    current_arg.push(ch);
                },
            }
        }

        if !current_arg.is_empty() {
            args.push(current_arg.trim().to_string());
        }

        args
    }

    /// Evaluate a function call.
    fn evaluate_function_call(&self, expr: &str, context: &Value) -> Result<EloValidationResult> {
        if let Some(paren_idx) = expr.find('(') {
            let func_name = &expr[..paren_idx].trim();
            let args_str = &expr[paren_idx + 1..expr.len() - 1];

            match *func_name {
                "today" => {
                    // Returns today's date
                    Ok(EloValidationResult {
                        valid: true,
                        error: None,
                    })
                },
                "now" => {
                    // Returns current datetime
                    Ok(EloValidationResult {
                        valid: true,
                        error: None,
                    })
                },
                "matches" => {
                    let parts = self.parse_function_args(args_str);
                    if parts.len() != 2 {
                        return Err(FraiseQLError::Validation {
                            message: "matches() requires 2 arguments".to_string(),
                            path:    None,
                        });
                    }

                    let field_val = self.get_value(&parts[0], context)?;
                    let pattern = self.get_value(&parts[1], context)?;

                    if let (Value::String(s), Value::String(p)) = (&field_val, &pattern) {
                        match regex::Regex::new(p) {
                            Ok(re) => {
                                let valid = re.is_match(s);
                                Ok(EloValidationResult {
                                    valid,
                                    error: if valid {
                                        None
                                    } else {
                                        Some(format!("'{}' does not match pattern '{}'", s, p))
                                    },
                                })
                            },
                            Err(_) => Err(FraiseQLError::Validation {
                                message: format!("Invalid regex pattern: {}", p),
                                path:    None,
                            }),
                        }
                    } else {
                        Err(FraiseQLError::Validation {
                            message: "matches() requires string arguments".to_string(),
                            path:    None,
                        })
                    }
                },
                "contains" => {
                    let parts = self.parse_function_args(args_str);
                    if parts.len() != 2 {
                        return Err(FraiseQLError::Validation {
                            message: "contains() requires 2 arguments".to_string(),
                            path:    None,
                        });
                    }

                    let field_val = self.get_value(&parts[0], context)?;
                    let needle = self.get_value(&parts[1], context)?;

                    if let (Value::String(s), Value::String(n)) = (&field_val, &needle) {
                        let valid = s.contains(n);
                        Ok(EloValidationResult {
                            valid,
                            error: if valid {
                                None
                            } else {
                                Some(format!("'{}' does not contain '{}'", s, n))
                            },
                        })
                    } else {
                        Err(FraiseQLError::Validation {
                            message: "contains() requires string arguments".to_string(),
                            path:    None,
                        })
                    }
                },
                "length" => {
                    let field_val = self.get_value(args_str, context)?;
                    if let Value::String(_s) = field_val {
                        Ok(EloValidationResult {
                            valid: true,
                            error: None,
                        })
                    } else {
                        Err(FraiseQLError::Validation {
                            message: "length() requires a string argument".to_string(),
                            path:    None,
                        })
                    }
                },
                "age" => {
                    let _field_val = self.get_value(args_str, context)?;
                    // Age calculation would go here
                    Ok(EloValidationResult {
                        valid: true,
                        error: None,
                    })
                },
                _ => Err(FraiseQLError::Validation {
                    message: format!("Unknown function: {}", func_name),
                    path:    None,
                }),
            }
        } else {
            Err(FraiseQLError::Validation {
                message: "Invalid function call".to_string(),
                path:    None,
            })
        }
    }

    /// Evaluate a simple value (field access or literal).
    fn evaluate_value(&self, expr: &str, context: &Value) -> Result<EloValidationResult> {
        let _val = self.get_value(expr, context)?;
        Ok(EloValidationResult {
            valid: true,
            error: None,
        })
    }

    /// Check if outer parentheses are matching
    fn are_matching_parens(&self, expr: &str) -> bool {
        if !expr.starts_with('(') || !expr.ends_with(')') {
            return false;
        }

        let mut count = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in expr.chars().enumerate() {
            if escape {
                escape = false;
                continue;
            }

            if ch == '\\' {
                escape = true;
                continue;
            }

            if ch == '"' && !in_string {
                in_string = true;
                continue;
            }

            if ch == '"' && in_string {
                in_string = false;
                continue;
            }

            if in_string {
                continue;
            }

            match ch {
                '(' => count += 1,
                ')' => {
                    count -= 1;
                    // If we're closing before the end, these aren't matching
                    if count == 0 && i < expr.len() - 1 {
                        return false;
                    }
                },
                _ => {},
            }
        }

        count == 0
    }

    /// Find operator position outside of parentheses and quotes
    fn find_operator_outside_parens(&self, expr: &str, op: &str) -> Option<usize> {
        let mut paren_count = 0;
        let mut in_string = false;
        let chars: Vec<char> = expr.chars().collect();

        for i in (0..chars.len()).rev() {
            let ch = chars[i];

            // Handle strings
            if ch == '"' {
                in_string = !in_string;
                continue;
            }

            if in_string {
                continue;
            }

            // Handle parentheses
            if ch == ')' {
                paren_count += 1;
                continue;
            }

            if ch == '(' {
                paren_count -= 1;
                continue;
            }

            // Only match operator if we're outside parentheses
            if paren_count == 0 {
                // Check if operator matches at this position
                let remaining: String = chars[i..].iter().collect();
                if remaining.starts_with(op) {
                    return Some(i);
                }
            }
        }

        None
    }

    /// Get the actual value of an expression (field reference or literal).
    fn get_value(&self, expr: &str, context: &Value) -> Result<Value> {
        let trimmed = expr.trim();

        // Remove quotes if string literal
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            let unquoted = &trimmed[1..trimmed.len() - 1];
            // Unescape common escape sequences
            let unescaped = unquoted
                .replace("\\\\", "\x00") // Temporary marker for literal backslash
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\r", "\r")
                .replace("\\\"", "\"")
                .replace("\\'", "'")
                .replace('\x00', "\\"); // Restore literal backslash
            return Ok(Value::String(unescaped));
        }

        // Try to parse as number
        if let Ok(i) = trimmed.parse::<i64>() {
            return Ok(Value::Number(i.into()));
        }

        // Try to parse as float
        if let Ok(f) = trimmed.parse::<f64>() {
            return Ok(json!(f));
        }

        // Boolean literals
        if trimmed == "true" {
            return Ok(Value::Bool(true));
        }
        if trimmed == "false" {
            return Ok(Value::Bool(false));
        }

        // null literal
        if trimmed == "null" {
            return Ok(Value::Null);
        }

        // Field access (e.g., "user.email" or "obj.field.subfield")
        if let Some(value) = self.access_field(trimmed, context) {
            return Ok(value);
        }

        Err(FraiseQLError::Validation {
            message: format!("Cannot resolve value: {}", trimmed),
            path:    None,
        })
    }

    /// Access a nested field in the context object.
    fn access_field(&self, path: &str, context: &Value) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();

        let mut current = context.clone();
        for part in parts {
            current = current.get(part)?.clone();
        }

        Some(current)
    }

    /// Compare two JSON values with proper type handling.
    fn compare_values(&self, left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => {
                let l_f64 = l.as_f64()?;
                let r_f64 = r.as_f64()?;
                Some(l_f64.partial_cmp(&r_f64)?)
            },
            (Value::String(l), Value::String(r)) => Some(l.cmp(r)),
            _ => None,
        }
    }
}
