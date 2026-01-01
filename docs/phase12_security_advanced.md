# Phase 12: Advanced Security Features

**Phase**: GREENFIELD → RED → GREEN → REFACTOR → QA
**Status**: Planning
**Dependencies**: Phase 10 (Auth), Phase 11 (RBAC)

---

## 🎯 Objective

Implement advanced security features in Rust:
1. **Audit Logging**: Track all GraphQL operations with context
2. **Security Constraints**: Rate limiting, IP filtering, request validation
3. **Async pyo3 Integration**: Complete async bindings for all security features

**Key Goals**:
- ✅ Production-ready audit logging (10-100x faster than Python)
- ✅ Real-time security constraints enforcement
- ✅ Complete async integration (fix Phase 11 placeholders)
- ✅ Comprehensive test coverage

---

## 📋 Context

### Current State (After Phase 11)

**Working**:
- ✅ Authentication system (async integrated)
- ✅ RBAC permission resolution (cached)
- ✅ Multi-tenancy support

**Known Limitations**:
- ⚠️ `get_user_permissions()` returns placeholder strings
- ⚠️ `has_permission()` returns placeholder strings
- ⚠️ No audit logging
- ⚠️ No security constraints

### What We're Adding

1. **Audit Logging System**:
   - Track all GraphQL queries/mutations
   - Include user context (ID, tenant, IP)
   - Store operation metadata (query, variables, errors)
   - Efficient storage (PostgreSQL JSON, time-series optimized)

2. **Security Constraints**:
   - Rate limiting (per user, per IP, per tenant)
   - IP allowlist/blocklist
   - Query complexity limits
   - Request size limits

3. **Complete Async Integration**:
   - Replace placeholder strings with real async calls
   - Use `pyo3_asyncio` for proper async/await
   - Full integration with Python async runtime

---

## 📁 Files to Modify/Create

### New Rust Files

1. **`fraiseql_rs/src/security/audit.rs`** (NEW)
   - Audit log entry struct
   - Async logging to PostgreSQL
   - Log level filtering

2. **`fraiseql_rs/src/security/constraints.rs`** (NEW)
   - Rate limiter (token bucket algorithm)
   - IP filter (CIDR matching)
   - Complexity analyzer
   - Request validator

3. **`fraiseql_rs/src/security/mod.rs`** (NEW)
   - Security module root
   - Re-exports

4. **`fraiseql_rs/src/security/py_bindings.rs`** (NEW)
   - Python bindings for audit logging
   - Python bindings for constraints
   - Async integration

### Modified Rust Files

5. **`fraiseql_rs/src/rbac/py_bindings.rs`** (MODIFY)
   - Replace placeholder strings with real async calls
   - Add `pyo3_asyncio` integration
   - Implement `get_user_permissions()` properly
   - Implement `has_permission()` properly

6. **`fraiseql_rs/src/lib.rs`** (MODIFY)
   - Add `mod security;`
   - Export security bindings

### New Python Files

7. **`src/fraiseql/enterprise/security/audit.py`** (NEW)
   - Python wrapper for audit logging
   - Helper functions for common audit scenarios

8. **`src/fraiseql/enterprise/security/constraints.py`** (NEW)
   - Python wrapper for security constraints
   - Configuration management

9. **`tests/test_security_audit.py`** (NEW)
   - 10+ tests for audit logging

10. **`tests/test_security_constraints.py`** (NEW)
    - 10+ tests for security constraints

### Documentation

11. **`docs/phase12_security_advanced.md`** (THIS FILE)
    - Complete phase documentation

---

## 🔧 Implementation Steps

### GREENFIELD Phase: Setup

#### Step 1: Create Rust Security Module Structure

**File**: `fraiseql_rs/src/security/mod.rs`

```rust
//! Advanced security features
//!
//! This module provides:
//! - Audit logging for GraphQL operations
//! - Security constraints (rate limiting, IP filtering)
//! - Async integration with Python

pub mod audit;
pub mod constraints;
pub mod py_bindings;

// Re-export main types
pub use audit::{AuditLogger, AuditEntry, AuditLevel};
pub use constraints::{RateLimiter, IpFilter, ComplexityAnalyzer};
```

#### Step 2: Add Dependencies

