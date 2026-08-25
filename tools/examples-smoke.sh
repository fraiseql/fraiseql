#!/usr/bin/env bash
# examples-smoke.sh — every schema example must load, compile, resolve its own
# queries, and answer over HTTP.
#
# This is the third and last tier of the examples gate, and the only one that runs
# anything. The other two are static:
#
#   tools/check-examples-integrity.sh   files exist, paths resolve, greps are anchored
#   tools/check-examples-compile.sh     every authoring artifact compiles
#
# Phase 09 of the 2026-08-22 program is what this exists for. Compiling a committed
# artifact is not testing an example, and a healthy container is not a working one:
# `async-jobs-subgraph` had three defects stacked behind each other, each invisible
# until the one before it was fixed, and the last of them booted healthy and answered
# ordinary queries while refusing the one query that makes it a subgraph. The only
# check that would have caught it is asking the thing the question a real client asks.
#
# What it does, per example directory carrying `sql/setup.sql` and a schema:
#
#   1. create a fresh database and load sql/setup.sql under ON_ERROR_STOP=1
#      (without it psql prints an error, keeps going, and exits 0 half-loaded — #1051,
#      #1072)
#   2. run schema.py, compile the result
#   3. run every queries/*.graphql through `fraiseql query`, which boots the compiled
#      schema in-process and exits non-zero if the operation does not resolve
#   4. boot `fraiseql-server` against it and POST one query to /graphql
#
# Step 4 runs for one example rather than all: it is the "does the binary serve this"
# check, and the schema it serves is not what makes it interesting. Steps 1-3 are what
# scale per example.
#
# ⚠ This gate never skips. A missing binary, a missing psql, an absent DATABASE_URL:
# each is a failure, not a pass. A check that quietly does nothing reads as green,
# which is the whole reason this phase exists.
#
# Requires: bash, psql, curl, a fraiseql CLI binary, a fraiseql-server binary, and a
# PostgreSQL the caller may CREATE DATABASE on.
#
#   make examples-smoke            # builds what it needs, uses the local rig
#   FRAISEQL_BIN=… SERVER_BIN=… DATABASE_URL=… tools/examples-smoke.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

fail_count=0
note() { printf '  %s\n' "$*"; }
bad() { printf '  ✗ %s\n' "$*"; fail_count=$((fail_count + 1)); }

# ---------------------------------------------------------------------------
# Preconditions. Each is a failure, never a skip.
# ---------------------------------------------------------------------------
FRAISEQL_BIN="${FRAISEQL_BIN:-}"
if [ -z "$FRAISEQL_BIN" ]; then
    for candidate in target/release/fraiseql-cli target/debug/fraiseql-cli \
                     target/release/fraiseql target/debug/fraiseql; do
        if [ -x "$candidate" ]; then FRAISEQL_BIN="$REPO_ROOT/$candidate"; break; fi
    done
fi
SERVER_BIN="${SERVER_BIN:-}"
if [ -z "$SERVER_BIN" ]; then
    for candidate in target/release/fraiseql-server target/debug/fraiseql-server; do
        if [ -x "$candidate" ]; then SERVER_BIN="$REPO_ROOT/$candidate"; break; fi
    done
fi

for tool in psql curl python3; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "✗ $tool is not on PATH. This gate does not skip."
        exit 1
    }
done
[ -n "$FRAISEQL_BIN" ] && [ -x "$FRAISEQL_BIN" ] || {
    echo "✗ no fraiseql CLI binary. Build one (cargo build -p fraiseql-cli) or set FRAISEQL_BIN."
    exit 1
}
[ -n "$SERVER_BIN" ] && [ -x "$SERVER_BIN" ] || {
    echo "✗ no fraiseql-server binary. Build one (cargo build -p fraiseql-server --features cli)"
    echo "  or set SERVER_BIN."
    exit 1
}
[ -n "${DATABASE_URL:-}" ] || {
    echo "✗ DATABASE_URL is not set. It must name a PostgreSQL this may CREATE DATABASE on."
    exit 1
}

# `import fraiseql` must work, because every example authors through it.
SDK_SRC="$REPO_ROOT/sdks/official/fraiseql-python/src"
PYTHONPATH="$SDK_SRC" python3 -c 'import fraiseql' 2>/dev/null || {
    echo "✗ cannot import the in-repo authoring SDK. Provide python3 and its deps (httpx)."
    exit 1
}

