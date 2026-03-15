#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return;
    };

    let Ok(clause) = fraiseql_db::WhereClause::from_graphql_json(&value) else {
        return;
    };

    // Run through all four dialects — must never panic
    let _ = fraiseql_db::GenericWhereGenerator::new(fraiseql_db::PostgresDialect).generate(&clause);
    let _ = fraiseql_db::GenericWhereGenerator::new(fraiseql_db::MySqlDialect).generate(&clause);
    let _ = fraiseql_db::GenericWhereGenerator::new(fraiseql_db::SqliteDialect).generate(&clause);
    let _ =
        fraiseql_db::GenericWhereGenerator::new(fraiseql_db::SqlServerDialect).generate(&clause);
});
