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
#                                  plus ORACLE_DOCKER_LABEL when set) and removed on exit.
#   ORACLE_PRIMARY_URL,            superuser URLs of a primary and a hot standby streaming
#   ORACLE_REPLICA_URL             from it, for the replica cells. When unset, the pair is
#                                  started as two containers (fixture/replica/) on a
#                                  network of their own, with the same labels.
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
  if [[ -n "$admin_url" && -z "$container" ]]; then "$python" -m oracle.db drop "$admin_url" oracle_concurrency; fi
  [[ -n "$container" ]] && docker rm -f -v "$container" >/dev/null 2>&1
  [[ ${#pair[@]} -gt 0 ]] && docker rm -f -v "${pair[@]}" >/dev/null 2>&1
  [[ -n "$network" ]] && docker network rm "$network" >/dev/null 2>&1
  [[ $keep -eq 0 ]] && rm -rf "$work"
}
trap cleanup EXIT

labels=(--label fraiseql.oracle=concurrency)
[[ -n "${ORACLE_DOCKER_LABEL:-}" ]] && labels+=(--label "$ORACLE_DOCKER_LABEL")
published_port() { docker port "$1" 5432/tcp | head -1 | sed 's/.*://'; }

if [[ -z "$admin_url" ]]; then
  container="$(docker run -d --rm "${labels[@]}" -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 "$pg_image")"
  admin_url="postgresql://postgres:oracle@127.0.0.1:$(published_port "$container")/postgres"
fi
"$python" -m oracle.db wait "$admin_url"

# The replica pair, unless given or unneeded (`--only` without replica).
only=""
for ((i = 1; i <= $#; i++)); do [[ "${!i}" == "--only" ]] && { j=$((i + 1)); only="${!j:-}"; }; done
if [[ -z "$primary_url" && ( -z "$only" || ",$only," == *",replica,"* ) ]]; then
  network="oracle-concurrency-$$"
  docker network create "${labels[@]}" "$network" >/dev/null
  pair+=("$(docker run -d --rm "${labels[@]}" --network "$network" --network-alias primary \
    -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 \
    -v "$here/fixture/replica/primary-init.sh:/docker-entrypoint-initdb.d/replication.sh:ro" "$pg_image")")
  pair+=("$(docker run -d --rm "${labels[@]}" --network "$network" \
    -e POSTGRES_PASSWORD=oracle -e ORACLE_PRIMARY_HOST=primary -p 127.0.0.1::5432 \
    -v "$here/fixture/replica/standby.sh:/standby.sh:ro" --entrypoint bash "$pg_image" /standby.sh)")
  primary_url="postgresql://postgres:oracle@127.0.0.1:$(published_port "${pair[0]}")/postgres"
  replica_url="postgresql://postgres:oracle@127.0.0.1:$(published_port "${pair[1]}")/postgres"
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