**File**: `fraiseql_rs/Cargo.toml`

Add to `[dependencies]`:
```toml
# Existing dependencies...

# Async runtime
tokio = { version = "1.36", features = ["full"] }
pyo3-asyncio = { version = "0.21", features = ["tokio-runtime"] }

# Time handling
chrono = "0.4"

# IP parsing
ipnetwork = "0.20"

# Rate limiting
governor = "0.6"
```

---

### RED Phase: Write Failing Tests

#### Step 3: Write Audit Logging Tests

**File**: `tests/test_security_audit.py`

```python
import pytest
from fraiseql.enterprise.security.audit import AuditLogger, AuditLevel
from fraiseql.db import Database


@pytest.mark.asyncio
async def test_audit_log_query():
    """Test logging a GraphQL query."""
    db = Database(...)
    logger = AuditLogger(db)

    await logger.log(
        level=AuditLevel.INFO,
        user_id=1,
        tenant_id=1,
        operation="query",
        query="{ users { id name } }",
        variables={},
        ip_address="192.168.1.1",
        user_agent="GraphQL Client",
    )

    # Verify log entry was created
    logs = await logger.get_recent_logs(tenant_id=1, limit=1)
    assert len(logs) == 1
    assert logs[0]["operation"] == "query"
    assert logs[0]["user_id"] == 1


@pytest.mark.asyncio
async def test_audit_log_mutation():
    """Test logging a GraphQL mutation."""
    db = Database(...)
    logger = AuditLogger(db)

    await logger.log(
        level=AuditLevel.WARN,
        user_id=1,
        tenant_id=1,
        operation="mutation",
        query="mutation { createUser(name: 'Test') { id } }",
        variables={"name": "Test"},
        ip_address="192.168.1.1",
        user_agent="GraphQL Client",
    )

    logs = await logger.get_recent_logs(tenant_id=1, limit=1)
    assert len(logs) == 1
    assert logs[0]["operation"] == "mutation"
    assert logs[0]["level"] == "WARN"


@pytest.mark.asyncio
async def test_audit_log_error():
    """Test logging a GraphQL error."""
    db = Database(...)
    logger = AuditLogger(db)

    await logger.log(
        level=AuditLevel.ERROR,
        user_id=1,
        tenant_id=1,
        operation="query",
        query="{ invalidField }",
        variables={},
        ip_address="192.168.1.1",
        user_agent="GraphQL Client",
        error="Field 'invalidField' not found",
    )

    logs = await logger.get_recent_logs(tenant_id=1, limit=1)
    assert len(logs) == 1
    assert logs[0]["level"] == "ERROR"
    assert "invalidField" in logs[0]["error"]


@pytest.mark.asyncio
async def test_audit_log_filtering():
    """Test filtering logs by level."""
    db = Database(...)
    logger = AuditLogger(db)

    # Log at different levels
    await logger.log(level=AuditLevel.INFO, user_id=1, tenant_id=1, ...)
    await logger.log(level=AuditLevel.WARN, user_id=1, tenant_id=1, ...)
    await logger.log(level=AuditLevel.ERROR, user_id=1, tenant_id=1, ...)

    # Get only ERROR logs
    logs = await logger.get_recent_logs(tenant_id=1, level=AuditLevel.ERROR)
    assert len(logs) == 1
    assert logs[0]["level"] == "ERROR"


@pytest.mark.asyncio
async def test_audit_log_tenant_isolation():
    """Test that logs are isolated by tenant."""
    db = Database(...)
    logger = AuditLogger(db)

    # Log for tenant 1
    await logger.log(level=AuditLevel.INFO, user_id=1, tenant_id=1, ...)

    # Log for tenant 2
    await logger.log(level=AuditLevel.INFO, user_id=2, tenant_id=2, ...)

    # Each tenant sees only their logs
    logs_t1 = await logger.get_recent_logs(tenant_id=1)
    logs_t2 = await logger.get_recent_logs(tenant_id=2)

    assert len(logs_t1) == 1
    assert len(logs_t2) == 1
    assert logs_t1[0]["tenant_id"] == 1
    assert logs_t2[0]["tenant_id"] == 2
```

#### Step 4: Write Security Constraints Tests

**File**: `tests/test_security_constraints.py`

