use super::*;

#[test]
fn test_classify_select_is_readonly() {
    let result = classify_sql("SELECT * FROM users");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
}

#[test]
fn test_classify_select_with_where_is_readonly() {
    let result = classify_sql("SELECT id, name FROM users WHERE active = true");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
}

#[test]
fn test_classify_insert_is_rejected() {
    let result = classify_sql("INSERT INTO users (name) VALUES ('Alice')");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
        other => panic!("expected WriteStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_update_is_rejected() {
    let result = classify_sql("UPDATE users SET name = 'Bob' WHERE id = 1");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
        other => panic!("expected WriteStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_delete_is_rejected() {
    let result = classify_sql("DELETE FROM users WHERE id = 1");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
        other => panic!("expected WriteStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_create_table_is_rejected() {
    let result = classify_sql("CREATE TABLE new_table (id INT, name TEXT)");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
        other => panic!("expected DdlStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_drop_is_rejected() {
    let result = classify_sql("DROP TABLE users");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
        other => panic!("expected DdlStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_truncate_is_rejected() {
    let result = classify_sql("TRUNCATE users");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
        other => panic!("expected DdlStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_explain_without_analyze_is_readonly() {
    let result = classify_sql("EXPLAIN SELECT * FROM users");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
}

#[test]
fn test_classify_explain_analyze_is_rejected() {
    let result = classify_sql("EXPLAIN ANALYZE SELECT * FROM users");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::ExplainAnalyze) => (),
        other => panic!("expected ExplainAnalyze, got {:?}", other),
    }
}

#[test]
fn test_classify_set_role_is_rejected() {
    let result = classify_sql("SET ROLE admin");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::PrivilegeEscalation) => (),
        other => panic!("expected PrivilegeEscalation, got {:?}", other),
    }
}

#[test]
fn test_classify_unknown_statement_is_rejected() {
    // Most DDL variants are explicitly handled, unknown ones fall through to Unknown
    let result = classify_sql("ANALYZE TABLE users");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(_) => (),
        other @ SqlClassification::ReadOnly => panic!("expected Rejected, got {:?}", other),
    }
}

#[test]
fn test_classify_call_is_rejected() {
    let result = classify_sql("CALL delete_all_users()");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::ProcedureCall) => (),
        other => panic!("expected ProcedureCall, got {:?}", other),
    }
}

#[test]
fn test_classify_copy_is_rejected() {
    let result = classify_sql("COPY users FROM '/tmp/data.csv'");
    assert!(result.is_ok());
    match result.unwrap() {
        SqlClassification::Rejected(RejectionReason::CopyStatement) => (),
        other => panic!("expected CopyStatement, got {:?}", other),
    }
}

#[test]
fn test_classify_invalid_sql_returns_error() {
    let result = classify_sql("INVALID SYNTAX HERE");
    assert!(result.is_err());
}
