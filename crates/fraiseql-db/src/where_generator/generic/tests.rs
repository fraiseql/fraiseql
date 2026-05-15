#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::GenericWhereGenerator;
use crate::{
    dialect::PostgresDialect,
    where_clause::{WhereClause, WhereOperator},
    where_generator::HierarchyContext,
};

fn field(path: &str, op: WhereOperator, val: serde_json::Value) -> WhereClause {
    WhereClause::Field {
        path:     vec![path.to_string()],
        operator: op,
        value:    val,
    }
}

// ── Core comparison / logical operators ──────────────────────────

#[test]
fn generic_eq_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("email", WhereOperator::Eq, json!("alice@example.com"));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "data->>'email' = $1");
    assert_eq!(params, vec![json!("alice@example.com")]);
}

#[test]
fn generic_and_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = WhereClause::And(vec![
        field("status", WhereOperator::Eq, json!("active")),
        field("age", WhereOperator::Gte, json!(18)),
    ]);
    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(sql.starts_with("(data->>'status' = $1 AND"));
    assert_eq!(params.len(), 2);
}

#[test]
fn generic_empty_and_returns_true() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = WhereClause::And(vec![]);
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "TRUE");
    assert!(params.is_empty());
}

#[test]
fn generic_empty_or_returns_false() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = WhereClause::Or(vec![]);
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "FALSE");
    assert!(params.is_empty());
}

#[test]
fn generic_not_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = WhereClause::Not(Box::new(field("deleted", WhereOperator::Eq, json!(true))));
    let (sql, _) = gen.generate(&clause).unwrap();
    assert!(sql.starts_with("NOT ("));
}

#[test]
fn generate_resets_counter() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("x", WhereOperator::Eq, json!(1));
    let (sql1, _) = gen.generate(&clause).unwrap();
    let (sql2, _) = gen.generate(&clause).unwrap();
    assert_eq!(sql1, sql2);
    // Both must reference $1, not $1 then $2
    assert!(sql1.contains("$1"));
    assert!(!sql1.contains("$2"));
}

#[test]
fn generate_with_param_offset() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("email", WhereOperator::Eq, json!("a@b.com"));
    let (sql, _) = gen.generate_with_param_offset(&clause, 2).unwrap();
    assert!(sql.contains("$3"), "Expected $3 (offset 2 + 1), got: {sql}");
}

// ── String operators ─────────────────────────────────────────────

#[test]
fn generic_icontains_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("email", WhereOperator::Icontains, json!("example.com"));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "data->>'email' ILIKE '%' || $1 || '%'");
    assert_eq!(params, vec![json!("example.com")]);
}

#[test]
fn generic_startswith_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Startswith, json!("Al"));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "data->>'name' LIKE $1 || '%'");
    assert_eq!(params, vec![json!("Al")]);
}

#[test]
fn generic_endswith_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Endswith, json!("son"));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "data->>'name' LIKE '%' || $1");
    assert_eq!(params, vec![json!("son")]);
}

// ── Array / IN operators ────────────────────────────────────────

#[test]
fn generic_in_postgres() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("status", WhereOperator::In, json!(["active", "pending"]));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "data->>'status' IN ($1, $2)");
    assert_eq!(params.len(), 2);
}

#[test]
fn generic_in_empty_returns_false() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("status", WhereOperator::In, json!([]));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "FALSE");
    assert!(params.is_empty());
}

#[test]
fn generic_nin_empty_returns_true() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("status", WhereOperator::Nin, json!([]));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(sql, "TRUE");
    assert!(params.is_empty());
}

// ── Security: no value interpolation ─────────────────────────────────────

#[test]
fn no_value_in_sql_string() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let injection = "'; DROP TABLE users; --";
    let clause = field("email", WhereOperator::Eq, json!(injection));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(!sql.contains(injection), "Value must not appear in SQL: {sql}");
    assert_eq!(params[0], json!(injection));
}

// ── PG-only: Vector operators ─────────────────────────────────────────────

