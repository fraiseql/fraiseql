use super::*;

#[test]
fn test_postgres_simple_identifier() {
    assert_eq!(quote_postgres_identifier("v_user"), "\"v_user\"");
}

#[test]
fn test_postgres_schema_qualified() {
    assert_eq!(quote_postgres_identifier("benchmark.v_user"), "\"benchmark\".\"v_user\"");
}

#[test]
fn test_postgres_three_part_name() {
    assert_eq!(
        quote_postgres_identifier("catalog.schema.table"),
        "\"catalog\".\"schema\".\"table\""
    );
}

#[test]
fn test_postgres_escapes_embedded_double_quote() {
    // A double-quote inside a PostgreSQL quoted identifier must be doubled ("").
    assert_eq!(quote_postgres_identifier("evil\"inject"), "\"evil\"\"inject\"");
}
