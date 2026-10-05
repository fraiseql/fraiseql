# Mutation Timing

Mutation timing stamps a PostgreSQL session variable before each mutation
function call, so SQL functions can compute their own execution duration
without application-level instrumentation. It is on by default.

## How it works

Every write (`Writer::execute_write`, which runs each mutation in one transaction)
executes, in that transaction and before the function:

```sql
SELECT set_config('fraiseql.started_at', clock_timestamp()::text, true);
```

The `true` argument to `set_config` makes it a `SET LOCAL`, scoping the
variable to the current transaction only. Your SQL function can then read
`current_setting('fraiseql.started_at')` and compare it with
`clock_timestamp()` to measure elapsed time. The timestamp is taken on the database's
clock, the same clock the change log uses to close the interval, so there is no
application-to-database skew.

Only mutations are stamped. A read does not set `fraiseql.started_at`; a view that needs
the time a statement started uses `statement_timestamp()`.

## Configuration

The stamp is on unless you turn it off, in `fraiseql.toml`:

```toml
[fraiseql.session_variables]
inject_started_at = false
```

(In a TOML schema file, the same key sits under `[session_variables]`.) The variable is
always named `fraiseql.started_at`. When the change log is enabled, a mutation is stamped
even with `inject_started_at = false`, because the change-log row reads the variable to
record the mutation's duration.

## Example SQL function

```sql
CREATE OR REPLACE FUNCTION fn_create_order(p_data jsonb)
RETURNS mutation_response AS $$
DECLARE
    v_started_at timestamptz;
    v_duration interval;
BEGIN
    v_started_at := current_setting('fraiseql.started_at')::timestamptz;

    -- ... perform the mutation ...

    v_duration := clock_timestamp() - v_started_at;
    RAISE LOG 'fn_create_order took %', v_duration;

    RETURN (true, 'Order created')::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

## Adapter API

An embedder constructing the adapter directly can stamp an additional variable, under a
name of its choosing, with the `with_mutation_timing` builder method:

```rust
let adapter = PostgresAdapter::new(&db_url)
    .await?
    .with_mutation_timing("fraiseql.started_at");
```

## Performance

Each stamped mutation costs one `set_config` call inside its transaction, applied with the
other session variables. Reads are not affected.

## Database support

Mutation timing is built on the PostgreSQL `set_config` / `current_setting`
session functions.
