# Database Compatibility Matrix

FraiseQL v2 supports four database backends with different levels of feature coverage.
PostgreSQL is the primary target; other backends are supported on a best-effort basis.

> **Cargo feature flags** for enabling each backend are documented separately in
> [`docs/features-compatibility-matrix.md`](features-compatibility-matrix.md).
> This document covers **SQL-level operation support** per dialect.

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

## Per-Operation Dialect Feature Matrix

The following sections detail which SQL operations each dialect supports, with exact
syntax differences and known limitations.

### WHERE Operators

#### Comparison and Containment

| Operator | PostgreSQL | MySQL | SQL Server | SQLite |
|----------|:----------:|:-----:|:----------:|:------:|
| `Eq` (`=`) | ✅ | ✅ | ✅ | ✅ |
| `Neq` (`!=` / `<>`) | ✅ `!=` | ✅ `!=` | ✅ `<>` | ✅ `!=` |
| `Gt` / `Gte` / `Lt` / `Lte` | ✅ | ✅ | ✅ | ✅ |
| `In` / `Nin` (NOT IN) | ✅ | ✅ | ✅ | ✅ |
| `IsNull` | ✅ | ✅ | ✅ | ✅ |

#### String Matching

| Operator | PostgreSQL | MySQL | SQL Server | SQLite |
|----------|:----------:|:-----:|:----------:|:------:|
| `Contains` (LIKE `%v%`) | ✅ | ✅ | ✅ | ✅ |
| `Icontains` | ✅ ILIKE | ✅ LIKE¹ | ✅ COLLATE CI_AI | ✅ LIKE¹ |
| `Startswith` / `Endswith` | ✅ | ✅ | ✅ | ✅ |
| `Istartswith` / `Iendswith` | ✅ ILIKE | ✅ LIKE¹ | ✅ COLLATE CI_AI | ✅ LIKE¹ |
| `Like` / `Nlike` | ✅ | ✅ | ✅ | ✅ |
| `Ilike` / `Nilike` | ✅ ILIKE | ✅ LIKE¹ | ✅ COLLATE CI_AI | ✅ LIKE¹ |
| `Regex` (`~`) | ✅ `~` | ✅ REGEXP | ❌ | ❌ |
| `Iregex` (`~*`) | ✅ `~*` | ❌ | ❌ | ❌ |
| `Nregex` (`!~`) | ✅ `!~` | ✅ NOT REGEXP | ❌ | ❌ |
| `Niregex` (`!~*`) | ✅ `!~*` | ❌ | ❌ | ❌ |

¹ MySQL with `utf8mb4_unicode_ci` and SQLite LIKE are case-insensitive by default.

#### JSON / Array Operators

| Operator | PostgreSQL | MySQL | SQL Server | SQLite |
|----------|:----------:|:-----:|:----------:|:------:|
| `ArrayContains` | ✅ `@>` | ✅ `JSON_CONTAINS()` | ✅ `EXISTS(OPENJSON)` | ✅ `EXISTS(json_each)` |
| `ArrayContainedBy` | ✅ `<@` | ❌ | ❌ | ❌ |
| `ArrayOverlaps` | ✅ `&&` | ✅ `JSON_OVERLAPS()` | ❌ | ❌ |
| `LenEq/Gt/Lt/Gte/Lte/Neq` | ✅ `jsonb_array_length()` | ❌ | ❌ | ❌ |
| `StrictlyContains` (JSONB `@>`) | ✅ | ❌ | ❌ | ❌ |

#### Domain-Specific Operators (PostgreSQL Only)

