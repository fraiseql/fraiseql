#!/usr/bin/env bash
# Red-capability pin for tools/check-fixture-relation-collisions.py (#1281).
#
# Run directly:  bash tools/tests/fixture_relation_collisions_test.sh
# Exits non-zero if any assertion fails.
#
# The gate must catch the shape it was written for — two fixture files declaring
# one `public` relation — and must NOT flag the shapes this repository uses on
# purpose, because a false positive here is paid for by renaming a fixture that
# was fine.
#
# No Rust toolchain, no cargo, no database: the gate reads .sql as text.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-fixture-relation-collisions.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# tree <dir> — a fixture tree with the two directories the gate scans.
tree() {
    mkdir -p "$1/tests/sql/postgres" "$1/docker/e2e"
}

# expect <label> <wanted-exit> <dir> [<substring>]
expect() {
    local label="$1" want="$2" dir="$3" needle="${4:-}" rc=0 out
    TESTS_RUN=$((TESTS_RUN + 1))
    set +e
    out="$(FIXTURE_COLLISION_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL  $label: exit $rc, wanted $want"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1)); return
    fi
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        echo "FAIL  $label: output did not mention '$needle'"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1)); return
    fi
    echo "PASS  $label"
}

echo "fixture relation collisions"
echo

# ── the shape the gate exists for ────────────────────────────────────────────
D="$WORK/collide"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id UUID PRIMARY KEY, data JSONB NOT NULL);
CREATE OR REPLACE VIEW v_user AS SELECT id, data FROM tb_user;
SQL
cat >"$D/docker/e2e/init-postgres.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id SERIAL PRIMARY KEY, name TEXT NOT NULL);
SQL
expect "two files declaring one relation is a collision" 1 "$D" "public.tb_user"

# ── the fix, in both of the forms the gate suggests ──────────────────────────
D="$WORK/prefix"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id UUID PRIMARY KEY, data JSONB NOT NULL);
SQL
cat >"$D/docker/e2e/init-postgres.sql" <<'SQL'
CREATE TABLE tb_e2e_user (id SERIAL PRIMARY KEY, name TEXT NOT NULL);
SQL
expect "a prefix of its own clears it" 0 "$D"

D="$WORK/schema"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id UUID PRIMARY KEY, data JSONB NOT NULL);
SQL
cat >"$D/docker/e2e/init-postgres.sql" <<'SQL'
CREATE SCHEMA IF NOT EXISTS e2e;
CREATE TABLE e2e.tb_user (id SERIAL PRIMARY KEY, name TEXT NOT NULL);
SQL
expect "a schema of its own clears it too" 0 "$D"

# ── shapes that must NOT be flagged ──────────────────────────────────────────
D="$WORK/within"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id UUID PRIMARY KEY, data JSONB NOT NULL);
CREATE OR REPLACE VIEW v_user AS SELECT id, data FROM tb_user;
CREATE OR REPLACE VIEW v_user AS SELECT id, data FROM tb_user;
SQL
expect "redeclaring within ONE file is that file's own business" 0 "$D"

D="$WORK/qualified"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS public.tb_user (id UUID PRIMARY KEY, data JSONB NOT NULL);
SQL
cat >"$D/docker/e2e/init-postgres.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS tb_user (id SERIAL PRIMARY KEY, name TEXT NOT NULL);
SQL
expect "an explicit \`public.\` qualifier is the same relation" 1 "$D" "public.tb_user"

D="$WORK/matview"; tree "$D"
cat >"$D/tests/sql/postgres/init.sql" <<'SQL'
CREATE MATERIALIZED VIEW mv_stats AS SELECT 1 AS n;
SQL
cat >"$D/docker/e2e/init-postgres.sql" <<'SQL'
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_stats AS SELECT 2 AS n;
SQL
expect "a materialized view collides like any other relation" 1 "$D" "public.mv_stats"

# ── the blind-gate guard ─────────────────────────────────────────────────────
#
# A gate that scans nothing reports OK, which is indistinguishable from a clean
# tree. Every gate this repository has lost, it lost this way.
D="$WORK/empty"; mkdir -p "$D"
expect "a tree with no fixture directories FAILS rather than passing vacuously" 1 "$D" \
    "scanned zero .sql files"

# ── the repository as committed ──────────────────────────────────────────────
expect "the tree as committed has one owner per relation" 0 "$REPO_ROOT"

echo
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "fixture relation collision self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "fixture relation collision self-test: $TESTS_RUN/$TESTS_RUN passed"
