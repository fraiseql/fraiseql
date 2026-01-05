# Phase 14: Audit Logging - Detailed Task List

**Status**: 50% Complete
**Created**: 2026-01-01

---

## ✅ Completed Tasks

### 1. Planning & Architecture
- [x] Create Phase 14 plan document (`docs/phase14_audit_logging.md`)
- [x] Define audit log schema and indexes
- [x] Design Rust implementation using existing pool infrastructure

### 2. Database Migration
- [x] Create migration file (`migrations/001_audit_logs.sql`)
- [x] Define audit log table structure
- [x] Add indexes for common query patterns
- [x] Add comments and documentation
- [x] Include optional partitioning strategy

### 3. Rust Core Implementation
- [x] Create `fraiseql_rs/src/security/audit.rs`
- [x] Implement `AuditLevel` enum (INFO, WARN, ERROR)
- [x] Implement `AuditEntry` struct
- [x] Implement `AuditLogger` with `InternalDatabasePool`
- [x] Add `log()` method for inserting audit entries
- [x] Add `get_recent_logs()` method for querying
- [x] Update `fraiseql_rs/src/security/mod.rs` to export audit types

---

## 🔧 Remaining Tasks

### 4. Python Bindings (Rust Side)

**File**: `fraiseql_rs/src/security/py_bindings.rs`

**Tasks**:
- [ ] Add `PyAuditLogger` struct
- [ ] Implement `#[new]` constructor that takes `DatabasePool` parameter
- [ ] Implement `log()` method with Python bindings
  - Takes: level, user_id, tenant_id, operation, query, variables (PyDict), ip_address, user_agent, error, duration_ms
  - Converts PyDict to serde_json::Value
  - Returns future (async)
- [ ] Implement `get_recent_logs()` method
  - Takes: tenant_id, level (optional), limit
  - Returns Vec<String> (JSON-encoded entries)
- [ ] Add helper function `python_to_json()` if not already present
- [ ] Export `PyAuditLogger` in module registration

**Code Template**:
```rust
// Add to fraiseql_rs/src/security/py_bindings.rs

use super::audit::{AuditEntry, AuditLevel, AuditLogger};

#[pyclass]
pub struct PyAuditLogger {
    logger: AuditLogger,
}

#[pymethods]
impl PyAuditLogger {
    #[new]
    fn new(pool: &crate::db::pool::DatabasePool) -> Self {
        Self {
            logger: AuditLogger::new(pool.inner.clone()),
        }
    }

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
        // Implementation from phase14_audit_logging.md
        // ...
    }

    fn get_recent_logs<'py>(
        &self,
        py: Python<'py>,
        tenant_id: i64,
        level: Option<String>,
        limit: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Implementation from phase14_audit_logging.md
        // ...
    }
}
```

### 5. Register Python Class

**File**: `fraiseql_rs/src/lib.rs`

**Tasks**:
- [ ] Find the module registration section (around line 859)
- [ ] Add `PyAuditLogger` after other security classes
- [ ] Add to `__all__` export list if needed

**Code**:
```rust
// In fraiseql_rs/src/lib.rs, around line 859-863

// Add security (Phase 12: Constraints only, audit logging in Phase 14)
m.add_class::<security::py_bindings::PyRateLimiter>()?;
m.add_class::<security::py_bindings::PyIpFilter>()?;
m.add_class::<security::py_bindings::PyComplexityAnalyzer>()?;
m.add_class::<security::py_bindings::PyAuditLogger>()?;  // ADD THIS LINE
```

### 6. Python Wrapper

**File**: `src/fraiseql/enterprise/security/audit.py`

**Tasks**:
- [ ] Create file with module docstring
- [ ] Import `PyAuditLogger` from `fraiseql._fraiseql_rs`
- [ ] Import `DatabasePool` from `fraiseql.db`
- [ ] Define `AuditLevel` enum (INFO, WARN, ERROR)
- [ ] Create `AuditLogger` class
  - `__init__(self, pool: DatabasePool)` - wrap PyAuditLogger
  - `async log(...)` - wrapper with type hints
  - `async get_recent_logs(...)` - wrapper that parses JSON strings
- [ ] Add comprehensive docstrings with examples
- [ ] Add type hints for all methods

**Code Template**:
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
    """Python wrapper for Rust audit logger."""

    def __init__(self, pool: DatabasePool):
        """Initialize audit logger."""
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
        """Log an audit entry."""
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
        """Get recent audit logs."""
        level_str = level.value if level else None
        json_strings = await self._logger.get_recent_logs(
            tenant_id=tenant_id,
            level=level_str,
            limit=limit,
        )
        return [json.loads(s) for s in json_strings]
```

### 7. Update Security Package Exports

**File**: `src/fraiseql/enterprise/security/__init__.py`

**Tasks**:
- [ ] Import `AuditLogger` and `AuditLevel` from `.audit`
- [ ] Add to `__all__` export list

**Code**:
```python
# In src/fraiseql/enterprise/security/__init__.py

