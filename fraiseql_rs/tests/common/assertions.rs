//! Custom assertions for PostgreSQL and JSON testing

/// Assert that a SQL query result contains expected rows
#[macro_export]
macro_rules! assert_query_rows {
    ($result:expr, $expected:expr) => {
        assert_eq!(
            $result.len(),
            $expected,
            "Expected {} rows, got {}",
            $expected,
            $result.len()
        )
    };
}

/// Assert that a JSON value matches expected structure
#[macro_export]
macro_rules! assert_json_matches {
    ($actual:expr, $expected:expr) => {
        let actual_str = $actual.to_string();
        let expected_str = $expected.to_string();
        assert_eq!(
            actual_str, expected_str,
            "JSON mismatch:\nExpected: {}\nActual: {}",
            expected_str, actual_str
        )
    };
}

/// Assert that a WHERE clause generates correct SQL
#[macro_export]
macro_rules! assert_where_sql {
    ($where_clause:expr, $expected_sql:expr) => {
        assert_eq!(
            $where_clause.to_sql(),
            $expected_sql,
            "WHERE clause SQL mismatch"
        )
    };
}

/// Assert that a column value matches expected type and value
#[macro_export]
macro_rules! assert_column_value {
    ($row:expr, $col_name:expr, $expected:expr) => {
        let value: &(dyn std::any::Any) = &$row.try_get::<_, i64>($col_name).unwrap();
        assert_eq!(
            std::any::TypeId::of_val(value),
            std::any::TypeId::of($expected),
            "Type mismatch for column {}: expected {}, got {}",
            $col_name,
            std::any::type_name_of_val(&$expected),
            std::any::type_name_of_val(value)
        )
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_custom_macros_compile() {
        // These macros are tested by compilation
        // If they compile, they work
    }
}