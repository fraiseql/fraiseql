# Performance

Measuring a query instead of guessing about it.

1. **Wall clock, repeated.** One timing is noise. This runs the same query 50
   times and reports first/min/median/max, then does the same for a wider
   selection over the same rows, so the cost of the payload is separated from the
   cost of the round trip.
2. **`QueryTraceBuilder`** — per-phase spans, so "the query is slow" becomes "the
   connection is slow" or "the projection is slow".
3. **`SqlQueryLogBuilder`** — one structured record per statement, with a slow
   threshold, which is what you actually ship to a log aggregator.

## Run it

```bash
createdb fraiseql_example
psql -v ON_ERROR_STOP=1 -d fraiseql_example -f ../basic/sql/setup.sql
export DATABASE_URL=postgresql://localhost/fraiseql_example

./run.sh
```

## What to read

Two habits worth copying:

- **The trace accounts for its own duration.** The benchmark loop is recorded as a
  phase, so the spans add up to the total. A trace whose phases cover a fraction of
  its own elapsed time tells you nothing about where the time went.
- **Both logged statements are really executed and really timed.** The builder
  starts its clock when it is constructed. A log record assembled around a duration
  nobody measured is worse than no record — it reads as evidence.

The numbers are from your machine and your database. They are useful as a ratio
between the two selections, not as an absolute.

## Uses

`examples/basic` — the blog schema.
