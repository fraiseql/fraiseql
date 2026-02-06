# Phase 3.10: Multi-Database Support Implementation Plan

**Objective**: Implement FraiseQL-inspired database abstraction layer for Fraisier
**Target**: v0.1.0 release with PostgreSQL, MySQL, SQLite support
**Pattern**: Trait-based abstraction with feature flags
**Status**: Ready for Implementation

---

## Overview

Apply proven FraiseQL database patterns to Fraisier:

- **Abstract Interface**: Single `FraiserDatabaseAdapter` trait
- **Multiple Implementations**: PostgreSQL, MySQL, SQLite
- **Feature Flags**: Conditional imports and configuration
- **Connection Pooling**: Unified pool metrics across databases
- **Type Safety**: Parameterized queries, no raw SQL
- **Async/Await**: Modern async I/O throughout

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│ Fraisier Application Layer                         │
├─────────────────────────────────────────────────────┤
│ FraiserDatabaseAdapter (Abstract Trait)            │
├───────────────────┬───────────────────┬─────────────┤
│ PostgreSQL        │ MySQL             │ SQLite      │
│ (psycopg3)        │ (asyncpg/aiomysql)│ (aiosqlite) │
├───────────────────┴───────────────────┴─────────────┤
│ Connection Pooling + Metrics                       │
└─────────────────────────────────────────────────────┘
```

---

## Implementation Plan

### Phase 3.10.1: Core Adapter Interface

**File**: `fraisier/db/adapter.py` (~200 lines)

**Deliverables**:

1. `FraiserDatabaseAdapter` abstract trait
2. `DatabaseType` enum (SQLITE, POSTGRESQL, MYSQL)
3. `QueryResult` typed result wrapper
4. `PoolMetrics` unified metrics structure

**Key Methods**:

```python
class FraiserDatabaseAdapter(ABC):
    @abstractmethod
    async def execute_query(
        self,
        query: str,
        params: List[Any] | None = None,
    ) -> List[Dict[str, Any]]:
        """Execute SELECT query."""
        pass

    @abstractmethod
    async def insert(
        self,
        table: str,
        data: Dict[str, Any],
    ) -> str:  # Return ID
        """Insert record, return ID."""
        pass

    @abstractmethod
    async def update(
        self,
        table: str,
        id: str,
        data: Dict[str, Any],
    ) -> bool:
        """Update record."""
        pass

    @abstractmethod
    async def delete(self, table: str, id: str) -> bool:
        """Delete record."""
        pass

    @abstractmethod
    async def health_check(self) -> bool:
        """Verify connectivity."""
        pass

    @abstractmethod
    def database_type(self) -> DatabaseType:
        """Return database type."""
        pass

    @abstractmethod
    def pool_metrics(self) -> PoolMetrics:
        """Return pool statistics."""
        pass
```

**Tests**: 15 tests

- Trait interface validation
- Method signatures
- Abstract method enforcement

---

### Phase 3.10.2: SQLite Adapter

**File**: `fraisier/db/sqlite_adapter.py` (~250 lines)

**Deliverables**:

1. `SqliteAdapter` implementation
2. Async connection handling (aiosqlite)
3. Row factory for dict results
4. Pool metrics (mock for SQLite)

**Key Features**:

```python
class SqliteAdapter(FraiserDatabaseAdapter):
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._conn = None

    async def connect(self) -> None:
        """Open connection."""
        import aiosqlite
        self._conn = await aiosqlite.connect(self.db_path)
        self._conn.row_factory = aiosqlite.Row

    async def execute_query(
        self,
        query: str,
        params: List[Any] | None = None,
    ) -> List[Dict[str, Any]]:
        """Execute SELECT with ? placeholders."""
        # Convert parameters to tuple (SQLite requirement)
        # Return list of dicts via row_factory
        pass
