#!/usr/bin/env bash
# Red-capability pin for tools/check-sdk-lockfile-freshness.py.
#
# Run directly:  bash tools/tests/sdk_lockfile_freshness_test.sh
# Exits non-zero if any assertion fails.
#
# The gate's claim is that a published SDK's lockfile cannot pin a version its manifest
# no longer claims, and that a lock format nobody has classified cannot be silently
# skipped. Every mutation below is a way that claim could be false while the gate still
# printed OK.
#
# Two are load-bearing beyond the rest:
#
#   * V4 changes ONLY package-lock.json's `.packages[""].version`, leaving the top-level
#     `.version` correct. npm records the root version twice and a bump can update one
#     site and not the other; a gate reading a single site passes this mutation while the
#     artifact is genuinely inconsistent. It is the reason locked_versions() returns a
#     list of sites rather than one value.
#   * C1 drops an unrecognised lock-shaped file into an SDK and requires FATAL. Without
#     it the gate degrades silently the moment a new packaging tool arrives — which is
#     not hypothetical: writing this gate turned up a tracked `bun.lock` in
#     fraiseql-typescript that a hand-written format list had missed.
#
# The fixture is assembled with `find`, never `git ls-files` — this test runs inside the
# Dagger ShellGates container, where the repository is `git init`ed with an empty index
# and `git ls-files` returns nothing at all. Copying uses mkdir + cp rather than
# `cpio -pdm`: shellBase installs exactly make, git, gawk, findutils, grep,
# ca-certificates and python3, and cpio is measured MISSING there, so a cpio fixture
# builder is green on every laptop and red in CI.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo carrying only what the gate reads: the gate itself, and every official
# SDK's TOP-LEVEL files (manifests and lockfiles). The gate scans sdks/official/*/ at
# depth 1 and nothing else, so this fixture is faithful rather than merely sufficient.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools"
    cp "$REPO_ROOT/tools/check-sdk-lockfile-freshness.py" "$dir/tools/"

    ( cd "$REPO_ROOT/sdks/official" && find . -maxdepth 1 -mindepth 1 -type d -printf '%f\0' ) \
        | while IFS= read -r -d '' sdk; do
              mkdir -p "$dir/sdks/official/$sdk"
              ( cd "$REPO_ROOT/sdks/official/$sdk" && find . -maxdepth 1 -type f -printf '%f\0' ) \
                  | while IFS= read -r -d '' f; do
                        cp "$REPO_ROOT/sdks/official/$sdk/$f" "$dir/sdks/official/$sdk/$f"
                    done
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

    # Exit 1 is a FINDING; exit 2 is the gate's own FATAL (a manifest it cannot read, a
    # lock format nobody classified). They are scored as different outcomes on purpose:
    # a mutation that happened to corrupt a manifest exits 2, and a harness asking only
    # "non-zero?" would score that as a successful red proof for an assertion it never
    # reached. `|| rc=$?` rather than a bare call, because `set -e` would abort this
    # script on the very non-zero exit each proof below exists to observe.
    local rc=0
    ( cd "$dir" && python3 tools/check-sdk-lockfile-freshness.py >"$dir/.out" 2>&1 ) || rc=$?
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

echo "sdk-lockfile-freshness gate self-test"
echo

echo "── the unmutated tree passes (or every assertion below is vacuous) ──"
expect pass P0 "repository as it stands" noop

echo
echo "── a lockfile pinning a version its manifest no longer claims ──"
expect fail V1 "uv.lock keeps the previous version (the measured #1225 shape)" \
    sed -i 's|^version = "2.15.0"$|version = "2.14.1"|' sdks/official/fraiseql-python/uv.lock

expect fail V2 "Cargo.lock keeps the previous version (the measured #1225 shape)" \
    sed -i 's|^version = "2.15.0"$|version = "2.14.1"|' sdks/official/fraiseql-rust/Cargo.lock

expect fail V3 "package-lock.json's top-level .version drifts" \
    python3 -c 'import json,pathlib
p=pathlib.Path("sdks/official/fraiseql-typescript/package-lock.json")
d=json.loads(p.read_text()); d["version"]="2.14.1"; p.write_text(json.dumps(d))'

# Load-bearing: the top-level .version stays CORRECT here. A gate reading only one of
# npm's two sites is green on this, with the shipped artifact internally inconsistent.
expect fail V4 'package-lock.json .packages[""].version drifts ALONE' \
    python3 -c 'import json,pathlib
p=pathlib.Path("sdks/official/fraiseql-typescript/package-lock.json")
d=json.loads(p.read_text()); d["packages"][""]["version"]="2.14.1"; p.write_text(json.dumps(d))'

expect fail V5 "the manifest is bumped and the lockfile is left behind" \
    sed -i 's|^version = "2.15.0"$|version = "2.16.0"|' sdks/official/fraiseql-rust/Cargo.toml

echo
echo "── a format or manifest the gate cannot read is FATAL, never a silent skip ──"
expect fatal C1 "an unclassified lock-shaped file arrives" \
    touch sdks/official/fraiseql-ruby/deno.lock

expect fatal C2 "a version-recording lock loses the manifest declaring its version" \
    rm sdks/official/fraiseql-rust/Cargo.toml

expect fatal C3 "uv.lock no longer records an editable root package" \
    sed -i 's|^source = { editable = "." }$|source = { registry = "https://pypi.org/simple" }|' sdks/official/fraiseql-python/uv.lock

expect fatal C4 "Cargo.lock no longer records its own root package" \
    sed -i 's|^name = "fraiseql-rust"$|name = "fraiseql-rust-renamed"|' sdks/official/fraiseql-rust/Cargo.lock

expect fatal C5 "pyproject declares a dynamic version the gate cannot resolve" \
    python3 -c 'import pathlib,re
p=pathlib.Path("sdks/official/fraiseql-python/pyproject.toml")
p.write_text(re.sub(r"^version = \"2\.15\.0\"$", "dynamic = [\"version\"]", p.read_text(), count=1, flags=re.M))'

expect fatal C6 "Cargo.toml inherits its version from a workspace" \
    sed -i 's|^version = "2.15.0"$|version.workspace = true|' sdks/official/fraiseql-rust/Cargo.toml

expect fatal C7 "package-lock.json records the root version in neither known site" \
    python3 -c 'import json,pathlib
p=pathlib.Path("sdks/official/fraiseql-typescript/package-lock.json")
d=json.loads(p.read_text()); d.pop("version",None); d["packages"][""].pop("version",None)
p.write_text(json.dumps(d))'

expect fatal C8 "discovery finds zero SDKs" \
    sh -c 'rm -rf sdks/official && mkdir -p sdks/official'

echo
echo "── a new SDK is discovered rather than hand-listed ──"
# A new SDK with no lockfile is legitimately fine; the point is that it is COUNTED, so a
# later lockfile in it cannot arrive unclassified. Guarded by C1 above.
expect pass N1 "an SDK with no lockfile at all is classified, not an error" \
    mkdir -p sdks/official/fraiseql-zig

expect fatal N2 "a new SDK arrives carrying an unclassified lock format" \
    sh -c 'mkdir -p sdks/official/fraiseql-zig && touch sdks/official/fraiseql-zig/build.zig.zon.lock'

echo
if [ "$TESTS_FAILED" -eq 0 ]; then
    echo "sdk-lockfile-freshness self-test: $TESTS_RUN/$TESTS_RUN assertions held."
else
    echo "sdk-lockfile-freshness self-test: $TESTS_FAILED of $TESTS_RUN assertions FAILED." >&2
    exit 1
fi
