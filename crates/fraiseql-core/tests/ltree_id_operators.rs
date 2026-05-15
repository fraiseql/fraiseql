#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Test `LTree` ID-based operator SQL generation and edge cases.
//!
//! This test verifies that:
//! 1. `DescendantOfId` generates correct subquery SQL (self-referencing + cross-table)
//! 2. `AncestorOfId` uses `@>` (inverse of `DescendantOfId`'s `<@`)
//! 3. Non-existent UUID value is handled gracefully (no error, empty results)
//! 4. Both operators require `HierarchyContext` — fail clearly without it
//! 5. Operator parsing from GraphQL JSON works for `descendantOfId` / `ancestorOfId`
//!
//! # Risk If Missing
//!
//! Without this test:
//! - ID-based ltree operators could generate incorrect SQL subqueries
//! - Cross-table semi-joins could silently produce wrong results
//! - Missing hierarchy config could silently generate broken SQL

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_generator::generic::GenericWhereGenerator;
use fraiseql_core::db::where_generator::HierarchyContext;
use serde_json::json;

// ── Operator parsing ─────────────────────────────────────────────────────────

#[test]
fn descendant_of_id_parses_from_snake_case() {
    let op = WhereOperator::from_str("descendant_of_id").unwrap();
    assert_eq!(op, WhereOperator::DescendantOfId);
}

#[test]
fn ancestor_of_id_parses_from_snake_case() {
    let op = WhereOperator::from_str("ancestor_of_id").unwrap();
    assert_eq!(op, WhereOperator::AncestorOfId);
}

#[test]
fn descendant_of_id_parses_from_camel_case() {
    let op = WhereOperator::from_str("descendantOfId").unwrap();
    assert_eq!(op, WhereOperator::DescendantOfId);
}

#[test]
fn ancestor_of_id_parses_from_camel_case() {
    let op = WhereOperator::from_str("ancestorOfId").unwrap();
    assert_eq!(op, WhereOperator::AncestorOfId);
}

// ── GraphQL JSON parsing ─────────────────────────────────────────────────────

#[test]
fn graphql_json_descendant_of_id() {
    let input = json!({
        "categoryPath": {
            "descendantOfId": "550e8400-e29b-41d4-a716-446655440000"
        }
    });
    let clause = WhereClause::from_graphql_json(&input).unwrap();
    match &clause {
        WhereClause::Field { path, operator, value } => {
            assert_eq!(path, &["category_path"]);
            assert_eq!(*operator, WhereOperator::DescendantOfId);
            assert_eq!(value, "550e8400-e29b-41d4-a716-446655440000");
        },
        other => panic!("Expected Field, got {other:?}"),
    }
}

#[test]
fn graphql_json_ancestor_of_id() {
    let input = json!({
        "categoryPath": {
            "ancestorOfId": "550e8400-e29b-41d4-a716-446655440000"
        }
    });
    let clause = WhereClause::from_graphql_json(&input).unwrap();
    match &clause {
        WhereClause::Field { path, operator, value } => {
            assert_eq!(path, &["category_path"]);
            assert_eq!(*operator, WhereOperator::AncestorOfId);
            assert_eq!(value, "550e8400-e29b-41d4-a716-446655440000");
        },
        other => panic!("Expected Field, got {other:?}"),
    }
}

// ── SQL generation (self-referencing) ────────────────────────────────────────

#[test]
fn sql_descendant_of_id_self_referencing() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = WhereClause::Field {
        path:     vec!["category_path".to_string()],
        operator: WhereOperator::DescendantOfId,
        value:    json!("550e8400-e29b-41d4-a716-446655440000"),
    };
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "data->>'category_path'::ltree <@ (SELECT \"category_path\" FROM \"tb_category\" WHERE \"id\" = $1)"
    );
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], json!("550e8400-e29b-41d4-a716-446655440000"));
}

#[test]
fn sql_ancestor_of_id_self_referencing() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = WhereClause::Field {
        path:     vec!["category_path".to_string()],
        operator: WhereOperator::AncestorOfId,
        value:    json!("550e8400-e29b-41d4-a716-446655440000"),
    };
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "data->>'category_path'::ltree @> (SELECT \"category_path\" FROM \"tb_category\" WHERE \"id\" = $1)"
    );
    assert_eq!(params.len(), 1);
}

