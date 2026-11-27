#!/usr/bin/env bash
# Init script of the replica cells' primary (/docker-entrypoint-initdb.d): lets the
# standby stream from it. wal_level, max_wal_senders and hot_standby keep their
# PostgreSQL 18 defaults (replica, 10, on).
set -euo pipefail
echo "host replication all all scram-sha-256" >>"$PGDATA/pg_hba.conf"
