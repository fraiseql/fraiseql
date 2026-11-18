//! Lexer for the condition DSL — tokenises a condition string into `Token` values.

use serde_json::Value;

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
                        let args = function_arguments(&ident, &mut chars)?;

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

                            let (value, written) = if chars.peek() == Some(&'\'') {
                                chars.next(); // consume opening quote
                                let inner = quoted(&mut chars).ok_or_else(|| {
                                    ObserverError::InvalidCondition {
                                        reason: format!(
                                            "the quoted value after `{ident} {op}` is never \
                                             closed: add the closing `'`"
                                        ),
                                    }
                                })?;
                                // #843: preserve quotedness. A quoted literal is
                                // a STRING — encode it as a JSON string so the
                                // evaluator's `serde_json::from_str` cannot
                                // reinterpret `'100'` as the number 100 (which
                                // made `code == '100'` silently false for a
                                // string field, and would make it silently true
                                // for a numeric one).
                                (Value::String(inner.clone()).to_string(), format!("'{inner}'"))
                            } else {
                                // Extract number or identifier
                                let mut raw = String::new();
                                while let Some(&c) = chars.peek() {
                                    if c.is_alphanumeric() || c == '.' || c == '-' {
                                        raw.push(c);
                                        chars.next();
                                    } else {
                                        break;
                                    }
                                }
                                let value =
                                    unquoted_operand(&ident, &op, &raw, chars.peek().copied())?;
                                (value, raw)
                            };
                            ordered_operand(&ident, &op, &value, &written)?;

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

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Read a quoted string whose opening quote is already consumed, through its
/// closing quote. `None` when the input ends first.
fn quoted(chars: &mut Chars<'_>) -> Option<String> {
    let mut inner = String::new();
    for c in chars.by_ref() {
        if c == '\'' {
            return Some(inner);
        }
        inner.push(c);
    }
    None
}

/// Check the unquoted operand of `field op`, as collected by the lexer.
///
/// It has to be a JSON number, `true`, `false` or `null`. Anything else (`-`,
/// `1.2.3`, `01`, a bare word) parsed, then failed or compared as text on every
/// event. `next` is the character the operand stopped at, named when nothing
/// was collected.
fn unquoted_operand(field: &str, op: &str, raw: &str, next: Option<char>) -> Result<String> {
    if raw.is_empty() {
        let found = next.map_or_else(
            || "has nothing after the operator".to_string(),
            |c| format!("is followed by `{c}`, which starts no value"),
        );
        return Err(ObserverError::InvalidCondition {
            reason: format!(
                "`{field} {op}` {found}: write the value to compare against, such as `{field} \
                 {op} 0` or `{field} {op} 'text'`"
            ),
        });
    }
    let literal = serde_json::from_str::<Value>(raw)
        .is_ok_and(|v| v.is_number() || v.is_boolean() || v.is_null());
    if literal {
        return Ok(raw.to_string());
    }
    Err(ObserverError::InvalidCondition {
        reason: format!(
            "`{field} {op} {raw}`: `{raw}` is not a number, `true`, `false` or `null`. Quote it \
             to compare as text: `{field} {op} '{raw}'`"
        ),
    })
}

/// Refuse an ordered operator whose operand is not a number: the evaluator
/// orders numbers only, so such a comparison failed on every event.
///
/// `value` is the operand as the evaluator reads it, `written` as it was typed.
fn ordered_operand(field: &str, op: &str, value: &str, written: &str) -> Result<()> {
    let ordered = matches!(op, ">" | "<" | ">=" | "<=");
    if !ordered || serde_json::from_str::<Value>(value).is_ok_and(|v| v.is_number()) {
        return Ok(());
    }
    Err(ObserverError::InvalidCondition {
        reason: format!(
            "`{field} {op} {written}`: `{op}` orders numbers, and `{written}` is not one. Write \
             a number such as `{field} {op} 10`, or compare with `==` or `!=`"
        ),
    })
}

/// Read the arguments of the call `name(`, whose opening parenthesis is already
/// consumed, through its closing parenthesis: quoted strings separated by
/// commas.
fn function_arguments(name: &str, chars: &mut Chars<'_>) -> Result<Vec<String>> {
    let refuse = |reason: String| Err(ObserverError::InvalidCondition { reason });
    let skip_whitespace = |chars: &mut Chars<'_>| {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    };
    let mut args = Vec::new();

    skip_whitespace(chars);
    if chars.peek() == Some(&')') {
        chars.next();
        return Ok(args);
    }
    loop {
        skip_whitespace(chars);
        match chars.next() {
            Some('\'') => {},
            Some(other) => {
                return refuse(format!(
                    "`{name}(` takes quoted arguments, as in `{name}('field')`: found `{other}`"
                ));
            },
            None => return refuse(format!("`{name}(` is never closed: add the closing `)`")),
        }
        let Some(arg) = quoted(chars) else {
            return refuse(format!(
                "a quoted argument of `{name}(` is never closed: add the closing `'`"
            ));
        };
        if args.len() >= MAX_CONDITION_FUNCTION_ARGS {
            return refuse(format!(
                "Too many function arguments (max {MAX_CONDITION_FUNCTION_ARGS})"
            ));
        }
        args.push(arg);

        skip_whitespace(chars);
        match chars.next() {
            Some(',') => {},
            Some(')') => return Ok(args),
            Some(other) => {
                return refuse(format!(
                    "`{name}(` expects `,` or `)` after an argument: found `{other}`"
                ));
            },
            None => return refuse(format!("`{name}(` is never closed: add the closing `)`")),
        }
    }
}
