#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items
#![allow(clippy::doc_markdown)] // Reason: doc comments reference SQL/API names without backticks

//! Performance benchmarks for fraiseql-db SQL generation
//!
//! Measures latency for:
//! - WHERE clause SQL generation (WhereSqlGenerator — raw string path)
//! - GenericWhereGenerator parameterized SQL across all four dialects
//! - Projection/SELECT generation (PostgreSQL, MySQL, SQLite)
//! - Collation mapping across database types
//! - `WhereClause::from_graphql_json` parsing

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_db::{
    CollationConfig, CollationMapper, DatabaseType, GenericWhereGenerator,
    MySqlProjectionGenerator, PostgresProjectionGenerator, SqliteProjectionGenerator, WhereClause,
    WhereOperator, WhereSqlGenerator,
    dialect::{MySqlDialect, PostgresDialect, SqlServerDialect, SqliteDialect},
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helper: build common WHERE clause fixtures
// ---------------------------------------------------------------------------

fn simple_eq_clause() -> WhereClause {
    WhereClause::Field {
        path:     vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("active"),
    }
}

fn and_clause() -> WhereClause {
    WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        },
        WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(18),
        },
    ])
}

fn complex_and_or_clause() -> WhereClause {
    WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["type".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("article"),
        },
        WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("published"),
            },
            WhereClause::And(vec![
                WhereClause::Field {
                    path:     vec!["status".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("draft"),
                },
                WhereClause::Field {
                    path:     vec!["author".to_string(), "role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("admin"),
                },
            ]),
        ]),
    ])
}

fn in_clause(count: usize) -> WhereClause {
    let values: Vec<serde_json::Value> = (0..count).map(|i| json!(format!("val_{i}"))).collect();
    WhereClause::Field {
        path:     vec!["tag".to_string()],
        operator: WhereOperator::In,
        value:    serde_json::Value::Array(values),
    }
}

// ---------------------------------------------------------------------------
// WhereSqlGenerator benchmarks (raw string SQL path)
// ---------------------------------------------------------------------------

