# Phase 14: Audit Logging

**Phase**: GREENFIELD → GREEN → QA
**Status**: Planning
**Dependencies**: Phase 10 (Auth), Phase 11 (RBAC), Phase 12 (Constraints)

---

## 🎯 Objective

Implement production-ready audit logging in Rust with PostgreSQL backend:
1. **Audit Logging**: Track all GraphQL operations with full context
2. **Multi-tenant Isolation**: Logs separated by tenant
3. **Efficient Storage**: PostgreSQL with JSONB, time-series optimized
4. **Fast Queries**: Indexed for common filtering patterns

**Key Goals**:
- ✅ 10-100x faster than Python logging
- ✅ Comprehensive context (user, tenant, IP, query, variables, errors)
- ✅ Production-ready async integration
- ✅ Efficient querying and filtering

---

## 📋 Context

### FraiseQL Architecture

**Important**: FraiseQL uses existing `tokio-postgres` + `deadpool-postgres`:
- ✅ Database pool already available (`InternalDatabasePool`)
- ✅ Async PostgreSQL support via tokio-postgres
- ❌ **Do NOT use sqlx** - use existing infrastructure

### What We're Adding

1. **Audit Log Table**:
   - `fraiseql_audit_logs` table with JSONB for variables
   - Indexes for common queries (tenant, level, timestamp)
   - Time-series optimized (partition-ready)

2. **Rust Audit Logger**:
   - Uses existing `InternalDatabasePool`
   - Async logging with tokio-postgres
   - Multi-tenant isolation

3. **Python Wrappers**:
   - `AuditLogger` class with async support
   - Helper methods for common scenarios
   - Integration with existing auth/RBAC

---

## 📁 Files to Create/Modify

### Database Migration

1. **`migrations/001_audit_logs.sql`** (NEW)
   - Create `fraiseql_audit_logs` table
   - Add indexes for performance
   - Optional: partitioning for large datasets

### Rust Files (New)

2. **`fraiseql_rs/src/security/audit.rs`** (NEW)
   - `AuditLogger` struct
   - `AuditEntry` struct
   - `AuditLevel` enum (INFO, WARN, ERROR)
   - Async logging methods

3. **`fraiseql_rs/src/security/py_bindings.rs`** (MODIFY)
   - Add `PyAuditLogger` wrapper
   - Async Python integration

### Python Files (New)

4. **`src/fraiseql/enterprise/security/audit.py`** (NEW)
   - Python `AuditLogger` wrapper
   - Helper functions
   - Type hints

### Tests (New)

5. **`tests/test_audit_logging.py`** (NEW)
   - 10+ comprehensive tests
   - Multi-tenant isolation tests
   - Performance validation

---

## 🔧 Implementation

### Database Migration

**File**: `migrations/001_audit_logs.sql`

```sql
-- Audit log table
CREATE TABLE IF NOT EXISTS fraiseql_audit_logs (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    level TEXT NOT NULL CHECK (level IN ('INFO', 'WARN', 'ERROR')),

    -- User context
    user_id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,

    -- Request details
    operation TEXT NOT NULL CHECK (operation IN ('query', 'mutation')),
    query TEXT NOT NULL,
    variables JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Client info
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,

    -- Error tracking
    error TEXT,

    -- Performance tracking (optional)
    duration_ms INTEGER,

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_audit_logs_tenant_timestamp
    ON fraiseql_audit_logs(tenant_id, timestamp DESC);

CREATE INDEX idx_audit_logs_tenant_level
    ON fraiseql_audit_logs(tenant_id, level, timestamp DESC);

CREATE INDEX idx_audit_logs_user
    ON fraiseql_audit_logs(user_id, timestamp DESC);

CREATE INDEX idx_audit_logs_timestamp
    ON fraiseql_audit_logs(timestamp DESC);

-- Optional: Add partitioning for large datasets
-- COMMENT: For production with millions of logs, consider time-based partitioning
-- Example: Partition by month
-- CREATE TABLE fraiseql_audit_logs_2026_01 PARTITION OF fraiseql_audit_logs
--     FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
```

