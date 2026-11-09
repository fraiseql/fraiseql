//! The catalog answer is read strictly: a row the reader cannot account for is an error,
//! never "no policy".

use std::collections::HashMap;

use super::{policies_reading_settings_sql, policy_setting_reads};

fn row(columns: &[(&str, &str)]) -> HashMap<String, serde_json::Value> {
    columns
        .iter()
        .map(|(k, v)| ((*k).to_string(), serde_json::Value::from(*v)))
        .collect()
}

#[test]
fn a_complete_row_is_read() {
    let reads = policy_setting_reads(&[row(&[
        ("schema_name", "public"),
        ("table_name", "tb_order"),
        ("policy_name", "by_region"),
        ("setting", "app.region"),
    ])])
    .expect("a complete row reads");
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].described(),
        "policy by_region on public.tb_order reads app.region, which comes from a request \
         header: any client can send any value"
    );
}

/// A row missing a column would otherwise read as no policy at all, the permissive answer.
#[test]
fn a_row_without_a_column_is_an_error() {
    let refusal = policy_setting_reads(&[row(&[
        ("schema_name", "public"),
        ("table_name", "tb_order"),
        ("setting", "app.region"),
    ])])
    .expect_err("a row without policy_name");
    assert!(refusal.to_string().contains("policy_name"), "{refusal}");
}

/// The setting names are inlined into the statement, each a quote-doubled literal.
#[test]
fn a_setting_name_is_inlined_as_a_quoted_literal() {
    let sql = policies_reading_settings_sql(&["app.region".to_string(), "o'brien".to_string()]);
    assert!(sql.contains("ARRAY['app.region', 'o''brien']::text[]"), "{sql}");
}
