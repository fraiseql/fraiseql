//! SQL generation oracle, executed against a real PostgreSQL.
//!
//! Seeded `WhereClause` trees are generated over a fixture of JSON rows, and each one is
//! judged by references that share no code with the generator under test:
//!
//! * **O1, PostgreSQL accepts it.** The parameterised generator's statement is executed through
//!   `PostgresAdapter::execute_where_query`. A statement PostgreSQL refuses (syntax, an unknown
//!   cast or operator, a type error) is recorded. PostgreSQL itself is the reference.
//! * **O2, the two generators agree.** The same tree is rendered by the inlining
//!   `WhereSqlGenerator` and executed over the same rows. The row sets, or the refusals, must match
//!   the parameterised path's.
//! * **O3, the rows are the table's.** A reference evaluator (`sql_oracle/reference.rs`), written
//!   from `docs/filter-operator-semantics.md` alone, answers each case over the same documents. The
//!   parameterised path must return its rows, or refuse before SQL where it refuses. Operators the
//!   table does not model are not judged.
//!
//! * **O4, pagination is consistent** (`sql_oracle/pages.rs`). Unpaged, offset and keyset reads,
//!   forward and backward, return the order the pagination contract states.
//!
//! The fixture lives in its own database, created with PostgreSQL's builtin `C.UTF-8`
//! locale, so text order and case folding are the table's on any server.
//!
//! A result is a *row set*, never "no error": a statement that runs and returns the wrong
//! rows is a disagreement.
//!
//! # Gate
//!
//! Every O1 refusal and O2 disagreement is keyed by its oracle, its kind and a fingerprint
//! of the tree, and compared both ways with `tests/sql_oracle_baseline.json`: a new entry
//! fails, and so does an entry that no longer occurs. `SQL_ORACLE_BLESS=1` rewrites the
//! baseline. The trees are generated from a fixed seed, so the cases are the same on every
//! run.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-db --test sql_oracle_pg -- --nocapture
//! ```
//!
//! Runs in the Dagger `integration: postgres` leg via its `--test '*'` sweep.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: the oracle reports its numbers and its diff

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use fraiseql_db::{
    ScalarFieldType, WhereClause, WhereOperator, WhereSqlGenerator,
    postgres::PostgresAdapter,
    traits::DatabaseAdapter,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use fraiseql_error::FraiseQLError;
use serde_json::{Value, json};

#[path = "sql_oracle/reference.rs"]
mod reference;

#[path = "sql_oracle/pages.rs"]
mod pages;

use reference::{Declared, Verdict};

/// The oracle's own database, created with PostgreSQL's builtin `C.UTF-8` locale so that
/// text order and case folding are the table's, not the server's default locale's.
const DATABASE: &str = "sql_oracle";

/// The fixture relation. Uniquely named so it cannot collide with other suites' fixtures.
const TABLE: &str = "sql_oracle_rows";

/// The seed every tree is drawn from. Changing it changes every case: re-bless.
const SEED: u64 = 0x5eed_0f0c_a1e5_0001;

/// Trees generated per variant, over the leaves that came out clean.
const TREES: usize = 400;

/// Deepest `And` / `Or` / `Not` nesting above a leaf.
const MAX_DEPTH: u32 = 3;

/// The fixture's documents, chosen so that each value pool below matches some rows and
/// not others, with absent keys and JSON `null` on every field. Stored values conform to
/// their declared types, as the semantics table assumes. Both the table and the reference
/// evaluator read these.
const ROWS: &[&str] = &[
    r#"{"id":1,"name":"alpha","n":5,"x":19.90,"b":true,"d":"2024-01-01","tags":["rust","graphql"]}"#,
    r#"{"id":2,"name":"O'Brien","n":10,"x":24.5,"b":false,"d":"2024-06-15","tags":["python"]}"#,
    r#"{"id":3,"name":"back\\slash","n":-3,"x":100,"b":true,"d":"2024-12-31","tags":["rust","sql"]}"#,
    r#"{"id":4,"name":"50%_off","n":0,"x":0.001,"b":false,"d":"2023-02-28","tags":[]}"#,
    r#"{"id":5,"name":"Été","n":9007199254740993,"x":-7.25,"tags":["O'Brien"]}"#,
    r#"{"id":6,"name":"","n":null,"x":null,"b":null,"d":null,"tags":null}"#,
    r#"{"id":7,"name":"日本","n":7,"b":true,"d":"2024-01-01"}"#,
    r#"{"id":8}"#,
];

/// The statements that provision the fixture relation from [`ROWS`].
fn fixture() -> Vec<String> {
    let values: Vec<String> = ROWS
        .iter()
        .map(|doc| format!("('{}'::jsonb)", doc.replace('\'', "''")))
        .collect();
    vec![
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (data jsonb NOT NULL)"),
        format!("INSERT INTO {TABLE} (data) VALUES {}", values.join(", ")),
    ]
}

fn rows() -> Vec<Value> {
    ROWS.iter().map(|doc| serde_json::from_str(doc).unwrap()).collect()
}

/// The declared types as the reference evaluator reads them: the same table as
/// [`declared_types`], kept apart so the reference shares nothing with the generators.
fn reference_types(field: &str) -> Option<Declared> {
    Some(match field {
        "name" | "tags" => Declared::Text,
        "n" => Declared::Int,
        "x" => Declared::Float,
        "b" => Declared::Boolean,
        "d" => Declared::Date,
        _ => return None,
    })
}

/// Declared types, as a compiled schema would supply them.
fn declared_types() -> SharedFieldTypes {
    Arc::new(FieldTypeMap::from_pairs([
        ("name", ScalarFieldType::Text),
        ("n", ScalarFieldType::Integer),
        ("x", ScalarFieldType::Numeric),
        ("b", ScalarFieldType::Boolean),
        ("d", ScalarFieldType::Date),
        ("tags", ScalarFieldType::Text),
    ]))
}

// ── Seeded generation ─────────────────────────────────────────────────────────

/// xorshift64*: deterministic, dependency-free, good enough to spread choices.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Text values: ordinary words, SQL quoting and comment syntax, LIKE metacharacters,
/// array-literal syntax, Unicode (precomposed, combining, CJK, astral) and NUL, plus strict
/// prefixes and suffixes of fixture values, so that a prefix match and a suffix match
/// answer differently.
fn text_values() -> Vec<Value> {
    [
        "",
        "alpha",
        "Alpha",
        "O'Brien",
        "back\\slash",
        "50%_off",
        "%",
        "_",
        "a_b",
        "$$x$$",
        "$t$x$t$",
        "-- c",
        "/* c */",
        "été",
        "e\u{301}",
        "日本",
        "\u{1F600}",
        "a\"b",
        "{a,b}",
        ")",
        "NUL\u{0}x",
        "ends\\",
        "al",
        "pha",
        "O'",
        "\\sl",
        "50%",
        "_off",
        "ÉT",
    ]
    .iter()
    .map(|s| json!(s))
    .collect()
}

fn number_values() -> Vec<Value> {
    vec![
        json!(0),
        json!(5),
        json!(-3),
        json!(10.5),
        json!(9_007_199_254_740_993_i64),
        json!(1e300),
        json!("5"),
        json!("abc"),
        Value::Null,
    ]
}

fn date_values() -> Vec<Value> {
    vec![
        json!("2024-01-01"),
        json!("2024-06-15"),
        json!("2025-01-01"),
        json!("not-a-date"),
    ]
}

fn bool_values() -> Vec<Value> {
    vec![json!(true), json!(false), json!("true")]
}

fn array_values() -> Vec<Value> {
    vec![
        json!(["rust"]),
        json!(["python", "sql"]),
        json!([]),
        json!(["O'Brien"]),
        json!(["rust", "graphql", "extra"]),
        json!([1]),
        json!([{"a": 1}]),
        json!([["rust"]]),
    ]
}

/// One operator family, with the fields it applies to and the values drawn for it.
struct Family {
    fields:    &'static [&'static str],
    operators: &'static [WhereOperator],
    values:    fn() -> Vec<Value>,
}