| Category | Operators | PostgreSQL | Others |
|----------|-----------|:----------:|:------:|
| **pgvector** | `CosineDistance`, `L2Distance`, `L1Distance`, `HammingDistance`, `InnerProduct`, `JaccardDistance` | ✅ | ❌ |
| **Network (INET/CIDR)** | `IsIPv4`, `IsIPv6`, `IsPrivate`, `IsPublic`, `IsLoopback`, `InSubnet`, `ContainsSubnet`, `ContainsIP`, `Overlaps` | ✅ | ❌ |
| **LTree** | `AncestorOf`, `DescendantOf`, `MatchesLquery`, `MatchesLtxtquery`, `MatchesAnyLquery`, `DepthEq/Neq/Gt/Gte/Lt/Lte`, `Lca` | ✅ | ❌ |
| **Rich filters** | `EmailDomainEq`, `EmailDomainIn`, `EmailDomainEndswith`, `EmailLocalPartStartswith`, `VinWmiEq`, `IbanCountryEq` | ✅ | ❌ planned |

### Pagination

| Feature | PostgreSQL | MySQL | SQL Server | SQLite |
|---------|:----------:|:-----:|:----------:|:------:|
| LIMIT/OFFSET | ✅ `LIMIT n OFFSET m` | ✅ `LIMIT n OFFSET m` | ✅ `OFFSET m ROWS FETCH NEXT n ROWS ONLY` | ✅ `LIMIT n OFFSET m` |
| Relay keyset (forward) | ✅ | ✅ | ✅ | ❌ |
| Relay keyset (backward) | ✅ | ✅ | ✅ | ❌ |
| Cursor types | Int64, UUID | Int64, UUID | Int64, UUID | — |

### Mutations

| Feature | PostgreSQL | MySQL | SQL Server | SQLite |
|---------|:----------:|:-----:|:----------:|:------:|
| `SupportsMutations` trait | ✅ | ✅ | ✅ | ❌ compile-time¹ |
| Function call syntax | `SELECT * FROM fn($1,$2)` | `SELECT * FROM fn(?,?)` | `SELECT * FROM fn(@p1,@p2)` | ❌ `Unsupported` |
| Returns `mutation_response` | ✅ | ✅ | ✅ | — |
| Mutation timing injection | ✅ session var | ❌ | ❌ | — |

¹ Schemas containing mutations targeting SQLite produce a compile-time warning from
`fraiseql compile`. At runtime, `execute_function_call` returns `FraiseQLError::Unsupported`.

### Window Functions

All four dialects use standard `OVER (PARTITION BY ... ORDER BY ...)` syntax for the
core window functions. The differences are in advanced frame clauses.

| Function | PostgreSQL | MySQL 8.0+ | SQL Server | SQLite 3.25+ |
|----------|:----------:|:----------:|:----------:|:------------:|
| `ROW_NUMBER()` | ✅ | ✅ | ✅ | ❌¹ |
| `RANK()` | ✅ | ✅ | ✅ | ❌¹ |
| `DENSE_RANK()` | ✅ | ✅ | ✅ | ❌¹ |
| `LAG()` / `LEAD()` | ✅ | ✅ | ✅ | ❌¹ |
| `NTILE()` | ✅ | ✅ | ✅ | ❌¹ |
| `PERCENT_RANK()` | ✅ | ✅ | ⚠️ version-dependent | ❌¹ |
| `CUME_DIST()` | ✅ | ✅ | ✅ | ❌¹ |
| Frame: `ROWS BETWEEN` | ✅ | ✅ | ✅ | ❌¹ |
| Frame: `EXCLUDE CURRENT ROW` | ✅ | ❌ | ❌ | ❌ |
| `FILTER (WHERE ...)` on aggregates | ✅ | ❌ | ❌ | ❌ |
| STDDEV/VARIANCE in window context | ✅ | ❌ | ✅ | ❌ |

¹ SQLite 3.25+ supports window functions natively, but FraiseQL does not generate
window function SQL for the SQLite dialect. Window queries return `FraiseQLError::Unsupported`.

### JSON Operations

Each dialect uses different syntax for JSON extraction and type coercion:

