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
admin_url="${ORACLE_ADMIN_URL:-}"
# shellcheck disable=SC2329 # invoked by the EXIT trap below
cleanup() {
  set +e
  if [[ -n "$admin_url" && -z "$container" ]]; then "$python" -m oracle.db drop "$admin_url" oracle_concurrency; fi
  [[ -n "$container" ]] && docker rm -f -v "$container" >/dev/null 2>&1
  [[ $keep -eq 0 ]] && rm -rf "$work"
}
trap cleanup EXIT

if [[ -z "$admin_url" ]]; then
  labels=(--label fraiseql.oracle=concurrency)
  [[ -n "${ORACLE_DOCKER_LABEL:-}" ]] && labels+=(--label "$ORACLE_DOCKER_LABEL")
  container="$(docker run -d --rm "${labels[@]}" -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 "$pg_image")"
  pg_port="$(docker port "$container" 5432/tcp | head -1 | sed 's/.*://')"
  admin_url="postgresql://postgres:oracle@127.0.0.1:$pg_port/postgres"
fi
"$python" -m oracle.db wait "$admin_url"

exec_rc=0
"$python" -m oracle --cli "$cli" --server "$server" --admin-url "$admin_url" --work "$work" "$@" || exec_rc=$?
if [[ $exec_rc -ne 0 && $keep -eq 0 ]]; then
  for log in "$work"/server.*.log; do
    [[ -f "$log" ]] || continue
    echo "--- $(basename "$log") (last 3000 bytes) ---" >&2
    tail -c 3000 "$log" >&2
  done
fi
exit "$exec_rc"
