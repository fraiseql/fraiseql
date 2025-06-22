# 🚀 UNIFIED CONTAINER TEST ARCHITECTURE

This directory uses FraiseQL's **unified container approach** for database testing.

```
┌─────────────────────────────────────────────┐
│  🐘 Single PostgreSQL Container (Session)   │
│     ↓ Unix Socket (Podman) / TCP (Docker)  │
│  📊 Connection Pool (2-10 connections)      │
│     ↓ Shared across ALL tests              │
│  🧪 Individual Test Transactions            │
│     ↓ Automatic rollback for isolation     │
└─────────────────────────────────────────────┘
```

## Key Benefits
- ⚡ **5-10x faster** than per-test containers
- 🔌 **Socket communication** for maximum speed
- 🔄 **Connection reuse** via pooling
- 📦 **Container caching** across test runs

## Usage
```python
@pytest.mark.database
async def test_something(db_connection):
    # You're using the unified container!
    result = await db_connection.execute("SELECT 1")
```

## Configuration
- **Implementation**: `tests/database_conftest.py`
- **Documentation**: `docs/testing/unified-container-testing.md`
- **Enable Podman**: `TESTCONTAINERS_PODMAN=true pytest`