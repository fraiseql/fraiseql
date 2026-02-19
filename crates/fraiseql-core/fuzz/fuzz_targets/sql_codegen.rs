#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Deserialize arbitrary JSON into a WhereClause, then generate SQL.
    // Neither step should panic on any input.
    if let Ok(clause) = serde_json::from_str::<fraiseql_core::db::WhereClause>(data) {
        let _ = fraiseql_core::db::WhereSqlGenerator::to_sql(&clause);
    }
});