```

**Migration Path**:

- Keep existing SQLite schema
- Wrap current code in adapter
- Add async/await gradually
- Maintain backward compatibility

**Tests**: 20 tests

- Connection handling
- Query execution
- Insert/update/delete operations
- Error handling
- Health check

---

### Phase 3.10.3: PostgreSQL Adapter

**File**: `fraisier/db/postgres_adapter.py` (~300 lines)

**Deliverables**:

1. `PostgresAdapter` implementation
2. Connection pooling (psycopg3 pool)
3. Parameter substitution ($1, $2, etc.)
4. Real pool metrics

**Key Features**:

```python
class PostgresAdapter(FraiserDatabaseAdapter):
    def __init__(self, connection_string: str, pool_size: int = 10):
        self.connection_string = connection_string
        self.pool_size = pool_size
        self._pool = None

    async def connect(self) -> None:
        """Create connection pool."""
        import psycopg_pool
        self._pool = psycopg_pool.AsyncConnectionPool(
            self.connection_string,
            min_size=1,
            max_size=self.pool_size,
            check=psycopg_pool.check_connection,
        )
        await self._pool.open()

    async def execute_query(
        self,
        query: str,
        params: List[Any] | None = None,
    ) -> List[Dict[str, Any]]:
        """Execute SELECT with $1, $2 placeholders."""
        # Convert ? placeholders to $N (if needed)
        # Use psycopg's psycopg.rows.dict_row
        pass

    def pool_metrics(self) -> PoolMetrics:
        """Return actual pool statistics."""
        return PoolMetrics(
            total_connections=len(self._pool._holders),
            active_connections=sum(
                1 for h in self._pool._holders
                if h._in_use
            ),
            idle_connections=sum(
                1 for h in self._pool._holders
                if not h._in_use
            ),
            waiting_requests=len(self._pool._waiting),
        )
```

**Dependencies**:

- `psycopg[binary]>=3.1.0` - PostgreSQL driver
- Connection pooling built-in (psycopg3)

**Tests**: 25 tests

- Connection pool creation
- Pool sizing (min/max)
- Query execution with $N parameters
- Parameter escaping
- Error handling (connection loss, query errors)
- Pool metrics accuracy
- Health check with reconnection

**Schema Migration**:

- Create migration scripts for Fraisier schema
- Support existing SQLite schema structure
- Add indexes for deployment queries

---

### Phase 3.10.4: MySQL Adapter

**File**: `fraisier/db/mysql_adapter.py` (~250 lines)

**Deliverables**:

1. `MysqlAdapter` implementation
2. Connection pooling (asyncpg-like)
3. Parameter substitution (%)
4. Pool metrics

**Key Features**:

```python
class MysqlAdapter(FraiserDatabaseAdapter):
    def __init__(
        self,
        connection_string: str,
        min_pool_size: int = 5,
        max_pool_size: int = 20,
    ):
        self.connection_string = connection_string
        self.min_pool_size = min_pool_size
        self.max_pool_size = max_pool_size
        self._pool = None

    async def connect(self) -> None:
        """Create connection pool."""
        import aiomysql
        # Parse connection string: mysql://user:pass@host:port/db
        self._pool = await aiomysql.create_pool(
            host=...,
            user=...,
            password=...,
            db=...,
            minsize=self.min_pool_size,
            maxsize=self.max_pool_size,
            autocommit=True,
        )

    async def execute_query(
        self,
        query: str,
        params: List[Any] | None = None,
    ) -> List[Dict[str, Any]]:
        """Execute SELECT with % placeholders."""
        # Convert ? or $N to % if needed
        # Return list of dicts
        pass
```

**Dependencies**:

- `aiomysql>=0.2.0` - MySQL driver with connection pooling

**Tests**: 20 tests

- Connection pool with min/max sizing
- Query execution with % parameters
- Connection timeout handling
- Pool exhaustion behavior
- Health check

---

### Phase 3.10.5: Database Factory & Configuration

**File**: `fraisier/db/factory.py` (~100 lines)

**Deliverables**:

1. `get_database_adapter()` factory function
2. Configuration from environment variables
3. Automatic connection pool initialization

**Configuration Priority**:

1. Environment variables
2. Config file (.env, config.yaml)
3. Hardcoded defaults (SQLite for dev)

**Usage**:

```python
# Environment-driven adapter selection
import os

DB_TYPE = os.getenv("FRAISIER_DB_TYPE", "sqlite")
DB_URL = os.getenv("FRAISIER_DB_URL", "sqlite://./fraisier.db")

async def get_database():
    """Factory to get database adapter."""
    adapter = create_adapter_from_url(DB_URL)
    await adapter.connect()
    return adapter

