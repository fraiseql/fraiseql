//! Lexer for the condition DSL — tokenises a condition string into `Token` values.

use super::ConditionParser;
use crate::error::{ObserverError, Result};

/// Maximum byte length of a condition expression.
///
/// A 4 `KiB` limit is generous for any real observer rule while preventing an
/// attacker-supplied condition string from driving the lexer through millions
/// of iterations and allocating unbounded token storage.
const MAX_CONDITION_INPUT_BYTES: usize = 4096;

/// Maximum number of arguments accepted in a single condition function call.
///
/// Functions like `in_set('a', 'b', ...)` with thousands of arguments would
/// cause unbounded `Vec` growth; 32 arguments covers all realistic use-cases.
const MAX_CONDITION_FUNCTION_ARGS: usize = 32;

/// Token types produced by the lexer.
#[derive(Debug, Clone)]
pub(super) enum Token {
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

impl ConditionParser {
    /// Tokenize a condition expression string into a list of [`Token`]s.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidCondition`] if the expression exceeds
    /// `MAX_CONDITION_INPUT_BYTES` or contains an unexpected character.
    #[allow(clippy::unused_self)] // Reason: method is part of a public API / trait consistency
    pub(super) fn tokenize(&self, condition: &str) -> Result<Vec<Token>> {
        if condition.len() > MAX_CONDITION_INPUT_BYTES {
            return Err(ObserverError::InvalidCondition {
                reason: format!(
                    "Condition expression too long ({} bytes, max {MAX_CONDITION_INPUT_BYTES})",
                    condition.len()
                ),
            });
        }
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
                                if args.len() >= MAX_CONDITION_FUNCTION_ARGS {
                                    return Err(ObserverError::InvalidCondition {
                                        reason: format!(
                                            "Too many function arguments (max {MAX_CONDITION_FUNCTION_ARGS})"
                                        ),
                                    });
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
}