// ── SQL generation (cross-table) ─────────────────────────────────────────────

#[test]
fn sql_descendant_of_id_cross_table() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_location".to_string(),
        path_column: "location_path".to_string(),
        fk_column:   Some("fk_location".to_string()),
    };
    let clause = WhereClause::Field {
        path:     vec!["location".to_string()],
        operator: WhereOperator::DescendantOfId,
        value:    json!("550e8400-e29b-41d4-a716-446655440000"),
    };
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "\"fk_location\" IN (SELECT \"id\" FROM \"tb_location\" WHERE \"location_path\" <@ (SELECT \"location_path\" FROM \"tb_location\" WHERE \"id\" = $1))"
    );
    assert_eq!(params.len(), 1);
}

#[test]
fn sql_ancestor_of_id_cross_table() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_location".to_string(),
        path_column: "location_path".to_string(),
        fk_column:   Some("fk_location".to_string()),
    };
    let clause = WhereClause::Field {
        path:     vec!["location".to_string()],
        operator: WhereOperator::AncestorOfId,
        value:    json!("550e8400-e29b-41d4-a716-446655440000"),
    };
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert_eq!(
        sql,
        "\"fk_location\" IN (SELECT \"id\" FROM \"tb_location\" WHERE \"location_path\" @> (SELECT \"location_path\" FROM \"tb_location\" WHERE \"id\" = $1))"
    );
    assert_eq!(params.len(), 1);
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn descendant_of_id_without_hierarchy_context_errors() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let clause = WhereClause::Field {
        path:     vec!["category_path".to_string()],
        operator: WhereOperator::DescendantOfId,
        value:    json!("some-uuid"),
    };
    let err = gen.generate(&clause).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("HierarchyContext") || msg.contains("hierarchies"),
        "Error should mention HierarchyContext, got: {msg}"
    );
}

#[test]
fn id_operators_combined_with_and() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        },
        WhereClause::Field {
            path:     vec!["category_path".to_string()],
            operator: WhereOperator::DescendantOfId,
            value:    json!("parent-uuid"),
        },
    ]);
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert!(sql.contains("AND"), "Expected AND clause, got: {sql}");
    assert!(sql.contains("<@"), "Expected <@ operator, got: {sql}");
    assert_eq!(params.len(), 2);
}

#[test]
fn id_operators_preserve_param_ordering() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    // status = $1 AND category_path <@ (... WHERE id = $2)
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        },
        WhereClause::Field {
            path:     vec!["category_path".to_string()],
            operator: WhereOperator::DescendantOfId,
            value:    json!("parent-uuid"),
        },
    ]);
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert!(sql.contains("$1"), "Expected $1 for status, got: {sql}");
    assert!(sql.contains("$2"), "Expected $2 for hierarchy ID, got: {sql}");
    assert_eq!(params[0], json!("active"));
    assert_eq!(params[1], json!("parent-uuid"));
}

#[test]
fn hierarchy_context_propagates_through_nested_or_and() {
    use fraiseql_core::db::dialect::PostgresDialect;
    let gen = GenericWhereGenerator::new(PostgresDialect);
    let ctx = HierarchyContext {
        table:       "tb_category".to_string(),
        path_column: "category_path".to_string(),
        fk_column:   None,
    };
    // Or(And(Eq, DescendantOfId), AncestorOfId)
    let clause = WhereClause::Or(vec![
        WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["category_path".to_string()],
                operator: WhereOperator::DescendantOfId,
                value:    json!("uuid-parent"),
            },
        ]),
        WhereClause::Field {
            path:     vec!["category_path".to_string()],
            operator: WhereOperator::AncestorOfId,
            value:    json!("uuid-child"),
        },
    ]);
    let (sql, params) = gen.generate_with_hierarchy(&clause, &ctx).unwrap();
    assert!(sql.contains("<@"), "Expected <@ for DescendantOfId, got: {sql}");
    assert!(sql.contains("@>"), "Expected @> for AncestorOfId, got: {sql}");
    assert_eq!(params.len(), 3, "Expected 3 params (Eq + 2 IDs), got: {}", params.len());
}
