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
- With read replicas, within the pin window a client reads its own writes.

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
| `replica` | `<config>:writer_reads_own_write` | while the replica's replay is paused, the writer reads the version it wrote |
| `replica` | `<config>:reads_after_catch_up` | once the replica has replayed the write, the writer and another principal both read it |
| `cutoff` | `committed_write_is_evicted` | after the client gave up inside COMMIT and the write committed, the cached read serves it |
| `cutoff` | `retry_executes_once` | after the client gave up inside COMMIT, its retry under the same `Idempotency-Key` does not execute again |
| `fence` | `inflight_read_not_stored` | a read running while a write completes is not stored for the reads after it |
| `matrix` | `<config>:k<rows>:<shape>:linearizable` | under 4 writers and 12 readers for 10 s, the checker finds no stale, fabricated or failed read of that shape |
| `pag` | `<config>:relay_<order>:each_stable_row_once` | while 3 writers insert and delete rows, each Relay walk returns every row that existed throughout it exactly once |
| `sub` | `tenant_a:exactly_once_in_commit_order` | each tenant-A subscriber receives every committed change once, none that was not committed, in commit order per account |
| `sub` | `tenant_b:receives_no_other_tenant` | a tenant-B subscriber receives none of tenant A's changes |
| `sec` | `scope:viewer_refused_secret`, `tenant:other_tenant_reads_null`, `session:rls_per_tenant` | a document one principal filled the cache with is not served to a principal with fewer rights, another tenant, or another session |

The concurrent cells:

- `matrix` runs three configs (cache off, TTL 0, TTL 5) at 1 and 20 contended rows. Its
  readers are half the writer's principal and half others of its tenant. Each worker
  draws its keys from its own seeded generator, and the histories are kept under
  `ORACLE_OUT`. `ORACLE_MATRIX_SECS` sets the duration.
- `pag` walks in three orders (primary key, `seq` descending and ascending, with ties),
  with the cache off and on. Offset pagination is not judged: each page is its own
  statement at READ COMMITTED, so offset walks skip and repeat rows under writes by
  design.
- `sub` makes 500 bumps over 20 accounts (`ORACLE_SUB_WRITES`), with the observer runtime
  feeding subscriptions from the change log.

A cell whose precondition fails stops the run with exit code 2, since its verdict would
mean nothing:

- `probe` first checks that each shape is cached at all. A write made directly in the
  database, which the server never sees, must not change the next read.
- `idem` first checks that two sends without a key record two effects.
- `cutoff` first checks that the client did give up, and that the write it gave up on
  was committed.
- `matrix`, `pag` and `sub` first check that writes succeeded, that every shape or order
  was read at least a minimum number of times, and that every subscriber was
  acknowledged.
- `fence` first checks that the slow shape is cached at all, and that its read really
  overlapped the write.
- `replica` first checks that another principal of the writer's tenant reads the old
  version while replay is paused, which proves reads do reach the lagging replica.

## The replica pair

The `replica` cells run against a primary and a hot standby streaming from it, set up
by the repo's own replication scripts (`docker/init/postgres-replication-init.sh` and
`postgres-standby-entrypoint.sh`, which the integration rigs use too). Each config is run
with the result cache off and on, at TTL 0.

- The cells do not wait on a replication delay. They hold the standby's replay with
  `pg_wal_replay_pause()`, so its lag is a state, not a race.
- After `pg_wal_replay_resume()`, the cells wait until the standby has replayed the
  primary's current WAL position before reading again.

## The cutoff and the fence

Both cells need two things to happen in a fixed order, and the fixture makes that order
certain instead of likely:

- **cutoff:** the mutations insert into `tb_commit_delay`, whose deferred constraint
  trigger makes COMMIT take 600 ms. The client gives up after 200 ms, so every round cuts
  the request inside COMMIT, and the database commits the write after the client has gone.
  Each cell runs 3 rounds and holds only if every round holds. `ORACLE_CUTOFF_ROUNDS` sets
  the number of rounds.
- **fence:** the `slow` shape's statement takes 300 ms and takes its snapshot when it
  starts. A write completes 100 ms into it. The precondition is that the slow read returns
  the version before the write, which proves the two overlapped.

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
- `ORACLE_PRIMARY_URL` and `ORACLE_REPLICA_URL` name the replica pair, as superuser URLs,
  and are required with `ORACLE_ADMIN_URL`. Without `ORACLE_ADMIN_URL`, the throwaway
  PostgreSQL is the primary, and its standby is a second container on a network of
  their own.
- `ORACLE_OUT` keeps the run's configs, logs and `outcomes.json`.

Exit codes:

- 0: the outcomes match the baseline;
- 1: they differ, with one line per difference (`NEW`, `GONE`, `NOT RUN`, `UNNAMED`);
- 2: the run could not judge.

`--bless` leaves a new entry's `issue` empty and the gate refuses it (`UNNAMED`), so
every recorded cell names its issue.

## CI

`dagger call concurrency-oracle --source=.` (`ConcurrencyOracle` in
`.dagger/oracle_concurrency.go`) builds the CLI and a server with `cli,observers`, and runs
`run.sh` against the seeded PostgreSQL 18 service the integration legs bind, with one
streaming standby of it for the replica cells. `--selftest` runs the self-test instead.
The leg is not wired into any workflow. `make oracle-concurrency` and
`make oracle-concurrency-selftest` are its local mirror.

## Self-test

`run.sh --selftest` needs no server. It runs the checker on one hand-built history per
rule, and each gate site on a baseline that differs from the outcomes by one entry.

The product mutants show that the cells catch the defects they were written for. Each
one rebuilds the server, so they run locally only:

```
tools/oracle/concurrency/mutants.sh                  # every mutant in mutants.json
tools/oracle/concurrency/mutants.sh fence_removed    # one
ORACLE_MUTANT_DIR=/srv/build/oracle-mutants CARGO_TARGET_DIR=/srv/build/oracle-mutants-target \
  tools/oracle/concurrency/mutants.sh                # keep the copy and its builds
```

`mutants.sh` copies HEAD with `git archive`, builds the server there unmutated, and
requires the oracle to match its baseline on it. Then, for each mutant:

1. it applies the mutant's exact text edits; each must occur once;
2. it rebuilds the server;
3. it runs the groups the mutant names;
4. it requires every cell in the mutant's `expect` list to be reported `NEW`;
5. it restores the copy's files from their saved bytes and checks them with `cmp`.

The working tree is never edited.

| Mutant | Edit | Must flip |
|---|---|---|
| `eviction_skipped` | a successful mutation evicts nothing | `probe:direct:evicted`, `matrix:cache_ttl0:k1:direct:linearizable` |
| `fence_removed` | a read is stored with no in-flight fence | `fence:inflight_read_not_stored` |
| `idempotency_check_removed` | the in-memory store never finds a key | `idem:sequential:executes_once` |
