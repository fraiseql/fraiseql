#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Snapshot tests for fraiseql-db SQL dialect generators.
//!
//! Each test covers one SQL construct across supported dialects, verifying that
//! dialect-specific SQL differences (placeholder style, JSON access, quoting)
//! are preserved across changes.
//!
//! To generate or update snapshots:
//! ```bash
//! INSTA_UPDATE=always cargo test --test dialect_snapshots -p fraiseql-db
//! ```

use fraiseql_db::{PostgresDialect, WhereClause, WhereOperator, postgres::PostgresWhereGenerator};
use insta::assert_snapshot;
use serde_json::json;

const fn pg() -> PostgresWhereGenerator {
    PostgresWhereGenerator::new(PostgresDialect)
}

// =============================================================================
// PostgreSQL — individual operators
// =============================================================================

mod pg_operators {
    use super::*;

    #[test]
    fn eq_string() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn eq_numeric() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn neq() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Neq,
            value:    json!("deleted"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn gt() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn gte() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(100),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn lt() {
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Lt,
            value:    json!(18),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn lte() {
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Lte,
            value:    json!(65),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn like() {
        let clause = WhereClause::Field {
            path:     vec!["title".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn ilike() {
        let clause = WhereClause::Field {
            path:     vec!["title".to_string()],
            operator: WhereOperator::Ilike,
            value:    json!("%rust%"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn contains() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Contains,
            value:    json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn icontains() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("alice"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn startswith() {
        let clause = WhereClause::Field {
            path:     vec!["username".to_string()],
            operator: WhereOperator::Startswith,
            value:    json!("admin"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn endswith() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Endswith,
            value:    json!("@example.com"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn in_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending", "review"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn nin_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Nin,
            value:    json!(["deleted", "banned"]),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn is_null_true() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn is_null_false() {
        let clause = WhereClause::Field {
            path:     vec!["published_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(false),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }
}

// =============================================================================
// PostgreSQL — compound clauses
// =============================================================================

mod pg_compound {
    use super::*;

    #[test]
    fn and_two_fields() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["published".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Field {
                path:     vec!["author_id".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("00000000-0000-0000-0000-000000000001"),
            },
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn or_two_fields() {
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("superuser"),
            },
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn nested_and_or() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path:     vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("admin"),
                },
                WhereClause::Field {
                    path:     vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("mod"),
                },
            ]),
        ]);
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn deep_nested_path() {
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };
        let (sql, _params) = pg().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn param_offset_two() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Alice"),
        };
        let (sql, _params) = pg().generate_with_param_offset(&clause, 2).unwrap();
        assert_snapshot!(sql);
    }
}

// =============================================================================
// MySQL — dialect parity
// =============================================================================

#[cfg(feature = "mysql")]
mod mysql_operators {
    use fraiseql_db::{MySqlDialect, mysql::MySqlWhereGenerator};
    use insta::assert_snapshot;
    use serde_json::json;

    use super::{WhereClause, WhereOperator};

    const fn my() -> MySqlWhereGenerator {
        MySqlWhereGenerator::new(MySqlDialect)
    }

    #[test]
    fn eq_string() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn like() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%alice%"),
        };
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn in_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn is_null_true() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn nested_path() {
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn and_compound() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gt,
                value:    json!(18),
            },
        ]);
        let (sql, _params) = my().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }
}

// =============================================================================
// SQLite — dialect parity
// =============================================================================

#[cfg(feature = "sqlite")]
mod sqlite_operators {
    use fraiseql_db::{SqliteDialect, sqlite::SqliteWhereGenerator};
    use insta::assert_snapshot;
    use serde_json::json;

    use super::{WhereClause, WhereOperator};

    const fn sq() -> SqliteWhereGenerator {
        SqliteWhereGenerator::new(SqliteDialect)
    }

    #[test]
    fn eq_string() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn like() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%alice%"),
        };
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn gt_numeric() {
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(50),
        };
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn is_null_true() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn nested_path() {
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn and_compound() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gt,
                value:    json!(18),
            },
        ]);
        let (sql, _params) = sq().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }
}

// =============================================================================
// SQL Server — dialect parity
// =============================================================================

#[cfg(feature = "sqlserver")]
mod sqlserver_operators {
    use fraiseql_db::{SqlServerDialect, sqlserver::SqlServerWhereGenerator};
    use insta::assert_snapshot;
    use serde_json::json;

    use super::{WhereClause, WhereOperator};

    const fn ss() -> SqlServerWhereGenerator {
        SqlServerWhereGenerator::new(SqlServerDialect)
    }

    #[test]
    fn eq_string() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };
        let (sql, _params) = ss().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn like() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Like,
            value:    json!("%alice%"),
        };
        let (sql, _params) = ss().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn is_null_true() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };
        let (sql, _params) = ss().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn nested_path() {
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };
        let (sql, _params) = ss().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }

    #[test]
    fn and_compound() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gt,
                value:    json!(18),
            },
        ]);
        let (sql, _params) = ss().generate(&clause).unwrap();
        assert_snapshot!(sql);
    }
}
