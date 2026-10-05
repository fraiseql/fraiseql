# PostgreSQL Compatibility

## Supported versions

| PostgreSQL | Status |
|------------|--------|
| **18.x** | Supported. The `integration (wire)` CI leg runs every `tests/*` binary against PostgreSQL 18 with SCRAM-SHA-256 authentication. |
| **17 and older** | Not supported. |

PostgreSQL 18 is the minimum version for all of FraiseQL (see
`docs/database-compatibility.md` in the repository). `fraiseql-wire` does not ask the server its
version, so it does not refuse an older one; it is simply not tested there, and FraiseQL's own
SQL may use PostgreSQL 18 features.

## What the crate relies on

- The frontend/backend protocol, version 3.0: startup, simple query, `RowDescription`,
  `DataRow`, `CommandComplete`, `ErrorResponse`, `ReadyForQuery`.
- SCRAM-SHA-256 or cleartext password authentication, optionally over TLS.
- Rows arrive one `DataRow` message at a time and are decoded as they arrive; the crate does
  not use libpq or any chunked-rows mode.

## When a new PostgreSQL major ships

1. Add the new major to the `integration (wire)` leg's PostgreSQL image.
2. Run the wire suite (`cargo test -p fraiseql-wire`, with the database bound).
3. Update the table above.
