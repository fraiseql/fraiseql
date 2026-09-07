#!/usr/bin/env bash
# Red-capability pin for .dagger/main.go's `suiteCountPrelude` (#1276).
#
# Run directly:  bash tools/tests/suite_marker_prelude_test.sh
# Exits non-zero if any assertion fails.
#
# The prelude shadows `cargo` with a shell function so every `cargo test …` line in a
# leg gets a `SUITE-RAN <args> :: test result: …` marker for free. To quote those
# counts it must buffer the run into `$out` — and buffering is where it went wrong.
# Written `out="$(…)"; rc=$?`, the assignment is a simple command carrying the
# substitution's status, so under the leg's `set -e` a FAILING cargo killed the shell
# at that line, before the `printf` that emits `$out`. The failing suite's whole
# output was discarded: no test names, no `test result: FAILED`, no panic. Twice the
# `integration (server)` leg — the only leg this prelude is injected into — exited 101
# with nothing in its log to say why.
#
# The prelude is READ OUT OF main.go rather than copied here. A copy would pass while
# the shipped constant regressed, which is the whole failure mode this file exists for.
#
# No cargo, no toolchain, no network: `cargo` is a fake on PATH, so this belongs in
# preflight rather than a heavy leg.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MAIN_GO="$REPO_ROOT/.dagger/main.go"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

[ -f "$MAIN_GO" ] || { echo "FATAL: $MAIN_GO is missing"; exit 2; }

# The shipped constant, verbatim. Go raw-string literals process no escapes, so the
# bytes between the backticks are exactly the shell the leg runs.
PRELUDE="$(python3 - "$MAIN_GO" <<'PY'
import re, sys
src = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r"const suiteCountPrelude = `(.*?)`", src, re.S)
if not m:
    sys.exit("FATAL: no `const suiteCountPrelude = ` raw-string literal in main.go")
print(m.group(1))
PY
)"
[ -n "$PRELUDE" ] || { echo "FATAL: extracted an empty prelude"; exit 2; }

# A fake `cargo`: prints a recognisable run to stdout AND stderr, exits with $FAKE_RC.
mkdir -p "$WORK/bin"
cat >"$WORK/bin/cargo" <<'FAKE'
#!/usr/bin/env bash
echo "FAKE-CARGO-STDOUT $*"
echo "FAKE-CARGO-STDERR" >&2
echo "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
exit "${FAKE_RC:-0}"
FAKE
chmod +x "$WORK/bin/cargo"

# Run the shipped prelude under the same `set -e` the legs use, with the fake first
# on PATH. `TRAILER` is the line a `set -e` abort must prevent.
run_leg() {
    FAKE_RC="$1" PATH="$WORK/bin:$PATH" bash -c "
set -e
$PRELUDE
cargo test -p demo --test probe
echo TRAILER-REACHED
" 2>&1
}

check() {
    local label="$1" ; shift
    TESTS_RUN=$((TESTS_RUN + 1))
    if "$@"; then
        echo "PASS  $label"
    else
        echo "FAIL  $label"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# `check ! cmd …` cannot work: `!` is a shell keyword, not a command `"$@"` can run.
check_not() {
    local label="$1" ; shift
    TESTS_RUN=$((TESTS_RUN + 1))
    if "$@"; then
        echo "FAIL  $label"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "PASS  $label"
    fi
}

# ── 1. The regression itself: a FAILING suite's output must reach the log ──
OUT_FAIL="$(run_leg 101)"; RC_FAIL=$?
printf '%s\n' "$OUT_FAIL" >"$WORK/fail.out"

check "a failing suite's stdout is printed, not discarded" \
    grep -q "FAKE-CARGO-STDOUT" "$WORK/fail.out"
check "...and its stderr too (the capture is 2>&1)" \
    grep -q "FAKE-CARGO-STDERR" "$WORK/fail.out"

# ── 2. The exit code survives the fix ──────────────────────────────────────
check "the suite's exit code is propagated" test "$RC_FAIL" -eq 101

# ── 3. ...and `set -e` still stops the leg at the failing suite ────────────
#
# The point of the fix is to print more, never to run more. A leg that carried on
# past a red suite would be a far worse defect than the one being fixed.
check_not "set -e still aborts the leg at the failing suite" \
    grep -q "TRAILER-REACHED" "$WORK/fail.out"

# ── 4. SUITE-START names the suite BEFORE it runs ──────────────────────────
#
# A suite killed by the leg timeout produces no cargo exit, so `$out` is never
# printed and SUITE-RAN never fires. The last SUITE-START is all a hang leaves.
check "SUITE-START is emitted for a failing suite" \
    grep -q "^SUITE-START test -p demo --test probe$" "$WORK/fail.out"
start_line="$(grep -n '^SUITE-START' "$WORK/fail.out" | head -1 | cut -d: -f1)"
out_line="$(grep -n 'FAKE-CARGO-STDOUT' "$WORK/fail.out" | head -1 | cut -d: -f1)"
check "SUITE-START precedes the suite's own output" \
    test -n "$start_line" -a -n "$out_line" -a "${start_line:-9}" -lt "${out_line:-0}"

# ── 5. The success path is unchanged: counts still reach SUITE-RAN ─────────
OUT_OK="$(run_leg 0)"; RC_OK=$?
printf '%s\n' "$OUT_OK" >"$WORK/ok.out"

check "a passing suite still exits 0" test "$RC_OK" -eq 0
check "a passing leg reaches the line after the suite" \
    grep -q "TRAILER-REACHED" "$WORK/ok.out"
check "SUITE-RAN carries the suite's own counts" \
    grep -q "^SUITE-RAN test -p demo --test probe :: test result: ok. 3 passed;" "$WORK/ok.out"

echo ""
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "suite-marker prelude self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-marker prelude self-test: $TESTS_RUN/$TESTS_RUN passed"