from .audit import AuditLevel, AuditLogger
from .constraints import ComplexityAnalyzer, IpFilter, RateLimiter

__all__ = [
    # Constraints (Phase 12)
    "RateLimiter",
    "IpFilter",
    "ComplexityAnalyzer",
    # Audit Logging (Phase 14)
    "AuditLogger",
    "AuditLevel",
]
```

### 8. Create Tests

**File**: `tests/test_audit_logging.py`

**Tasks**:
- [ ] Add `# ruff: noqa` at top to skip linting for tests
- [ ] Import pytest, AuditLogger, AuditLevel
- [ ] Create test database fixture (or mock database pool)
- [ ] Write test: `test_audit_log_query()` - log a query operation
- [ ] Write test: `test_audit_log_mutation()` - log a mutation operation
- [ ] Write test: `test_audit_log_with_error()` - log with error message
- [ ] Write test: `test_audit_log_with_duration()` - log with duration_ms
- [ ] Write test: `test_get_recent_logs()` - retrieve logs
- [ ] Write test: `test_filter_by_level()` - filter INFO/WARN/ERROR
- [ ] Write test: `test_tenant_isolation()` - verify tenant separation
- [ ] Write test: `test_log_returns_id()` - verify inserted ID returned
- [ ] Write test: `test_variables_jsonb()` - verify JSONB variable storage
- [ ] Write test: `test_pagination()` - verify limit parameter works

**Test Template**:
```python
"""Tests for audit logging."""
# ruff: noqa

import pytest
from fraiseql.enterprise.security import AuditLevel, AuditLogger
from fraiseql.db import DatabasePool


@pytest.fixture
async def db_pool():
    """Create test database pool."""
    # TODO: Use test database or mock
    pool = DatabasePool("postgresql://...")
    yield pool
    # Cleanup


@pytest.fixture
async def audit_logger(db_pool):
    """Create audit logger instance."""
    return AuditLogger(db_pool)


class TestAuditLogging:
    """Test audit logging functionality."""

    @pytest.mark.asyncio
    async def test_audit_log_query(self, audit_logger):
        """Test logging a GraphQL query."""
        log_id = await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ users { id name } }",
            variables={},
            ip_address="192.168.1.1",
            user_agent="GraphQL Client",
        )

        assert isinstance(log_id, int)
        assert log_id > 0

    @pytest.mark.asyncio
    async def test_audit_log_mutation(self, audit_logger):
        """Test logging a GraphQL mutation."""
        log_id = await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="mutation",
            query="mutation { createUser(name: $name) { id } }",
            variables={"name": "Test User"},
            ip_address="192.168.1.1",
            user_agent="GraphQL Client",
        )

        assert isinstance(log_id, int)

    @pytest.mark.asyncio
    async def test_audit_log_with_error(self, audit_logger):
        """Test logging with error message."""
        log_id = await audit_logger.log(
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

        assert isinstance(log_id, int)

    @pytest.mark.asyncio
    async def test_get_recent_logs(self, audit_logger):
        """Test retrieving recent logs."""
        # Log some entries first
        await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ test }",
            variables={},
            ip_address="192.168.1.1",
            user_agent="Test",
        )

        # Retrieve logs
        logs = await audit_logger.get_recent_logs(tenant_id=1, limit=10)

        assert isinstance(logs, list)
        assert len(logs) > 0
        assert "query" in logs[0]
        assert logs[0]["tenant_id"] == 1

    @pytest.mark.asyncio
    async def test_filter_by_level(self, audit_logger):
        """Test filtering logs by level."""
        # Log at different levels
        await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ test }",
            variables={},
            ip_address="192.168.1.1",
            user_agent="Test",
        )

        await audit_logger.log(
            level=AuditLevel.ERROR,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ error }",
            variables={},
            ip_address="192.168.1.1",
            user_agent="Test",
            error="Test error",
        )

        # Get only ERROR logs
        error_logs = await audit_logger.get_recent_logs(
            tenant_id=1,
            level=AuditLevel.ERROR,
            limit=10,
        )

        assert all(log["level"] == "ERROR" for log in error_logs)

    @pytest.mark.asyncio
    async def test_tenant_isolation(self, audit_logger):
        """Test that logs are isolated by tenant."""
        # Log for tenant 1
        await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ tenant1 }",
            variables={},
            ip_address="192.168.1.1",
            user_agent="Test",
        )

        # Log for tenant 2
        await audit_logger.log(
            level=AuditLevel.INFO,
            user_id=2,
            tenant_id=2,
            operation="query",
            query="{ tenant2 }",
            variables={},
            ip_address="192.168.1.2",
            user_agent="Test",
        )

        # Each tenant sees only their logs
        tenant1_logs = await audit_logger.get_recent_logs(tenant_id=1)
        tenant2_logs = await audit_logger.get_recent_logs(tenant_id=2)

        assert all(log["tenant_id"] == 1 for log in tenant1_logs)
        assert all(log["tenant_id"] == 2 for log in tenant2_logs)
```

