# Mutation Transaction Retry

PostgreSQL aborts a transaction it picked as a deadlock victim (`40P01`, *deadlock detected*)
or could not serialize (`40001`, *serialization failure*). Running the same transaction again
usually succeeds. FraiseQL can do that for you: it re-runs the mutation's whole transaction,
from `BEGIN`, a few times before returning the error.

**Retry is off by default.** A retried mutation runs its function's body again. Everything
inside the transaction is rolled back between attempts, but an effect outside it is not: a
`dblink` or `pg_background` call, a `nextval` (sequences are not transactional), a call
out to another system. Such an effect can happen once per attempt. The server cannot see
whether a function has one, so you opt in, and a mutation whose function has one can opt
out (below).

## What is retried

A mutation's transaction runs again when, and only when:

- PostgreSQL raises `40P01` or `40001`, from the function, from anything the transaction
  sends with it (the change-log outbox row, a pg_tviews report), or from the `COMMIT` (a
  deferred constraint or trigger), and
- the policy has attempts left.

Each attempt is a new transaction against the data as it is then: the session variables, the
call, the outbox row and every report are sent again. A batch (`execute_writes`) commits all of
its calls or none, so the whole batch runs again from its first call. A dry run is retried like
a commit and still persists nothing.

Before the next attempt, the failed transaction is rolled back explicitly on the same
connection, then the attempt waits a random time (below). The connection is held across the
wait.

## What never is

- **A lost connection.** Whether the write committed is unknown, so it is not run again. That
  includes a `ROLLBACK` before a retry that fails: the attempt's own error is returned.
- **Any other error.** A constraint violation (`23505`), a refusal by pg_tviews (`0A000`), a
  timeout: running it again would fail the same way.
- **A refusal** by the mutation's row gate. It is a decision, not a failure.
- **Anything after a successful `COMMIT`.**
- **A mutation that opts out**, and any batch that holds one (below).

When the attempts run out, the last attempt's error is returned, exactly as it would be
without retry: the same SQLSTATE and message, and in a batch the same call number.

## Configuration

The server file:

```toml
# server.toml
[transaction_retry]
max_attempts = 3   # the first attempt included; 1 turns retry off
base_ms = 10       # the first retry waits up to this long
max_ms = 200       # no wait exceeds this (at most 5000)
```

`fraiseql run`, in `fraiseql.toml`, takes the same keys as `[database.transaction_retry]`. A
setting made in both files is refused.

**An empty `[transaction_retry]` section opts in at `max_attempts = 3`, `base_ms = 10`,
`max_ms = 200`.** Without the section, a mutation runs once.

`FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS` (or `--transaction-retry-max-attempts`) overrides
`max_attempts`. Without the section, it opts in at the section's other defaults.

The `n`th retry waits a random time between 0 and `base_ms · 2ⁿ⁻¹`, capped at `max_ms`
("full jitter"): two transactions that deadlocked each other do not retry in step.

Each bound is checked at startup and refused by name: `max_attempts` of at least 1, `base_ms`
no more than `max_ms`, `max_ms` no more than 5000. A misspelt key is refused.

Tenant pools follow the server's `[transaction_retry]`. A tenant registration cannot set its
own.

## Opting a mutation out

A mutation whose function has an effect outside its transaction can say so in the schema. It
then runs once, whatever `[transaction_retry]` says, and returns the abort's error as it would
without retry:

```python
@fraiseql.mutation(sql_source="fn_charge_card", retry_on_abort=False)
def charge_card(input: ChargeInput) -> Charge: ...
```

```toml
# fraiseql.toml
[mutations.charge_card]
sql_source = "fn_charge_card"
return_type = "Charge"
retry_on_abort = false
```

The default, `retry_on_abort = true`, follows the server: with no `[transaction_retry]`, a
mutation runs once either way.

**A batch holding a mutation that opts out runs once as a whole.** A batch commits all of its
calls or none, so retrying it would run the opted-out call again too.

The compiled schema carries the flag only on a mutation that opts out. A mutation not retried
because of it is logged at `debug`; it is not counted as a retry or an exhaustion.

## Observability

On `/metrics`, labelled by `function` and `sqlstate`:

- `fraiseql_mutation_transaction_retries_total` counts each retry;
- `fraiseql_mutation_transaction_retries_exhausted_total` counts each mutation whose last
  attempt also failed with `40P01` or `40001`.

Each retry is also logged at `debug`, with the attempt, the SQLSTATE and the wait.

## See also

[Mutation timing](mutation-timing.md): the session variable each attempt stamps again.
