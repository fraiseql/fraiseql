# Mutation Timing

Mutation timing injects a PostgreSQL session variable before each mutation
function call, allowing SQL functions to compute their own execution duration
without application-level instrumentation.

## How it works

When enabled, `execute_function_call` wraps each mutation in a transaction and
executes:

```sql
SELECT set_config('fraiseql.started_at', clock_timestamp()::text, true);
```

The `true` argument to `set_config` makes it a `SET LOCAL`, scoping the
variable to the current transaction only. Your SQL function can then read
`current_setting('fraiseql.started_at')` and compare it with
`clock_timestamp()` to measure elapsed time.

## Configuration

Add the following to your `fraiseql.toml`:

```toml
[database.mutation_timing]
enabled = true
# Optional: override the default variable name
# variable_name = "fraiseql.started_at"
```

The variable name defaults to `fraiseql.started_at`.

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

If constructing the adapter programmatically (outside the TOML config flow),
use the `with_mutation_timing` builder method:

```rust
let adapter = PostgresAdapter::new(&db_url)
    .await?
    .with_mutation_timing("fraiseql.started_at");
```

## Performance

When disabled (the default), there is zero overhead: the existing
single-query code path is used. When enabled, each mutation function call
acquires one additional round-trip for the `set_config` call within the
same transaction.

## Database support

Mutation timing is PostgreSQL-only. The `set_config` / `current_setting`
functions are PostgreSQL-specific. Other adapters (MySQL, SQLite, SQL Server)
are unaffected.
