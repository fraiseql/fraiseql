# Error Handling

A FraiseQL query can fail in three places, and code that handles only the first
accepts the other two silently:

1. as `Err(FraiseQLError)` from `Executor::execute` — the document never became an
   execution, so there is no response at all;
2. **in-band**, as a `data`/`errors` GraphQL response that `execute` returns as
   `Ok`. Treating that as success is how a failed query reports as a good one;
3. **not at all.** A `limit` the engine cannot read is dropped rather than
   rejected, so the query succeeds and returns the wrong number of rows.

This example runs seven deliberately broken queries and prints, for each, which of
the three happened. It ends by failing a connection on purpose.

## Run it

```bash
createdb fraiseql_example
psql -v ON_ERROR_STOP=1 -d fraiseql_example -f ../basic/sql/setup.sql
export DATABASE_URL=postgresql://localhost/fraiseql_example

./run.sh
```

## What to read

`classify()` in `src/main.rs` sorts an error by **who has to act** — the caller
(fix the query), the operator (fix the deployment), or nobody (retry) — rather than
by variant name. That is the distinction that matters when the error reaches a
status code.

Then read the last two cases of the output. Neither is what it should be, and one
raises nothing at all; both are tracked as
[#1197](https://github.com/fraiseql/fraiseql/issues/1197). Every case here is
executed rather than described, so this example prints what the engine in this tree
actually does — including where that is wrong.

## Uses

`examples/basic` — the blog schema.
