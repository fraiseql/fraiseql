# ADR-0002: Native Rust Database Drivers per Backend

## Status: Superseded (2026-08-01)

> **Superseded by the G2 decision (2026-08-01, [#374](https://github.com/fraiseql/fraiseql/issues/374)).**
> FraiseQL is PostgreSQL-only as of v2.15.0: the MySQL, SQLite and SQL Server
> adapters were removed. This ADR is retained as the historical record of the
> multi-backend design it documents. See
> [`docs/database-compatibility.md`](../database-compatibility.md).


## Context

FraiseQL supports multiple databases (PostgreSQL, MySQL, SQLite, SQL Server). Using a single ORM or abstraction layer (Diesel, SeaORM, sqlx) creates constraints: ORMs add runtime overhead and limit low-level control needed for streaming and optimization. Different databases have different performance characteristics and wire protocol capabilities.

## Decision

Use native Rust drivers for each database backend:

- **PostgreSQL**: tokio-postgres with custom wire protocol extension for streaming
- **MySQL**: sqlx async with manual query building
- **SQLite**: rusqlite for synchronous access
- **SQL Server**: tds (Tabular Data Stream) crate

No ORM. Implement database-agnostic traits (`DatabaseConnection`, `QueryExecutor`) with per-backend implementations. Custom wire protocol for streaming JSON results without buffering.

## Consequences

**Positive:**

- Maximum performance and control
- Backend-specific optimizations possible
- Sub-millisecond time-to-first-byte streaming
- No ORM overhead

**Negative:**

- More code per database backend
- Higher implementation effort
- Larger testing surface

## Alternatives Considered

1. **Single ORM (Diesel/SeaORM)**: Reduces code but limits streaming and optimization
2. **ODBC abstraction**: Adds network round-trip overhead
3. **Runtime database selection**: More flexible but increases per-query overhead
