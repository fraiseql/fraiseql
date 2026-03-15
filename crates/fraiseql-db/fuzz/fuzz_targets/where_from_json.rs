#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(s) else {
        return;
    };

    match fraiseql_db::WhereClause::from_graphql_json(&value) {
        Ok(clause) => {
            let _ = serde_json::to_string(&clause);
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Error must produce non-empty message");
        }
    }
});
