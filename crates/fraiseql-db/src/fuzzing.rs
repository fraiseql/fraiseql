//! Fuzz-target support: arbitrary WHERE trees, and the same tree with other operands.
//!
//! Compiled only under `cfg(fuzzing)`, which cargo-fuzz sets; it is not part of the API.
//!
//! [`WhereClause`]'s [`Arbitrary`] builds `Field`, `NativeField`, `And`, `Or`, `Not` and
//! `Typed` nodes, up to [`MAX_DEPTH`] deep. It does not build `InHierarchy`, `Guarded`,
//! `Localized`, `Unaccented` or `KeyIn`: each names tables, views, collations or functions
//! that only a compiled schema supplies, and a made-up one tests the fixture, not the
//! generator. Keys are drawn mostly from a small vocabulary, so trees reuse fields and
//! `Typed` annotations apply, and otherwise are any string. A native column's cast comes
//! from the casts the compiler emits: it is schema data, never client input.

use std::sync::Arc;

use arbitrary::{Arbitrary, Result, Unstructured};
use serde_json::{Map, Number, Value};

use crate::{
    WhereClause, WhereOperator, types::sql_hints::ScalarFieldType, where_clause::FieldTypeMap,
};

/// How deep a generated tree nests connectives and annotations.
pub const MAX_DEPTH: usize = 4;

const KEYS: &[&str] = &["a", "n", "name", "tags", "id", "created_at", "price"];
const CASTS: &[&str] = &["", "uuid", "int4", "int8", "text", "numeric", "timestamptz"];

fn key(u: &mut Unstructured<'_>) -> Result<String> {
    if u.ratio(3, 4)? {
        Ok((*u.choose(KEYS)?).to_string())
    } else {
        String::arbitrary(u)
    }
}

fn path(u: &mut Unstructured<'_>) -> Result<Vec<String>> {
    let len = u.int_in_range(1..=3)?;
    (0..len).map(|_| key(u)).collect()
}

fn number(u: &mut Unstructured<'_>) -> Result<Value> {
    Ok(match u.int_in_range(0..=2)? {
        0 => Value::from(i64::arbitrary(u)?),
        1 => Value::from(u64::arbitrary(u)?),
        _ => Number::from_f64(f64::arbitrary(u)?).map_or(Value::Null, Value::Number),
    })
}

/// Any JSON value, nesting arrays and objects at most `depth` deep.
fn json(u: &mut Unstructured<'_>, depth: usize) -> Result<Value> {
    let kinds = if depth == 0 { 3 } else { 5 };
    Ok(match u.int_in_range(0..=kinds)? {
        0 => Value::Null,
        1 => Value::Bool(bool::arbitrary(u)?),
        2 => number(u)?,
        3 => Value::String(String::arbitrary(u)?),
        4 => Value::Array(
            (0..u.int_in_range(0..=4)?).map(|_| json(u, depth - 1)).collect::<Result<_>>()?,
        ),
        _ => {
            let mut map = Map::new();
            for _ in 0..u.int_in_range(0..=3)? {
                map.insert(key(u)?, json(u, depth - 1)?);
            }
            Value::Object(map)
        },
    })
}

fn clause(u: &mut Unstructured<'_>, depth: usize) -> Result<WhereClause> {
    let kinds = if depth == 0 { 1 } else { 5 };
    Ok(match u.int_in_range(0..=kinds)? {
        0 => WhereClause::Field {
            path:     path(u)?,
            operator: WhereOperator::arbitrary(u)?,
            value:    json(u, 2)?,
        },
        1 => WhereClause::NativeField {
            column:   key(u)?,
            pg_cast:  (*u.choose(CASTS)?).to_string(),
            operator: WhereOperator::arbitrary(u)?,
            value:    json(u, 2)?,
        },
        2 => WhereClause::And(children(u, depth)?),
        3 => WhereClause::Or(children(u, depth)?),
        4 => WhereClause::Not(Box::new(clause(u, depth - 1)?)),
        _ => {
            let mut pairs = Vec::new();
            for _ in 0..u.int_in_range(0..=4)? {
                pairs.push(((*u.choose(KEYS)?).to_string(), ScalarFieldType::arbitrary(u)?));
            }
            WhereClause::Typed {
                types: Arc::new(FieldTypeMap::from_pairs(pairs)),
                inner: Box::new(clause(u, depth - 1)?),
            }
        },
    })
}

fn children(u: &mut Unstructured<'_>, depth: usize) -> Result<Vec<WhereClause>> {
    (0..u.int_in_range(0..=4)?).map(|_| clause(u, depth - 1)).collect()
}

impl<'a> Arbitrary<'a> for WhereClause {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        clause(u, MAX_DEPTH)
    }
}