```python
import pytest
from fraiseql.enterprise.security.constraints import (
    RateLimiter,
    IpFilter,
    ComplexityAnalyzer,
)


@pytest.mark.asyncio
async def test_rate_limiter_allow():
    """Test rate limiter allows requests under limit."""
    limiter = RateLimiter(max_requests=10, window_seconds=60)

    # First request should be allowed
    assert await limiter.check("user:1") is True


@pytest.mark.asyncio
async def test_rate_limiter_block():
    """Test rate limiter blocks requests over limit."""
    limiter = RateLimiter(max_requests=2, window_seconds=60)

    # First 2 requests allowed
    assert await limiter.check("user:1") is True
    assert await limiter.check("user:1") is True

    # 3rd request blocked
    assert await limiter.check("user:1") is False


@pytest.mark.asyncio
async def test_rate_limiter_multi_user():
    """Test rate limiter tracks users separately."""
    limiter = RateLimiter(max_requests=2, window_seconds=60)

    # User 1: 2 requests (at limit)
    assert await limiter.check("user:1") is True
    assert await limiter.check("user:1") is True
    assert await limiter.check("user:1") is False

    # User 2: still has quota
    assert await limiter.check("user:2") is True


@pytest.mark.asyncio
async def test_ip_filter_allowlist():
    """Test IP allowlist."""
    filter = IpFilter(allowlist=["192.168.1.0/24"])

    assert await filter.check("192.168.1.100") is True
    assert await filter.check("10.0.0.1") is False


@pytest.mark.asyncio
async def test_ip_filter_blocklist():
    """Test IP blocklist."""
    filter = IpFilter(blocklist=["10.0.0.0/8"])

    assert await filter.check("192.168.1.100") is True
    assert await filter.check("10.0.0.1") is False


@pytest.mark.asyncio
async def test_complexity_analyzer():
    """Test query complexity analysis."""
    analyzer = ComplexityAnalyzer(max_complexity=100)

    # Simple query (low complexity)
    simple = "{ user { id name } }"
    assert await analyzer.check(simple) is True

    # Complex query (high complexity)
    complex = """
    {
        users {
            posts {
                comments {
                    author {
                        posts {
                            comments { id }
                        }
                    }
                }
            }
        }
    }
    """
    assert await analyzer.check(complex) is False
```

---

### GREEN Phase: Implement Features

#### Step 5: Implement Audit Logging (Rust)

**File**: `fraiseql_rs/src/security/audit.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    INFO,
    WARN,
    ERROR,
}

impl AuditLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditLevel::INFO => "INFO",
            AuditLevel::WARN => "WARN",
            AuditLevel::ERROR => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub level: AuditLevel,
    pub user_id: i64,
    pub tenant_id: i64,
    pub operation: String,
    pub query: String,
    pub variables: serde_json::Value,
    pub ip_address: String,
    pub user_agent: String,
    pub error: Option<String>,
}

pub struct AuditLogger {
    pool: PgPool,
}

impl AuditLogger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Log an audit entry
    pub async fn log(&self, entry: AuditEntry) -> Result<i64, sqlx::Error> {
        let id = sqlx::query_scalar!(
            r#"
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
                error
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id
            "#,
            entry.timestamp,
            entry.level.as_str(),
            entry.user_id,
            entry.tenant_id,
            entry.operation,
            entry.query,
            entry.variables,
            entry.ip_address,
            entry.user_agent,
            entry.error,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get recent logs for a tenant
    pub async fn get_recent_logs(
        &self,
        tenant_id: i64,
        level: Option<AuditLevel>,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, sqlx::Error> {
        let level_filter = level.map(|l| l.as_str());

        let rows = if let Some(lvl) = level_filter {
            sqlx::query!(
                r#"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error
                FROM fraiseql_audit_logs
                WHERE tenant_id = $1 AND level = $2
                ORDER BY timestamp DESC
                LIMIT $3
                "#,
                tenant_id,
                lvl,
                limit,
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query!(
                r#"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error
                FROM fraiseql_audit_logs
                WHERE tenant_id = $1
                ORDER BY timestamp DESC
                LIMIT $2
                "#,
                tenant_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?
        };

        let entries = rows
            .into_iter()
            .map(|row| {
                let level = match row.level.as_str() {
                    "INFO" => AuditLevel::INFO,
                    "WARN" => AuditLevel::WARN,
                    "ERROR" => AuditLevel::ERROR,
                    _ => AuditLevel::INFO,
                };

                AuditEntry {
                    id: Some(row.id),
                    timestamp: row.timestamp,
                    level,
                    user_id: row.user_id,
                    tenant_id: row.tenant_id,
                    operation: row.operation,
                    query: row.query,
                    variables: row.variables,
                    ip_address: row.ip_address,
                    user_agent: row.user_agent,
                    error: row.error,
                }
            })
            .collect();

        Ok(entries)
    }
}
```

