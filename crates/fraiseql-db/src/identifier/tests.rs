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
fn test_mysql_simple_identifier() {
    assert_eq!(quote_mysql_identifier("v_user"), "`v_user`");
}

#[test]
fn test_mysql_schema_qualified() {
    assert_eq!(quote_mysql_identifier("mydb.v_user"), "`mydb`.`v_user`");
}

#[test]
fn test_mysql_three_part_name() {
    assert_eq!(quote_mysql_identifier("catalog.schema.table"), "`catalog`.`schema`.`table`");
}

#[test]
fn test_sqlite_simple_identifier() {
    assert_eq!(quote_sqlite_identifier("v_user"), "\"v_user\"");
}

#[test]
fn test_sqlite_schema_qualified() {
    assert_eq!(quote_sqlite_identifier("main.v_user"), "\"main\".\"v_user\"");
}

#[test]
fn test_sqlite_three_part_name() {
    assert_eq!(
        quote_sqlite_identifier("catalog.schema.table"),
        "\"catalog\".\"schema\".\"table\""
    );
}

#[test]
fn test_sqlserver_simple_identifier() {
    assert_eq!(quote_sqlserver_identifier("v_user"), "[v_user]");
}

#[test]
fn test_sqlserver_schema_qualified() {
    assert_eq!(quote_sqlserver_identifier("dbo.v_user"), "[dbo].[v_user]");
}

#[test]
fn test_sqlserver_three_part_name() {
    assert_eq!(quote_sqlserver_identifier("catalog.schema.table"), "[catalog].[schema].[table]");
}

// Delimiter-escape tests — the delimiter character must be doubled inside the quoted name.

#[test]
fn test_postgres_escapes_embedded_double_quote() {
    // A double-quote inside a PostgreSQL quoted identifier must be doubled ("").
    assert_eq!(quote_postgres_identifier("evil\"inject"), "\"evil\"\"inject\"");
}

#[test]
fn test_sqlite_escapes_embedded_double_quote() {
    assert_eq!(quote_sqlite_identifier("evil\"inject"), "\"evil\"\"inject\"");
}

#[test]
fn test_mysql_escapes_embedded_backtick() {
    // A backtick inside a MySQL quoted identifier must be doubled (``).
    assert_eq!(quote_mysql_identifier("evil`inject"), "`evil``inject`");
}

#[test]
fn test_sqlserver_escapes_embedded_bracket() {
    // A closing bracket ']' inside a SQL Server quoted identifier must be doubled ']]'.
    // Single identifier component containing ']':
    assert_eq!(quote_sqlserver_identifier("evil]inject"), "[evil]]inject]");
    // Schema-qualified name where each part escapes its own ']':
    assert_eq!(quote_sqlserver_identifier("dbo.evil]inject"), "[dbo].[evil]]inject]");
}
