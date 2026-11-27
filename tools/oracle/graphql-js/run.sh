#!/usr/bin/env bash
# Run the graphql-js differential oracle end to end (see README.md).
#
#   tools/oracle/graphql-js/run.sh [oracle.js options, e.g. --bless or --report out.json]
#
# Environment:
#   FRAISEQL_CLI, FRAISEQL_SERVER  binaries under test (default: target/release/*)
#   ORACLE_ADMIN_URL               a PostgreSQL URL with CREATE DATABASE rights. When
#                                  unset, a throwaway PostgreSQL 18 container is started
#                                  (ORACLE_PG_IMAGE, labelled fraiseql.oracle=graphql-js
#                                  plus ORACLE_DOCKER_LABEL when set) and removed on exit.
#   ORACLE_DB_NAME                 fixture database name (default oracle_graphql_js)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
cli="${FRAISEQL_CLI:-$repo/target/release/fraiseql-cli}"
server="${FRAISEQL_SERVER:-$repo/target/release/fraiseql-server}"
db_name="${ORACLE_DB_NAME:-oracle_graphql_js}"
pg_image="${ORACLE_PG_IMAGE:-ghcr.io/fraiseql/pgvector:pg18}"

for bin in "$cli" "$server"; do
  [[ -x "$bin" ]] || { echo "✗ missing binary: $bin (build it, or set FRAISEQL_CLI / FRAISEQL_SERVER)" >&2; exit 2; }
done

cd "$here"
if [[ ! -d node_modules/graphql ]]; then
  echo "→ installing oracle dependencies (npm ci)"
  npm ci --no-audit --no-fund --ignore-scripts >/dev/null
fi

work="$(mktemp -d)"
container=""
server_pid=""
admin_url="${ORACLE_ADMIN_URL:-}"
cleanup() {
  [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null && wait "$server_pid" 2>/dev/null
  if [[ -n "$admin_url" && -z "$container" ]]; then node lib/db.js drop "$admin_url" "$db_name" || true; fi
  [[ -n "$container" ]] && docker rm -f -v "$container" >/dev/null 2>&1
  rm -rf "$work"
}
trap cleanup EXIT

free_port() { node -e 'const s=require("net").createServer().listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})'; }

if [[ -z "$admin_url" ]]; then
  labels=(--label fraiseql.oracle=graphql-js)
  [[ -n "${ORACLE_DOCKER_LABEL:-}" ]] && labels+=(--label "$ORACLE_DOCKER_LABEL")
  container="$(docker run -d --rm "${labels[@]}" -e POSTGRES_PASSWORD=oracle -p 127.0.0.1::5432 "$pg_image")"
  pg_port="$(docker port "$container" 5432/tcp | head -1 | sed 's/.*://')"
  admin_url="postgresql://postgres:oracle@127.0.0.1:$pg_port/postgres"
fi
node lib/db.js wait "$admin_url"
db_url="$(node lib/db.js create "$admin_url" "$db_name")"

"$cli" compile fixture/fraiseql.toml --types fixture/schema.json -o "$work/schema.compiled.json" >"$work/compile.log" 2>&1 \
  || { cat "$work/compile.log" >&2; exit 2; }

port="$(free_port)"
FRAISEQL_INTROSPECTION_ENABLED=true FRAISEQL_INTROSPECTION_REQUIRE_AUTH=false FRAISEQL_ENV=development \
  DATABASE_URL="$db_url" FRAISEQL_SCHEMA_PATH="$work/schema.compiled.json" FRAISEQL_BIND_ADDR="127.0.0.1:$port" \
  "$server" >"$work/server.log" 2>&1 &
server_pid=$!
url="http://127.0.0.1:$port/graphql"
for _ in $(seq 600); do
  curl -sf -o /dev/null -XPOST "$url" -H 'content-type: application/json' -d '{"query":"{__typename}"}' && break
  kill -0 "$server_pid" 2>/dev/null || { echo "✗ fraiseql-server exited during boot:" >&2; tail -c 4000 "$work/server.log" >&2; exit 2; }
  sleep 0.1
done
curl -sf -o /dev/null -XPOST "$url" -H 'content-type: application/json' -d '{"query":"{__typename}"}' \
  || { echo "✗ fraiseql-server did not answer within 60s" >&2; tail -c 4000 "$work/server.log" >&2; exit 2; }

node oracle.js --server "$url" --database "$db_url" "$@"
