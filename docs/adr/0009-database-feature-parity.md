# ADR-0009: Asymmetric Database Feature Parity

## Status: Accepted

## Context

FraiseQL supports four database backends: PostgreSQL, MySQL, SQL Server, and SQLite.
Not all GraphQL features can be implemented uniformly across all backends because some
features depend on database-specific constructs that have no direct equivalents.

Specific constraints:

- **Stored procedures / functions**: SQLite has no equivalent to PostgreSQL `fn_*` functions
- **JSONB columns**: PostgreSQL-only; MySQL has JSON but no binary storage type with operator support
- **LISTEN/NOTIFY**: PostgreSQL pub/sub mechanism; no equivalent in MySQL/SQL Server/SQLite
- **Window functions**: All four databases support them, but with dialect differences
- **Wire protocol**: PostgreSQL wire protocol (v3) is unique to PostgreSQL

Two design alternatives were considered:

**Option A**: Feature parity gate — only advertise a feature as supported once all four
databases implement it. This means mutations, subscriptions, fact tables, and streaming
would all be "not supported" until MySQL/SQL Server/SQLite equivalents are written.

**Option B**: Asymmetric parity — PostgreSQL is the primary target with full feature
coverage. Other databases are supported on a best-effort basis, with explicit
`FraiseQLError::Unsupported` for missing features rather than silent failures.

## Decision

**Option B: Asymmetric feature parity.**

PostgreSQL is designated the primary database target and will always have full feature
coverage. Other databases are supported for the features they can reasonably implement.
Features that are PostgreSQL-specific by nature (JSONB, LISTEN/NOTIFY, wire protocol)
are PostgreSQL-only by design, not by oversight.

This is an explicit tradeoff, publicly documented in `docs/database-compatibility.md`.

## Consequences

**Positive**:

- PostgreSQL users get the full FraiseQL feature set immediately
- MySQL and SQL Server users get the subset of features their database supports
- New database features do not require simultaneous multi-database implementation
- SQLite can be used for local development without blocking production features

**Negative**:

- A schema authored in Python/TS may fail at runtime on a non-PostgreSQL database
- Developers must consult the compatibility matrix before targeting a specific database

**Mitigations**:

- `fraiseql compile` emits warnings when mutations/relay/subscriptions are combined with SQLite
- `docs/database-compatibility.md` is maintained as the authoritative feature matrix
- `FraiseQLError::Unsupported` is returned with a descriptive message (not a panic)
- Cross-database parity tests ensure that supported features behave identically across backends

## See Also

- `docs/database-compatibility.md` — Full feature matrix
- `docs/adr/0002-database-driver-choices.md` — Driver selection per database