#### Step 6: Implement Security Constraints (Rust)

**File**: `fraiseql_rs/src/security/constraints.rs`

```rust
use governor::{Quota, RateLimiter as GovernorRateLimiter};
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    limiters: Arc<RwLock<HashMap<String, GovernorRateLimiter<String,
        governor::state::direct::NotKeyed,
        governor::clock::DefaultClock>>>>,
    quota: Quota,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(max_requests).unwrap());
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            quota,
        }
    }

    pub async fn check(&self, key: &str) -> bool {
        let mut limiters = self.limiters.write().await;

        let limiter = limiters
            .entry(key.to_string())
            .or_insert_with(|| GovernorRateLimiter::direct(self.quota));

        limiter.check().is_ok()
    }
}

/// IP filter with allowlist and blocklist
pub struct IpFilter {
    allowlist: Vec<IpNetwork>,
    blocklist: Vec<IpNetwork>,
}

impl IpFilter {
    pub fn new(
        allowlist: Vec<String>,
        blocklist: Vec<String>,
    ) -> Result<Self, String> {
        let allowlist_parsed: Result<Vec<_>, _> = allowlist
            .iter()
            .map(|s| s.parse::<IpNetwork>())
            .collect();

        let blocklist_parsed: Result<Vec<_>, _> = blocklist
            .iter()
            .map(|s| s.parse::<IpNetwork>())
            .collect();

        Ok(Self {
            allowlist: allowlist_parsed.map_err(|e| e.to_string())?,
            blocklist: blocklist_parsed.map_err(|e| e.to_string())?,
        })
    }

    pub async fn check(&self, ip: &str) -> bool {
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return false,
        };

        // Check blocklist first
        if self.blocklist.iter().any(|net| net.contains(ip_addr)) {
            return false;
        }

        // If allowlist is empty, allow all (except blocked)
        if self.allowlist.is_empty() {
            return true;
        }

        // Check allowlist
        self.allowlist.iter().any(|net| net.contains(ip_addr))
    }
}

/// Query complexity analyzer
pub struct ComplexityAnalyzer {
    max_complexity: usize,
}

impl ComplexityAnalyzer {
    pub fn new(max_complexity: usize) -> Self {
        Self { max_complexity }
    }

    pub async fn check(&self, query: &str) -> bool {
        let complexity = self.calculate_complexity(query);
        complexity <= self.max_complexity
    }

    fn calculate_complexity(&self, query: &str) -> usize {
        // Simple heuristic: count nesting depth and field count
        let depth = query.matches('{').count();
        let fields = query.split_whitespace().count();

        depth * 10 + fields
    }
}
```

#### Step 7: Add Python Bindings

**File**: `fraiseql_rs/src/security/py_bindings.rs`

