# Basic Query

The smallest thing that is genuinely FraiseQL: load a compiled schema, put an
executor on top of a PostgreSQL connection, run one GraphQL query, print the
response. No HTTP layer, no code generation.

`schema.compiled.json` already contains the SQL for every query the schema
declares. The executor matches the incoming document to one of those templates,
binds its variables, and projects the JSONB result. That is the whole runtime.

## Run it

```bash
createdb fraiseql_example
psql -v ON_ERROR_STOP=1 -d fraiseql_example -f ../basic/sql/setup.sql
export DATABASE_URL=postgresql://localhost/fraiseql_example

./run.sh
```

`run.sh` compiles `../basic/schema.json` first, because the compiled schema is a
build artifact and is gitignored. Once it exists, `cargo run` on its own works.

## What to read

`src/main.rs`, in order: `CompiledSchema::from_json`, `PostgresAdapter::new`,
`Executor::new`, `executor.execute`. Four calls, and the last one is the query.

Note the check after `execute` returns. A GraphQL response carries resolution
errors **in-band**, so `Ok` is not yet success — see `../error-handling`.

## Uses

`examples/basic` — the blog schema (`User`, `Post`).