WORKDIR="$(mktemp -d)"
CREATED_DBS=()
cleanup() {
    for db in ${CREATED_DBS+"${CREATED_DBS[@]}"}; do
        psql "$DATABASE_URL" -q -c "DROP DATABASE IF EXISTS \"$db\"" >/dev/null 2>&1 || true
    done
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

# Swap the database name in a libpq URL, keeping everything else.
db_url_for() {
    DB_NAME="$1" python3 - "$DATABASE_URL" <<'PY'
import os, sys
from urllib.parse import urlsplit, urlunsplit
u = urlsplit(sys.argv[1])
print(urlunsplit((u.scheme, u.netloc, "/" + os.environ["DB_NAME"], u.query, u.fragment)))
PY
}

# ---------------------------------------------------------------------------
# The examples this covers.
#
# `find`, not a hand-kept list, so a new example directory carrying a setup.sql
# and a schema is picked up rather than silently uncovered — the shape that let
# examples/ go unchecked for its whole life.
# ---------------------------------------------------------------------------
mapfile -t EXAMPLES < <(
    find examples -mindepth 2 -maxdepth 3 -path '*/sql/setup.sql' -print0 \
        | xargs -0 -n1 dirname | xargs -n1 dirname | sort -u
)
[ "${#EXAMPLES[@]}" -gt 0 ] || {
    echo "✗ found no example with sql/setup.sql at all — the search is wrong, not the tree."
    exit 1
}

echo "→ smoke-testing ${#EXAMPLES[@]} example(s) against $(psql "$DATABASE_URL" -tAc 'SHOW server_version' 2>/dev/null || echo PostgreSQL)"

smoke_one() {
    local dir="$1" name db url work
    name="$(basename "$dir")"
    db="fraiseql_smoke_${name//[^a-zA-Z0-9]/_}"
    echo
    echo "── $dir"

    # A pristine copy: authoring writes next to itself, and a gate that dirties the
    # tree it checks is one a developer learns to skip.
    work="$WORKDIR/$name"
    rm -rf "$work"; cp -r "$dir" "$work"

    # 1. Load the SQL.
    psql "$DATABASE_URL" -q -c "DROP DATABASE IF EXISTS \"$db\"" >/dev/null 2>&1 || true
    if ! psql "$DATABASE_URL" -q -c "CREATE DATABASE \"$db\"" >/dev/null 2>&1; then
        bad "cannot CREATE DATABASE $db"
        return
    fi
    CREATED_DBS+=("$db")
    url="$(db_url_for "$db")"
    if ! out=$(psql -v ON_ERROR_STOP=1 -q "$url" -f "$work/sql/setup.sql" 2>&1); then
        bad "sql/setup.sql does not load under ON_ERROR_STOP=1"
        printf '%s\n' "$out" | tail -5 | sed 's/^/      /'
        return
    fi
    note "ok   sql/setup.sql loads clean"

    # 2. Author and compile.
    if [ -f "$work/schema.py" ]; then
        if ! out=$(cd "$work" && PYTHONPATH="$SDK_SRC" python3 schema.py 2>&1); then
            bad "schema.py does not run"
            printf '%s\n' "$out" | tail -5 | sed 's/^/      /'
            return
        fi
    fi
    local input=() types=()
    if [ -f "$work/fraiseql.toml" ]; then
        input=(fraiseql.toml)
        [ -f "$work/types.json" ] && types=(--types types.json)
    elif [ -f "$work/schema.json" ]; then
        input=(schema.json)
    else
        bad "no fraiseql.toml and no schema.json to compile"
        return
    fi
    # From the example's own directory: [domain_discovery] resolves its root against
    # the CWD, so compiling from the repo root can discover a different domain.
    if ! out=$(cd "$work" && "$FRAISEQL_BIN" compile "${input[@]}" "${types[@]}" \
                   -o schema.compiled.json 2>&1); then
        bad "${input[*]} does not compile"
        printf '%s\n' "$out" | tail -10 | sed 's/^/      /'
        return
    fi
    note "ok   ${input[*]} compiles"

    # 3. Every shipped query must resolve.
    local ran=0
    if [ -d "$work/queries" ]; then
        local query vars variables
        while IFS= read -r query; do
            [ -n "$query" ] || continue
            vars="$query.vars.json"
            variables=()
            if [ -f "$vars" ]; then
                variables=(--variables "$(cat "$vars")")
            elif grep -qE '^[[:space:]]*(query|mutation)[[:space:]][^(]*\(' "$query"; then
                # A shipped operation nobody can run is the defect this gate is for.
                bad "$(basename "$query") declares variables but has no $(basename "$vars")"
                continue
            fi
            if out=$(cd "$work" && "$FRAISEQL_BIN" query --schema schema.compiled.json \
                         --database "$url" "${variables[@]}" "$(cat "$query")" 2>&1); then
                ran=$((ran + 1))
            else
                bad "$(basename "$query") does not resolve"
                printf '%s\n' "$out" | tail -6 | sed 's/^/      /'
            fi
        done < <(find "$work/queries" -name '*.graphql' | sort)
    fi
    if [ "$ran" -gt 0 ]; then
        note "ok   $ran queries/*.graphql resolve against the database"
    else
        bad "no query resolved — an example that answers nothing is not smoke-tested"
    fi

    # Remember one working pair for the HTTP step.
    if [ -z "${SERVE_DIR:-}" ]; then
        SERVE_DIR="$work"
        SERVE_URL="$url"
    fi
}

for dir in "${EXAMPLES[@]}"; do
    smoke_one "$dir"
done

# ---------------------------------------------------------------------------
# 4. It boots, and it answers.
#
# `fraiseql query` proves the schema resolves; it does not prove the binary that
# actually ships can serve it. Booting the server and POSTing one query is the half
# that catches "compiles fine, panics at startup" and "healthy, but refuses the
# query" — see the release-smoke workflow for the same argument one layer up.
# ---------------------------------------------------------------------------
echo
echo "── fraiseql-server boots and answers"
if [ -z "${SERVE_DIR:-}" ]; then
    bad "no example got far enough to serve; skipping the HTTP check would hide that"
else
    port="${SMOKE_PORT:-18123}"
    log="$WORKDIR/server.log"
    DATABASE_URL="$SERVE_URL" \
    FRAISEQL_SCHEMA_PATH="$SERVE_DIR/schema.compiled.json" \
    FRAISEQL_BIND_ADDR="127.0.0.1:$port" \
    FRAISEQL_ENV=development \
        "$SERVER_BIN" >"$log" 2>&1 &
    server_pid=$!

    ready=0
    for _ in $(seq 1 60); do
        if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then ready=1; break; fi
        kill -0 "$server_pid" 2>/dev/null || break
        sleep 0.5
    done

    if [ "$ready" -ne 1 ]; then
        bad "the server did not become healthy in 30s"
        tail -15 "$log" | sed 's/^/      /'
    else
        note "ok   /health responds"
        # Health is not the question a client asks. Ask a real one.
        root="$(python3 - "$SERVE_DIR/schema.compiled.json" <<'PY'
import json, sys
schema = json.load(open(sys.argv[1]))
for q in schema.get("queries", []):
    if q.get("returns_list"):
        print(q["name"]); break
PY
)"
        if [ -z "$root" ]; then
            bad "no list query in the compiled schema to ask for"
        else
            body="$(printf '{"query": "{ %s { __typename } }"}' "$root")"
            if response=$(curl -fsS -X POST "http://127.0.0.1:$port/graphql" \
                              -H 'Content-Type: application/json' -d "$body" 2>&1); then
                # A GraphQL response carries resolution errors in-band, so a 200 is
                # not an answer. #1071's container was healthy and answered
                # "Federation is not enabled in this build" to the one query that
                # mattered.
                if printf '%s' "$response" | grep -q '"errors"'; then
                    bad "the server answered { $root } with errors: $response"
                else
                    note "ok   { $root { __typename } } answered: $(printf '%s' "$response" | head -c 120)"
                fi
            else
                bad "POST /graphql failed: $response"
                tail -15 "$log" | sed 's/^/      /'
            fi
        fi
    fi

    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
fi

echo
if [ "$fail_count" -ne 0 ]; then
    echo "examples smoke FAILED — $fail_count check(s) did not pass."
    exit 1
fi
echo "OK: every schema example loads, compiles, resolves its queries, and the server answers."
