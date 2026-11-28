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
pids=()
admin_url="${ORACLE_ADMIN_URL:-}"
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; done
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

# Four servers over one database: the fixture; the fixture with a low complexity
# ceiling; the fixture with trusted documents in strict mode; and the fixture serving
# the subscription in fixture/subscriptions.json. lib/conformance.js probes the last
# three. The subscription stays off the main server, so the differential compares a
# schema the reference SDL describes in full.
sed 's/^max_query_complexity = .*/max_query_complexity = 5/' fixture/fraiseql.toml >"$work/limits.toml"
{ cat fixture/fraiseql.toml; printf '\n[security.trusted_documents]\nenabled = true\nmode = "strict"\nmanifest_path = "%s"\n' "$here/fixture/trusted-documents.json"; } >"$work/trusted.toml"
node -e 'const fs=require("fs");const s=JSON.parse(fs.readFileSync("fixture/schema.json"));s.subscriptions=JSON.parse(fs.readFileSync("fixture/subscriptions.json"));fs.writeFileSync(process.argv[1],JSON.stringify(s))' "$work/subs.schema.json"

started_url=""
start_server() { # name toml [types]: starts it, sets started_url and appends the pid to pids
  local name=$1 toml=$2 types=${3:-fixture/schema.json} port
  "$cli" compile "$toml" --types "$types" -o "$work/$name.compiled.json" >"$work/$name.compile.log" 2>&1 \
    || { cat "$work/$name.compile.log" >&2; return 2; }
  port="$(free_port)"
  FRAISEQL_INTROSPECTION_ENABLED=true FRAISEQL_INTROSPECTION_REQUIRE_AUTH=false FRAISEQL_ENV=development \
    DATABASE_URL="$db_url" FRAISEQL_SCHEMA_PATH="$work/$name.compiled.json" FRAISEQL_BIND_ADDR="127.0.0.1:$port" \
    "$server" >"$work/$name.log" 2>&1 &
  pids+=($!)
  started_url="http://127.0.0.1:$port/graphql"
}
wait_server() { # name url pid: any HTTP answer means up (strict mode answers a raw query with 400)
  local name=$1 url=$2 pid=$3 code
  for _ in $(seq 600); do
    code="$(curl -s -o /dev/null -w '%{http_code}' -XPOST "$url" -H 'content-type: application/json' -d '{"query":"{__typename}"}' || true)"
    [[ "$code" != "000" ]] && return 0
    kill -0 "$pid" 2>/dev/null || { echo "✗ fraiseql-server ($name) exited during boot:" >&2; tail -c 4000 "$work/$name.log" >&2; return 2; }
    sleep 0.1
  done
  echo "✗ fraiseql-server ($name) did not answer within 60s" >&2; tail -c 4000 "$work/$name.log" >&2; return 2
}

start_server main fixture/fraiseql.toml; main_url=$started_url; wait_server main "$main_url" "${pids[0]}"
start_server limits "$work/limits.toml"; limits_url=$started_url; wait_server limits "$limits_url" "${pids[1]}"
start_server trusted "$work/trusted.toml"; trusted_url=$started_url; wait_server trusted "$trusted_url" "${pids[2]}"
start_server subs fixture/fraiseql.toml "$work/subs.schema.json"; subs_url=$started_url; wait_server subs "$subs_url" "${pids[3]}"

node oracle.js --server "$main_url" --limits-server "$limits_url" --trusted-server "$trusted_url" \
  --subscription-server "$subs_url" --database "$db_url" "$@"