const TEXT_OPS: &[WhereOperator] = &[
    WhereOperator::Eq,
    WhereOperator::Neq,
    WhereOperator::Gt,
    WhereOperator::Lte,
    WhereOperator::Contains,
    WhereOperator::Icontains,
    WhereOperator::Startswith,
    WhereOperator::Istartswith,
    WhereOperator::Endswith,
    WhereOperator::Iendswith,
    WhereOperator::Like,
    WhereOperator::Ilike,
    WhereOperator::Nlike,
    WhereOperator::Nilike,
    WhereOperator::Regex,
    WhereOperator::Iregex,
    WhereOperator::Nregex,
    WhereOperator::Niregex,
];

const ORDERED_OPS: &[WhereOperator] = &[
    WhereOperator::Eq,
    WhereOperator::Neq,
    WhereOperator::Gt,
    WhereOperator::Gte,
    WhereOperator::Lt,
    WhereOperator::Lte,
];

const ARRAY_OPS: &[WhereOperator] = &[
    WhereOperator::ArrayContains,
    WhereOperator::ArrayContainedBy,
    WhereOperator::ArrayOverlaps,
];

const FAMILIES: &[Family] = &[
    Family {
        fields:    &["name"],
        operators: TEXT_OPS,
        values:    text_values,
    },
    Family {
        fields:    &["n", "x"],
        operators: ORDERED_OPS,
        values:    number_values,
    },
    Family {
        fields:    &["d"],
        operators: ORDERED_OPS,
        values:    date_values,
    },
    Family {
        fields:    &["b"],
        operators: &[WhereOperator::Eq, WhereOperator::Neq],
        values:    bool_values,
    },
    Family {
        fields:    &["tags"],
        operators: ARRAY_OPS,
        values:    array_values,
    },
];

