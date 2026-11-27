//! The reference evaluator (O3), written from `docs/filter-operator-semantics.md` and
//! from nothing else.
//!
//! It shares no code with `fraiseql-db` beyond the `WhereClause` and `WhereOperator` types
//! it reads: declared types, operand admission, comparison, case folding, `LIKE` matching
//! and array containment are all its own. A generator that answers differently from it is
//! wrong, unless the table is.

use std::cmp::Ordering;

use fraiseql_db::{WhereClause, WhereOperator};
use serde_json::Value;

/// A field's declared type, as the table's "The comparison type" section names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    Text,
    Int,
    Float,
    Boolean,
    Date,
}

/// The reference's answer for one filter over the fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The ids of the rows the filter is true on, in ascending order.
    Rows(Vec<i64>),
    /// The filter is refused before SQL.
    Refused,
    /// The table does not say: an operator it does not model, or a stored value outside
    /// the view's contract. The case is not judged.
    Unjudged,
}

/// Three-valued truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    const fn of(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }

    const fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

/// Neither refused nor true/false: the table does not cover this case.
struct Unmodelled;

/// The comparison type of one leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Text,
    Number,
    Boolean,
    Date,
}

/// Evaluate `clause` over `rows`. `declared` names each field's declared type; it applies
/// only when the clause is wrapped in `WhereClause::Typed`.
pub fn evaluate(
    clause: &WhereClause,
    rows: &[Value],
    declared: fn(&str) -> Option<Declared>,
) -> Verdict {
    let (inner, decl): (&WhereClause, Option<fn(&str) -> Option<Declared>>) = match clause {
        WhereClause::Typed { inner, .. } => (inner, Some(declared)),
        other => (other, None),
    };
    match admit(inner, decl) {
        Ok(true) => {},
        Ok(false) => return Verdict::Refused,
        Err(Unmodelled) => return Verdict::Unjudged,
    }
    let mut ids = Vec::new();
    for row in rows {
        match truth(inner, row, decl) {
            Ok(Truth::True) => ids.push(row["id"].as_i64().expect("every fixture row has an id")),
            Ok(_) => {},
            Err(Unmodelled) => return Verdict::Unjudged,
        }
    }
    ids.sort_unstable();
    Verdict::Rows(ids)
}

// ── Admission: what is refused before SQL ────────────────────────────────────

/// `Ok(false)` when some leaf is refused, `Err` when some leaf is not modelled.
fn admit(
    clause: &WhereClause,
    decl: Option<fn(&str) -> Option<Declared>>,
) -> Result<bool, Unmodelled> {
    match clause {
        WhereClause::And(cs) | WhereClause::Or(cs) => {
            let mut ok = true;
            for c in cs {
                ok &= admit(c, decl)?;
            }
            Ok(ok)
        },
        WhereClause::Not(c) => admit(c, decl),
        WhereClause::Field {
            path,
            operator,
            value,
        } => admit_leaf(&field(path)?, operator, value, decl),
        _ => Err(Unmodelled),
    }
}

fn field(path: &[String]) -> Result<String, Unmodelled> {
    match path {
        [one] => Ok(one.clone()),
        _ => Err(Unmodelled),
    }
}

