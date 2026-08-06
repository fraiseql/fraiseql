use super::{
    ENTITY_CHANGE_LOG_CONTRACT_SQL, entity_change_log_provision_sql,
    entity_change_log_provision_statements,
};

/// The statement splitter must reproduce every DDL object of the script: the
/// preamble (schema + 2 drops), the backbone CREATE, the ALTERs, the sequence
/// wiring, indexes and the constraint — and never split inside a `$$` body.
#[test]
fn provision_statements_cover_the_script() {
    let stmts = entity_change_log_provision_statements();
    assert!(
        stmts.len() >= 8,
        "expected the full statement list, got {}: {:?}",
        stmts.len(),
        stmts.iter().map(|s| &s[..s.len().min(40)]).collect::<Vec<_>>()
    );
    assert!(stmts[0].starts_with("CREATE SCHEMA IF NOT EXISTS core"));
    assert!(
        stmts
            .iter()
            .any(|s| s.starts_with("CREATE TABLE IF NOT EXISTS core.tb_entity_change_log"))
    );
    for s in &stmts {
        let dollars = s.matches("$$").count();
        assert!(dollars % 2 == 0, "unbalanced $$ in split statement: {s}");
    }
}

/// The provisioner carries the owning migration's contract, not a local copy.
#[test]
fn provision_script_embeds_the_owning_contract() {
    let script = entity_change_log_provision_sql();
    assert!(script.contains("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE"));
    assert!(script.ends_with(ENTITY_CHANGE_LOG_CONTRACT_SQL));
    // Contract invariants the consumers rely on: the nullable object_id (a
    // checkpoint upsert has no entity id) and the UUID id column.
    assert!(ENTITY_CHANGE_LOG_CONTRACT_SQL.contains("ADD COLUMN IF NOT EXISTS object_id"));
    assert!(!ENTITY_CHANGE_LOG_CONTRACT_SQL.contains("object_id          TEXT"));
}