fn where_sql_generator_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("where_sql_generator");

    group.bench_function("simple_eq", |b| {
        let clause = simple_eq_clause();
        b.iter(|| WhereSqlGenerator::to_sql(black_box(&clause)).unwrap());
    });

    group.bench_function("and_two_conditions", |b| {
        let clause = and_clause();
        b.iter(|| WhereSqlGenerator::to_sql(black_box(&clause)).unwrap());
    });

    group.bench_function("complex_and_or_nested", |b| {
        let clause = complex_and_or_clause();
        b.iter(|| WhereSqlGenerator::to_sql(black_box(&clause)).unwrap());
    });

    for count in [3, 10, 50, 100] {
        group.bench_with_input(BenchmarkId::new("in_clause", count), &count, |b, &n| {
            let clause = in_clause(n);
            b.iter(|| WhereSqlGenerator::to_sql(black_box(&clause)).unwrap());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// GenericWhereGenerator benchmarks (parameterized SQL, all dialects)
// ---------------------------------------------------------------------------

fn generic_where_generator_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("generic_where_generator");

    // Simple equality across all four dialects
    let clause = simple_eq_clause();

    group.bench_function("postgres_simple_eq", |b| {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        b.iter(|| gen.generate(black_box(&clause)).unwrap());
    });

    group.bench_function("mysql_simple_eq", |b| {
        let gen = GenericWhereGenerator::new(MySqlDialect);
        b.iter(|| gen.generate(black_box(&clause)).unwrap());
    });

    group.bench_function("sqlite_simple_eq", |b| {
        let gen = GenericWhereGenerator::new(SqliteDialect);
        b.iter(|| gen.generate(black_box(&clause)).unwrap());
    });

    group.bench_function("sqlserver_simple_eq", |b| {
        let gen = GenericWhereGenerator::new(SqlServerDialect);
        b.iter(|| gen.generate(black_box(&clause)).unwrap());
    });

    // Complex nested AND/OR across dialects
    let complex = complex_and_or_clause();

    group.bench_function("postgres_complex_and_or", |b| {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        b.iter(|| gen.generate(black_box(&complex)).unwrap());
    });

    group.bench_function("mysql_complex_and_or", |b| {
        let gen = GenericWhereGenerator::new(MySqlDialect);
        b.iter(|| gen.generate(black_box(&complex)).unwrap());
    });

    // IN clause scaling (PostgreSQL dialect)
    for count in [3, 10, 50, 100] {
        group.bench_with_input(BenchmarkId::new("postgres_in_clause", count), &count, |b, &n| {
            let gen = GenericWhereGenerator::new(PostgresDialect);
            let clause = in_clause(n);
            b.iter(|| gen.generate(black_box(&clause)).unwrap());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// WhereClause::from_graphql_json parsing benchmarks
// ---------------------------------------------------------------------------

fn where_clause_parsing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("where_clause_parsing");

    let simple_json = json!({ "status": { "eq": "active" } });
    group.bench_function("simple_field", |b| {
        b.iter(|| WhereClause::from_graphql_json(black_box(&simple_json)).unwrap());
    });

    let multi_json = json!({
        "status": { "eq": "active" },
        "name": { "icontains": "john" },
        "age": { "gte": 18, "lte": 65 }
    });
    group.bench_function("multi_field_multi_op", |b| {
        b.iter(|| WhereClause::from_graphql_json(black_box(&multi_json)).unwrap());
    });

    let logical_json = json!({
        "_or": [
            { "role": { "eq": "admin" } },
            { "_and": [
                { "role": { "eq": "editor" } },
                { "department": { "eq": "engineering" } }
            ]}
        ]
    });
    group.bench_function("nested_logical", |b| {
        b.iter(|| WhereClause::from_graphql_json(black_box(&logical_json)).unwrap());
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Projection generation benchmarks
// ---------------------------------------------------------------------------

fn projection_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("projection_generation");

    // Varying field counts
    let field_sets: Vec<(usize, Vec<String>)> = vec![
        (1, vec!["id".to_string()]),
        (
            5,
            vec![
                "id".to_string(),
                "firstName".to_string(),
                "lastName".to_string(),
                "email".to_string(),
                "createdAt".to_string(),
            ],
        ),
        (15, (0..15).map(|i| format!("field{i}")).collect()),
    ];

    // PostgreSQL projection
    let pg = PostgresProjectionGenerator::new();
    for (count, fields) in &field_sets {
        group.bench_with_input(BenchmarkId::new("postgres", count), fields, |b, fields| {
            b.iter(|| pg.generate_projection_sql(black_box(fields)).unwrap());
        });
    }

    // MySQL projection
    let mysql = MySqlProjectionGenerator::new();
    for (count, fields) in &field_sets {
        group.bench_with_input(BenchmarkId::new("mysql", count), fields, |b, fields| {
            b.iter(|| mysql.generate_projection_sql(black_box(fields)).unwrap());
        });
    }

    // SQLite projection
    let sqlite = SqliteProjectionGenerator::new();
    for (count, fields) in &field_sets {
        group.bench_with_input(BenchmarkId::new("sqlite", count), fields, |b, fields| {
            b.iter(|| sqlite.generate_projection_sql(black_box(fields)).unwrap());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Collation mapping benchmarks
// ---------------------------------------------------------------------------

fn collation_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("collation_mapping");

    let config = CollationConfig::default();

    let db_types = [
        ("postgres", DatabaseType::PostgreSQL),
        ("mysql", DatabaseType::MySQL),
        ("sqlite", DatabaseType::SQLite),
        ("sqlserver", DatabaseType::SQLServer),
    ];

    for (name, db_type) in &db_types {
        group.bench_with_input(BenchmarkId::from_parameter(name), db_type, |b, &dt| {
            let mapper = CollationMapper::new(config.clone(), dt);
            b.iter(|| mapper.map_locale(black_box("fr-FR")).unwrap());
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    where_sql_generator_benchmarks,
    generic_where_generator_benchmarks,
    where_clause_parsing_benchmarks,
    projection_benchmarks,
    collation_benchmarks
);
criterion_main!(benches);
