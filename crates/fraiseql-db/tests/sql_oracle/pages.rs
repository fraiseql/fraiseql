//! O4: pagination is consistent.
//!
//! Each ordering × filter × page size is read five ways, and every sequence must equal the
//! first:
//!
//! 1. **reference**: the fixture sorted here, from the ordering contract in
//!    `docs/features/pagination.md` (the keys in order, each by its type and direction with NULLs
//!    last for `ASC` and first for `DESC` unless placed, then the cursor column ascending), and
//!    filtered by the O3 reference evaluator;
//! 2. **unpaged**: one read under that ordering;
//! 3. **offset**: `LIMIT`/`OFFSET` pages, concatenated;
//! 4. **keyset forward**: relay pages after each page's last row's cursor;
//! 5. **keyset backward**: relay pages before each page's first row's cursor, from the end.

use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use fraiseql_db::{
    CursorValue, NullsOrder, OrderByClause, OrderDirection, RelayCursor, RelayDatabaseAdapter,
    ScalarFieldType, WhereClause, WhereOperator,
    keyset::SORT_KEYS_KEY,
    postgres::PostgresAdapter,
    traits::DatabaseAdapter,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use serde_json::{Value, json};

use super::{
    baseline_path, gate,
    reference::{self, Declared, Verdict},
    rig,
};

const DATABASE: &str = "sql_oracle_pages";
const TABLE: &str = "sql_oracle_pages";
const PAGE_SIZES: [usize; 4] = [1, 2, 5, 100];

/// One sort key the orderings draw from: a document field and the type it sorts as.
const KEYS: &[(&str, ScalarFieldType)] = &[
    ("g", ScalarFieldType::Integer),
    ("s", ScalarFieldType::Text),
    ("t", ScalarFieldType::DateTime),
    ("x", ScalarFieldType::Numeric),
];

/// Values with ties, NULLs, absent keys, case and code-point order (`s`), equal instants
/// in different offsets (`t`), and equal decimals written differently (`x`).
fn documents() -> Vec<Value> {
    let g_values = [
        Some(3),
        Some(1),
        Some(2),
        None,
        Some(1),
        Some(3),
        Some(2),
        Some(1),
    ];
    let s_values = [
        Some("b"),
        Some("a"),
        Some("B"),
        Some("é"),
        Some(""),
        None,
        Some("a"),
        Some("日"),
    ];
    let t_values = [
        Some("2024-01-01T10:00:00+02:00"),
        Some("2024-01-01T08:00:00Z"),
        Some("2024-01-01T09:00:00Z"),
        None,
        Some("2023-12-31T23:30:00-01:00"),
        Some("2024-01-01T08:00:00Z"),
    ];
    let x_values = [
        json!(1.5),
        json!(1.50),
        json!(-2),
        Value::Null,
        json!(10),
        json!(0.25),
        json!(1.5),
    ];
    (1..=24_i64)
        .map(|id| {
            let i = usize::try_from(id).unwrap();
            let mut doc = serde_json::Map::new();
            doc.insert("id".into(), json!(id));
            // Absent, not null, on a few rows per key.
            if id % 7 != 0 {
                doc.insert(
                    "g".into(),
                    g_values[i % g_values.len()].map_or(Value::Null, |v| json!(v)),
                );
            }
            if id % 11 != 0 {
                doc.insert(
                    "s".into(),
                    s_values[i % s_values.len()].map_or(Value::Null, |v| json!(v)),
                );
            }
            doc.insert("t".into(), t_values[i % t_values.len()].map_or(Value::Null, |v| json!(v)));
            if id % 5 != 0 {
                doc.insert("x".into(), x_values[i % x_values.len()].clone());
            }
            Value::Object(doc)
        })
        .collect()
}

fn fixture(docs: &[Value]) -> Vec<String> {
    let values: Vec<String> = docs
        .iter()
        .map(|d| format!("({}, '{}'::jsonb)", d["id"], d.to_string().replace('\'', "''")))
        .collect();
    vec![
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!("INSERT INTO {TABLE} (id, data) VALUES {}", values.join(", ")),
    ]
}

/// One key of an ordering, as the oracle names and sorts it.
#[derive(Clone, Copy)]
struct Key {
    field:     &'static str,
    ty:        ScalarFieldType,
    direction: OrderDirection,
    nulls:     Option<NullsOrder>,
}

impl Key {
    fn name(self) -> String {
        let dir = match self.direction {
            OrderDirection::Asc => "asc",
            OrderDirection::Desc => "desc",
            _ => panic!("a direction this oracle does not know"),
        };
        let nulls = match self.nulls {
            None => "default",
            Some(NullsOrder::First) => "first",
            Some(NullsOrder::Last) => "last",
            Some(_) => panic!("a NULL placement this oracle does not know"),
        };
        format!("{}.{dir}.{nulls}", self.field)
    }

    fn clause(self) -> OrderByClause {
        let mut c = OrderByClause::new(self.field.to_string(), self.direction);
        c.field_type = self.ty;
        c.nulls = self.nulls;
        c
    }

    const fn nulls_last(self) -> bool {
        match self.nulls {
            Some(NullsOrder::Last) => true,
            Some(NullsOrder::First) => false,
            Some(_) => panic!("a NULL placement this oracle does not know"),
            None => matches!(self.direction, OrderDirection::Asc),
        }
    }
}

/// Every single key in every direction and placement, every ordered pair of distinct keys
/// in their default placements, and no ordering at all.
fn orderings() -> Vec<Vec<Key>> {
    let mut out = vec![Vec::new()];
    let key = |(field, ty): (&'static str, ScalarFieldType), direction, nulls| Key {
        field,
        ty,
        direction,
        nulls,
    };
    for &k in KEYS {
        for direction in [OrderDirection::Asc, OrderDirection::Desc] {
            for nulls in [None, Some(NullsOrder::First), Some(NullsOrder::Last)] {
                out.push(vec![key(k, direction, nulls)]);
            }
        }
    }
    for &a in KEYS {
        for &b in KEYS {
            if a.0 != b.0 {
                out.push(vec![
                    key(a, OrderDirection::Asc, None),
                    key(b, OrderDirection::Desc, None),
                ]);
            }
        }
    }
    out
}

fn page_types() -> SharedFieldTypes {
    Arc::new(FieldTypeMap::from_pairs([
        ("g", ScalarFieldType::Integer),
        ("s", ScalarFieldType::Text),
        ("x", ScalarFieldType::Numeric),
    ]))
}

fn reference_page_types(field: &str) -> Option<Declared> {
    Some(match field {
        "g" => Declared::Int,
        "s" => Declared::Text,
        "x" => Declared::Float,
        _ => return None,
    })
}

/// No filter, and two that drop rows including NULL ones.
fn filters() -> Vec<(&'static str, Option<WhereClause>)> {
    let typed = |field: &str, operator, value| {
        Some(WhereClause::Typed {
            types: page_types(),
            inner: Box::new(WhereClause::Field {
                path: vec![field.to_string()],
                operator,
                value,
            }),
        })
    };
    vec![
        ("all", None),
        ("g_gte_2", typed("g", WhereOperator::Gte, json!(2))),
        ("s_neq_b", typed("s", WhereOperator::Neq, json!("b"))),
    ]
}

// ── The reference order ──────────────────────────────────────────────────────

/// A key's value on a document, ready to compare; `None` when null or absent.
fn sort_value(doc: &Value, key: Key) -> Option<SortValue> {
    let v = doc.get(key.field).filter(|v| !v.is_null())?;
    Some(match key.ty {
        ScalarFieldType::Integer => SortValue::Int(v.as_i64().expect("an integer key")),
        ScalarFieldType::Numeric => SortValue::Num(v.as_f64().expect("a numeric key")),
        ScalarFieldType::DateTime => SortValue::Int(instant(v.as_str().expect("a timestamp"))),
        _ => SortValue::Text(v.as_str().expect("a text key").to_string()),
    })
}

#[derive(PartialEq, PartialOrd)]
enum SortValue {
    Int(i64),
    Num(f64),
    Text(String),
}

/// Seconds since the epoch of `YYYY-MM-DDTHH:MM:SS` followed by `Z` or `±HH:MM`.
fn instant(ts: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| ts[r].parse::<i64>().unwrap();
    let (year, month, day) = (n(0..4), n(5..7), n(8..10));
    let secs = n(11..13) * 3600 + n(14..16) * 60 + n(17..19);
    let offset = match &ts[19..] {
        "Z" => 0,
        o => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            sign * (o[1..3].parse::<i64>().unwrap() * 3600 + o[4..6].parse::<i64>().unwrap() * 60)
        },
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + secs - offset
}

fn reference_order(docs: &[Value], keys: &[Key], filter: Option<&WhereClause>) -> Vec<i64> {
    let kept: Vec<i64> = match filter {
        None => docs.iter().map(|d| d["id"].as_i64().unwrap()).collect(),
        Some(f) => match reference::evaluate(f, docs, reference_page_types) {
            Verdict::Rows(ids) => ids,
            other => panic!("the page filters are all judged rows: {other:?}"),
        },
    };
    let mut rows: Vec<&Value> =
        docs.iter().filter(|d| kept.contains(&d["id"].as_i64().unwrap())).collect();
    rows.sort_by(|a, b| {
        for &key in keys {
            let ord = match (sort_value(a, key), sort_value(b, key)) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => {
                    if key.nulls_last() {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                },
                (Some(_), None) => {
                    if key.nulls_last() {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                },
                (Some(x), Some(y)) => {
                    let o = x.partial_cmp(&y).expect("comparable sort values");
                    if matches!(key.direction, OrderDirection::Desc) {
                        o.reverse()
                    } else {
                        o
                    }
                },
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        a["id"].as_i64().cmp(&b["id"].as_i64())
    });
    rows.iter().map(|d| d["id"].as_i64().unwrap()).collect()
}

// ── The engine's reads ───────────────────────────────────────────────────────

type Read = Result<Vec<i64>, String>;

fn ordering_with_identity(keys: &[Key]) -> Vec<OrderByClause> {
    let mut clauses: Vec<OrderByClause> = keys.iter().map(|k| k.clause()).collect();
    let mut id = OrderByClause::new("id".to_string(), OrderDirection::Asc);
    id.field_type = ScalarFieldType::Integer;
    clauses.push(id);
    clauses
}

fn id_of(doc: &Value) -> i64 {
    doc["id"].as_i64().expect("every page row has an id")
}

async fn unpaged(a: &PostgresAdapter, keys: &[Key], filter: Option<&WhereClause>) -> Read {
    let order = ordering_with_identity(keys);
    let rows = a
        .execute_where_query(TABLE, filter, None, None, Some(&order))
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| id_of(r.as_value())).collect())
}

async fn offset_pages(
    a: &PostgresAdapter,
    keys: &[Key],
    filter: Option<&WhereClause>,
    size: usize,
) -> Read {
    let order = ordering_with_identity(keys);
    let limit = u32::try_from(size).unwrap();
    let mut out = Vec::new();
    for page in 0..=documents().len() {
        let offset = u32::try_from(page * size).unwrap();
        let rows = a
            .execute_where_query(TABLE, filter, Some(limit), Some(offset), Some(&order))
            .await
            .map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(out);
        }
        out.extend(rows.iter().map(|r| id_of(r.as_value())));
    }
    Err("offset pages did not end".to_string())
}

/// The cursor a page row resumes from: its position, and its sort-key values as the
/// adapter added them.
fn cursor(doc: &Value) -> Result<RelayCursor, String> {
    let sort_keys = match doc.get(SORT_KEYS_KEY) {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().map(|v| v.as_str().map(str::to_string)).collect(),
        Some(other) => return Err(format!("sort keys are not a list: {other}")),
    };
    Ok(RelayCursor {
        position: CursorValue::Int64(id_of(doc)),
        sort_keys,
    })
}

async fn keyset_pages(
    a: &PostgresAdapter,
    keys: &[Key],
    filter: Option<&WhereClause>,
    size: usize,
    forward: bool,
) -> Read {
    let order: Vec<OrderByClause> = keys.iter().map(|k| k.clause()).collect();
    let order = (!order.is_empty()).then_some(order.as_slice());
    let limit = u32::try_from(size + 1).unwrap();
    let mut out: Vec<i64> = Vec::new();
    let mut at: Option<RelayCursor> = None;
    for _ in 0..=documents().len() {
        let (after, before) = if forward {
            (at.clone(), None)
        } else {
            (None, at.clone())
        };
        let page = a
            .execute_relay_page(TABLE, "id", after, before, limit, forward, filter, order, false)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<Value> = page.into_rows().into_iter().map(|r| r.as_value().clone()).collect();
        let more = rows.len() > size;
        // A backward page's extra row, the one past the page, is its first.
        let kept: &[Value] = if forward {
            &rows[..rows.len().min(size)]
        } else {
            &rows[rows.len().saturating_sub(size)..]
        };
        let ids: Vec<i64> = kept.iter().map(id_of).collect();
        if forward {
            out.extend(ids);
        } else {
            out.splice(0..0, ids);
        }
        if !more {
            return Ok(out);
        }
        let edge = if forward { kept.last() } else { kept.first() };
        at = Some(cursor(edge.ok_or("a page with more rows has none")?)?);
    }
    Err("keyset pages did not end".to_string())
}

// ── Judging ──────────────────────────────────────────────────────────────────

/// Where `got` first departs from `want`, for the report.
fn departure(want: &[i64], got: &Read) -> Option<String> {
    match got {
        Err(e) => Some(format!("refused: {e}")),
        Ok(got) if got == want => None,
        Ok(got) => {
            let i = want.iter().zip(got).take_while(|(a, b)| a == b).count();
            Some(format!(
                "first difference at {i}: want {:?}, got {:?} (lengths {} and {})",
                want.get(i),
                got.get(i),
                want.len(),
                got.len()
            ))
        },
    }
}

#[tokio::test]
async fn offset_keyset_and_unpaged_reads_agree_with_the_ordering_contract() {
    let docs = documents();
    let Some(adapter) = rig(DATABASE, fixture(&docs)).await else {
        eprintln!("skip: no PostgreSQL bound (DATABASE_URL unset)");
        return;
    };
    let mut report = BTreeMap::new();
    let mut cases = 0_usize;
    for keys in orderings() {
        let ordering = if keys.is_empty() {
            "unordered".to_string()
        } else {
            keys.iter().map(|k| k.name()).collect::<Vec<_>>().join(",")
        };
        for (filter_name, filter) in filters() {
            let filter = filter.as_ref();
            let want = reference_order(&docs, &keys, filter);
            let mut judge = |kind: &str, size: Option<usize>, got: &Read| {
                cases += 1;
                if let Some(how) = departure(&want, got) {
                    let size = size.map_or_else(|| "-".to_string(), |s| s.to_string());
                    report.insert(format!("O4:{kind}:{ordering}|{filter_name}|{size}"), how);
                }
            };
            judge("unpaged", None, &unpaged(&adapter, &keys, filter).await);
            for size in PAGE_SIZES {
                judge("offset", Some(size), &offset_pages(&adapter, &keys, filter, size).await);
                let fwd = keyset_pages(&adapter, &keys, filter, size, true).await;
                judge("keyset_forward", Some(size), &fwd);
                let back = keyset_pages(&adapter, &keys, filter, size, false).await;
                judge("keyset_backward", Some(size), &back);
            }
        }
    }
    eprintln!("sql oracle pages: cases = {cases}, entries = {}", report.len());
    gate(&baseline_path("sql_oracle_pages_baseline.json"), &report);
}
