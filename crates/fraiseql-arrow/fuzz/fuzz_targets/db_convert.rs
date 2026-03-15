#![no_main]

use std::collections::HashMap;
use std::sync::Arc;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(rows) = serde_json::from_str::<Vec<HashMap<String, serde_json::Value>>>(s) else {
        return;
    };

    if rows.is_empty() || rows.len() > 100 {
        return;
    }

    // Build a simple schema from first row's keys (all Utf8)
    let first = &rows[0];
    let fields: Vec<arrow::datatypes::Field> = first
        .keys()
        .map(|k| arrow::datatypes::Field::new(k, arrow::datatypes::DataType::Utf8, true))
        .collect();

    if fields.is_empty() {
        return;
    }

    let schema = Arc::new(arrow::datatypes::Schema::new(fields));

    // convert_db_rows_to_arrow must never panic
    let _ = fraiseql_arrow::db_convert::convert_db_rows_to_arrow(&rows, &schema);
});