#[test]
fn generic_pg_cosine_distance() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("embedding", WhereOperator::CosineDistance, json!([0.1, 0.2]));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(sql.contains("<=>"), "Expected <=> operator, got: {sql}");
    assert!(sql.contains("::vector"), "Expected ::vector cast, got: {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn generic_pg_network_ipv4() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("ip", WhereOperator::IsIPv4, json!(true));
    let (sql, _) = gen.generate(&clause).unwrap();
    assert!(sql.contains("family("), "Expected family() call, got: {sql}");
    assert!(sql.contains("= 4"), "Expected = 4, got: {sql}");
}

#[test]
fn generic_pg_ltree_ancestor_of() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("path", WhereOperator::AncestorOf, json!("europe.france"));
    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(sql.contains("@>") && sql.contains("ltree"), "Got: {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn non_pg_vector_op_returns_error() {
    use crate::dialect::MySqlDialect;
    let gen = GenericWhereGenerator::new(MySqlDialect);
    let clause = field("embedding", WhereOperator::CosineDistance, json!([0.1]));
    let err = gen.generate(&clause).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("VectorDistance") || msg.contains("not supported"), "Got: {msg}");
}

#[test]
fn non_pg_network_op_returns_error() {
    use crate::dialect::SqliteDialect;
    let gen = GenericWhereGenerator::new(SqliteDialect);
    let clause = field("ip", WhereOperator::IsIPv4, json!(true));
    let err = gen.generate(&clause).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Inet") || msg.contains("not supported"), "Got: {msg}");
}

// ── LIKE metacharacter escaping (C3 fix verification) ──────────────

#[test]
fn escape_like_literal_escapes_percent_and_underscore() {
    assert_eq!(super::escape_like_literal("50%"), "50\\%");
    assert_eq!(super::escape_like_literal("user_name"), "user\\_name");
    assert_eq!(super::escape_like_literal("a%b_c\\d"), "a\\%b\\_c\\\\d");
    assert_eq!(super::escape_like_literal("plain"), "plain");
}

#[test]
fn contains_escapes_like_metacharacters() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Contains, json!("50%off"));
    let (_sql, params) = gen.generate(&clause).unwrap();
    // The param value must have % escaped so it's treated as a literal.
    assert_eq!(params[0], json!("50\\%off"));
}

#[test]
fn startswith_escapes_like_metacharacters() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Startswith, json!("user_"));
    let (_sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(params[0], json!("user\\_"));
}

#[test]
fn endswith_escapes_like_metacharacters() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Endswith, json!("100%"));
    let (_sql, params) = gen.generate(&clause).unwrap();
    assert_eq!(params[0], json!("100\\%"));
}

// ── Regex complexity guard (C5 fix verification) ──────────────────

#[test]
fn regex_rejects_nested_quantifiers() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Regex, json!("(a+)+$"));
    let err = gen.generate(&clause).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nested quantifiers"), "Got: {msg}");
}

#[test]
fn regex_rejects_star_star_pattern() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Regex, json!("(x*)*"));
    let err = gen.generate(&clause).unwrap_err();
    assert!(err.to_string().contains("nested quantifiers"));
}

#[test]
fn regex_rejects_too_long_pattern() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let long_pattern = "a".repeat(1_001);
    let clause = field("name", WhereOperator::Regex, json!(long_pattern));
    let err = gen.generate(&clause).unwrap_err();
    assert!(err.to_string().contains("maximum length"));
}

#[test]
fn regex_allows_safe_patterns() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Regex, json!("^[a-z]+$"));
    assert!(gen.generate(&clause).is_ok());
}

#[test]
fn iregex_also_validates_pattern() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("name", WhereOperator::Iregex, json!("(a+)+"));
    assert!(gen.generate(&clause).is_err());
}

// --- Issue #250: HierarchyContext ---

#[test]
fn hierarchy_context_none_is_backward_compatible() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("email", WhereOperator::Eq, json!("test@example.com"));
    let (sql, _) = gen.generate(&clause).unwrap();
    assert!(sql.contains("= $1"));
}

#[test]
fn hierarchy_context_can_be_constructed() {
    let ctx = HierarchyContext {
        table: "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column: None,
    };
    assert_eq!(ctx.table, "tb_category");
    assert_eq!(ctx.path_column, "category_path");
    assert!(ctx.fk_column.is_none());
}