### Rust Implementation

**File**: `fraiseql_rs/src/security/audit.rs`

```rust
//! Audit logging for GraphQL operations
//!
//! Uses existing `InternalDatabasePool` from `crate::db::pool`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::db::pool::InternalDatabasePool;

/// Audit log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    /// Informational messages
    INFO,
    /// Warnings
    WARN,
    /// Errors
    ERROR,
}

impl AuditLevel {
    /// Convert to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditLevel::INFO => "INFO",
            AuditLevel::WARN => "WARN",
            AuditLevel::ERROR => "ERROR",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s {
            "INFO" => AuditLevel::INFO,
            "WARN" => AuditLevel::WARN,
            "ERROR" => AuditLevel::ERROR,
            _ => AuditLevel::INFO,
        }
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry ID (None for new entries)
    pub id: Option<i64>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Log level
    pub level: AuditLevel,
    /// User ID
    pub user_id: i64,
    /// Tenant ID
    pub tenant_id: i64,
    /// Operation type (query, mutation)
    pub operation: String,
    /// GraphQL query string
    pub query: String,
    /// Query variables (JSONB)
    pub variables: serde_json::Value,
    /// Client IP address
    pub ip_address: String,
    /// Client user agent
    pub user_agent: String,
    /// Error message (if any)
    pub error: Option<String>,
    /// Query duration in milliseconds (optional)
    pub duration_ms: Option<i32>,
}

/// Audit logger with PostgreSQL backend
#[derive(Clone)]
pub struct AuditLogger {
    pool: Arc<InternalDatabasePool>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(pool: Arc<InternalDatabasePool>) -> Self {
        Self { pool }
    }

    /// Log an audit entry
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn log(&self, entry: AuditEntry) -> Result<i64, anyhow::Error> {
        let query = r#"
            INSERT INTO fraiseql_audit_logs (
                timestamp,
                level,
                user_id,
                tenant_id,
                operation,
                query,
                variables,
                ip_address,
                user_agent,
                error,
                duration_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
        "#;

        // Execute query and get ID
        let variables_str = serde_json::to_string(&entry.variables)?;

        // Note: This is a simplified implementation
        // In production, we'd use prepared statements with the pool's execute method
        let result = self.pool.execute_query(query).await?;

        // Parse the result to get the ID
        if let Some(first_row) = result.first() {
            if let Some(id_value) = first_row.get("id") {
                if let Some(id) = id_value.as_i64() {
                    return Ok(id);
                }
            }
        }

        Err(anyhow::anyhow!("Failed to get inserted audit log ID"))
    }

    /// Get recent logs for a tenant
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn get_recent_logs(
        &self,
        tenant_id: i64,
        level: Option<AuditLevel>,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, anyhow::Error> {
        let query = if let Some(lvl) = level {
            format!(
                r#"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error, duration_ms
                FROM fraiseql_audit_logs
                WHERE tenant_id = {} AND level = '{}'
                ORDER BY timestamp DESC
                LIMIT {}
                "#,
                tenant_id,
                lvl.as_str(),
                limit
            )
        } else {
            format!(
                r#"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error, duration_ms
                FROM fraiseql_audit_logs
                WHERE tenant_id = {}
                ORDER BY timestamp DESC
                LIMIT {}
                "#,
                tenant_id, limit
            )
        };

        let rows = self.pool.execute_query(&query).await?;

        let entries: Result<Vec<_>, anyhow::Error> = rows
            .into_iter()
            .map(|row| {
                Ok(AuditEntry {
                    id: row.get("id").and_then(|v| v.as_i64()),
                    timestamp: {
                        let ts_str = row.get("timestamp")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| anyhow::anyhow!("Missing timestamp"))?;
                        DateTime::parse_from_rfc3339(ts_str)?
                            .with_timezone(&Utc)
                    },
                    level: {
                        let level_str = row.get("level")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| anyhow::anyhow!("Missing level"))?;
                        AuditLevel::from_str(level_str)
                    },
                    user_id: row.get("user_id")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::anyhow!("Missing user_id"))?,
                    tenant_id: row.get("tenant_id")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id"))?,
                    operation: row.get("operation")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing operation"))?
                        .to_string(),
                    query: row.get("query")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing query"))?
                        .to_string(),
                    variables: {
                        let vars_str = row.get("variables")
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}");
                        serde_json::from_str(vars_str)?
                    },
                    ip_address: row.get("ip_address")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing ip_address"))?
                        .to_string(),
                    user_agent: row.get("user_agent")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing user_agent"))?
                        .to_string(),
                    error: row.get("error").and_then(|v| v.as_str()).map(String::from),
                    duration_ms: row.get("duration_ms").and_then(|v| v.as_i64()).map(|v| v as i32),
                })
            })
            .collect();

        entries
    }
}
```

