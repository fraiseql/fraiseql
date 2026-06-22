use super::*;

#[test]
fn accepts_postgresql_scheme() {
    assert_eq!(
        parse_database_url("postgresql://user@localhost/db").expect("accepted"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn accepts_postgres_alias() {
    assert_eq!(
        parse_database_url("postgres://user@localhost/db").expect("accepted"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn accepts_postgresql_with_query_string() {
    assert_eq!(
        parse_database_url("postgresql://user:pw@host:5432/db?sslmode=require")
            .expect("query-string parameters must not affect scheme parsing"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn accepts_mysql_scheme() {
    assert_eq!(
        parse_database_url("mysql://localhost:3306/mydb").expect("accepted"),
        DatabaseScheme::MySql,
    );
}

#[test]
fn accepts_sqlite_scheme() {
    assert_eq!(
        parse_database_url("sqlite://./mydb.db").expect("accepted"),
        DatabaseScheme::Sqlite,
    );
}

#[test]
fn accepts_sqlserver_scheme() {
    assert_eq!(
        parse_database_url("sqlserver://localhost:1433").expect("accepted"),
        DatabaseScheme::SqlServer,
    );
}

#[test]
fn rejects_unknown_scheme_with_clear_message() {
    let err = parse_database_url("redis://localhost:6379")
        .expect_err("redis:// is not a supported database scheme")
        .to_string();
    assert!(
        err.starts_with(GUARD_MESSAGE_PREFIX),
        "diagnostic must start with the operator-facing prefix: {err}"
    );
    assert!(err.contains("\"redis\""), "missing observed-scheme reproduction: {err}");
}

#[test]
fn rejects_empty_string() {
    let err = parse_database_url("").expect_err("empty URL must be rejected").to_string();
    assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "{err}");
}

#[test]
fn rejects_url_without_scheme() {
    let err = parse_database_url("localhost:5432")
        .expect_err("URL without a scheme must be rejected")
        .to_string();
    assert!(err.contains("\"localhost:5432\""), "{err}");
}

#[test]
fn required_feature_matrix() {
    assert_eq!(DatabaseScheme::Postgres.required_feature(), None);
    assert_eq!(DatabaseScheme::MySql.required_feature(), Some("mysql"));
    assert_eq!(DatabaseScheme::Sqlite.required_feature(), Some("sqlite"));
    assert_eq!(DatabaseScheme::SqlServer.required_feature(), Some("sqlserver"));
}

mod sqlite_guard {
    use fraiseql_core::schema::{CompiledSchema, MutationDefinition, MutationOperation};

    use super::super::guard_sqlite_mutations;

    fn custom_mutation(name: &str) -> MutationDefinition {
        // `MutationDefinition::new` defaults `operation` to `Custom`.
        MutationDefinition::new(name, "MutationResponse")
    }

    fn op_mutation(name: &str, operation: MutationOperation) -> MutationDefinition {
        MutationDefinition {
            operation,
            ..MutationDefinition::new(name, "MutationResponse")
        }
    }

    #[test]
    fn accepts_schema_with_no_mutations() {
        let schema = CompiledSchema::default();
        guard_sqlite_mutations(&schema).expect("read-only schema must be allowed on SQLite");
    }

    #[test]
    fn accepts_insert_and_delete_mutations() {
        let mut schema = CompiledSchema::default();
        schema.mutations.push(op_mutation(
            "createUser",
            MutationOperation::Insert {
                table: "users".into(),
            },
        ));
        schema.mutations.push(op_mutation(
            "deleteUser",
            MutationOperation::Delete {
                table: "users".into(),
            },
        ));

        guard_sqlite_mutations(&schema)
            .expect("direct-SQL Insert/Delete mutations must be allowed on SQLite");
    }

    #[test]
    fn rejects_update_and_custom_mutations_and_names_them() {
        let mut schema = CompiledSchema::default();
        // An Insert is allowed and must not be flagged.
        schema.mutations.push(op_mutation(
            "createUser",
            MutationOperation::Insert {
                table: "users".into(),
            },
        ));
        schema.mutations.push(op_mutation(
            "updateUser",
            MutationOperation::Update {
                table: "users".into(),
            },
        ));
        schema.mutations.push(custom_mutation("doMagic"));

        let err = guard_sqlite_mutations(&schema)
            .expect_err("SQLite + Update/custom mutations must be rejected at startup")
            .to_string();

        assert!(err.contains("Insert/Delete"), "missing capability callout: {err}");
        assert!(err.contains("updateUser"), "missing update mutation name: {err}");
        assert!(err.contains("doMagic"), "missing custom mutation name: {err}");
        assert!(err.contains("2 Update or custom"), "missing offender count: {err}");
        assert!(!err.contains("createUser"), "Insert mutation must not be flagged: {err}");
    }

    #[test]
    fn truncates_long_mutation_lists_with_suffix() {
        let mut schema = CompiledSchema::default();
        for i in 0..5 {
            schema.mutations.push(custom_mutation(&format!("m{i}")));
        }

        let err = guard_sqlite_mutations(&schema)
            .expect_err("custom mutations must be rejected on SQLite")
            .to_string();

        assert!(err.contains("m0"), "missing first sample: {err}");
        assert!(err.contains("m2"), "missing third sample: {err}");
        assert!(!err.contains("m4"), "must truncate beyond sample window: {err}");
        assert!(err.contains("+2 more"), "missing overflow suffix: {err}");
    }
}
