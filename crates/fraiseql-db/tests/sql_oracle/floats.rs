//! O5: a `Float` operand selects, and a stored `Float` serves, the value its text names.
//!
//! Every value here is a double whose shortest decimal text names it exactly, and is
//! stored as `float8` in a JSON document, as a view builds one.
//!
//! - **filter**: a typed `Float` leaf compares as an exact decimal (see
//!   `docs/filter-operator-semantics.md`), so its rows are those whose stored double compares so
//!   with the operand's. The operand is read from its text the way a request body is
//!   (`serde_json`), and, as a control, exactly.
//! - **served**: each stored document, read alone, serves the double it stores, bit for bit.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_db::{
    ScalarFieldType, WhereClause, WhereOperator,
    traits::DatabaseAdapter,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use serde_json::{Value, json};

use super::{baseline_path, gate, rig};

const DATABASE: &str = "sql_oracle_floats";
const FILTER_TABLE: &str = "sql_oracle_float_filter";
const SERVED_TABLE: &str = "sql_oracle_float_served";

/// Doubles a `serde_json` built without `float_roundtrip` reads back one step off from
/// their shortest text, their neighbours, and doubles it reads exactly.
const VALUES: &[&str] = &[
    "1.7976931348623155e308",
    "1.7976931348623153e308",
    "-2.0625905461646524e225",
    "1.0715660391465826e-75",
    "-1.81996730402717e-179",
    "-1.603964615428183e143",
    "3.0261999441573203e-52",
    "3.909076791246025e-46",
    "2.2250738585072014e-308",
    "0.30000000000000004",
    "1e300",
    "5e-324",
    "0.5",
];

/// The largest double: an operand, and a stored value only in the served table.
const MAX: &str = "1.7976931348623157e308";

fn table(name: &str, values: &[&str]) -> Vec<String> {
    let rows: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("({i}, jsonb_build_object('id', {i}, 'r', '{v}'::float8))"))
        .collect();
    vec![
        format!("DROP TABLE IF EXISTS {name}"),
        format!("CREATE TABLE {name} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!("INSERT INTO {name} (id, data) VALUES {}", rows.join(", ")),
    ]
}

fn exact(text: &str) -> f64 {
    text.parse().expect("a double's text")
}

fn float_types() -> SharedFieldTypes {
    Arc::new(FieldTypeMap::from_pairs([("r", ScalarFieldType::Numeric)]))
}

fn ids(rows: &[fraiseql_db::JsonbValue]) -> Vec<i64> {
    let mut out: Vec<i64> =
        rows.iter().map(|r| r.as_value()["id"].as_i64().expect("an id")).collect();
    out.sort_unstable();
    out
}

#[tokio::test]
async fn float_operands_select_and_stored_floats_serve_the_value_their_text_names() {
    let mut fixture = table(FILTER_TABLE, VALUES);
    let served_values: Vec<&str> = VALUES.iter().copied().chain([MAX]).collect();
    fixture.extend(table(SERVED_TABLE, &served_values));
    let Some(adapter) = rig(DATABASE, fixture).await else {
        eprintln!("skip: no PostgreSQL bound (DATABASE_URL unset)");
        return;
    };
    let mut report = BTreeMap::new();
    let mut cases = 0_usize;

    let operators: [(&str, WhereOperator, fn(f64, f64) -> bool); 5] = [
        // Exact equality is the contract: a margin is the defect this oracle looks for.
        ("eq", WhereOperator::Eq, |v, o| v.total_cmp(&o).is_eq()),
        ("gt", WhereOperator::Gt, |v, o| v > o),
        ("gte", WhereOperator::Gte, |v, o| v >= o),
        ("lt", WhereOperator::Lt, |v, o| v < o),
        ("lte", WhereOperator::Lte, |v, o| v <= o),
    ];
    for text in VALUES.iter().copied().chain([MAX]) {
        let o = exact(text);
        let request: Value = serde_json::from_str(text).expect("a JSON number");
        let readings = [("request", request), ("exact", json!(o))];
        for (name, operator, holds) in &operators {
            let want: Vec<i64> = (0_i64..)
                .zip(VALUES)
                .filter(|(_, v)| holds(exact(v), o))
                .map(|(i, _)| i)
                .collect();
            for (via, operand) in &readings {
                cases += 1;
                let leaf = WhereClause::Typed {
                    types: float_types(),
                    inner: Box::new(WhereClause::Field {
                        path:     vec!["r".to_string()],
                        operator: operator.clone(),
                        value:    operand.clone(),
                    }),
                };
                let got = adapter
                    .execute_where_query(FILTER_TABLE, Some(&leaf), None, None, None)
                    .await
                    .map(|rows| ids(&rows));
                let verdict = match got {
                    Ok(got) if got == want => None,
                    Ok(got) => Some(format!("rows {got:?}, exact comparison selects {want:?}")),
                    Err(e) => Some(format!("refused: {e}")),
                };
                if let Some(v) = verdict {
                    report.insert(format!("O5:filter:{via}:{name}:{text}"), v);
                }
            }
        }
    }

    for (i, text) in (0_i64..).zip(&served_values) {
        cases += 1;
        let one = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(i),
        };
        let got = adapter.execute_where_query(SERVED_TABLE, Some(&one), None, None, None).await;
        let verdict = match got.as_deref() {
            Ok([row]) => match row.as_value()["r"].as_f64() {
                Some(served) if served.to_bits() == exact(text).to_bits() => None,
                Some(served) => Some(format!("served {served:e}")),
                None => Some(format!("served {}", row.as_value()["r"])),
            },
            Ok(rows) => Some(format!("{} rows", rows.len())),
            Err(e) => Some(format!("refused: {e}")),
        };
        if let Some(v) = verdict {
            report.insert(format!("O5:served:{text}"), v);
        }
    }

    eprintln!("sql oracle floats: cases = {cases}, entries = {}", report.len());
    gate(&baseline_path("sql_oracle_floats_baseline.json"), &report);
}
