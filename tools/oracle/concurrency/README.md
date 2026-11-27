# Concurrency oracle

What a client may read while writes happen. Writers bump per-account version counters
through GraphQL mutations, and readers read the counters back through several kinds of
view. Each cell checks one documented promise. A cell that does not hold must be
recorded in `baseline.json` with the issue that tracks it, and the gate compares in both
directions.

## The promises

From [cache.md](../../../docs/modules/cache.md) and the server's mutation path:

- On one server, a cached read is never staler than the last write made through it.
- A successful mutation evicts every cached entry of every view it reaches.
- A mutation carrying an `Idempotency-Key` header executes at most once per key.

## Read shapes

Each shape reads one account's `version` through a different kind of view
(`fixture/db.sql`):

| Shape | Query | How the view reaches the account |
|---|---|---|
| `direct` | `account(id)` | its own view, `v_account` |
| `list` | `accounts(where: { id })` | the same view, as a list |
| `nested` | `items { account { version } }` | a subquery over `v_account` |
| `nestedcol` | `items { accountVersion }` | a scalar subquery over `tb_account` |
| `fn` | `itemsFn { accountVersion }` | a plpgsql function, over a table `v_item` also reads |
| `notefn` | `notesFn { accountVersion }` | a plpgsql function, over a table no other view reads |
| `noteatomic` | `notesAtomic { accountVersion }` | a `BEGIN ATOMIC` SQL function, over a table no other view reads |
| `summary` | `accountSummaries(where: { id })` | a second view over `tb_account` |

## Cells

Keys are `CONC:<group>:<cell>`.

| Group | Cell | Holds when |
|---|---|---|
| `probe` | `<shape>:evicted` | after a cached read and a write through the same server, the next read returns the written version |
| `probe` | `error_outcome:consistent` | after a mutation that writes and then returns an error outcome, what is served is what is stored |
| `idem` | `sequential:executes_once` | three sends under one key record one effect |
| `idem` | `concurrent:executes_once` | five concurrent sends under one key, each running 400 ms, record one effect |

A cell whose precondition fails stops the run with exit code 2, since its verdict would
mean nothing:

- `probe` first checks that each shape is cached at all. A write made directly in the
  database, which the server never sees, must not change the next read.
- `idem` first checks that two sends without a key record two effects.

## The checker

`checker.py` judges the histories of the concurrent cells. A history is one line per
operation, with monotonic invoke and complete times. A read is:

- **stale** when it returns less than a version some write on its key had already
  returned before the read was sent;
- **fabricated** when it returns a version no write could have produced by the time it
  completed;
- an **error** when it returned no version.

A write and a read that overlap may be ordered either way. `selftest.py` holds one
hand-built history per rule, each with its expected verdict.

## Running

```
tools/oracle/concurrency/run.sh              # judge against baseline.json
tools/oracle/concurrency/run.sh --only probe # one group
tools/oracle/concurrency/run.sh --bless      # rewrite baseline.json from this run
tools/oracle/concurrency/run.sh --selftest   # the checker and the gate, no server
```

- `FRAISEQL_CLI` and `FRAISEQL_SERVER` name the binaries under test. The server needs
  the features `cli,observers`.
- `ORACLE_ADMIN_URL` names a PostgreSQL to create the fixture database in. Without it,
  a throwaway PostgreSQL 18 container is started.
- `ORACLE_OUT` keeps the run's configs, logs and `outcomes.json`.

Exit codes:

- 0: the outcomes match the baseline;
- 1: they differ, with one line per difference (`NEW`, `GONE`, `NOT RUN`, `UNNAMED`);
- 2: the run could not judge.

`--bless` leaves a new entry's `issue` empty and the gate refuses it (`UNNAMED`), so
every recorded cell names its issue.
