#!/usr/bin/env bash
# Run the concurrency oracle end to end (see README.md).
#
#   tools/oracle/concurrency/run.sh [--bless | --selftest | --only GROUPS]
#
# Environment:
#   FRAISEQL_CLI, FRAISEQL_SERVER  binaries under test. The server must be built with the
#                                  features cli,observers (default: target/release/*).
#   ORACLE_ADMIN_URL               a PostgreSQL URL with CREATE DATABASE rights. When
#                                  unset, a throwaway PostgreSQL 18 container is started
#                                  (ORACLE_PG_IMAGE, labelled fraiseql.oracle=concurrency
#                                  plus ORACLE_DOCKER_LABEL when set) and removed on exit,
#                                  with its volumes.
#   ORACLE_PRIMARY_URL,            superuser URLs of a primary and a hot standby streaming
#   ORACLE_REPLICA_URL             from it, for the replica cells; required with
#                                  ORACLE_ADMIN_URL. Without ORACLE_ADMIN_URL, the
#                                  throwaway PostgreSQL is the primary, and its standby
#                                  is a second container on a network of their own
#                                  (docker/init/postgres-*.sh).
#   ORACLE_OUT                     keep the run's files (configs, logs, histories,
#                                  outcomes.json) in this directory instead of a
#                                  temporary one.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
cli="${FRAISEQL_CLI:-$repo/target/release/fraiseql-cli}"
server="${FRAISEQL_SERVER:-$repo/target/release/fraiseql-server}"
pg_image="${ORACLE_PG_IMAGE:-ghcr.io/fraiseql/pgvector:pg18}"

cd "$here"
uv sync --frozen --quiet
python="$here/.venv/bin/python"

if [[ " $* " == *" --selftest "* ]]; then
  exec "$python" -m oracle --selftest
fi

for bin in "$cli" "$server"; do
  [[ -x "$bin" ]] || { echo "✗ missing binary: $bin (build it, or set FRAISEQL_CLI / FRAISEQL_SERVER)" >&2; exit 2; }
done

if [[ -n "${ORACLE_OUT:-}" ]]; then
  mkdir -p "$ORACLE_OUT"
  work="$(cd "$ORACLE_OUT" && pwd)"
  keep=1
else
  work="$(mktemp -d)"
  keep=0
fi
container=""
pair=()
network=""
admin_url="${ORACLE_ADMIN_URL:-}"
primary_url="${ORACLE_PRIMARY_URL:-}"
replica_url="${ORACLE_REPLICA_URL:-}"
# shellcheck disable=SC2329 # invoked by the EXIT trap below
cleanup() {
  set +e
  if [[ -n "$admin_url" && -z "$container" ]]; then
    "$python" -m oracle.db drop "$admin_url" oracle_concurrency
    [[ -n "$primary_url" ]] && "$python" -m oracle.db drop "$primary_url" oracle_concurrency_replica
  fi
  [[ -n "$container" ]] && docker rm -f -v "$container" >/dev/null 2>&1
  [[ ${#pair[@]} -gt 0 ]] && docker rm -f -v "${pair[@]}" >/dev/null 2>&1
  [[ -n "$network" ]] && docker network rm "$network" >/dev/null 2>&1
  [[ $keep -eq 0 ]] && rm -rf "$work"
}
trap cleanup EXIT

labels=(--label fraiseql.oracle=concurrency)
[[ -n "${ORACLE_DOCKER_LABEL:-}" ]] && labels+=(--label "$ORACLE_DOCKER_LABEL")
published_port() { docker port "$1" 5432/tcp | head -1 | sed 's/.*://'; }

# The replica cells need a primary with a hot standby streaming from it. Unless given,
# the primary is also the fixture database's server, prepared with the repo's own
# replication setup (docker/init/), and the standby is started only when the replica
# cells run.
only=""
for ((i = 1; i <= $#; i++)); do [[ "${!i}" == "--only" ]] && { j=$((i + 1)); only="${!j:-}"; }; done
wants_replica=0
[[ -z "$only" || ",$only," == *",replica,"* ]] && wants_replica=1

if [[ -z "$admin_url" ]]; then
  network="oracle-concurrency-$$"
  docker network create "${labels[@]}" "$network" >/dev/null
  container="$(docker run -d --rm "${labels[@]}" --network "$network" --network-alias primary \
    -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 \
    -v "$repo/docker/init/postgres-replication-init.sh:/docker-entrypoint-initdb.d/02-replication.sh:ro" \
    "$pg_image")"
  admin_url="postgresql://postgres:oracle@127.0.0.1:$(published_port "$container")/postgres"
  if [[ -z "$primary_url" && $wants_replica -eq 1 ]]; then
    "$python" -m oracle.db wait "$admin_url"
    pair+=("$(docker run -d --rm "${labels[@]}" --network "$network" --user postgres \
      -e PGPASSWORD=fraiseql_repl_password -e PRIMARY_HOST=primary -e PRIMARY_USER=postgres \
      -e PRIMARY_PASSWORD=oracle -e PRIMARY_DB=postgres -e REPLICATION_SLOT=oracle_concurrency \
      -p 127.0.0.1::5432 \
      -v "$repo/docker/init/postgres-standby-entrypoint.sh:/standby-entrypoint.sh:ro" \
      "$pg_image" /bin/bash /standby-entrypoint.sh)")
    primary_url="$admin_url"
    replica_url="postgresql://postgres:oracle@127.0.0.1:$(published_port "${pair[0]}")/postgres"
  fi
fi
"$python" -m oracle.db wait "$admin_url"
if [[ $wants_replica -eq 1 && -z "$primary_url" ]]; then
  echo "✗ the replica cells need ORACLE_PRIMARY_URL and ORACLE_REPLICA_URL with ORACLE_ADMIN_URL" >&2
  exit 2
fi
if [[ -n "$primary_url" ]]; then
  "$python" -m oracle.db wait "$primary_url"
  "$python" -m oracle.db wait "$replica_url"
fi

exec_rc=0
"$python" -m oracle --cli "$cli" --server "$server" --admin-url "$admin_url" --work "$work" \
  --primary-url "$primary_url" --replica-url "$replica_url" "$@" || exec_rc=$?
if [[ $exec_rc -ne 0 && $keep -eq 0 ]]; then
  for log in "$work"/server.*.log; do
    [[ -f "$log" ]] || continue
    echo "--- $(basename "$log") (last 3000 bytes) ---" >&2
    tail -c 3000 "$log" >&2
  done
fi
exit "$exec_rc"