fn admit_leaf(
    field: &str,
    op: &WhereOperator,
    o: &Value,
    decl: Option<fn(&str) -> Option<Declared>>,
) -> Result<bool, Unmodelled> {
    let declared = decl.and_then(|d| d(field));
    Ok(match op {
        WhereOperator::Eq
        | WhereOperator::Neq
        | WhereOperator::Gt
        | WhereOperator::Gte
        | WhereOperator::Lt
        | WhereOperator::Lte => comparison_type(declared, std::slice::from_ref(o))
            .is_some_and(|cmp| operand_fits(cmp, declared, o)),
        WhereOperator::In | WhereOperator::Nin => match o.as_array() {
            Some(items) if items.is_empty() => true,
            Some(items) => comparison_type(declared, items)
                .is_some_and(|cmp| items.iter().all(|i| operand_fits(cmp, declared, i))),
            None => false,
        },
        WhereOperator::IsNull | WhereOperator::IsNotNull => o.is_boolean() || o.is_null(),
        WhereOperator::Contains
        | WhereOperator::Icontains
        | WhereOperator::Startswith
        | WhereOperator::Istartswith
        | WhereOperator::Endswith
        | WhereOperator::Iendswith => o.as_str().is_some_and(text_fits),
        WhereOperator::Like
        | WhereOperator::Ilike
        | WhereOperator::Nlike
        | WhereOperator::Nilike => {
            o.as_str().is_some_and(|p| text_fits(p) && !ends_in_lone_escape(p))
        },
        WhereOperator::ArrayContains
        | WhereOperator::ArrayContainedBy
        | WhereOperator::ArrayOverlaps => o
            .as_array()
            .is_some_and(|items| items.iter().all(|i| !i.is_object() && !i.is_array())),
        _ => return Err(Unmodelled),
    })
}

/// A text operand may not contain NUL.
fn text_fits(s: &str) -> bool {
    !s.contains('\0')
}

fn ends_in_lone_escape(p: &str) -> bool {
    let trailing = p.chars().rev().take_while(|c| *c == '\\').count();
    trailing % 2 == 1
}

/// The declared type, else the operands' shared JSON shape (a mix is text). `None` when an
/// operand is `null`: no comparison type admits it.
fn comparison_type(declared: Option<Declared>, operands: &[Value]) -> Option<Cmp> {
    if operands.iter().any(Value::is_null) {
        return None;
    }
    if let Some(d) = declared {
        return Some(match d {
            Declared::Text => Cmp::Text,
            Declared::Int | Declared::Float => Cmp::Number,
            Declared::Boolean => Cmp::Boolean,
            Declared::Date => Cmp::Date,
        });
    }
    let shape = |v: &Value| {
        if v.is_number() {
            Cmp::Number
        } else if v.is_boolean() {
            Cmp::Boolean
        } else {
            Cmp::Text
        }
    };
    let first = shape(&operands[0]);
    Some(if operands.iter().all(|v| shape(v) == first) {
        first
    } else {
        Cmp::Text
    })
}

/// Whether `o` is readable as the comparison type. Without a declaration the type came
/// from the operand's shape, so only the text rule (no NUL) can refuse it.
fn operand_fits(cmp: Cmp, declared: Option<Declared>, o: &Value) -> bool {
    match declared {
        None => o.as_str().is_none_or(text_fits),
        Some(Declared::Int) => o.is_i64(),
        Some(Declared::Float) => o.is_number(),
        Some(Declared::Boolean) => o.is_boolean(),
        Some(Declared::Date) => o.as_str().is_some_and(is_date),
        Some(Declared::Text) => cmp == Cmp::Text && o.as_str().is_some_and(text_fits),
    }
}

/// `YYYY-MM-DD`, a real calendar date.
fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| text[r].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

// ── Truth over one row ───────────────────────────────────────────────────────

fn truth(
    clause: &WhereClause,
    row: &Value,
    decl: Option<fn(&str) -> Option<Declared>>,
) -> Result<Truth, Unmodelled> {
    match clause {
        WhereClause::And(cs) => {
            let mut acc = Truth::True;
            for c in cs {
                match truth(c, row, decl)? {
                    Truth::False => acc = Truth::False,
                    Truth::Unknown if acc == Truth::True => acc = Truth::Unknown,
                    _ => {},
                }
            }
            Ok(acc)
        },
        WhereClause::Or(cs) => {
            let mut acc = Truth::False;
            for c in cs {
                match truth(c, row, decl)? {
                    Truth::True => acc = Truth::True,
                    Truth::Unknown if acc == Truth::False => acc = Truth::Unknown,
                    _ => {},
                }
            }
            Ok(acc)
        },
        WhereClause::Not(c) => Ok(truth(c, row, decl)?.not()),
        WhereClause::Field {
            path,
            operator,
            value,
        } => {
            let name = field(path)?;
            leaf(
                operator,
                value,
                row.get(&name).filter(|v| !v.is_null()),
                decl.and_then(|d| d(&name)),
            )
        },
        _ => Err(Unmodelled),
    }
}