/// Every leaf: each family's fields × its operators × its values, plus null tests and
/// `in` / `nin` over one and over two of the family's values.
fn leaves() -> Vec<WhereClause> {
    let mut out = Vec::new();
    for family in FAMILIES {
        let values = (family.values)();
        for field in family.fields {
            let mut push = |operator: WhereOperator, value: Value| {
                out.push(WhereClause::Field {
                    path: vec![(*field).to_string()],
                    operator,
                    value,
                });
            };
            for op in family.operators {
                for v in &values {
                    push(op.clone(), v.clone());
                }
            }
            for op in [WhereOperator::IsNull, WhereOperator::IsNotNull] {
                push(op.clone(), json!(true));
                push(op.clone(), json!(false));
                push(op, json!("yes"));
            }
            if family.operators != ARRAY_OPS {
                push(WhereOperator::In, json!([]));
                push(WhereOperator::Nin, json!([]));
                for (i, v) in values.iter().enumerate() {
                    let next = &values[(i + 1) % values.len()];
                    for op in [WhereOperator::In, WhereOperator::Nin] {
                        push(op.clone(), json!([v]));
                        push(op, json!([v, next]));
                    }
                }
            }
        }
    }
    out
}

/// A tree over `pool`, the leaves both paths answered alike on their own: whatever it
/// disagrees on is the composition, not a leaf.
fn tree(rng: &mut Rng, pool: &[WhereClause], depth: u32) -> WhereClause {
    if depth == MAX_DEPTH || rng.below(3) == 0 {
        return rng.pick(pool).clone();
    }
    match rng.below(3) {
        0 => WhereClause::Not(Box::new(tree(rng, pool, depth + 1))),
        kind => {
            let k = 1 + rng.below(3);
            let children = (0..k).map(|_| tree(rng, pool, depth + 1)).collect();
            if kind == 1 {
                WhereClause::And(children)
            } else {
                WhereClause::Or(children)
            }
        },
    }
}

/// `clause` as its variant runs it: as written, or under the declared types.
fn variant(name: &str, clause: WhereClause) -> WhereClause {
    if name == "typed" {
        WhereClause::Typed {
            types: declared_types(),
            inner: Box::new(clause),
        }
    } else {
        clause
    }
}

const VARIANTS: [&str; 2] = ["typed", "untyped"];

// ── Execution ─────────────────────────────────────────────────────────────────

/// What one path answered: the matching ids, or the class of its refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    Rows(Vec<i64>),
    /// The generator refused the tree before any SQL reached PostgreSQL.
    GeneratorRefused,
    /// PostgreSQL refused the statement.
    PostgresRefused,
}

impl Answer {
    const fn class(&self) -> &'static str {
        match self {
            Self::Rows(_) => "rows",
            Self::GeneratorRefused => "generator_refused",
            Self::PostgresRefused => "postgres_refused",
        }
    }
}

const fn refusal(e: &FraiseQLError) -> Answer {
    if matches!(e, FraiseQLError::Database { .. }) {
        Answer::PostgresRefused
    } else {
        Answer::GeneratorRefused
    }
}

fn ids(rows: impl Iterator<Item = Value>) -> Answer {
    let mut ids: Vec<i64> =
        rows.map(|d| d["id"].as_i64().expect("every fixture row has an id")).collect();
    ids.sort_unstable();
    Answer::Rows(ids)
}

