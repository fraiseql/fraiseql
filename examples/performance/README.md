# Performance

Measuring a query instead of guessing about it.

One timing is noise. This runs the same query 50 times and reports
first/min/median/max, then does the same for a wider selection over the same rows,
so the cost of the payload is separated from the cost of the round trip.

## Run it

```bash
createdb fraiseql_example
psql -v ON_ERROR_STOP=1 -d fraiseql_example -f ../basic/sql/setup.sql
export DATABASE_URL=postgresql://localhost/fraiseql_example

./run.sh
```

## What to read

The numbers are from your machine and your database. They are useful as a ratio
between the two selections, not as an absolute.

In a running server, per-operation latency is the `fraiseql_query_duration_seconds`
histogram on the Prometheus endpoint (`/metrics` by default).

## Uses

`examples/basic` — the blog schema.
