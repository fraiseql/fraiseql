# Database Compatibility Matrix

FraiseQL v2 supports four database backends with different levels of feature coverage.
PostgreSQL is the primary target; other backends are supported on a best-effort basis.

## Feature Matrix

| Feature | PostgreSQL | MySQL | SQL Server | SQLite |
|---------|:----------:|:-----:|:----------:|:------:|
| **SELECT queries** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Mutations (`fn_*`)** | ✅ | ✅ | ✅ | ❌ |
| **Relay (keyset pagination, forward)** | ✅ v2.0 | ✅ v2.1 | ✅ v2.0 | ❌ |
| **Relay (backward pagination)** | ✅ | ✅ | ✅ v2.1 | ❌ |
| **Aggregate queries** | ✅ | ✅ | ✅ | ✅ limited |
| **Window functions** | ✅ Full | ⚠️ Partial | ⚠️ Partial | ❌ |
| **Fact table queries (JSONB)** | ✅ | ❌ | ❌ | ❌ |
| **Subscriptions (LISTEN/NOTIFY)** | ✅ | ❌ | ❌ | ❌ |
| **Field-level encryption** | ✅ | ✅ | ✅ | ❌ |
| **APQ (in-memory)** | ✅ | ✅ | ✅ | ✅ |
| **APQ (Redis-backed)** | ✅ | ✅ | ✅ | ✅ |
| **Row-level security** | ✅ Native RLS | ✅ SQL WHERE | ✅ SQL WHERE | ✅ SQL WHERE |
| **`execute_function_call`** | ✅ | ✅ | ✅ | ❌ |
| **Federation support** | ✅ | ✅ | ✅ | ❌ |
| **Wire protocol streaming** | ✅ fraiseql-wire | ❌ | ❌ | ❌ |
| **Arrow Flight** | ✅ fraiseql-arrow | ❌ | ❌ | ❌ |
| **Cross-database parity tests** | ✅ | ✅ | ✅ | — |

**Legend**:

- ✅ Fully supported
- ✅ v2.1 — Added in v2.1
- ⚠️ Partial/limited support (see notes below)
- ❌ Not supported — explicit `FraiseQLError::Unsupported` returned at runtime

---

## Notes Per Feature

### Mutations

SQLite has no stored procedure support. Calling a mutation on SQLite returns
`FraiseQLError::Unsupported` at runtime. **Use PostgreSQL for any schema containing mutations.**

The `fraiseql compile` CLI warns when a schema containing mutations targets SQLite:

```
Warning: Schema contains N mutation(s) but target database is SQLite.
         Mutations are not supported on SQLite.
         See: https://fraiseql.dev/docs/database-compatibility
```

### Relay Pagination

- **MySQL**: Added in v2.1. Uses `CHAR(36)` for UUID cursors and `JSON_UNQUOTE(JSON_EXTRACT)` for sort columns.
- **SQL Server**: Forward pagination uses `OFFSET/FETCH NEXT`. Backward pagination uses an
  inner DESC subquery with outer re-sort (fixed in v2.1).
- **SQLite**: No implementation. Relay queries return `FraiseQLError::Unsupported`.

### Window Functions

PostgreSQL supports the full WINDOW clause including `FILTER (WHERE ...)` on aggregates
and all frame specifications.

MySQL 8+ supports window functions but lacks the `FILTER` clause and has limitations
with `ROWS BETWEEN` in some configurations.

SQL Server supports most window functions but has `ROWS vs RANGE` differences and some
version constraints on `PERCENT_RANK`.

See [`docs/modules/window-functions.md`](modules/window-functions.md) for the per-function
dialect support table.

### Fact Table Queries

Fact tables (`tf_*`) use JSONB columns for flexible dimension storage. This is a
PostgreSQL-only data type. MySQL and SQL Server do not support fact table queries.

A MySQL-compatible approach using `JSON_EXTRACT` paths is planned for a future release.

See [`docs/modules/fact-table.md`](modules/fact-table.md) for the design rationale.

### Subscriptions

GraphQL subscriptions use PostgreSQL's `LISTEN/NOTIFY` mechanism. This is
PostgreSQL-specific. No equivalent is implemented for MySQL, SQL Server, or SQLite.

### Wire Protocol Streaming

`fraiseql-wire` is a custom streaming protocol built on top of PostgreSQL's wire protocol.
It is not portable to other databases by design.

### SQLite Scope

SQLite is supported for **local development and testing** only. It implements full SELECT
queries but lacks stored procedures (mutations), JSONB (fact tables), and LISTEN/NOTIFY
(subscriptions). Do not deploy SQLite in production for any schema using these features.

---

## Choosing a Database

| Use case | Recommended database |
|----------|---------------------|
| Production with full feature set | PostgreSQL |
| Enterprise / SQL Server shops | SQL Server (most features) |
| MySQL-only environments | MySQL (no fact tables or subscriptions) |
| Local development, no mutations | SQLite |
| Streaming analytics | PostgreSQL + fraiseql-wire |

---

## Related

- [`docs/adr/0009-database-feature-parity.md`](adr/0009-database-feature-parity.md) — Decision record for asymmetric feature parity
- [`docs/modules/window-functions.md`](modules/window-functions.md) — Window function dialect details
- [`docs/modules/fact-table.md`](modules/fact-table.md) — Fact table design
- [`docs/adr/0002-database-driver-choices.md`](adr/0002-database-driver-choices.md) — Driver selection rationale