/// One leaf over one row's value `v` (`None` when null or absent).
fn leaf(
    op: &WhereOperator,
    operand: &Value,
    value: Option<&Value>,
    declared: Option<Declared>,
) -> Result<Truth, Unmodelled> {
    if matches!(op, WhereOperator::IsNull | WhereOperator::IsNotNull) {
        let asserted = operand.as_bool().unwrap_or(true);
        let is_null = value.is_none();
        let text = Truth::of(is_null == asserted);
        return Ok(if *op == WhereOperator::IsNotNull {
            text.not()
        } else {
            text
        });
    }
    // The table: `in: []` is false for every row, null and absent included.
    if *op == WhereOperator::In && operand.as_array().is_some_and(Vec::is_empty) {
        return Ok(Truth::False);
    }
    let Some(value) = value else {
        return Ok(Truth::Unknown);
    };
    Ok(match op {
        WhereOperator::Eq
        | WhereOperator::Neq
        | WhereOperator::Gt
        | WhereOperator::Gte
        | WhereOperator::Lt
        | WhereOperator::Lte => {
            let cmp = comparison_type(declared, std::slice::from_ref(operand)).ok_or(Unmodelled)?;
            let ord = compare(cmp, value, operand)?;
            Truth::of(match op {
                WhereOperator::Eq => ord == Ordering::Equal,
                WhereOperator::Neq => ord != Ordering::Equal,
                WhereOperator::Gt => ord == Ordering::Greater,
                WhereOperator::Gte => ord != Ordering::Less,
                WhereOperator::Lt => ord == Ordering::Less,
                _ => ord != Ordering::Greater,
            })
        },
        WhereOperator::In | WhereOperator::Nin => {
            let items = operand.as_array().ok_or(Unmodelled)?;
            let mut found = false;
            if !items.is_empty() {
                let cmp = comparison_type(declared, items).ok_or(Unmodelled)?;
                for i in items {
                    found |= compare(cmp, value, i)? == Ordering::Equal;
                }
            }
            Truth::of(found == (*op == WhereOperator::In))
        },
        WhereOperator::Contains
        | WhereOperator::Icontains
        | WhereOperator::Startswith
        | WhereOperator::Istartswith
        | WhereOperator::Endswith
        | WhereOperator::Iendswith => {
            let fold = matches!(
                op,
                WhereOperator::Icontains | WhereOperator::Istartswith | WhereOperator::Iendswith
            );
            let (text, pattern) = (
                folded(&as_text(value)?, fold),
                folded(operand.as_str().ok_or(Unmodelled)?, fold),
            );
            Truth::of(match op {
                WhereOperator::Contains | WhereOperator::Icontains => text.contains(&pattern),
                WhereOperator::Startswith | WhereOperator::Istartswith => {
                    text.starts_with(&pattern)
                },
                _ => text.ends_with(&pattern),
            })
        },
        WhereOperator::Like
        | WhereOperator::Ilike
        | WhereOperator::Nlike
        | WhereOperator::Nilike => {
            let fold = matches!(op, WhereOperator::Ilike | WhereOperator::Nilike);
            let text: Vec<char> = folded(&as_text(value)?, fold).chars().collect();
            let pattern: Vec<char> =
                folded(operand.as_str().ok_or(Unmodelled)?, fold).chars().collect();
            let m = like(&text, &pattern);
            Truth::of(if matches!(op, WhereOperator::Like | WhereOperator::Ilike) {
                m
            } else {
                !m
            })
        },
        WhereOperator::ArrayContains
        | WhereOperator::ArrayContainedBy
        | WhereOperator::ArrayOverlaps => {
            let field = value.as_array().ok_or(Unmodelled)?;
            if field.iter().any(|e| e.is_array() || e.is_object()) {
                return Err(Unmodelled);
            }
            let operand = operand.as_array().ok_or(Unmodelled)?;
            let has = |set: &[Value], e: &Value| set.iter().any(|s| json_eq(s, e));
            Truth::of(match op {
                WhereOperator::ArrayContains => operand.iter().all(|e| has(field, e)),
                WhereOperator::ArrayContainedBy => field.iter().all(|e| has(operand, e)),
                _ => operand.iter().any(|e| has(field, e)),
            })
        },
        _ => return Err(Unmodelled),
    })
}

