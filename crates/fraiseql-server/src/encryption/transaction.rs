//! Transaction context management for encrypted operations.
//!
//! Tracks transaction metadata, user context, and ensures consistent
//! encryption key usage throughout transaction lifecycle.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::secrets_manager::SecretsError;

/// Transaction isolation level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Read uncommitted (highest concurrency, lowest isolation)
    ReadUncommitted,
    /// Read committed (most common)
    ReadCommitted,
    /// Repeatable read
    RepeatableRead,
    /// Serializable (strongest isolation, lowest concurrency)
    Serializable,
}

impl std::fmt::Display for IsolationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadUncommitted => write!(f, "READ UNCOMMITTED"),
            Self::ReadCommitted => write!(f, "READ COMMITTED"),
            Self::RepeatableRead => write!(f, "REPEATABLE READ"),
            Self::Serializable => write!(f, "SERIALIZABLE"),
        }
    }
}

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Transaction started
    Active,
    /// Committed (not yet finalized)
    Committed,
    /// Rolled back
    RolledBack,
    /// Error occurred
    Error,
}

impl std::fmt::Display for TransactionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Committed => write!(f, "committed"),
            Self::RolledBack => write!(f, "rolled back"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Context for transaction with encryption awareness
#[derive(Debug, Clone)]
pub struct TransactionContext {
    /// Unique transaction ID
    pub transaction_id:  String,
    /// User initiating transaction
    pub user_id:         String,
    /// User session ID
    pub session_id:      String,
    /// HTTP request ID for correlation
    pub request_id:      String,
    /// Transaction start time
    pub started_at:      DateTime<Utc>,
    /// Isolation level
    pub isolation_level: IsolationLevel,
    /// Current state
    pub state:           TransactionState,
    /// Encryption key version used in transaction
    pub key_version:     u32,
    /// List of operations in transaction
    pub operations:      Vec<String>,
    /// Additional context data
    pub metadata:        HashMap<String, String>,
    /// User role for access control
    pub user_role:       Option<String>,
    /// Client IP address for audit
    pub client_ip:       Option<String>,
}

impl TransactionContext {
    /// Create new transaction context
    pub fn new(
        user_id: impl Into<String>,
        session_id: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        // Generate unique transaction ID
        let transaction_id = format!(
            "txn_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        Self {
            transaction_id,
            user_id: user_id.into(),
            session_id: session_id.into(),
            request_id: request_id.into(),
            started_at: Utc::now(),
            isolation_level: IsolationLevel::ReadCommitted,
            state: TransactionState::Active,
            key_version: 1,
            operations: Vec::new(),
            metadata: HashMap::new(),
            user_role: None,
            client_ip: None,
        }
    }

    /// Set isolation level
    pub fn with_isolation(mut self, level: IsolationLevel) -> Self {
        self.isolation_level = level;
        self
    }

    /// Set key version
    pub fn with_key_version(mut self, version: u32) -> Self {
        self.key_version = version;
        self
    }

    /// Add operation to transaction
    pub fn add_operation(&mut self, operation: impl Into<String>) {
        self.operations.push(operation.into());
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set user role
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.user_role = Some(role.into());
        self
    }

    /// Set client IP
    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    /// Mark transaction as committed
    pub fn commit(&mut self) {
        self.state = TransactionState::Committed;
    }

    /// Mark transaction as rolled back
    pub fn rollback(&mut self) {
        self.state = TransactionState::RolledBack;
        self.operations.clear();
    }

    /// Mark transaction as error
    pub fn error(&mut self) {
        self.state = TransactionState::Error;
    }

    /// Get transaction duration
    pub fn duration(&self) -> chrono::Duration {
        Utc::now() - self.started_at
    }

    /// Check if transaction is still active
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// Get operation count
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }
}

/// Transaction savepoint for nested transactions
#[derive(Debug, Clone)]
pub struct Savepoint {
    /// Savepoint name
    pub name:              String,
    /// Transaction ID this savepoint belongs to
    pub transaction_id:    String,
    /// Created at timestamp
    pub created_at:        DateTime<Utc>,
    /// Operations before savepoint
    pub operations_before: usize,
}

