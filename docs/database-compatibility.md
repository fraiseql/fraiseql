# Database Compatibility

**FraiseQL supports PostgreSQL. It does not support MySQL, SQLite or SQL Server.**

MySQL, SQLite and SQL Server adapters were removed in v2.15.0. This page records
what was removed, why, and what to do if you were relying on it.

## Supported

| Backend | Status |
|---------|--------|
| PostgreSQL 14+ | Supported — the only backend |

A `mysql://`, `sqlite://` or `sqlserver://` database URL is refused at startup
with an explanatory error, in both `fraiseql-server` and `fraiseql run`. It is
not silently downgraded and it does not reach a driver.

## Why they were removed

Three audit passes over `fraiseql-db` found the non-PostgreSQL paths had never
been executed against a real database. The evidence was not marginal:

- Every field-projected query failed on MySQL and SQLite: the runtime spliced a
  PostgreSQL-only `jsonb_build_object(...)` projection into their SQL
  ([#799](https://github.com/fraiseql/fraiseql/issues/799)). That is the primary
  query shape.
- MySQL boolean equality never matched `true`, and `neq: true` matched every row
  ([#831](https://github.com/fraiseql/fraiseql/issues/831)).
- MySQL numeric comparison rounded to an integer, so `19.99` and `20.4` compared
  equal ([#830](https://github.com/fraiseql/fraiseql/issues/830)).
- Boolean `ORDER BY` on MySQL collapsed every sort key to 0
  ([#829](https://github.com/fraiseql/fraiseql/issues/829)).
- Cursor-paginated sorts were silently ignored
  ([#832](https://github.com/fraiseql/fraiseql/issues/832)).
- A client-controlled `where` field name could break out of a MySQL string
  literal ([#833](https://github.com/fraiseql/fraiseql/issues/833)), and a
  multi-argument SQLite `DELETE` dropped every filter after the first, widening
  the delete ([#834](https://github.com/fraiseql/fraiseql/issues/834)).
- Compiled dialect templates contained SQL that cannot run: `LEAST`/`GREATEST`
  on SQLite, a literal `*` in a SQL Server `LIKE`, TLD arithmetic that returns
  the wrong substring ([#721](https://github.com/fraiseql/fraiseql/issues/721)).

Fixing all of that means maintaining three more per-dialect integration matrices
in CI forever, and gating every future SQL change on all four. FraiseQL's design
is PostgreSQL-shaped throughout — the Trinity view model, JSONB `data` columns,
RLS-based tenancy, `LISTEN/NOTIFY` subscriptions, WAL-based CDC — so the other
three could only ever have offered the commodity query surface. Advertising
support that fails on the primary query shape, while shipping two security
defects, is worse than not offering it.

Both security defects are resolved: `#834` left with the SQLite adapter, and
`#833`'s fix — validating `where` field names against the GraphQL identifier
pattern at the parse boundary, the same rule `orderBy` already enforced — was
kept, because it protects PostgreSQL too.

## If you were using a non-PostgreSQL backend

Migrate to PostgreSQL. Given the defects above, a deployment on MySQL, SQLite or
SQL Server was returning wrong results on filters, sorts and projections rather
than working, so treat the data as suspect rather than as a baseline to
reproduce.

The following are gone: the `mysql`, `sqlite` and `sqlserver` Cargo features on
every crate; `MySqlAdapter` / `SqliteAdapter` / `SqlServerAdapter` and their
introspectors; the `MySqlDialect` / `SqliteDialect` / `SqlServerDialect`
implementations; the per-dialect projection generators; the `MySQL`, `SQLite`
and `SQLServer` variants of `DatabaseType`; and the `[collation.database_overrides.*]`
config tables for those engines — a config still carrying one now fails to parse
rather than being silently ignored.

## PostgreSQL requirements

- PostgreSQL 14 or newer.
- The `pg_stat_statements` extension for `/api/v1/query-stats` (optional).
- Views projecting a non-NULL JSONB `data` column — see
  [the architecture overview](architecture/overview.md).