### Python Bindings

**File**: `fraiseql_rs/src/security/py_bindings.rs` (add to existing file)

```rust
// Add to existing py_bindings.rs

use super::audit::{AuditEntry, AuditLevel, AuditLogger};

/// Python wrapper for audit logger
#[pyclass]
pub struct PyAuditLogger {
    logger: AuditLogger,
}

#[pymethods]
impl PyAuditLogger {
    /// Create a new audit logger from existing pool
    #[new]
    fn new(pool: &crate::db::pool::DatabasePool) -> Self {
        Self {
            logger: AuditLogger::new(pool.inner.clone()),
        }
    }

    /// Log an audit entry
    fn log<'py>(
        &self,
        py: Python<'py>,
        level: &str,
        user_id: i64,
        tenant_id: i64,
        operation: String,
        query: String,
        variables: &Bound<'py, PyDict>,
        ip_address: String,
        user_agent: String,
        error: Option<String>,
        duration_ms: Option<i32>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let level = match level {
            "INFO" => AuditLevel::INFO,
            "WARN" => AuditLevel::WARN,
            "ERROR" => AuditLevel::ERROR,
            _ => AuditLevel::INFO,
        };

        // Convert PyDict to serde_json::Value
        let variables_json: serde_json::Value = {
            let mut map = serde_json::Map::new();
            for (key, value) in variables.iter() {
                let key_str: String = key.extract()?;
                let value_json = python_to_json(&value)?;
                map.insert(key_str, value_json);
            }
            serde_json::Value::Object(map)
        };

        let entry = AuditEntry {
            id: None,
            timestamp: chrono::Utc::now(),
            level,
            user_id,
            tenant_id,
            operation,
            query,
            variables: variables_json,
            ip_address,
            user_agent,
            error,
            duration_ms,
        };

        let logger = self.logger.clone();

        future_into_py(py, async move {
            let id = logger.log(entry).await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Failed to log: {}", e)
                ))?;
            Ok(id)
        })
    }

    /// Get recent logs for a tenant
    fn get_recent_logs<'py>(
        &self,
        py: Python<'py>,
        tenant_id: i64,
        level: Option<String>,
        limit: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let level_enum = level.and_then(|s| match s.as_str() {
            "INFO" => Some(AuditLevel::INFO),
            "WARN" => Some(AuditLevel::WARN),
            "ERROR" => Some(AuditLevel::ERROR),
            _ => None,
        });

        let logger = self.logger.clone();

        future_into_py(py, async move {
            let entries = logger.get_recent_logs(tenant_id, level_enum, limit).await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Failed to get logs: {}", e)
                ))?;

            // Convert to JSON strings (Python will parse them)
            let json_strings: Result<Vec<String>, _> = entries
                .iter()
                .map(serde_json::to_string)
                .collect();

            json_strings.map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Failed to serialize logs: {}", e)
                )
            })
        })
    }
}

// Helper function to convert Python objects to JSON
fn python_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    // ... (same as in constraints py_bindings)
}
```

### Python Wrapper

**File**: `src/fraiseql/enterprise/security/audit.py`