async fn parameterised(adapter: &PostgresAdapter, clause: &WhereClause) -> Answer {
    match adapter.execute_where_query(TABLE, Some(clause), None, None, None).await {
        Ok(rows) => ids(rows.iter().map(|r| r.as_value().clone())),
        Err(e) => refusal(&e),
    }
}

async fn inlined(adapter: &PostgresAdapter, clause: &WhereClause) -> Answer {
    let sql = match WhereSqlGenerator::to_sql(clause) {
        Ok(sql) => sql,
        Err(e) => return refusal(&e),
    };
    match adapter
        .execute_raw_query(&format!("SELECT data FROM {TABLE} WHERE {sql}"))
        .await
    {
        Ok(rows) => ids(rows.into_iter().map(|mut r| r.remove("data").unwrap_or_default())),
        Err(_) => Answer::PostgresRefused,
    }
}

/// FNV-1a over the tree's JSON form: stable across runs and platforms, for a tree that
/// holds no hash map.
fn fingerprint(clause: &WhereClause) -> String {
    let text = serde_json::to_string(clause).unwrap();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The entries one case contributes to the report, keyed `oracle:kind:scope:variant:fingerprint`.
fn judge(
    variant: &str,
    clause: &WhereClause,
    param: &Answer,
    inline: &Answer,
    reference: &Verdict,
) -> Vec<(String, String)> {
    // The declared types are the same for every typed case, and serialise in hash order:
    // the variant names them, and the fingerprint is the tree they wrap.
    let shown = match clause {
        WhereClause::Typed { inner, .. } => inner.as_ref(),
        other => other,
    };
    let fp = fingerprint(shown);
    let tree = serde_json::to_string(shown).unwrap();
    let mut out = Vec::new();
    if *param == Answer::PostgresRefused {
        out.push((format!("O1:postgres_refused:{variant}:{fp}"), tree.clone()));
    }
    if param != inline {
        let kind = match (param, inline) {
            (Answer::Rows(_), Answer::Rows(_)) => "rows_differ".to_string(),
            _ => format!("{}_vs_{}", param.class(), inline.class()),
        };
        out.push((format!("O2:{kind}:{variant}:{fp}"), tree.clone()));
    }
    let o3 = match (reference, param) {
        (Verdict::Unjudged, _) | (Verdict::Refused, Answer::GeneratorRefused) => None,
        (Verdict::Rows(want), Answer::Rows(got)) if want == got => None,
        (Verdict::Rows(_), Answer::Rows(_)) => Some("rows_differ".to_string()),
        (Verdict::Rows(_), other) => Some(format!("{}_vs_rows", other.class())),
        (Verdict::Refused, other) => Some(format!("{}_vs_refused", other.class())),
    };
    if let Some(kind) = o3 {
        out.push((format!("O3:{kind}:{variant}:{fp}"), tree));
    }
    out
}

fn baseline_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join(file)
}

/// `url` with its database name replaced by `name`.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = url.split_once('?').map_or((url, None), |(b, q)| (b, Some(q)));
    let (authority, _) = base.rsplit_once('/').expect("a database URL names its database");
    query.map_or_else(|| format!("{authority}/{name}"), |q| format!("{authority}/{name}?{q}"))
}

/// (Re)create `database` on the bound server with the builtin `C.UTF-8` locale, then run
/// `fixture` in it. Each test has a database of its own, so they run in parallel.
async fn rig(database: &str, fixture: Vec<String>) -> Option<PostgresAdapter> {
    let pg = fraiseql_test_support::postgres().await?;
    let (admin, connection) =
        fraiseql_db::tokio_postgres::connect(pg.url(), fraiseql_db::tokio_postgres::NoTls)
            .await
            .expect("connect to the bound PostgreSQL");
    tokio::spawn(connection);
    admin
        .simple_query(&format!("DROP DATABASE IF EXISTS {database} WITH (FORCE)"))
        .await
        .expect("drop a previous oracle database");
    admin
        .simple_query(&format!(
            "CREATE DATABASE {database} TEMPLATE template0 ENCODING 'UTF8' \
             LOCALE_PROVIDER builtin BUILTIN_LOCALE 'C.UTF-8'"
        ))
        .await
        .expect("create the oracle database with the builtin C.UTF-8 locale");
    let adapter = PostgresAdapter::new(&with_database(pg.url(), database))
        .await
        .expect("connect to the oracle database");
    for stmt in fixture {
        adapter.execute_raw_query(&stmt).await.expect("provision the oracle fixture");
    }
    Some(adapter)
}

