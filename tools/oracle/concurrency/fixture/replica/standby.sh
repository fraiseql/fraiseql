#!/usr/bin/env bash
# Entrypoint of the replica cells' standby: copies the primary at ORACLE_PRIMARY_HOST
# with pg_basebackup, then runs as a hot standby streaming from it. POSTGRES_PASSWORD is
# the primary's superuser password; `-R` writes it into primary_conninfo.
set -euo pipefail
host="${ORACLE_PRIMARY_HOST:?}"
export PGPASSWORD="${POSTGRES_PASSWORD:?}"
for _ in $(seq 240); do
  pg_isready -q -h "$host" -U postgres && break
  sleep 0.5
done
data=/var/lib/postgresql/standby
mkdir -p "$data"
chown postgres "$data"
chmod 700 "$data"
for _ in $(seq 60); do
  gosu postgres pg_basebackup -h "$host" -U postgres -D "$data" -R -X stream && break
  rm -rf "${data:?}"/*
  sleep 1
done
exec gosu postgres postgres -D "$data" -c listen_addresses='*'
