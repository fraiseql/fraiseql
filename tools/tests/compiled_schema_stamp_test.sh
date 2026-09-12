#!/usr/bin/env bash
# Unit tests for tools/check-compiled-schema-stamp.sh.
#
# Run directly:  bash tools/tests/compiled_schema_stamp_test.sh
# Exits non-zero if any assertion fails.
#
# The gate couples the compiled schemas CI boots to the version this tree builds
# (#1304). Both directions of error are silent in the same way the defect was:
#
#   – too lax, and a stale stamp ships. `release-smoke.yml` is `release/*`- and
#     `v*`-triggered, so the first witness to a stale artifact is the tag, where
#     the server refuses to boot;
#   – vacuous, and it is worse than absent. The gate discovers its own subjects,
#     so an extraction that matches nothing reports success over a tree it never
#     looked at — which is why "found none" and "none of them is a file" are exit
#     2 (cannot run), distinct from exit 1 (a finding).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-compiled-schema-stamp.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# new_tree <dir> <version> — a minimal repo shape the gate accepts as a root.
new_tree() {
    local dir="$1" version="$2"
    mkdir -p "$dir/.github/workflows" "$dir/docker/e2e"
    printf '[workspace.package]\nversion = "%s"\n' "$version" >"$dir/Cargo.toml"
}

# artifact <dir> <path> [stamp] — a compiled schema, stamped or not.
artifact() {
    local dir="$1" path="$2" stamp="${3:-}"
    mkdir -p "$(dirname "$dir/$path")"
    if [ -n "$stamp" ]; then
        printf '{\n  "fraiseql_version": "%s",\n  "types": []\n}\n' "$stamp" >"$dir/$path"
    else
        printf '{\n  "types": []\n}\n' >"$dir/$path"
    fi
}

# boots <dir> <path...> — a workflow that boots each path.
boots() {
    local dir="$1"; shift
    {
        printf 'jobs:\n  smoke:\n    steps:\n'
        for p in "$@"; do
            printf '      - env:\n          FRAISEQL_SCHEMA_PATH: %s\n' "$p"
        done
    } >"$dir/.github/workflows/smoke.yml"
}

# mounts_file <dir> <path> — a .dagger Go source naming the path as a FILE literal, the
# shape `source.File("docker/e2e/schema.compiled.json")` takes.
mounts_file() {
    local dir="$1" path="$2"
    mkdir -p "$dir/.dagger"
    printf 'package main\n\nfunc svc() { schema := source.File("%s") }\n' "$path" \
        >>"$dir/.dagger/main.go"
}

# mounts_dir <dir> <dirpath> — the shape the federation call site actually uses:
# `source.File("crates/.../federation/" + schemaFile)`. Only the DIRECTORY survives as a
# literal, so a file-only sweep sees nothing here — which is how the two federation
# fixtures went unstamped past a green gate.
mounts_dir() {
    local dir="$1" dirpath="$2"
    mkdir -p "$dir/.dagger"
    printf 'package main\n\nfunc svc() { schema := source.File("%s" + schemaFile) }\n' "$dirpath" \
        >>"$dir/.dagger/main.go"
}

# assert_gate <name> <expected-exit> <dir>
assert_gate() {
    local name="$1" want_exit="$2" dir="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out rc
    set +e
    out="$(COMPILED_STAMP_ROOT="$dir" bash "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "  ok: $name"
    fi
}

echo "=== check-compiled-schema-stamp.sh ==="

# ── The defect itself: an artifact from a build that is not this one ─────────────
d="$WORK/stale"; new_tree "$d" 2.16.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
assert_gate "a-stale-stamp-is-a-finding" 1 "$d"

# The pre-2.15.0 shape: no stamp at all. The server refuses it, so the gate must too.
d="$WORK/unstamped"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json
boots "$d" docker/e2e/schema.compiled.json
assert_gate "an-unstamped-artifact-is-a-finding" 1 "$d"

d="$WORK/current"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
assert_gate "the-matching-stamp-passes" 0 "$d"

# ── It must not stop at the first subject it likes ───────────────────────────────
d="$WORK/second-stale"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
artifact "$d" docker/e2e/schema.with-source.compiled.json 2.14.0
boots "$d" docker/e2e/schema.compiled.json docker/e2e/schema.with-source.compiled.json
assert_gate "a-stale-second-subject-is-still-found" 1 "$d"

# ── Vacuity: a gate that checked nothing must not report success ─────────────────
d="$WORK/no-refs"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
printf 'jobs:\n  build:\n    steps:\n      - run: echo hi\n' >"$d/.github/workflows/smoke.yml"
assert_gate "no-reference-cannot-run" 2 "$d"

# Every reference is a container path or a mount — nothing in the tree to check.
d="$WORK/foreign-refs"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" /etc/fraiseql/schema.compiled.json
assert_gate "references-that-are-not-ours-cannot-run" 2 "$d"

d="$WORK/no-workflows"; new_tree "$d" 2.15.0
rm -rf "$d/.github/workflows"
assert_gate "no-workflow-dir-cannot-run" 2 "$d"

d="$WORK/no-cargo"; new_tree "$d" 2.15.0
rm -f "$d/Cargo.toml"
assert_gate "no-version-to-compare-against-cannot-run" 2 "$d"

# ── The same artifact booted by several steps is one subject, and still passes ───
d="$WORK/dedup"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json docker/e2e/schema.compiled.json
assert_gate "a-repeated-reference-is-one-subject" 0 "$d"

# ── The Dagger legs boot compiled schemas too, and set FRAISEQL_SCHEMA_PATH from Go to
#    a CONTAINER path — so the workflow rule above cannot reach the host file ──────────
d="$WORK/dagger-file-stale"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
artifact "$d" docker/e2e/other.compiled.json 2.14.0
mounts_file "$d" docker/e2e/other.compiled.json
assert_gate "a-dagger-file-literal-is-a-subject" 1 "$d"

# The regression this whole branch exists for: the path is built by concatenation, so only
# the directory is a literal. Unstamped fixtures inside it must still be found.
d="$WORK/dagger-dir-unstamped"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
artifact "$d" crates/srv/tests/fixtures/federation/schema_users.json
mounts_dir "$d" crates/srv/tests/fixtures/federation/
assert_gate "a-dagger-directory-literal-is-a-subject" 1 "$d"

# ...and passes once they name this build, so the case above fails for the stamp and not
# merely for existing.
d="$WORK/dagger-dir-ok"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
artifact "$d" crates/srv/tests/fixtures/federation/schema_users.json 2.15.0
mounts_dir "$d" crates/srv/tests/fixtures/federation/
assert_gate "a-stamped-dagger-directory-subject-passes" 0 "$d"

# A container path in Go is absolute and is not ours to check; it must not be mistaken for
# a subject, nor make the run vacuous when a real workflow subject exists.
d="$WORK/dagger-container-path"; new_tree "$d" 2.15.0
artifact "$d" docker/e2e/schema.compiled.json 2.15.0
boots "$d" docker/e2e/schema.compiled.json
mounts_file "$d" /schema.compiled.json
assert_gate "an-absolute-dagger-path-is-not-a-subject" 0 "$d"

echo ""
echo "ran $TESTS_RUN, failed $TESTS_FAILED"
[ "$TESTS_FAILED" -eq 0 ]