#[test]
fn hierarchy_context_cross_table() {
    let ctx = HierarchyContext {
        table: "tb_location".to_string(),
        path_column: "location_path".to_string(),
        fk_column: Some("fk_location".to_string()),
    };
    assert_eq!(ctx.fk_column, Some("fk_location".to_string()));
}

// --- Issue #250: descendantOfId / ancestorOfId SQL generation ---

#[test]
fn descendant_of_id_self_referencing() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::DescendantOfId, json!("550e8400-e29b-41d4-a716-446655440000"));
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "data->>'category_path'::ltree <@ (SELECT \"category_path\" FROM \"tb_category\" WHERE \"id\" = $1)"
    );
    assert_eq!(params, vec![json!("550e8400-e29b-41d4-a716-446655440000")]);
}

#[test]
fn ancestor_of_id_self_referencing() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::AncestorOfId, json!("550e8400-e29b-41d4-a716-446655440000"));
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "data->>'category_path'::ltree @> (SELECT \"category_path\" FROM \"tb_category\" WHERE \"id\" = $1)"
    );
    assert_eq!(params, vec![json!("550e8400-e29b-41d4-a716-446655440000")]);
}

#[test]
fn descendant_of_id_cross_table() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_location".to_string(),
        path_column: "location_path".to_string(),
        fk_column:   Some("fk_location".to_string()),
    };
    let clause = field("location", WhereOperator::DescendantOfId, json!("550e8400-e29b-41d4-a716-446655440000"));
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "\"fk_location\" IN (SELECT \"id\" FROM \"tb_location\" WHERE \"location_path\" <@ (SELECT \"location_path\" FROM \"tb_location\" WHERE \"id\" = $1))"
    );
    assert_eq!(params, vec![json!("550e8400-e29b-41d4-a716-446655440000")]);
}

#[test]
fn ancestor_of_id_cross_table() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_location".to_string(),
        path_column: "location_path".to_string(),
        fk_column:   Some("fk_location".to_string()),
    };
    let clause = field("location", WhereOperator::AncestorOfId, json!("550e8400-e29b-41d4-a716-446655440000"));
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "\"fk_location\" IN (SELECT \"id\" FROM \"tb_location\" WHERE \"location_path\" @> (SELECT \"location_path\" FROM \"tb_location\" WHERE \"id\" = $1))"
    );
    assert_eq!(params, vec![json!("550e8400-e29b-41d4-a716-446655440000")]);
}

#[test]
fn descendant_of_id_without_hierarchy_ctx_errors() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = field("category_path", WhereOperator::DescendantOfId, json!("some-id"));
    // Without hierarchy context, should fail
    let result = gen.generate(&clause);
    assert!(result.is_err());
}

#[test]
fn descendant_of_id_mysql_unsupported() {
    use crate::dialect::MySqlDialect;
    let gen = GenericWhereGenerator::new(MySqlDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::DescendantOfId, json!("some-id"));
    let err = gen.generate_with_hierarchy(&clause, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not supported") || msg.contains("LTreeIdSubquery"), "Got: {msg}");
}

#[test]
fn ancestor_of_id_sqlite_unsupported() {
    use crate::dialect::SqliteDialect;
    let gen = GenericWhereGenerator::new(SqliteDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::AncestorOfId, json!("some-id"));
    let err = gen.generate_with_hierarchy(&clause, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not supported") || msg.contains("LTreeIdSubquery"), "Got: {msg}");
}

#[test]
fn descendant_of_id_sqlserver_unsupported() {
    use crate::dialect::SqlServerDialect;
    let gen = GenericWhereGenerator::new(SqlServerDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::DescendantOfId, json!("some-id"));
    let err = gen.generate_with_hierarchy(&clause, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not supported") || msg.contains("LTreeIdSubquery"), "Got: {msg}");
}

#[test]
fn ltree_id_subquery_escapes_adversarial_identifiers() {
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       r#"evil"table"#.to_string(),
        path_column: r#"evil"col"#.to_string(),
        fk_column:   None,
    };
    let clause = field("category_path", WhereOperator::DescendantOfId, json!("some-id"));
    let (sql, _) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert!(
        sql.contains(r#""evil""table""#),
        "Table name should have doubled quotes, got: {sql}"
    );
    assert!(
        sql.contains(r#""evil""col""#),
        "Column name should have doubled quotes, got: {sql}"
    );
}