```rust
use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use sqlx::PgPool;

use super::audit::{AuditLogger, AuditEntry, AuditLevel};
use super::constraints::{RateLimiter, IpFilter, ComplexityAnalyzer};

#[pyclass]
pub struct PyAuditLogger {
    logger: AuditLogger,
}

#[pymethods]
impl PyAuditLogger {
    #[new]
    pub fn new(connection_string: String) -> PyResult<Self> {
        // Note: This is simplified - in production, share pool with DB module
        let pool = PgPool::connect(&connection_string)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to connect: {}", e)
            ))?;

        Ok(Self {
            logger: AuditLogger::new(pool),
        })
    }

    pub fn log<'p>(
        &self,
        py: Python<'p>,
        level: &str,
        user_id: i64,
        tenant_id: i64,
        operation: String,
        query: String,
        variables: HashMap<String, serde_json::Value>,
        ip_address: String,
        user_agent: String,
        error: Option<String>,
    ) -> PyResult<&'p PyAny> {
        let level = match level {
            "INFO" => AuditLevel::INFO,
            "WARN" => AuditLevel::WARN,
            "ERROR" => AuditLevel::ERROR,
            _ => AuditLevel::INFO,
        };

        let entry = AuditEntry {
            id: None,
            timestamp: chrono::Utc::now(),
            level,
            user_id,
            tenant_id,
            operation,
            query,
            variables: serde_json::to_value(variables).unwrap(),
            ip_address,
            user_agent,
            error,
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

    pub fn get_recent_logs<'p>(
        &self,
        py: Python<'p>,
        tenant_id: i64,
        level: Option<String>,
        limit: i64,
    ) -> PyResult<&'p PyAny> {
        let level = level.and_then(|s| match s.as_str() {
            "INFO" => Some(AuditLevel::INFO),
            "WARN" => Some(AuditLevel::WARN),
            "ERROR" => Some(AuditLevel::ERROR),
            _ => None,
        });

        let logger = self.logger.clone();

        future_into_py(py, async move {
            let entries = logger.get_recent_logs(tenant_id, level, limit).await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Failed to get logs: {}", e)
                ))?;

            Ok(entries)
        })
    }
}

#[pyclass]
pub struct PyRateLimiter {
    limiter: RateLimiter,
}

#[pymethods]
impl PyRateLimiter {
    #[new]
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            limiter: RateLimiter::new(max_requests, window_seconds),
        }
    }

    pub fn check<'p>(&self, py: Python<'p>, key: String) -> PyResult<&'p PyAny> {
        let limiter = self.limiter.clone();
        future_into_py(py, async move {
            Ok(limiter.check(&key).await)
        })
    }
}

#[pyclass]
pub struct PyIpFilter {
    filter: IpFilter,
}

#[pymethods]
impl PyIpFilter {
    #[new]
    pub fn new(allowlist: Vec<String>, blocklist: Vec<String>) -> PyResult<Self> {
        let filter = IpFilter::new(allowlist, blocklist)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;

        Ok(Self { filter })
    }

    pub fn check<'p>(&self, py: Python<'p>, ip: String) -> PyResult<&'p PyAny> {
        let filter = self.filter.clone();
        future_into_py(py, async move {
            Ok(filter.check(&ip).await)
        })
    }
}

#[pyclass]
pub struct PyComplexityAnalyzer {
    analyzer: ComplexityAnalyzer,
}

#[pymethods]
impl PyComplexityAnalyzer {
    #[new]
    pub fn new(max_complexity: usize) -> Self {
        Self {
            analyzer: ComplexityAnalyzer::new(max_complexity),
        }
    }

    pub fn check<'p>(&self, py: Python<'p>, query: String) -> PyResult<&'p PyAny> {
        let analyzer = self.analyzer.clone();
        future_into_py(py, async move {
            Ok(analyzer.check(&query).await)
        })
    }
}

pub fn register_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    let security = PyModule::new(py, "security")?;
    security.add_class::<PyAuditLogger>()?;
    security.add_class::<PyRateLimiter>()?;
    security.add_class::<PyIpFilter>()?;
    security.add_class::<PyComplexityAnalyzer>()?;
    parent_module.add_submodule(security)?;
    Ok(())
}
```

#### Step 8: Fix RBAC Async Placeholders

**File**: `fraiseql_rs/src/rbac/py_bindings.rs` (MODIFY)

Replace placeholder implementations:

```rust
// OLD (placeholder):
pub fn get_user_permissions<'p>(
    &self,
    py: Python<'p>,
    user_id: i64,
    tenant_id: i64,
) -> PyResult<&'p PyAny> {
    future_into_py(py, async move {
        Ok(Python::with_gil(|py| {
            PyList::new(
                py,
                &["permission1".to_string(), "permission2".to_string()],
            )
            .into()
        }))
    })
}

// NEW (real implementation):
pub fn get_user_permissions<'p>(
    &self,
    py: Python<'p>,
    user_id: i64,
    tenant_id: i64,
) -> PyResult<&'p PyAny> {
    let resolver = self.resolver.clone();

    future_into_py(py, async move {
        let permissions = resolver.get_user_permissions(user_id, tenant_id).await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to get permissions: {}", e)
            ))?;

        Ok(Python::with_gil(|py| {
            PyList::new(py, &permissions).into()
        }))
    })
}

// Same for has_permission():
pub fn has_permission<'p>(
    &self,
    py: Python<'p>,
    user_id: i64,
    tenant_id: i64,
    permission: String,
) -> PyResult<&'p PyAny> {
    let resolver = self.resolver.clone();

    future_into_py(py, async move {
        let has = resolver.has_permission(user_id, tenant_id, &permission).await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to check permission: {}", e)
            ))?;

        Ok(has)
    })
}
```