impl Savepoint {
    /// Create new savepoint
    pub fn new(
        name: impl Into<String>,
        transaction_id: impl Into<String>,
        operations_count: usize,
    ) -> Self {
        Self {
            name:              name.into(),
            transaction_id:    transaction_id.into(),
            created_at:        Utc::now(),
            operations_before: operations_count,
        }
    }
}

/// Transaction manager for coordinating encrypted operations
pub struct TransactionManager {
    /// Active transactions by ID
    active_transactions: HashMap<String, TransactionContext>,
    /// Savepoints by transaction ID
    savepoints:          HashMap<String, Vec<Savepoint>>,
}

impl TransactionManager {
    /// Create new transaction manager
    pub fn new() -> Self {
        Self {
            active_transactions: HashMap::new(),
            savepoints:          HashMap::new(),
        }
    }

    /// Begin transaction
    pub fn begin(&mut self, context: TransactionContext) -> Result<String, SecretsError> {
        let txn_id = context.transaction_id.clone();

        if self.active_transactions.contains_key(&txn_id) {
            return Err(SecretsError::ValidationError(format!(
                "Transaction {} already active",
                txn_id
            )));
        }

        self.active_transactions.insert(txn_id.clone(), context);
        Ok(txn_id)
    }

    /// Get active transaction
    pub fn get_transaction(&self, txn_id: &str) -> Option<&TransactionContext> {
        self.active_transactions.get(txn_id)
    }

    /// Get mutable transaction reference
    pub fn get_transaction_mut(&mut self, txn_id: &str) -> Option<&mut TransactionContext> {
        self.active_transactions.get_mut(txn_id)
    }

    /// Commit transaction
    pub fn commit(&mut self, txn_id: &str) -> Result<(), SecretsError> {
        if let Some(txn) = self.active_transactions.get_mut(txn_id) {
            txn.commit();
            self.savepoints.remove(txn_id);
            Ok(())
        } else {
            Err(SecretsError::ValidationError(format!("Transaction {} not found", txn_id)))
        }
    }

    /// Rollback transaction
    pub fn rollback(&mut self, txn_id: &str) -> Result<(), SecretsError> {
        if let Some(txn) = self.active_transactions.get_mut(txn_id) {
            txn.rollback();
            self.savepoints.remove(txn_id);
            Ok(())
        } else {
            Err(SecretsError::ValidationError(format!("Transaction {} not found", txn_id)))
        }
    }

    /// Create savepoint
    pub fn savepoint(&mut self, txn_id: &str, name: impl Into<String>) -> Result<(), SecretsError> {
        if let Some(txn) = self.active_transactions.get(txn_id) {
            let savepoint = Savepoint::new(name, txn_id, txn.operation_count());
            self.savepoints.entry(txn_id.to_string()).or_default().push(savepoint);
            Ok(())
        } else {
            Err(SecretsError::ValidationError(format!("Transaction {} not found", txn_id)))
        }
    }

    /// Rollback to savepoint
    pub fn rollback_to_savepoint(&mut self, txn_id: &str, name: &str) -> Result<(), SecretsError> {
        if let Some(savepoints) = self.savepoints.get_mut(txn_id) {
            if let Some(sp_idx) = savepoints.iter().position(|sp| sp.name == name) {
                let savepoint = savepoints.remove(sp_idx);

                if let Some(txn) = self.active_transactions.get_mut(txn_id) {
                    // Trim operations to what existed before savepoint
                    txn.operations.truncate(savepoint.operations_before);
                    return Ok(());
                }
            }
            Err(SecretsError::ValidationError(format!("Savepoint {} not found", name)))
        } else {
            Err(SecretsError::ValidationError(format!(
                "Transaction {} has no savepoints",
                txn_id
            )))
        }
    }

    /// Get list of active transaction IDs
    pub fn active_transactions(&self) -> Vec<&str> {
        self.active_transactions.keys().map(|s| s.as_str()).collect()
    }

    /// Count active transactions
    pub fn active_count(&self) -> usize {
        self.active_transactions.len()
    }

    /// Clear completed transactions
    pub fn cleanup_completed(&mut self) {
        self.active_transactions.retain(|_, txn| txn.is_active());
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), txn_id);
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

        assert!(result.is_ok());
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

        assert!(result.is_ok());
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

        assert!(result.is_ok());
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
        assert!(result.is_ok());

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
}
