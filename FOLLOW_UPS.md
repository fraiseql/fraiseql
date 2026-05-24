## (no open follow-ups)

The previous F028 entry was closed in commit e760033ce (Wave 8) — the
`ViewName` newtype now flows through every public cache invalidation
signature.

Wave 8 also flagged one future expansion not blocking the current backlog:

### F031 expansion — executor DB-bound property coverage

**Deferred from:** Wave 8 (commit fcee0374b)

**Reason deferred:** the 9 property tests in
`crates/fraiseql-core/tests/property/property_executor.rs` cover every
public no-DB executor entry point (`parse_query`, `QueryMatcher::match_query`,
`extract_root_field_names`). The full `Executor::execute` end-to-end
pipeline needs either a testcontainer Postgres bootstrap (too slow for
proptest's case count) or a comprehensive mock `DatabaseAdapter` that
behaves like Postgres under arbitrary WHERE/ORDER/LIMIT/projection.

**Suggested follow-up:** build a deterministic in-memory mock adapter that
implements `DatabaseAdapter` with table-shaped fixtures (rows + RLS policy
fixtures), then add property tests asserting (a) `execute(query, vars)`
never returns rows that violate the RLS policy for the user's
`SecurityContext`, (b) repeated `execute` with the same input + cache
warm yields byte-equal responses, (c) variable type-checking rejects
mistyped inputs without panicking. Multi-day investment; gate on demand
(no in-the-wild bug reports yet).