#### Step 9: Add Rust Methods to Resolver

**File**: `fraiseql_rs/src/rbac/resolver.rs` (ADD METHODS)

```rust
impl RbacResolver {
    // ... existing methods ...

    /// Get all permissions for a user
    pub async fn get_user_permissions(
        &self,
        user_id: i64,
        tenant_id: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        // Check cache first
        let cache_key = format!("user_permissions:{}:{}", tenant_id, user_id);
        if let Some(cached) = self.cache.read().await.get(&cache_key) {
            return Ok(cached.permissions.clone());
        }

        // Fetch from database
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT p.name
            FROM fraiseql_permissions p
            JOIN fraiseql_role_permissions rp ON p.id = rp.permission_id
            JOIN fraiseql_user_roles ur ON rp.role_id = ur.role_id
            WHERE ur.user_id = $1 AND p.tenant_id = $2
            "#,
            user_id,
            tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;

        let permissions: Vec<String> = rows.into_iter().map(|r| r.name).collect();

        // Cache the result
        let entry = CacheEntry {
            tenant_id,
            permissions: permissions.clone(),
            timestamp: std::time::Instant::now(),
        };
        self.cache.write().await.insert(cache_key, entry);

        Ok(permissions)
    }

    /// Check if user has a specific permission
    pub async fn has_permission(
        &self,
        user_id: i64,
        tenant_id: i64,
        permission: &str,
    ) -> Result<bool, sqlx::Error> {
        let permissions = self.get_user_permissions(user_id, tenant_id).await?;
        Ok(permissions.contains(&permission.to_string()))
    }
}
```

#### Step 10: Create Python Wrappers

**File**: `src/fraiseql/enterprise/security/audit.py`

```python
"""Audit logging for GraphQL operations."""

from enum import Enum
from typing import Optional, Dict, Any, List
from datetime import datetime

from fraiseql_rs import PyAuditLogger


class AuditLevel(Enum):
    """Audit log levels."""
    INFO = "INFO"
    WARN = "WARN"
    ERROR = "ERROR"


class AuditLogger:
    """Python wrapper for Rust audit logger."""

    def __init__(self, connection_string: str):
        """Initialize audit logger.

        Args:
            connection_string: PostgreSQL connection string
        """
        self._logger = PyAuditLogger(connection_string)

    async def log(
        self,
        level: AuditLevel,
        user_id: int,
        tenant_id: int,
        operation: str,
        query: str,
        variables: Dict[str, Any],
        ip_address: str,
        user_agent: str,
        error: Optional[str] = None,
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
        )

    async def get_recent_logs(
        self,
        tenant_id: int,
        level: Optional[AuditLevel] = None,
        limit: int = 100,
    ) -> List[Dict[str, Any]]:
        """Get recent audit logs.

        Args:
            tenant_id: Tenant ID
            level: Optional filter by log level
            limit: Maximum number of logs to return

        Returns:
            List of audit log entries
        """
        level_str = level.value if level else None
        return await self._logger.get_recent_logs(
            tenant_id=tenant_id,
            level=level_str,
            limit=limit,
        )
```

**File**: `src/fraiseql/enterprise/security/constraints.py`