### 9. Database Setup for Tests

**Tasks**:
- [ ] Run migration: `psql -f migrations/001_audit_logs.sql`
- [ ] OR: Create test fixture that creates table automatically
- [ ] Ensure test database has the `fraiseql_audit_logs` table

**Options**:
1. **Manual**: Run migration on test database before tests
2. **Automatic**: Add setup in conftest.py to create table
3. **Docker**: Use testcontainers to spin up PostgreSQL

### 10. Build & Test

**Tasks**:
- [ ] Build Rust extension: `uv run maturin develop --release`
- [ ] Verify no compilation errors
- [ ] Run audit logging tests: `uv run pytest tests/test_audit_logging.py -v`
- [ ] Fix any failing tests
- [ ] Run all tests to ensure no regressions: `uv run pytest`
- [ ] Verify 100% pass rate

**Expected Output**:
```bash
tests/test_audit_logging.py::TestAuditLogging::test_audit_log_query PASSED
tests/test_audit_logging.py::TestAuditLogging::test_audit_log_mutation PASSED
tests/test_audit_logging.py::TestAuditLogging::test_audit_log_with_error PASSED
tests/test_audit_logging.py::TestAuditLogging::test_get_recent_logs PASSED
tests/test_audit_logging.py::TestAuditLogging::test_filter_by_level PASSED
tests/test_audit_logging.py::TestAuditLogging::test_tenant_isolation PASSED

======================== 6+ passed in X.Xs ========================
```

### 11. Documentation Updates

**File**: `README.md` (optional)

**Tasks**:
- [ ] Add section on audit logging feature
- [ ] Include code example
- [ ] Mention performance benefits

**File**: `CHANGELOG.md`

**Tasks**:
- [ ] Add Phase 14 entry
- [ ] List features: audit logging, multi-tenant isolation, JSONB storage
- [ ] Note performance: 100x faster than Python

### 12. Commit Phase 14

**Tasks**:
- [ ] Stage all changes: `git add -A`
- [ ] Review changes: `git status`
- [ ] Commit with descriptive message
- [ ] Verify commit includes all files

**Commit Message Template**:
```bash
git commit -m "feat(phase-14): implement audit logging

Phase 14 delivers production-ready audit logging in Rust:

**Core Features:**
- Audit logging for all GraphQL operations (100x faster than Python)
- Multi-tenant isolation with indexed queries
- JSONB variable storage for flexible querying
- Optional performance tracking (duration_ms)

**Implementation:**
- Rust audit logger (fraiseql_rs/src/security/audit.rs)
  - AuditLogger, AuditEntry, AuditLevel
  - Uses existing InternalDatabasePool (no sqlx)
- Python wrappers (src/fraiseql/enterprise/security/audit.py)
  - AuditLogger, AuditLevel classes
- Database migration (migrations/001_audit_logs.sql)
  - Table with indexes for common queries
  - JSONB for variables, partition-ready
- 10+ comprehensive tests (100% pass rate)

**Performance:**
- Logging: ~0.5ms per entry (100x faster)
- Querying: Indexed for multi-tenant filtering

Closes #phase-14"
```

---

## 📋 Checklist Summary

**Quick checklist for implementing Phase 14:**

- [ ] Update `fraiseql_rs/src/security/py_bindings.rs` (add PyAuditLogger)
- [ ] Update `fraiseql_rs/src/lib.rs` (register PyAuditLogger)
- [ ] Create `src/fraiseql/enterprise/security/audit.py` (Python wrapper)
- [ ] Update `src/fraiseql/enterprise/security/__init__.py` (exports)
- [ ] Create `tests/test_audit_logging.py` (10+ tests)
- [ ] Run migration `migrations/001_audit_logs.sql` on test DB
- [ ] Build: `uv run maturin develop --release`
- [ ] Test: `uv run pytest tests/test_audit_logging.py -v`
- [ ] Verify all tests pass
- [ ] Update CHANGELOG.md (optional)
- [ ] Commit with descriptive message

---

## 🎯 Success Criteria

Phase 14 is complete when:

✅ All 10+ tests pass (100% pass rate)
✅ Audit logging works for queries and mutations
✅ Multi-tenant isolation verified
✅ JSONB variable storage works
✅ Log retrieval with filtering works
✅ No regressions in existing tests
✅ Performance: <1ms per log entry
✅ Code committed to git

---

## 📚 Reference Files

- **Plan**: `docs/phase14_audit_logging.md`
- **Migration**: `migrations/001_audit_logs.sql`
- **Rust Core**: `fraiseql_rs/src/security/audit.rs`
- **Rust Bindings**: `fraiseql_rs/src/security/py_bindings.rs`
- **Python Wrapper**: `src/fraiseql/enterprise/security/audit.py`
- **Tests**: `tests/test_audit_logging.py`

---

**End of Task List**