/// The report under construction: oracle entries, and per-kind counts.
#[derive(Default)]
struct Report {
    entries: BTreeMap<String, String>,
    counts:  BTreeMap<String, usize>,
    /// The fixture's documents, as the reference evaluator reads them.
    rows:    Vec<Value>,
}

impl Report {
    fn count(&mut self, key: String) {
        *self.counts.entry(key).or_default() += 1;
    }

    /// Judge one case; true when no oracle reported it.
    async fn case(
        &mut self,
        adapter: &PostgresAdapter,
        scope: &str,
        variant_name: &str,
        clause: &WhereClause,
    ) -> bool {
        let param = parameterised(adapter, clause).await;
        let inline = inlined(adapter, clause).await;
        let reference = reference::evaluate(clause, &self.rows, reference_types);
        self.count(format!("{scope}:{variant_name}:cases"));
        if reference == Verdict::Unjudged {
            self.count(format!("{scope}:{variant_name}:o3_unjudged"));
        }
        let found = judge(&format!("{scope}:{variant_name}"), clause, &param, &inline, &reference);
        for (key, tree) in &found {
            self.count(key.rsplitn(4, ':').nth(3).unwrap().to_string());
            self.entries.insert(key.clone(), tree.clone());
        }
        found.is_empty()
    }
}

/// Run every leaf, then the trees over the leaves no oracle reported: a tree entry is then
/// a composition defect, not a leaf repeated.
async fn run(adapter: &PostgresAdapter) -> Report {
    let mut report = Report {
        rows: rows(),
        ..Report::default()
    };
    let leaves = leaves();
    let mut rng = Rng(SEED);
    for name in VARIANTS {
        let mut pool = Vec::new();
        for leaf in &leaves {
            if report.case(adapter, "leaf", name, &variant(name, leaf.clone())).await {
                pool.push(leaf.clone());
            }
        }
        for _ in 0..TREES {
            let t = variant(name, tree(&mut rng, &pool, 0));
            report.case(adapter, "tree", name, &t).await;
        }
    }
    report
}

#[tokio::test]
async fn generated_trees_agree_with_postgres_and_across_generators() {
    let Some(adapter) = rig(DATABASE, fixture()).await else {
        eprintln!("skip: no PostgreSQL bound (DATABASE_URL unset)");
        return;
    };
    let Report {
        entries: report,
        counts,
        ..
    } = run(&adapter).await;
    for (k, v) in &counts {
        eprintln!("sql oracle: {k} = {v}");
    }

    gate(&baseline_path("sql_oracle_baseline.json"), &report);
}

/// Compare `report` both ways with the baseline at `path`, or rewrite it under
/// `SQL_ORACLE_BLESS`.
fn gate(path: &std::path::Path, report: &BTreeMap<String, String>) {
    if std::env::var_os("SQL_ORACLE_BLESS").is_some() {
        let text = serde_json::to_string_pretty(report).unwrap() + "\n";
        std::fs::write(path, text).expect("write the baseline");
        eprintln!("sql oracle: blessed {} entries into {}", report.len(), path.display());
        return;
    }
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e} (bless with SQL_ORACLE_BLESS=1)", path.display()));
    let baseline: BTreeMap<String, String> = serde_json::from_str(&text).expect("baseline is JSON");
    let got: BTreeSet<&String> = report.keys().collect();
    let want: BTreeSet<&String> = baseline.keys().collect();
    let new: Vec<_> = got.difference(&want).collect();
    let gone: Vec<_> = want.difference(&got).collect();
    for k in &new {
        eprintln!("NEW   {k}  {}", report[**k]);
    }
    for k in &gone {
        eprintln!("GONE  {k}  {}", baseline[**k]);
    }
    assert!(
        new.is_empty() && gone.is_empty(),
        "the SQL oracle disagrees with its baseline {}: {} new, {} gone (listed above). A new \
         entry is a refusal or a disagreement that did not occur before; a gone one no longer \
         occurs. Fix it, or re-bless with SQL_ORACLE_BLESS=1 and say why in the commit.",
        path.display(),
        new.len(),
        gone.len()
    );
}