```python
"""Security constraints (rate limiting, IP filtering, etc.)."""

from typing import List

from fraiseql_rs import PyRateLimiter, PyIpFilter, PyComplexityAnalyzer


class RateLimiter:
    """Rate limiter using token bucket algorithm."""

    def __init__(self, max_requests: int, window_seconds: int):
        """Initialize rate limiter.

        Args:
            max_requests: Maximum requests per window
            window_seconds: Time window in seconds
        """
        self._limiter = PyRateLimiter(max_requests, window_seconds)

    async def check(self, key: str) -> bool:
        """Check if request is allowed.

        Args:
            key: Rate limit key (e.g., "user:123", "ip:192.168.1.1")

        Returns:
            True if request is allowed, False if rate limited
        """
        return await self._limiter.check(key)


class IpFilter:
    """IP filter with allowlist and blocklist."""

    def __init__(
        self,
        allowlist: List[str] = None,
        blocklist: List[str] = None,
    ):
        """Initialize IP filter.

        Args:
            allowlist: CIDR ranges to allow (empty = allow all)
            blocklist: CIDR ranges to block
        """
        self._filter = PyIpFilter(
            allowlist or [],
            blocklist or [],
        )

    async def check(self, ip: str) -> bool:
        """Check if IP is allowed.

        Args:
            ip: IP address to check

        Returns:
            True if IP is allowed, False if blocked
        """
        return await self._filter.check(ip)


class ComplexityAnalyzer:
    """Query complexity analyzer."""

    def __init__(self, max_complexity: int):
        """Initialize complexity analyzer.

        Args:
            max_complexity: Maximum allowed complexity score
        """
        self._analyzer = PyComplexityAnalyzer(max_complexity)

    async def check(self, query: str) -> bool:
        """Check if query complexity is acceptable.

        Args:
            query: GraphQL query string

        Returns:
            True if complexity is acceptable, False if too complex
        """
        return await self._analyzer.check(query)
```

---

### REFACTOR Phase: Cleanup

#### Step 11: Update Module Exports

**File**: `fraiseql_rs/src/lib.rs` (MODIFY)

```rust
mod auth;
mod rbac;
mod security;  // NEW

use pyo3::prelude::*;

#[pymodule]
fn fraiseql_rs(py: Python, m: &PyModule) -> PyResult<()> {
    // Register auth module
    auth::py_bindings::register_module(py, m)?;

    // Register RBAC module
    rbac::py_bindings::register_module(py, m)?;

    // Register security module (NEW)
    security::py_bindings::register_module(py, m)?;

    Ok(())
}
```

#### Step 12: Create Migration for Audit Log Table

**File**: `migrations/001_audit_logs.sql` (NEW)

```sql
-- Audit log table
CREATE TABLE IF NOT EXISTS fraiseql_audit_logs (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    level TEXT NOT NULL,  -- INFO, WARN, ERROR
    user_id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    operation TEXT NOT NULL,  -- query, mutation
    query TEXT NOT NULL,
    variables JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    error TEXT
);

-- Indexes for common queries
CREATE INDEX idx_audit_logs_tenant_timestamp
    ON fraiseql_audit_logs(tenant_id, timestamp DESC);

CREATE INDEX idx_audit_logs_tenant_level
    ON fraiseql_audit_logs(tenant_id, level, timestamp DESC);

CREATE INDEX idx_audit_logs_user
    ON fraiseql_audit_logs(user_id, timestamp DESC);
```

---

### QA Phase: Verification

#### Step 13: Run All Tests

```bash
# Build Rust extension
maturin develop

# Run all tests
uv run pytest tests/test_security_audit.py -v
uv run pytest tests/test_security_constraints.py -v
uv run pytest tests/test_rust_rbac.py -v  # Verify RBAC fixes

# Expected: 40+ tests pass (10 audit + 10 constraints + 19 RBAC)
```

#### Step 14: Manual Testing

```python
# Test audit logging
from fraiseql.enterprise.security.audit import AuditLogger, AuditLevel

logger = AuditLogger("postgresql://...")
await logger.log(
    level=AuditLevel.INFO,
    user_id=1,
    tenant_id=1,
    operation="query",
    query="{ users { id } }",
    variables={},
    ip_address="192.168.1.1",
    user_agent="Test",
)

logs = await logger.get_recent_logs(tenant_id=1, limit=10)
print(f"Found {len(logs)} logs")

# Test rate limiting
from fraiseql.enterprise.security.constraints import RateLimiter

limiter = RateLimiter(max_requests=5, window_seconds=60)
for i in range(7):
    allowed = await limiter.check("user:1")
    print(f"Request {i+1}: {'✅ Allowed' if allowed else '❌ Blocked'}")

# Test IP filtering
from fraiseql.enterprise.security.constraints import IpFilter

filter = IpFilter(blocklist=["10.0.0.0/8"])
print(await filter.check("192.168.1.1"))  # True
print(await filter.check("10.0.0.1"))     # False

# Test complexity analysis
from fraiseql.enterprise.security.constraints import ComplexityAnalyzer

analyzer = ComplexityAnalyzer(max_complexity=50)
simple = "{ user { id } }"
complex = "{ users { posts { comments { author { posts { id } } } } } }"
print(await analyzer.check(simple))   # True
print(await analyzer.check(complex))  # False
```

