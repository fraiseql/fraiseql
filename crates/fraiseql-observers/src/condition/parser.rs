//! Pratt parser for the condition DSL — turns a token stream into a `ConditionAst`.

use crate::error::{ObserverError, Result};

use super::{ConditionAst, ConditionParser, Token};

impl ConditionParser {
    pub(super) fn parse_tokens(&self, tokens: &[Token]) -> Result<ConditionAst> {
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
