# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-01-13

### Added

- Initial release of fraiseql-wire
- Async JSON streaming from Postgres 17
- Connection via TCP or Unix sockets
- Simple Query protocol support (no prepared statements)
- SQL predicate pushdown with `where_sql()`
- Rust-side predicate filtering with `where_rust()`
- SERVER-side `ORDER BY` support
- Configurable chunk size for memory control
- Automatic query cancellation on drop
- Bounded memory usage (scales with chunk size, not result size)
- Backpressure via async channels
- `FraiseClient` high-level API with fluent query builder
- Connection string parsing (postgres:// and postgres:///)
- Comprehensive error types with context
- Module-level documentation
- Integration tests with real Postgres
- Examples demonstrating key use cases

### Design Constraints

- Single `data` column (json/jsonb type)
- View naming convention: `v_{entity}`
- Read-only operations only (no INSERT/UPDATE/DELETE)
- No prepared statements (Simple Query protocol only)
- No transaction support
- One active query per connection
- Sequential result streaming (no client-side reordering)

### Features NOT Included

- Arbitrary SQL support (limited to SELECT with WHERE/ORDER BY)
- Multi-column result sets
- Client-side sorting or aggregation
- Server-side cursors
- COPY protocol
- Transactions
- Write operations
- Analytical SQL (GROUP BY, HAVING, window functions)
- Fact tables (`tf_{entity}`)
- Arrow data plane (`va_{entity}`)
- Connection pooling
- TLS support
- SCRAM authentication (cleartext only)
- Typed streaming (returns `serde_json::Value`)

### Performance Characteristics

- Time-to-first-row: sub-millisecond (no buffering)
- Memory overhead: O(chunk_size) only
- Protocol overhead: minimal (from-scratch implementation)
- Latency: optimized for streaming use cases
- Not optimized for: single-row retrieval, batch operations

### Documentation

- Comprehensive README with quick start
- API documentation for all public modules
- Integration test examples
- Error handling patterns
- Advanced filtering example
- Contributing guidelines

---

## How to Read This Changelog

- **Added** for new features
- **Changed** for changes in existing functionality
- **Deprecated** for soon-to-be removed features
- **Removed** for now removed features
- **Fixed** for any bug fixes
- **Security** for vulnerability fixes

[0.1.0]: https://github.com/fraiseql/fraiseql-wire/releases/tag/v0.1.0
