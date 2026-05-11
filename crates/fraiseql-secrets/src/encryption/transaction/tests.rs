#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_isolation_level_display() {
    assert_eq!(IsolationLevel::ReadUncommitted.to_string(), "READ UNCOMMITTED");
    assert_eq!(IsolationLevel::ReadCommitted.to_string(), "READ COMMITTED");
    assert_eq!(IsolationLevel::RepeatableRead.to_string(), "REPEATABLE READ");
    assert_eq!(IsolationLevel::Serializable.to_string(), "SERIALIZABLE");
}

#[test]
fn test_transaction_state_display() {
    assert_eq!(TransactionState::Active.to_string(), "active");
    assert_eq!(TransactionState::Committed.to_string(), "committed");
    assert_eq!(TransactionState::RolledBack.to_string(), "rolled back");
    assert_eq!(TransactionState::Error.to_string(), "error");
}

#[test]
fn test_transaction_context_creation() {
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    assert_eq!(ctx.user_id, "user123");
    assert_eq!(ctx.session_id, "sess456");
    assert_eq!(ctx.request_id, "req789");
    assert_eq!(ctx.state, TransactionState::Active);
    assert_eq!(ctx.isolation_level, IsolationLevel::ReadCommitted);
    assert_eq!(ctx.key_version, 1);
    assert!(ctx.transaction_id.starts_with("txn_"));
}

#[test]
fn test_transaction_context_with_isolation() {
    let ctx = TransactionContext::new("user123", "sess456", "req789")
        .with_isolation(IsolationLevel::Serializable);
    assert_eq!(ctx.isolation_level, IsolationLevel::Serializable);
}

#[test]
fn test_transaction_context_with_key_version() {
    let ctx = TransactionContext::new("user123", "sess456", "req789").with_key_version(2);
    assert_eq!(ctx.key_version, 2);
}

#[test]
fn test_transaction_context_add_operation() {
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    ctx.add_operation("INSERT users");
    ctx.add_operation("UPDATE roles");
    assert_eq!(ctx.operation_count(), 2);
}

#[test]
fn test_transaction_context_with_metadata() {
    let ctx =
        TransactionContext::new("user123", "sess456", "req789").with_metadata("source", "api");
    assert_eq!(ctx.metadata.get("source"), Some(&"api".to_string()));
}

#[test]
fn test_transaction_context_with_role() {
    let ctx = TransactionContext::new("user123", "sess456", "req789").with_role("admin");
    assert_eq!(ctx.user_role, Some("admin".to_string()));
}

#[test]
fn test_transaction_context_with_client_ip() {
    let ctx =
        TransactionContext::new("user123", "sess456", "req789").with_client_ip("192.168.1.1");
    assert_eq!(ctx.client_ip, Some("192.168.1.1".to_string()));
}

#[test]
fn test_transaction_context_commit() {
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    assert_eq!(ctx.state, TransactionState::Active);
    ctx.commit();
    assert_eq!(ctx.state, TransactionState::Committed);
}

#[test]
fn test_transaction_context_rollback() {
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    ctx.add_operation("INSERT users");
    ctx.rollback();
    assert_eq!(ctx.state, TransactionState::RolledBack);
    assert_eq!(ctx.operation_count(), 0);
}

#[test]
fn test_transaction_context_error() {
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    ctx.error();
    assert_eq!(ctx.state, TransactionState::Error);
}

#[test]
fn test_transaction_context_is_active() {
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    assert!(ctx.is_active());
    ctx.commit();
    assert!(!ctx.is_active());
}

#[test]
fn test_savepoint_creation() {
    let sp = Savepoint::new("sp1", "txn123", 5);
    assert_eq!(sp.name, "sp1");
    assert_eq!(sp.transaction_id, "txn123");
    assert_eq!(sp.operations_before, 5);
}

#[test]
fn test_transaction_manager_begin() {
    let mut manager = TransactionManager::new();
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    let txn_id = ctx.transaction_id.clone();

    let result = manager.begin(ctx);
    let returned_id = result.unwrap_or_else(|e| panic!("expected Ok from begin: {e}"));
    assert_eq!(returned_id, txn_id);
    assert_eq!(manager.active_count(), 1);
}

#[test]
fn test_transaction_manager_get_transaction() {
    let mut manager = TransactionManager::new();
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    let txn_id = ctx.transaction_id.clone();

    manager.begin(ctx).unwrap();

    let retrieved = manager.get_transaction(&txn_id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().user_id, "user123");
}

#[test]
fn test_transaction_manager_commit() {
    let mut manager = TransactionManager::new();
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    let txn_id = ctx.transaction_id.clone();

    manager.begin(ctx).unwrap();
    let result = manager.commit(&txn_id);

    result.unwrap_or_else(|e| panic!("expected Ok from commit: {e}"));
    let txn = manager.get_transaction(&txn_id);
    assert_eq!(txn.unwrap().state, TransactionState::Committed);
}

#[test]
fn test_transaction_manager_rollback() {
    let mut manager = TransactionManager::new();
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    let txn_id = ctx.transaction_id.clone();

    manager.begin(ctx).unwrap();
    let result = manager.rollback(&txn_id);

    result.unwrap_or_else(|e| panic!("expected Ok from rollback: {e}"));
    let txn = manager.get_transaction(&txn_id);
    assert_eq!(txn.unwrap().state, TransactionState::RolledBack);
}

#[test]
fn test_transaction_manager_savepoint() {
    let mut manager = TransactionManager::new();
    let ctx = TransactionContext::new("user123", "sess456", "req789");
    let txn_id = ctx.transaction_id.clone();

    manager.begin(ctx).unwrap();
    let result = manager.savepoint(&txn_id, "sp1");

    result.unwrap_or_else(|e| panic!("expected Ok from savepoint: {e}"));
}

#[test]
fn test_transaction_manager_rollback_to_savepoint() {
    let mut manager = TransactionManager::new();
    let mut ctx = TransactionContext::new("user123", "sess456", "req789");
    ctx.add_operation("OP1");
    let txn_id = ctx.transaction_id.clone();

    manager.begin(ctx).unwrap();
    manager.savepoint(&txn_id, "sp1").unwrap();

    {
        let txn = manager.get_transaction_mut(&txn_id).unwrap();
        txn.add_operation("OP2");
    }

    let result = manager.rollback_to_savepoint(&txn_id, "sp1");
    result.unwrap_or_else(|e| panic!("expected Ok from rollback_to_savepoint: {e}"));

    let txn = manager.get_transaction(&txn_id).unwrap();
    assert_eq!(txn.operation_count(), 1);
}

#[test]
fn test_transaction_manager_active_transactions() {
    let mut manager = TransactionManager::new();
    let ctx1 = TransactionContext::new("user1", "sess1", "req1");
    let ctx2 = TransactionContext::new("user2", "sess2", "req2");

    manager.begin(ctx1).unwrap();
    manager.begin(ctx2).unwrap();

    let active = manager.active_transactions();
    assert_eq!(active.len(), 2);
}

#[test]
fn test_transaction_manager_cleanup_completed() {
    let mut manager = TransactionManager::new();
    let ctx1 = TransactionContext::new("user1", "sess1", "req1");
    let ctx2 = TransactionContext::new("user2", "sess2", "req2");

    let id1 = ctx1.transaction_id.clone();

    manager.begin(ctx1).unwrap();
    manager.begin(ctx2).unwrap();

    manager.commit(&id1).unwrap();
    manager.cleanup_completed();

    assert_eq!(manager.active_count(), 1);
    assert!(manager.get_transaction(&id1).is_none());
}
