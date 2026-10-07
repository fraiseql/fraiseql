#!/usr/bin/env bash
# Red-capability pin for tools/check-cargo-lock-path-versions.py.
#
# Run directly:  bash tools/tests/cargo_lock_path_versions_test.sh
# Exits non-zero if any assertion fails.
#
# The gate's claim is that no Cargo.lock in the tree records a path package at a version
# its manifest no longer declares, and that a record it cannot match to exactly one
# manifest stops the gate rather than being skipped. Every mutation below is a way that
# claim could be false while the gate still printed OK. V1 and V2 are the two shapes the
# 2.16.0 cut actually shipped.
#
# The fixture is assembled with `find`, never `git ls-files`, and copied with mkdir + cp:
# this test runs inside the Dagger ShellGates container, whose repository has an empty
# index and whose image carries no cpio.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo carrying only what the gate reads: the gate itself, every Cargo.toml and
# every Cargo.lock, pruned exactly as the gate prunes its own walk.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools"
    cp "$REPO_ROOT/tools/check-cargo-lock-path-versions.py" "$dir/tools/"
    ( cd "$REPO_ROOT" && find . \( -name .git -o -name target -o -name node_modules -o -name .venv \) -prune \
          -o \( -name Cargo.toml -o -name Cargo.lock \) -type f -print0 ) \
        | while IFS= read -r -d '' f; do
              mkdir -p "$dir/$(dirname "$f")"
              cp "$REPO_ROOT/$f" "$dir/$f"
          done
}

# expect <pass|fail|fatal> <case-id> <description> <mutation...>
expect() {
    local want="$1" id="$2" desc="$3"; shift 3
    local dir="$WORK/$id" got
    TESTS_RUN=$((TESTS_RUN + 1))
    make_fixture "$dir"
    ( cd "$dir" && "$@" ) || {
        printf '  ❌ %-4s %s — the mutation itself failed\n' "$id" "$desc"
        TESTS_FAILED=$((TESTS_FAILED + 1)); return
    }

    # Exit 1 is a FINDING and exit 2 the gate's own FATAL, scored apart: a mutation that
    # merely broke a manifest exits 2, and a harness asking only "non-zero?" would count
    # that as a red proof of an assertion it never reached.
    local rc=0
    ( cd "$dir" && python3 tools/check-cargo-lock-path-versions.py >"$dir/.out" 2>&1 ) || rc=$?
    case "$rc" in
        0) got=pass ;;
        1) got=fail ;;
        2) got=fatal ;;
        *) got="rc$rc" ;;
    esac

    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        sed 's/^/        /' "$dir/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

noop() { :; }

# Set the version recorded for one [[package]] of a lockfile; fail when no record names it,
# so a mutation aimed at a package the tree no longer has cannot pass as a no-op.
set_lock_version() {
    python3 - "$@" <<'PY'
import pathlib, re, sys
path, name, version = sys.argv[1:]
p = pathlib.Path(path)
text, count = re.subn(
    rf'(\nname = "{re.escape(name)}"\nversion = ")[^"]*(")', rf"\g<1>{version}\g<2>", p.read_text()
)
if count != 1:
    sys.exit(f"mutation matched {count} records named {name!r} in {path}")
p.write_text(text)
PY
}

# Rewrite the first line matching <regex> in <file>; fail when none does.
sub_first() {
    python3 - "$@" <<'PY'
import pathlib, re, sys
path, pattern, replacement = sys.argv[1:]
p = pathlib.Path(path)
text, count = re.subn(pattern, replacement, p.read_text(), count=1, flags=re.M)
if count != 1:
    sys.exit(f"mutation matched nothing: {pattern!r} in {path}")
p.write_text(text)
PY
}

# A version no release will ever carry.
NEXT="999.0.0"

echo "cargo-lock-path-versions gate self-test"
echo

echo "── the unmutated tree passes (or every assertion below is vacuous) ──"
expect pass P0 "repository as it stands" noop

echo
echo "── a lockfile recording a path package at a version its manifest no longer declares ──"
expect fail V1 "a fuzz lockfile keeps a sibling crate at the previous version (the 2.16.0 shape)" \
    set_lock_version crates/fraiseql-db/fuzz/Cargo.lock fraiseql-error 2.15.0

expect fail V2 "fraiseql-client's lockfile keeps its own record (2.3.0 through 13 releases)" \
    set_lock_version sdks/official/fraiseql-rust/fraiseql-client/Cargo.lock fraiseql-client 2.3.0

expect fail V3 "the workspace version moves and the lockfiles are left behind" \
    sub_first Cargo.toml '^(\[workspace\.package\](?:\n(?!\[).*)*?\nversion = ")[^"]*' "\g<1>$NEXT"

expect fail V4 "a standalone manifest moves and its own lockfile is left behind" \
    sub_first sdks/official/fraiseql-rust/fraiseql-client/Cargo.toml '^version = "[^"]*"' "version = \"$NEXT\""

echo
echo "── what the gate must NOT judge ──"
expect pass N1 "a registry package's version is the resolver's business" \
    set_lock_version crates/fraiseql-db/fuzz/Cargo.lock serde 1.0.0

expect pass N2 "a stale lockfile under target/ is a local build artefact, never committed" \
    sh -c 'mkdir -p crates/fraiseql-db/fuzz/target/x && sed "s/^version = \"[0-9][^\"]*\"\$/version = \"0.0.1\"/" crates/fraiseql-db/fuzz/Cargo.lock > crates/fraiseql-db/fuzz/target/x/Cargo.lock'

echo
echo "── a record the gate cannot match to one manifest is FATAL, never a silent skip ──"
expect fatal C1 "a path package no Cargo.toml declares" \
    sub_first crates/fraiseql-db/fuzz/Cargo.lock '^name = "fraiseql-db-fuzz"$' 'name = "fraiseql-db-fuzz-renamed"'

expect fatal C2 "two manifests claim one name at different versions" \
    sh -c 'mkdir -p examples/namesake && sed "s/^version = \"[^\"]*\"/version = \"'"$NEXT"'\"/" sdks/official/fraiseql-rust/fraiseql-client/Cargo.toml > examples/namesake/Cargo.toml'

expect fatal C3 "an inherited version with no [workspace.package] version to inherit" \
    sub_first Cargo.toml '^(\[workspace\.package\](?:\n(?!\[).*)*?)\nversion = "[^"]*"' '\g<1>'

expect fatal C4 "discovery finds no Cargo.lock at all" \
    sh -c 'find . -name Cargo.lock -delete'

echo
if [ "$TESTS_FAILED" -eq 0 ]; then
    echo "cargo-lock-path-versions self-test: $TESTS_RUN/$TESTS_RUN assertions held."
else
    echo "cargo-lock-path-versions self-test: $TESTS_FAILED of $TESTS_RUN assertions FAILED." >&2
    exit 1
fi
