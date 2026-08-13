#!/bin/bash
# Bring up a real PostgreSQL streaming standby of the test primary (#957).
#
# Owned once and used by BOTH rigs — `docker/docker-compose.test.yml` locally and
# `pgStandbyService` in `.dagger/main.go` — alongside
# `postgres-replication-init.sh`, which prepares the primary.
#
# Why a real standby and not a second database: `max_lag_ms` routes on *measured*
# replication lag, and lag is only measurable where `pg_is_in_recovery()` is true.
# The pre-existing read-replica suites stand an independent database in for a
# replica, which is enough to observe *which* server answered but reports no lag
# at all — so a bounded-staleness guarantee proven against it would be proven
# against a server that can never be stale.
#
# Replaces the image entrypoint entirely (there is no initdb here — the data
# directory is a physical copy of the primary), so it runs as `postgres` and
# `exec postgres` directly.
set -euo pipefail

PGDATA="${PGDATA:-/var/lib/postgresql/data}"
PRIMARY_HOST="${PRIMARY_HOST:-postgres-test}"
PRIMARY_PORT="${PRIMARY_PORT:-5432}"
PRIMARY_USER="${PRIMARY_USER:-fraiseql_test}"
PRIMARY_DB="${PRIMARY_DB:-test_fraiseql}"
PRIMARY_PASSWORD="${PRIMARY_PASSWORD:-fraiseql_test_password}"
REPLICATION_USER="${REPLICATION_USER:-fraiseql_repl}"
REPLICATION_SLOT="${REPLICATION_SLOT:-fraiseql_standby}"

# Drop this standby's slot if a previous incarnation left one behind.
#
# Restarting a standby is how the rig re-clones one — notably after the failover
# test promotes it, which is one-way. The old slot survives on the primary, and
# `pg_basebackup --create-slot` then fails with "replication slot already exists"
# on *every* retry, so the container never becomes healthy and the failure looks
# like an unreachable primary rather than a leftover. The `WHERE EXISTS` keeps
# this a no-op on the normal first boot.
drop_stale_slot() {
    PGPASSWORD="$PRIMARY_PASSWORD" psql \
        --host="$PRIMARY_HOST" --port="$PRIMARY_PORT" \
        --username="$PRIMARY_USER" --dbname="$PRIMARY_DB" \
        --no-password --quiet --tuples-only --no-align \
        --command="SELECT pg_drop_replication_slot('$REPLICATION_SLOT')
                   WHERE EXISTS (SELECT 1 FROM pg_replication_slots
                                 WHERE slot_name = '$REPLICATION_SLOT'
                                   AND NOT active)"
}

# Always re-clone. A data directory left by an earlier run belongs to an earlier
# primary — a different cluster system identifier — and streaming from it fails
# with a message about the timeline rather than about the stale volume.
find "$PGDATA" -mindepth 1 -delete

# The primary's healthcheck gates this container's start, but the replication
# role is created by an initdb-time script, and initdb-time scripts finish before
# the *real* server accepts network connections. Retry until it does.
#
# `--checkpoint=fast` is load-bearing, not a speed-up: the default spread
# checkpoint waits for the primary's next scheduled one, which is up to
# `checkpoint_timeout` (5 minutes by default) away on an idle test primary. The
# first standby of a freshly-started primary happens to find a recent startup
# checkpoint and clones immediately; the second one hangs.
until drop_stale_slot && pg_basebackup \
    --host="$PRIMARY_HOST" \
    --port="$PRIMARY_PORT" \
    --username="$REPLICATION_USER" \
    --pgdata="$PGDATA" \
    --format=plain \
    --wal-method=stream \
    --write-recovery-conf \
    --create-slot --slot="$REPLICATION_SLOT" \
    --checkpoint=fast \
    --no-password; do
    echo "standby: primary not ready for base backup yet; retrying" >&2
    find "$PGDATA" -mindepth 1 -delete
    sleep 1
done

chmod 0700 "$PGDATA"

# `--write-recovery-conf` writes `standby.signal` plus `primary_conninfo` and
# `primary_slot_name`. The slot is what keeps this honest under a deliberately
# paused replay: without it the primary is free to recycle WAL the standby has
# not replayed, and the suite's induced lag would surface as a broken standby
# instead of as lag.
exec postgres