```python
"""Audit logging for GraphQL operations."""

from enum import Enum
from typing import Any, Optional

import json

from fraiseql._fraiseql_rs import PyAuditLogger
from fraiseql.db import DatabasePool


class AuditLevel(Enum):
    """Audit log levels."""

    INFO = "INFO"
    WARN = "WARN"
    ERROR = "ERROR"


class AuditLogger:
    """Python wrapper for Rust audit logger.

    Example:
        >>> from fraiseql.db import DatabasePool
        >>> pool = DatabasePool("postgresql://...")
        >>> logger = AuditLogger(pool)
        >>>
        >>> await logger.log(
        ...     level=AuditLevel.INFO,
        ...     user_id=1,
        ...     tenant_id=1,
        ...     operation="query",
        ...     query="{ users { id name } }",
        ...     variables={},
        ...     ip_address="192.168.1.1",
        ...     user_agent="GraphQL Client",
        ... )
    """

    def __init__(self, pool: DatabasePool):
        """Initialize audit logger.

        Args:
            pool: Database pool instance
        """
        self._logger = PyAuditLogger(pool)

    async def log(
        self,
        level: AuditLevel,
        user_id: int,
        tenant_id: int,
        operation: str,
        query: str,
        variables: dict[str, Any],
        ip_address: str,
        user_agent: str,
        error: Optional[str] = None,
        duration_ms: Optional[int] = None,
    ) -> int:
        """Log an audit entry.

        Args:
            level: Log level
            user_id: ID of user performing operation
            tenant_id: Tenant ID
            operation: Operation type (query, mutation)
            query: GraphQL query string
            variables: Query variables
            ip_address: Client IP address
            user_agent: Client user agent
            error: Optional error message
            duration_ms: Optional query duration in milliseconds

        Returns:
            ID of created log entry
        """
        return await self._logger.log(
            level=level.value,
            user_id=user_id,
            tenant_id=tenant_id,
            operation=operation,
            query=query,
            variables=variables,
            ip_address=ip_address,
            user_agent=user_agent,
            error=error,
            duration_ms=duration_ms,
        )

    async def get_recent_logs(
        self,
        tenant_id: int,
        level: Optional[AuditLevel] = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        """Get recent audit logs.

        Args:
            tenant_id: Tenant ID
            level: Optional filter by log level
            limit: Maximum number of logs to return

        Returns:
            List of audit log entries (as dicts)
        """
        level_str = level.value if level else None
        json_strings = await self._logger.get_recent_logs(
            tenant_id=tenant_id,
            level=level_str,
            limit=limit,
        )

        # Parse JSON strings to dicts
        return [json.loads(s) for s in json_strings]
```

---

## ✅ Acceptance Criteria

### Audit Logging
- ✅ Can log GraphQL queries and mutations
- ✅ Logs include full context (user, tenant, IP, user agent, variables)
- ✅ Supports multiple log levels (INFO, WARN, ERROR)
- ✅ Can filter logs by level and tenant
- ✅ Multi-tenant isolation works
- ✅ 10+ tests pass

### Performance
- ✅ Audit logging 10-100x faster than Python
- ✅ All operations complete in <10ms
- ✅ Efficient querying with indexes

---

## 🚫 DO NOT

- ❌ Don't use sqlx (use existing tokio-postgres pool)
- ❌ Don't break existing auth/RBAC/constraints tests
- ❌ Don't add synchronous blocking calls
- ❌ Don't store sensitive data in audit logs (passwords, tokens)
- ❌ Don't skip tenant isolation
- ❌ Don't forget to add database indexes

---

## 📊 Expected Results

### Test Results
```bash
tests/test_audit_logging.py::test_audit_log_query PASSED
tests/test_audit_logging.py::test_audit_log_mutation PASSED
tests/test_audit_logging.py::test_audit_log_error PASSED
tests/test_audit_logging.py::test_audit_log_filtering PASSED
tests/test_audit_logging.py::test_audit_log_tenant_isolation PASSED
... 5 more tests ...

======================== 10 passed in 5.2s ========================
```

### Performance
```
Audit Logging:
  Python implementation: 50ms per log
  Rust implementation:   0.5ms per log
  Speedup: 100x
```

---

**End of Phase 14 Plan**
