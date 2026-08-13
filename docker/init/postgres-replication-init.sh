#!/bin/bash
# Enable physical streaming replication on the test primary (#957).
#
# Owned once and mounted by BOTH rigs — `docker/docker-compose.test.yml` locally
# and `pgService` in `.dagger/main.go` — for the same reason the SQL fixtures in
# `tests/sql/postgres/` are (#936, P03): a replication setup that existed on only
# one rig would make the read-replica lag suite green on one and red on the other.
#
# Runs from `/docker-entrypoint-initdb.d`, i.e. while the entrypoint's temporary
# local-only server is up and before the real one starts, so the `pg_hba.conf`
# line appended here is in force for every subsequent boot.
#
# `wal_level`, `max_wal_senders` and `hot_standby` already default to
# `replica` / `10` / `on` on the pinned image; they are set explicitly anyway so
# a future base-image change cannot silently disable streaming and leave the lag
# suite measuring a replica that never receives WAL.
set -euo pipefail

REPLICATION_USER="${REPLICATION_USER:-fraiseql_repl}"
REPLICATION_PASSWORD="${REPLICATION_PASSWORD:-fraiseql_repl_password}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    -v repl_user="$REPLICATION_USER" -v repl_password="$REPLICATION_PASSWORD" <<-'EOSQL'
	CREATE ROLE :"repl_user" WITH REPLICATION LOGIN PASSWORD :'repl_password';
EOSQL

{
    echo "wal_level = replica"
    echo "max_wal_senders = 10"
    echo "hot_standby = on"
    # The bounded-staleness suite deliberately stops a standby's replay for a
    # few seconds. Replication slots already hold the WAL, but a floor here means
    # a suite that pauses replay can never surface as a *broken* standby — which
    # would read as the routing failing rather than as the lag it induced.
    echo "wal_keep_size = 256MB"
} >>"$PGDATA/postgresql.conf"

# initdb's generated `pg_hba.conf` allows replication connections from the local
# socket and loopback only — `host all all all` does NOT match them, because
# `replication` is a distinct pseudo-database in the database column. The standby
# connects over the container network, so it needs its own line.
echo "host replication $REPLICATION_USER all scram-sha-256" >>"$PGDATA/pg_hba.conf"
