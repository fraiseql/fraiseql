#!/usr/bin/env bash
# Run the cross-transport differential oracle end to end (see README.md).
#
#   tools/oracle/transports/run.sh [oracle options, e.g. --bless or --report out.json]
#
# Environment:
#   FRAISEQL_CLI, FRAISEQL_SERVER  binaries under test. The server must be built with the
#                                  features cli,rest,grpc,mcp,arrow,observers (default:
#                                  target/release/*).
#   ORACLE_ADMIN_URL               a PostgreSQL URL with CREATE DATABASE rights. When
#                                  unset, a throwaway PostgreSQL 18 container is started
#                                  (ORACLE_PG_IMAGE, labelled fraiseql.oracle=transports
#                                  plus ORACLE_DOCKER_LABEL when set) and removed on exit.
#   ORACLE_DB_NAME                 fixture database name (default oracle_transports)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
cli="${FRAISEQL_CLI:-$repo/target/release/fraiseql-cli}"
server="${FRAISEQL_SERVER:-$repo/target/release/fraiseql-server}"
db_name="${ORACLE_DB_NAME:-oracle_transports}"
pg_image="${ORACLE_PG_IMAGE:-ghcr.io/fraiseql/pgvector:pg18}"

for bin in "$cli" "$server"; do
  [[ -x "$bin" ]] || { echo "✗ missing binary: $bin (build it, or set FRAISEQL_CLI / FRAISEQL_SERVER)" >&2; exit 2; }
done

cd "$here"
uv sync --frozen --quiet
# The venv's interpreter, not `uv run`, and never through a shell function when
# backgrounded: the pid in `pids` must be the process itself, or cleanup kills a wrapper
# and leaves the process running.
python="$here/.venv/bin/python"
py() { "$python" "$@"; }

work="$(mktemp -d)"
container=""
pids=()
admin_url="${ORACLE_ADMIN_URL:-}"
# shellcheck disable=SC2329 # invoked by the EXIT trap below
cleanup() {
  set +e
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; done
  if [[ -n "$admin_url" && -z "$container" ]]; then py -m oracle.db drop "$admin_url" "$db_name"; fi
  [[ -n "$container" ]] && docker rm -f -v "$container" >/dev/null 2>&1
  rm -rf "$work"
}
trap cleanup EXIT

free_port() { py -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'; }

if [[ -z "$admin_url" ]]; then
  labels=(--label fraiseql.oracle=transports)
  [[ -n "${ORACLE_DOCKER_LABEL:-}" ]] && labels+=(--label "$ORACLE_DOCKER_LABEL")
  container="$(docker run -d --rm "${labels[@]}" -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 "$pg_image")"
  pg_port="$(docker port "$container" 5432/tcp | head -1 | sed 's/.*://')"
  admin_url="postgresql://postgres:oracle@127.0.0.1:$pg_port/postgres"
fi
py -m oracle.db wait "$admin_url"
db_url="$(py -m oracle.db create "$admin_url" "$db_name")"
(cd "$work" && DATABASE_URL="$db_url" "$cli" setup >"$work/setup.log" 2>&1) || { cat "$work/setup.log" >&2; exit 2; }
py -m oracle.db load "$db_url" fixture/data.sql

# The issuer writes a key generated for this run into $work; nothing is committed.
issuer_port="$(free_port)"
"$python" -m oracle.issuer serve --key "$work/issuer.pem" --port "$issuer_port" >"$work/issuer.log" 2>&1 &
pids+=($!)
issuer="http://127.0.0.1:$issuer_port"
for _ in $(seq 100); do [[ -s "$work/issuer.pem" ]] && curl -sf -o /dev/null "$issuer/jwks.json" && break; sleep 0.1; done

"$cli" compile fixture/fraiseql.toml --types fixture/schema.json -o "$work/schema.compiled.json" >"$work/compile.log" 2>&1 \
  || { cat "$work/compile.log" >&2; exit 2; }
http_port="$(free_port)"
flight_port="$(free_port)"
sed -e "s#@HTTP_PORT@#$http_port#" -e "s#@FLIGHT_PORT@#$flight_port#" -e "s#@SCHEMA@#$work/schema.compiled.json#" \
  -e "s#@DATABASE_URL@#$db_url#" -e "s#@ISSUER@#$issuer#g" fixture/server.toml.in >"$work/server.toml"
FRAISEQL_ENV=development FLIGHT_SESSION_SECRET="$(py -c 'import secrets; print(secrets.token_hex(32))')" \
  "$server" --config "$work/server.toml" >"$work/server.log" 2>&1 &
pids+=($!)
for _ in $(seq 600); do
  curl -sf -o /dev/null "http://127.0.0.1:$http_port/health" && break
  kill -0 "${pids[1]}" 2>/dev/null || { echo "✗ fraiseql-server exited during boot:" >&2; tail -c 4000 "$work/server.log" >&2; exit 2; }
  sleep 0.1
done

rc=0
py -m oracle --http "http://127.0.0.1:$http_port" --flight "grpc://127.0.0.1:$flight_port" \
  --issuer "$issuer" --key "$work/issuer.pem" --db "$db_url" "$@" || rc=$?
if [[ $rc -eq 2 ]]; then
  echo "--- fraiseql-server log (last 6000 bytes) ---" >&2
  tail -c 6000 "$work/server.log" >&2
fi
exit "$rc"