# Then in code:
db = await get_database()
results = await db.execute_query("SELECT * FROM v_deployment")
```

**Tests**: 10 tests

- Factory selection logic
- Configuration parsing
- Connection initialization
- Error handling for missing config

---

### Phase 3.10.6: Database Tests & Fixtures

**File**: `tests/test_database_adapters.py` (~400 lines)

**Test Coverage**:

#### SQLite Tests (20 tests)

```python
class TestSqliteAdapter:
    @pytest.fixture
    async def adapter(self):
        adapter = SqliteAdapter(":memory:")
        await adapter.connect()
        yield adapter
        await adapter.close()

    async def test_execute_query(self, adapter):
        """Test SELECT query execution."""
        results = await adapter.execute_query(
            "SELECT 1 as id",
        )
        assert len(results) == 1
        assert results[0]["id"] == 1

    async def test_insert_returns_id(self, adapter):
        """Test INSERT returns ID."""
        await adapter.execute_query(
            """CREATE TABLE test (
                id INTEGER PRIMARY KEY,
                name TEXT
            )"""
        )
        id = await adapter.insert("test", {"name": "test"})
        assert id is not None

    # ... 17 more tests
```

#### PostgreSQL Tests (25 tests)

```python
class TestPostgresAdapter:
    @pytest.fixture
    async def adapter(self):
        adapter = PostgresAdapter(
            "postgresql://postgres:postgres@localhost/fraisier_test"
        )
        await adapter.connect()
        # Setup test tables
        yield adapter
        # Cleanup
        await adapter.close()

    async def test_parameter_substitution(self, adapter):
        """Test $N parameter handling."""
        results = await adapter.execute_query(
            "SELECT $1 as value",
            ["test"],
        )
        assert results[0]["value"] == "test"

    async def test_pool_metrics(self, adapter):
        """Test pool metrics accuracy."""
        metrics = adapter.pool_metrics()
        assert metrics.total_connections >= 1
        assert metrics.idle_connections >= 0
        assert metrics.active_connections >= 0

    # ... 22 more tests
```

#### MySQL Tests (20 tests)

- Similar structure to PostgreSQL
- Test % parameter handling
- Pool sizing validation

---

### Phase 3.10.7: Migration Layer

**File**: `fraisier/db/migrations/` (~500 lines total)

**Deliverables**:

1. Base migration interface
2. SQLite migration (schema creation)
3. PostgreSQL migration (schema + indexes)
4. MySQL migration (schema + indexes)

**Migration Files**:

```
migrations/
├── 001_create_deployment_tables.sql
├── 002_create_deployment_indexes.sql
├── 003_create_fraise_state_tables.sql
└── run_migrations.py
```

**Migration Runner**:

```python
async def run_migrations(adapter: FraiserDatabaseAdapter):
    """Run pending migrations for database type."""
    db_type = adapter.database_type()

    migrations_dir = f"migrations/{db_type.value}"
    for migration_file in sorted(os.listdir(migrations_dir)):
        sql = open(os.path.join(migrations_dir, migration_file)).read()
        await adapter.execute_query(sql)
```

**Tests**: 15 tests

- Migration file loading
- Migration execution
- Idempotency (running twice doesn't fail)
- Schema validation

---

### Phase 3.10.8: Backward Compatibility & Deprecation

**File**: `fraisier/database.py` (modifications)

**Strategy**:

1. Keep current `get_db()` API working
2. Have it use adapter internally
3. Gradually migrate callsites to async/await
4. Provide wrapper for sync code during transition

```python
# Old API (still works)
def get_db():
    """Legacy sync interface."""
    # Returns adapter wrapped in sync wrapper
    return _legacy_sqlite_adapter

# New API (preferred)
async def get_database():
    """Modern async interface."""
    return await get_database_adapter()

# Wrapper for transition period
class SyncDatabaseWrapper:
    """Wraps async adapter for sync code."""
    def __init__(self, adapter):
        self._adapter = adapter
        self._loop = asyncio.new_event_loop()

    def execute(self, query, params=None):
        """Sync wrapper around async execute."""
        return self._loop.run_until_complete(
            self._adapter.execute_query(query, params)
        )