---

## ✅ Acceptance Criteria

### Audit Logging
- ✅ Can log GraphQL queries and mutations
- ✅ Logs include user context (ID, tenant, IP, user agent)
- ✅ Supports multiple log levels (INFO, WARN, ERROR)
- ✅ Can filter logs by level and tenant
- ✅ Multi-tenant isolation works
- ✅ 10+ tests pass

### Security Constraints
- ✅ Rate limiter correctly limits requests
- ✅ Rate limiter tracks users separately
- ✅ IP filter supports allowlist/blocklist
- ✅ IP filter handles CIDR notation
- ✅ Complexity analyzer detects complex queries
- ✅ 10+ tests pass

### RBAC Fixes
- ✅ `get_user_permissions()` returns real data (not placeholders)
- ✅ `has_permission()` works correctly
- ✅ Caching works for permission lookups
- ✅ 19 RBAC tests still pass

### Performance
- ✅ Audit logging 10-100x faster than Python
- ✅ Rate limiting O(1) complexity
- ✅ IP filtering O(log n) complexity
- ✅ All operations complete in <10ms

---

## 🚫 DO NOT

- ❌ Don't break existing auth/RBAC tests
- ❌ Don't add synchronous blocking calls
- ❌ Don't store sensitive data in audit logs (passwords, tokens)
- ❌ Don't skip tenant isolation in audit logs
- ❌ Don't forget to add indexes on audit log table

---

## 📊 Expected Results

### Test Results
```bash
tests/test_security_audit.py::test_audit_log_query PASSED
tests/test_security_audit.py::test_audit_log_mutation PASSED
tests/test_security_audit.py::test_audit_log_error PASSED
tests/test_security_audit.py::test_audit_log_filtering PASSED
tests/test_security_audit.py::test_audit_log_tenant_isolation PASSED
... 5 more audit tests ...

tests/test_security_constraints.py::test_rate_limiter_allow PASSED
tests/test_security_constraints.py::test_rate_limiter_block PASSED
tests/test_security_constraints.py::test_rate_limiter_multi_user PASSED
tests/test_security_constraints.py::test_ip_filter_allowlist PASSED
tests/test_security_constraints.py::test_ip_filter_blocklist PASSED
... 5 more constraint tests ...

tests/test_rust_rbac.py::test_get_user_permissions PASSED  # Fixed!
tests/test_rust_rbac.py::test_has_permission PASSED        # Fixed!
... 17 more RBAC tests ...

======================== 40 passed in 15.2s ========================
```

### Performance Benchmarks
```
Audit Logging:
  Python implementation: 50ms per log
  Rust implementation:   0.5ms per log
  Speedup: 100x

Rate Limiting:
  Python implementation: 10ms per check
  Rust implementation:   0.1ms per check
  Speedup: 100x

IP Filtering:
  Python implementation: 5ms per check
  Rust implementation:   0.05ms per check
  Speedup: 100x
```

---

## 📝 Documentation Updates

After implementation, update:

1. **README.md**: Add security features section
2. **docs/features/security.md**: Comprehensive security guide
3. **CHANGELOG.md**: Add Phase 12 entry
4. **Version bump**: 1.9.2 → 1.9.3 (or 1.10.0 if major)

---

## 🎯 Next Steps (Phase 13)

After Phase 12 completes:

**Phase 13: Advanced GraphQL Features**
- Fragment support
- Custom directives
- Subscription support
- DataLoader pattern

**Phase 14: Production Deployment**
- Docker images
- Kubernetes manifests
- Monitoring dashboards
- Production benchmarks

---

**End of Phase 12 Plan**
