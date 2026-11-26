#!/usr/bin/env bash
# Red-capability pin for tools/clippy-config-paths-gate.sh.
#
# Run directly:  bash tools/tests/clippy_config_paths_test.sh
# Exits non-zero if any assertion fails.
#
# The wrapped command is a stand-in that prints what clippy prints and exits as clippy
# would, so the pin needs no compile. Proving clippy really prints that line for an
# unresolvable path is a one-off check against a real `cargo clippy`; this pins the
# wrapper's reading of it: the warning fails a green run, a clean run passes, and the
# command's own failure is never swallowed.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/clippy-config-paths-gate.sh"

TESTS_RUN=0
TESTS_FAILED=0

# The configuration warning as clippy prints it, then its location line.
WARNING='warning: `fraiseql_db::Writer::execute_write` does not refer to a reachable function
 --> .clippy.toml:72:5'

# expect <rc> <text the output must contain, or ""> <description> -- <command…>
expect() {
    local want_rc="$1" want_text="$2" desc="$3" out rc
    shift 4
    TESTS_RUN=$((TESTS_RUN + 1))
    out="$(bash "$GATE" "$@" 2>&1)"
    rc=$?
    if [ "$rc" = "$want_rc" ] && { [ -z "$want_text" ] || grep -cF -- "$want_text" >/dev/null <<<"$out"; }; then
        printf '  ✅ %s\n' "$desc"
    else
        printf '  ❌ %s (rc=%s, want %s, %s)\n' "$desc" "$rc" "$want_rc" "${want_text:-no text}"
        printf '%s\n' "$out" | sed 's/^/      /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "clippy-config-paths-gate"

expect 0 "Finished" "a clean clippy run passes" -- \
    bash -c 'echo "    Finished \`dev\` profile"; exit 0'

expect 1 "could not resolve a path in .clippy.toml" \
    "the warning on stderr fails a run clippy itself passed" -- \
    bash -c 'printf "%s\n" "$1" >&2; exit 0' _ "$WARNING"

expect 1 ".clippy.toml:72:5" "the refusal shows where the path is" -- \
    bash -c 'printf "%s\n" "$1" >&2; exit 0' _ "$WARNING"

expect 1 "could not resolve a path in .clippy.toml" \
    "the warning on stdout fails the run too" -- \
    bash -c 'printf "%s\n" "$1"; exit 0' _ "$WARNING"

expect 101 "error: could not compile" "a failing clippy run keeps its status" -- \
    bash -c 'echo "error: could not compile \`x\`" >&2; exit 101'

expect 1 "could not resolve a path in .clippy.toml" \
    "a failing run with the warning reports the warning" -- \
    bash -c 'printf "%s\n" "$1" >&2; exit 101' _ "$WARNING"

expect 2 "usage:" "no command is a usage error" --

echo
echo "$TESTS_RUN run, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