/// A present value read as text: a string's contents, else its JSON text. A non-integer
/// number is not judged: its stored text (`19.90`) is not recoverable from the parsed
/// value (`19.9`).
fn as_text(v: &Value) -> Result<String, Unmodelled> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) if n.is_i64() || n.is_u64() => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(Unmodelled),
    }
}

/// `v` against `o` in the comparison type. A stored value outside it is not judged.
fn compare(cmp: Cmp, v: &Value, o: &Value) -> Result<Ordering, Unmodelled> {
    match cmp {
        // Code-point order: Rust orders `str` by its UTF-8 bytes, which is code-point order.
        Cmp::Text => Ok(as_text(v)?.as_str().cmp(&as_text(o)?)),
        Cmp::Number => num_cmp(v, o).ok_or(Unmodelled),
        Cmp::Boolean => Ok(v.as_bool().ok_or(Unmodelled)?.cmp(&o.as_bool().ok_or(Unmodelled)?)),
        // ISO dates order as text.
        Cmp::Date => {
            let (a, b) = (v.as_str().ok_or(Unmodelled)?, o.as_str().ok_or(Unmodelled)?);
            if is_date(a) && is_date(b) {
                Ok(a.cmp(b))
            } else {
                Err(Unmodelled)
            }
        },
    }
}

/// Exact for two integers; otherwise by `f64`, which is exact for every non-integer the
/// fixture and the operand pools hold.
fn num_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    if let (Some(x), Some(y)) = (a.as_i64(), b.as_i64()) {
        return Some(x.cmp(&y));
    }
    a.as_f64()?.partial_cmp(&b.as_f64()?)
}

/// JSON value equality, with `1` equal to `1.0`.
fn json_eq(a: &Value, b: &Value) -> bool {
    if a.is_number() && b.is_number() {
        num_cmp(a, b) == Some(Ordering::Equal)
    } else {
        a == b
    }
}

/// Unicode simple case mapping: a character whose lower case is one character maps to it;
/// one whose lower case is several (full mapping only) is kept.
fn folded(s: &str, fold: bool) -> String {
    if !fold {
        return s.to_string();
    }
    s.chars()
        .map(|c| {
            let mut l = c.to_lowercase();
            match (l.next(), l.next()) {
                (Some(one), None) => one,
                _ => c,
            }
        })
        .collect()
}

/// SQL `LIKE`: `%` any run, `_` one character, `\` makes the next character literal. The
/// whole of `t` must match.
fn like(t: &[char], p: &[char]) -> bool {
    match p.split_first() {
        None => t.is_empty(),
        Some(('%', rest)) => (0..=t.len()).any(|i| like(&t[i..], rest)),
        Some(('_', rest)) => !t.is_empty() && like(&t[1..], rest),
        Some(('\\', rest)) => match rest.split_first() {
            Some((lit, rest)) => t.first() == Some(lit) && like(&t[1..], rest),
            None => false,
        },
        Some((c, rest)) => t.first() == Some(c) && like(&t[1..], rest),
    }
}