```

**Tests**: 10 tests

- Legacy API still works
- Sync wrapper functionality
- No breaking changes

---

## Test Summary

**Total Tests**: 135+ tests

| Component | Tests | Status |
|-----------|-------|--------|
| Adapter Interface | 15 | New |
| SQLite Adapter | 20 | New |
| PostgreSQL Adapter | 25 | New |
| MySQL Adapter | 20 | New |
| Factory/Config | 10 | New |
| Migrations | 15 | New |
| Backward Compat | 10 | New |
| Integration | 20 | New |

---

## Code Quality Targets

- ✅ 100% ruff compliance
- ✅ Full type hints (Python 3.10+)
- ✅ Comprehensive docstrings
- ✅ No raw SQL (parameterized only)
- ✅ Async/await throughout
- ✅ Connection pool validation
- ✅ Error handling for all databases

---

## Configuration Examples

### Development (SQLite)

```bash
export FRAISIER_DB_TYPE=sqlite
export FRAISIER_DB_URL=sqlite://./fraisier.db
fraisier list
```

### Production (PostgreSQL)

```bash
export FRAISIER_DB_TYPE=postgresql
export FRAISIER_DB_URL=postgresql://user:pass@prod.example.com/fraisier
fraisier list
```

### Alternative (MySQL)

```bash
export FRAISIER_DB_TYPE=mysql
export FRAISIER_DB_URL=mysql://user:pass@db.example.com/fraisier
fraisier list
```

---

## Files to Create/Modify

### New Files (8)

1. `fraisier/db/adapter.py` - Base trait
2. `fraisier/db/sqlite_adapter.py` - SQLite impl
3. `fraisier/db/postgres_adapter.py` - PostgreSQL impl
4. `fraisier/db/mysql_adapter.py` - MySQL impl
5. `fraisier/db/factory.py` - Factory & config
6. `tests/test_database_adapters.py` - Tests
7. `fraisier/db/migrations/` - Migration files
8. `fraisier/db/__init__.py` - Package exports

### Modified Files (2)

1. `fraisier/database.py` - Integration layer
2. `fraisier/cli.py` - No changes needed (adapter handles it)

### Total Lines of Code

- **Production Code**: ~1,200 lines
- **Test Code**: ~1,000 lines
- **Migration SQL**: ~300 lines

---

## Dependencies to Add

**pyproject.toml additions**:

```toml
[project]
dependencies = [
    # ... existing ...
    # Database drivers (optional via feature flags)
    "aiosqlite>=0.19.0",           # SQLite (already in dev)
    "psycopg[binary]>=3.1.0",      # PostgreSQL
    "aiomysql>=0.2.0",              # MySQL
]

[project.optional-dependencies]
postgres = ["psycopg[binary]>=3.1.0"]
mysql = ["aiomysql>=0.2.0"]
sqlite = ["aiosqlite>=0.19.0"]
all-databases = ["psycopg[binary]>=3.1.0", "aiomysql>=0.2.0", "aiosqlite>=0.19.0"]
```

---

## Implementation Order

1. **Week 1**: Create adapter interface + SQLite impl
2. **Week 1**: Add tests (85 tests)
3. **Week 2**: PostgreSQL adapter + 25 tests
4. **Week 2**: MySQL adapter + 20 tests
5. **Week 3**: Factory, migrations, integration
6. **Week 3**: Performance testing & optimization
7. **Week 4**: Documentation & examples

---

## Success Criteria

- ✅ All 135+ tests passing
- ✅ 100% ruff compliance
- ✅ 3 databases supported (SQLite, PostgreSQL, MySQL)
- ✅ Connection pooling working (PostgreSQL + MySQL)
- ✅ Backward compatibility maintained
- ✅ Performance parity with original (or better)
- ✅ Documentation complete
- ✅ Ready for v0.1.0 release

---

## Benefits Over Phase 3 Current State

| Feature | Current | With 3.10 |
|---------|---------|-----------|
| Database Support | SQLite only | 3 databases |
| Connection Pooling | None | Full pooling |
| Production Ready | Partial | Complete |
| Type Safety | Basic | Full |
| Performance | Good | Better (pool + indexes) |
| Scalability | Limited | Enterprise-grade |
| Async Support | No | Full |

---

## Alignment with FraiseQL

This implementation adopts proven patterns from FraiseQL:

✅ **Trait-based abstraction** (FraiseQL's `DatabaseAdapter` trait)
✅ **Feature flags** (Cargo features → Python config)
✅ **Database-specific implementations** (Same structure)
✅ **Connection pooling** (deadpool → psycopg_pool, aiomysql)
✅ **Type preservation** (QueryParam → Python types)
✅ **Parameterized queries** (No raw SQL)
✅ **Error mapping** (FraiseQLError → FraiserError)
✅ **Pool metrics** (Unified PoolMetrics struct)

---

## Phase 3.10 Deliverables Summary

By implementing Phase 3.10, Fraisier becomes:

1. **Multi-database compatible** - PostgreSQL, MySQL, SQLite
2. **Production-grade** - Connection pooling, metrics, health checks
3. **Type-safe** - No SQL injection risks
4. **Scalable** - Enterprise connection management
5. **Well-tested** - 135+ tests covering all scenarios
6. **FraiseQL-aligned** - Using proven database abstraction patterns
7. **v0.1.0-ready** - Complete and verified

This positions Fraisier as a production deployment tool supporting any database system, exactly as intended for the FraiseQL ecosystem.