| Operation | PostgreSQL | MySQL | SQL Server | SQLite |
|-----------|-----------|-------|-----------|--------|
| **Extract scalar** | `data->>'field'` | `JSON_UNQUOTE(JSON_EXTRACT(data, '$.field'))` | `JSON_VALUE(data, '$.field')` | `json_extract(data, '$.field')` |
| **Nested path** | `data->'a'->'b'->>'c'` | `JSON_UNQUOTE(JSON_EXTRACT(data, '$.a.b.c'))` | `JSON_VALUE(data, '$.a.b.c')` | `json_extract(data, '$.a.b.c')` |
| **Array length** | `jsonb_array_length(expr)` | `JSON_LENGTH(expr)` | `(SELECT COUNT(*) FROM OPENJSON(expr))` | `json_array_length(expr)` |
| **Numeric cast** | `(expr)::numeric` | `CAST(expr AS DECIMAL)` | `CAST(expr AS FLOAT)` | `CAST(expr AS REAL)` |
| **Parameter cast** | `($1::text)::numeric` | implicit | implicit | implicit |

### Full-Text Search

| Variant | PostgreSQL | MySQL | SQL Server | SQLite |
|---------|-----------|-------|-----------|--------|
| **Matches** (full query syntax) | `to_tsvector(col) @@ to_tsquery($1)` | `MATCH(col) AGAINST(? IN NATURAL LANGUAGE MODE)` | `CONTAINS(col, @p1)` | ❌ |
| **PlainQuery** (word-level) | `to_tsvector(col) @@ plainto_tsquery($1)` | `MATCH(col) AGAINST(? IN BOOLEAN MODE)` | `CONTAINS(col, @p1)` | ❌ |
| **PhraseQuery** (exact phrase) | `to_tsvector(col) @@ phraseto_tsquery($1)` | `MATCH(col) AGAINST(? IN NATURAL LANGUAGE MODE)` | `FREETEXT(col, @p1)` | ❌ |
| **WebsearchQuery** (Google-like) | `to_tsvector(col) @@ websearch_to_tsquery($1)` | ❌ | ❌ | ❌ |
| **Variants available** | 4 | 3 | 2 | 0 |
| **Index requirement** | `GIN` on `tsvector` column | `FULLTEXT` index | `FULLTEXT` index | — |

### Aggregate Functions

| Function | PostgreSQL | MySQL | SQL Server | SQLite |
|----------|-----------|-------|-----------|--------|
| `COUNT` / `SUM` / `AVG` / `MIN` / `MAX` | ✅ | ✅ | ✅ | ✅ |
| `STDDEV_SAMP` | ✅ `STDDEV_SAMP()` | ✅ `STDDEV_SAMP()` | ✅ `STDEV()` | ❌ returns NULL |
| `VAR_SAMP` | ✅ `VAR_SAMP()` | ✅ `VAR_SAMP()` | ✅ `VAR()` | ❌ returns NULL |
| `STRING_AGG` | ✅ `STRING_AGG(col, sep)` | ✅ `GROUP_CONCAT(col SEPARATOR sep)` | ✅ `STRING_AGG(CAST(col AS NVARCHAR(MAX)), sep)` | ✅ `GROUP_CONCAT(col, sep)` |
| `ARRAY_AGG` | ✅ `ARRAY_AGG(col)` | ✅ `JSON_ARRAYAGG(col)` | ⚠️ emulated¹ | ⚠️ emulated² |
| `BOOL_AND` | ✅ `BOOL_AND(col)` | ❌ | ❌ | ❌ |
| `BOOL_OR` | ✅ `BOOL_OR(col)` | ❌ | ❌ | ❌ |

¹ SQL Server emulates `ARRAY_AGG` via `'[' + STRING_AGG('"' + CAST(col AS NVARCHAR(MAX)) + '"', ',') + ']'`.
² SQLite emulates `ARRAY_AGG` via `'[' || GROUP_CONCAT('"' || col || '"', ',') || ']'`.

### Common Table Expressions (CTEs)

| Feature | PostgreSQL | MySQL 8.0+ | SQL Server | SQLite 3.8.3+ |
|---------|:----------:|:----------:|:----------:|:-------------:|
| Basic CTE (`WITH ... AS`) | ✅ | ✅ | ✅ | ✅ |
| Recursive CTE (`WITH RECURSIVE`) | ✅ | ✅ | ✅ | ✅ |
| Multiple CTEs | ✅ | ✅ | ✅ | ✅ |