/// Operand values to substitute into a tree; see [`revalue`].
#[derive(Debug, Clone, Arbitrary)]
pub struct Operands {
    /// Strings, used in turn.
    pub strings: Vec<String>,
    /// Numbers, used in turn; a non-finite one stands as `0`.
    pub numbers: Vec<f64>,
}

impl Operands {
    fn string(&self, next: &mut usize) -> Option<Value> {
        *next += 1;
        (!self.strings.is_empty())
            .then(|| Value::String(self.strings[*next % self.strings.len()].clone()))
    }

    fn number(&self, next: &mut usize) -> Option<Value> {
        *next += 1;
        (!self.numbers.is_empty()).then(|| {
            let n = self.numbers[*next % self.numbers.len()];
            Number::from_f64(if n.is_finite() { n } else { 0.0 }).map_or(Value::Null, Value::Number)
        })
    }
}

fn value(v: &Value, operands: &Operands, next: &mut usize) -> Value {
    match v {
        Value::String(_) => operands.string(next).unwrap_or_else(|| v.clone()),
        // An integer stays an integer: a float in its place changes its shape.
        Value::Number(n) if n.is_f64() => operands.number(next).unwrap_or_else(|| v.clone()),
        Value::Number(n) => {
            n.as_i64().map_or_else(|| v.clone(), |i| Value::from(i.wrapping_add(1)))
        },
        Value::Bool(b) => Value::Bool(!b),
        Value::Array(items) => {
            Value::Array(items.iter().map(|i| value(i, operands, next)).collect())
        },
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, i)| (k.clone(), value(i, operands, next))).collect())
        },
        Value::Null => Value::Null,
    }
}

/// Whether the operator's operand is the test's sense (`true` or `false`), not a value:
/// the null tests and the network classifications.
const fn operand_is_its_sense(operator: &WhereOperator) -> bool {
    matches!(
        operator,
        WhereOperator::IsNull
            | WhereOperator::IsNotNull
            | WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsLoopback
            | WhereOperator::IsMulticast
            | WhereOperator::IsLinkLocal
            | WhereOperator::IsDocumentation
            | WhereOperator::IsCarrierGrade
    )
}

/// `clause` with every operand value replaced by one of the same JSON shape.
///
/// A string by a string, a float by a float, an integer by its successor, a boolean by
/// its negation, arrays and objects element by element with their lengths and keys kept.
/// A test whose operand is its sense (a null test, a network classification) keeps it;
/// so are keys, operators and annotations kept.
#[must_use]
pub fn revalue(clause: &WhereClause, operands: &Operands) -> WhereClause {
    fn walk(c: &WhereClause, o: &Operands, next: &mut usize) -> WhereClause {
        match c {
            WhereClause::Field {
                path,
                operator,
                value: v,
            } => WhereClause::Field {
                path:     path.clone(),
                operator: operator.clone(),
                value:    if operand_is_its_sense(operator) {
                    v.clone()
                } else {
                    value(v, o, next)
                },
            },
            WhereClause::NativeField {
                column,
                pg_cast,
                operator,
                value: v,
            } => WhereClause::NativeField {
                column:   column.clone(),
                pg_cast:  pg_cast.clone(),
                operator: operator.clone(),
                value:    if operand_is_its_sense(operator) {
                    v.clone()
                } else {
                    value(v, o, next)
                },
            },
            WhereClause::And(cs) => WhereClause::And(cs.iter().map(|c| walk(c, o, next)).collect()),
            WhereClause::Or(cs) => WhereClause::Or(cs.iter().map(|c| walk(c, o, next)).collect()),
            WhereClause::Not(c) => WhereClause::Not(Box::new(walk(c, o, next))),
            WhereClause::Typed { types, inner } => WhereClause::Typed {
                types: Arc::clone(types),
                inner: Box::new(walk(inner, o, next)),
            },
            other => other.clone(),
        }
    }
    walk(clause, operands, &mut 0)
}

/// Whether `sql`'s placeholders outside quoted literals and identifiers are exactly `$1`
/// to `$params`, each at least once.
#[must_use]
pub fn placeholders_are_one_to(sql: &str, params: usize) -> bool {
    let mut seen = vec![false; params];
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                // A doubled quote stays inside the literal.
                while let Some(d) = chars.next() {
                    if d == c {
                        if chars.peek() == Some(&c) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            },
            '$' => {
                let mut digits = String::new();
                while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(*d);
                    chars.next();
                }
                match digits.parse::<usize>() {
                    Ok(n) if (1..=params).contains(&n) => seen[n - 1] = true,
                    Ok(_) => return false,
                    Err(_) => {},
                }
            },
            _ => {},
        }
    }
    seen.iter().all(|s| *s)
}
