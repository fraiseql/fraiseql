#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let fields: Vec<String> = data.lines().map(String::from).collect();
    if fields.is_empty() || fields.len() > 100 {
        return;
    }

    let gen = fraiseql_db::PostgresProjectionGenerator::new();
    let _ = gen.generate_projection_sql(&fields);
});