All four dialects use identical `WITH RECURSIVE` syntax for recursive CTEs.

### Advanced Features

| Feature | PostgreSQL | MySQL | SQL Server | SQLite |
|---------|:----------:|:-----:|:----------:|:------:|
| **Advisory locks** | ✅ `pg_try_advisory_lock()` | ❌ | ❌ | ❌ |
| **Row-level security** | ✅ Native `CREATE POLICY` | ⚠️ WHERE injection | ⚠️ WHERE injection | ⚠️ WHERE injection |
| **Field-level encryption** | ✅ via `fraiseql-secrets` | ✅ via `fraiseql-secrets` | ✅ via `fraiseql-secrets` | ❌ |
| **Connection pooling** | ✅ `deadpool-postgres` (default: 25) | ✅ `sqlx` (default: 10) | ✅ `bb8` + `tiberius` (default: 10) | ✅ `sqlx` (default: 5) |
| **Upsert** | ✅ | ✅ | ✅ | ✅ |
| **EXPLAIN support** | ✅ `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` | ❌ | ❌ | ❌ |

### SQL Syntax Differences

These low-level dialect differences are handled automatically by FraiseQL's SQL generation
layer. They are documented here for debugging and troubleshooting.

| Syntax element | PostgreSQL | MySQL | SQL Server | SQLite |
|----------------|-----------|-------|-----------|--------|
| **Identifier quoting** | `"col"` | `` `col` `` | `[col]` | `"col"` |
| **Placeholders** | `$1, $2, $3` | `?, ?, ?` | `@p1, @p2, @p3` | `?, ?, ?` |
| **String concatenation** | `\|\|` | `CONCAT()` | `+` | `\|\|` |
| **Always-true literal** | `TRUE` | `TRUE` | `1=1` | `1=1` |
| **Always-false literal** | `FALSE` | `FALSE` | `1=0` | `1=0` |
| **Case-sensitive LIKE** | `LIKE` (default) | `LIKE BINARY` | `LIKE ... COLLATE Latin1_General_CS_AS` | N/A |
| **Case-insensitive LIKE** | `ILIKE` | `LIKE` (default) | `LIKE ... COLLATE Latin1_General_CI_AI` | `LIKE` (default) |

---

## Runtime Behavior for Unsupported Features

When a query uses a feature that the target dialect does not support, FraiseQL returns a
structured error rather than silently degrading.

| Error scenario | Error type | HTTP status |
|---------------|-----------|:-----------:|
| Mutation on SQLite | `FraiseQLError::Unsupported` | 501 |
| Relay pagination on SQLite | `FraiseQLError::Unsupported` | 501 |
| Window function on SQLite | `FraiseQLError::Unsupported` | 501 |
| FTS on SQLite | `FraiseQLError::Unsupported` | 501 |
| `WebsearchQuery` on MySQL/SQL Server | `FraiseQLError::Unsupported` | 501 |
| `ArrayContainedBy` on non-PostgreSQL | `FraiseQLError::Unsupported` | 501 |
| pgvector/INET/LTree on non-PostgreSQL | `FraiseQLError::Unsupported` | 501 |
| Rich filter operators on non-PostgreSQL | `FraiseQLError::Validation` | 400 |

The `SupportsMutations` and `RelayDatabaseAdapter` traits enforce compile-time boundaries:
adapters that do not implement these traits cannot be used with mutation or relay code paths.
The SQLite adapter deliberately omits both trait implementations.

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

- [`docs/features-compatibility-matrix.md`](features-compatibility-matrix.md) — Cargo feature flags and crate-level backend support
- [`docs/adr/0009-database-feature-parity.md`](adr/0009-database-feature-parity.md) — Decision record for asymmetric feature parity
- [`docs/modules/window-functions.md`](modules/window-functions.md) — Window function dialect details
- [`docs/modules/fact-table.md`](modules/fact-table.md) — Fact table design
- [`docs/adr/0002-database-driver-choices.md`](adr/0002-database-driver-choices.md) — Driver selection rationale
