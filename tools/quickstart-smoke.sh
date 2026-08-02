#!/usr/bin/env bash
# quickstart-smoke.sh — execute docs/guides/getting-started.md VERBATIM (#734).
#
# The doc is the fixture: every fenced code block is extracted in order and
# executed against a real PostgreSQL. If the doc drifts from reality — a
# phantom API, a wrong flag, a wrong port — this script fails, which is the
# point. Do not paraphrase doc content here; fix the doc instead.
#
# Three environment substitutions are permitted, and each first ASSERTS the
# original doc text, so any other change to those blocks still fails:
#   1. `cargo install fraiseql-cli fraiseql-server` → prebuilt binaries on PATH
#   2. `pip install fraiseql`                       → the in-repo SDK (PYTHONPATH)
#   3. the doc's example DATABASE_URL               → $SMOKE_DATABASE_URL
#
# Requirements: bash, python3, psql, curl; fraiseql-cli and fraiseql-server on
# PATH; SMOKE_DATABASE_URL pointing at a PostgreSQL this run may write to (a
# scratch database `quickstart_smoke` is created and dropped there).
#
# A second phase scaffolds a project with `fraiseql init` and runs its Python
# authoring skeleton against the in-repo SDK — the regeneration path the cargo
# test suite cannot cover (it needs Python).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOC="$REPO_ROOT/docs/guides/getting-started.md"
SDK_SRC="$REPO_ROOT/sdks/official/fraiseql-python/src"
WORKDIR="$(mktemp -d)"
BLOCKS="$WORKDIR/blocks"
SERVER_PID=""

: "${SMOKE_DATABASE_URL:?SMOKE_DATABASE_URL must point at a writable PostgreSQL}"

cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
    psql "$SMOKE_DATABASE_URL" -q -c "DROP DATABASE IF EXISTS quickstart_smoke WITH (FORCE)" 2>/dev/null || true
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() { echo "quickstart-smoke: FAIL — $*" >&2; exit 1; }

# ── extract every fenced code block, in order ────────────────────────────────
mkdir -p "$BLOCKS"
python3 - "$DOC" "$BLOCKS" <<'PY'
import sys, re, pathlib
doc, outdir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
blocks = re.findall(r"```(\w+)\n(.*?)```", doc.read_text(), re.DOTALL)
for i, (lang, body) in enumerate(blocks):
    (outdir / f"{i:02}.{lang}").write_text(body)
print(f"extracted {len(blocks)} blocks")
PY

block() { cat "$BLOCKS/$1"; }
assert_block_contains() {
    grep -qF "$2" "$BLOCKS/$1" || fail "doc block $1 no longer contains '$2' — update this script's step map to match the doc"
}

# Scratch database, so the doc's CREATE TABLE/VIEW statements meet an empty
# database exactly as a new user's would.
psql "$SMOKE_DATABASE_URL" -q -v ON_ERROR_STOP=1 \
    -c "DROP DATABASE IF EXISTS quickstart_smoke WITH (FORCE)" \
    -c "CREATE DATABASE quickstart_smoke"
SCRATCH_URL="$(python3 - "$SMOKE_DATABASE_URL" <<'PY'
import sys
url = sys.argv[1]
base, _, _db = url.rpartition("/")
print(f"{base}/quickstart_smoke")
PY
)"

cd "$WORKDIR"

# `python` shim: docs say `python`, minimal containers ship only `python3`.
mkdir -p "$WORKDIR/bin"
ln -sf "$(command -v python3)" "$WORKDIR/bin/python"
export PATH="$WORKDIR/bin:$PATH"

# ── step map: block index → action ───────────────────────────────────────────
# 00 bash   cargo install            (substitution 1: prebuilt binaries)
# 01 bash   pip install fraiseql     (substitution 2: in-repo SDK)
# 02 python schema.py                (written to disk, doc names the file)
# 03 bash   python schema.py
# 04 sql    setup.sql                (written to disk, doc names the file)
# 05 bash   export DATABASE_URL + psql  (substitution 3: scratch URL)
# 06 bash   fraiseql-cli compile
# 07 bash   fraiseql-server          (backgrounded; readiness-waited)
# 08 bash   curl the endpoint
# 09 json   expected response

echo "## step 1 — install (substituted: prebuilt binaries)"
assert_block_contains 00.bash "cargo install fraiseql-cli fraiseql-server"
command -v fraiseql-cli >/dev/null || fail "fraiseql-cli not on PATH"
command -v fraiseql-server >/dev/null || fail "fraiseql-server not on PATH"

echo "## step 2 — SDK (substituted: in-repo SDK via PYTHONPATH)"
assert_block_contains 01.bash "pip install fraiseql"
export PYTHONPATH="$SDK_SRC"

echo "## step 2b — author schema.py and export"
cp "$BLOCKS/02.python" schema.py
# shellcheck disable=SC1090
source "$BLOCKS/03.bash"
[ -f schema.json ] || fail "python schema.py did not produce schema.json"

echo "## step 3 — database objects"
cp "$BLOCKS/04.sql" setup.sql
assert_block_contains 05.bash 'export DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres"'
sed "s|postgres://postgres:postgres@localhost:5432/postgres|$SCRATCH_URL|" "$BLOCKS/05.bash" > 05.substituted.bash
# shellcheck disable=SC1090
source 05.substituted.bash

echo "## step 4 — compile"
# shellcheck disable=SC1090
source "$BLOCKS/06.bash"
[ -f schema.compiled.json ] || fail "compile did not produce schema.compiled.json"

echo "## step 5 — serve (backgrounded for the smoke)"
assert_block_contains 07.bash "fraiseql-server --schema-path schema.compiled.json"
# The doc serves on the default 127.0.0.1:8000; a foreign listener there would
# make every later assertion meaningless.
curl -s -o /dev/null --max-time 2 "http://localhost:8000/" 2>/dev/null &&
    fail "port 8000 is already in use — the quickstart serves on the default bind address"
bash "$BLOCKS/07.bash" > server.log 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 30); do
    curl -s -o /dev/null "http://localhost:8000/graphql" && break
    kill -0 "$SERVER_PID" 2>/dev/null || { cat server.log >&2; fail "fraiseql-server exited during boot"; }
    sleep 1
done

echo "## step 6 — query"
RESPONSE="$(bash "$BLOCKS/08.bash")"
echo "$RESPONSE"
assert_block_contains 09.json '"name":"Ada"'
echo "$RESPONSE" | grep -qF '"name":"Ada"' || fail "response does not match the doc's expected output: $RESPONSE"
echo "$RESPONSE" | grep -qF '"email":"ada@example.com"' || fail "response does not match the doc's expected output: $RESPONSE"

kill "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

# ── phase 2: the init scaffold's Python skeleton regenerates its schema ──────
echo "## phase 2 — fraiseql init skeleton runs against the SDK"
fraiseql-cli init smoke_blog --no-git >/dev/null
cd smoke_blog
python schema/schema.py
python3 - <<'PY'
import json
s = json.load(open("schema.json"))
assert len(s["queries"]) == 5, f"skeleton must regenerate the 5 scaffold queries, got {len(s['queries'])}"
assert len(s["mutations"]) == 9, f"skeleton must regenerate the 9 scaffold mutations, got {len(s['mutations'])}"
PY
fraiseql-cli compile schema.json -o schema.compiled.json >/dev/null
echo "quickstart-smoke: OK"
