//! Pratt parser for the condition DSL — turns a token stream into a `ConditionAst`.

use super::{ConditionAst, ConditionParser, Token};
use crate::error::{ObserverError, Result};

/// Maximum nesting depth for condition expressions.
///
/// Prevents stack overflow from deeply-nested `NOT NOT NOT … expr` chains
/// or deeply-parenthesised conditions. 64 levels is far beyond any legitimate
/// condition authored by a human or a schema tool, while still leaving enough
/// headroom for machine-generated conditions from schema linters.
const MAX_CONDITION_DEPTH: usize = 64;

impl ConditionParser {
    /// Parse a token stream into a [`ConditionAst`].
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidCondition`] if the token stream is empty,
    /// exceeds `MAX_CONDITION_DEPTH`, or contains unexpected tokens.
    pub(super) fn parse_tokens(&self, tokens: &[Token]) -> Result<ConditionAst> {
        let mut pos = 0;
        let ast = self.parse_or(tokens, &mut pos, 0)?;
        if pos < tokens.len() {
            return Err(ObserverError::InvalidCondition {
                reason: "Unexpected tokens after condition".to_string(),
            });
        }
        Ok(ast)
    }

    fn parse_or(&self, tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ConditionAst> {
        let mut left = self.parse_and(tokens, pos, depth)?;

        while *pos < tokens.len() {
            if matches!(tokens[*pos], Token::Or) {
                *pos += 1;
                let right = self.parse_and(tokens, pos, depth)?;
                left = ConditionAst::Or {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_and(&self, tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ConditionAst> {
        let mut left = self.parse_not(tokens, pos, depth)?;

        while *pos < tokens.len() {
            if matches!(tokens[*pos], Token::And) {
                *pos += 1;
                let right = self.parse_not(tokens, pos, depth)?;
                left = ConditionAst::And {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_not(&self, tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ConditionAst> {
        if depth > MAX_CONDITION_DEPTH {
            return Err(ObserverError::InvalidCondition {
                reason: format!(
                    "Condition expression exceeds maximum nesting depth \
                     ({MAX_CONDITION_DEPTH})"
                ),
            });
        }

        if *pos < tokens.len() && matches!(tokens[*pos], Token::Not) {
            *pos += 1;
            let expr = self.parse_not(tokens, pos, depth + 1)?;
            return Ok(ConditionAst::Not {
                expr: Box::new(expr),
            });
        }

        self.parse_primary(tokens, pos, depth)
    }

    fn parse_primary(
        &self,
        tokens: &[Token],
        pos: &mut usize,
        depth: usize,
    ) -> Result<ConditionAst> {
        if *pos >= tokens.len() {
            return Err(ObserverError::InvalidCondition {
                reason: "Unexpected end of condition".to_string(),
            });
        }

        match &tokens[*pos] {
            Token::LParen => {
                *pos += 1;
                let ast = self.parse_or(tokens, pos, depth + 1)?;
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
                    op: op.clone(),
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

    /// Parse a function call into a [`ConditionAst`] node.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidCondition`] if `name` is not a recognised
    /// built-in function or the argument count is wrong.
    #[allow(clippy::unused_self)] // Reason: method is part of a public API / trait consistency
    pub(super) fn parse_function(&self, name: &str, args: &[String]) -> Result<ConditionAst> {
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
}
