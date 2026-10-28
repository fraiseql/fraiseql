#!/usr/bin/env bash
# Unit tests for tools/bench-quiet-window.sh.
#
# Run directly:  bash tools/tests/bench_quiet_window_test.sh
# Exits non-zero if any assertion fails.
#
# The gate reads a process listing and /proc/loadavg; both are fixtures here, and its
# sleep is a no-op, so every case runs in milliseconds. Each way the box can be busy is
# red on its own, the job's own ancestors are never counted against it, and a box that
# goes quiet between attempts is measured with the wait it took recorded.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/bench-quiet-window.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A listing in `ps -eo pid=,ppid=,comm=` shape: the job is pid 500, run by its own
# Runner.Worker (400) under the runner listener (300).
quiet_ps() {
    printf '%s\n' \
        '  1     0 systemd' \
        '300     1 Runner.Listener' \
        '400   300 Runner.Worker' \
        '450   400 bash' \
        '500   450 bash' \
        '610     1 dockerd' \
        '620     1 postgres'
}

# run_gate <name> <ps-file> <loadavg-file...> -- <gate args...>
# Loadavg files are read one per attempt, the last one repeating.
run_gate() {
    local name="$1" ps_file="$2"
    shift 2
    local loads=()
    while [ "$1" != "--" ]; do loads+=("$1"); shift; done
    shift
    QW_PS_FILE="$ps_file" \
    QW_LOADAVG_FILES="${loads[*]}" \
    QW_SELF_PID=500 \
    QW_SLEEP=: \
        bash "$GATE" "$@" > "$WORK/$name.out" 2> "$WORK/$name.err"
}

# check <name> <expected-rc> <expected-substring-in-stdout>
check() {
    local name="$1" want_rc="$2" want="$3" rc="$4"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [ "$rc" -ne "$want_rc" ]; then
        echo "FAIL [$name]: rc $rc, want $want_rc"
        sed 's/^/  | /' "$WORK/$name.out" "$WORK/$name.err"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if ! grep -qF -- "$want" "$WORK/$name.out"; then
        echo "FAIL [$name]: stdout lacks: $want"
        sed 's/^/  | /' "$WORK/$name.out" "$WORK/$name.err"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "ok   [$name]"
}

quiet_ps > "$WORK/quiet.ps"
printf '0.20 0.35 0.50 1/400 9999\n' > "$WORK/low.load"
printf '6.10 7.00 8.20 9/400 9999\n' > "$WORK/high.load"

# 1. An idle box is measured at once, with what it saw recorded.
rc=0; run_gate idle "$WORK/quiet.ps" "$WORK/low.load" -- --max-load 1.5 --retries 3 || rc=$?
check idle 0 '"quiet":true,"attempts":1,"waited_s":0,"load1":0.20,"load5":0.35,"load15":0.50,"busy":[]' "$rc"

# 2. The job's own Runner.Worker is its ancestor: it never makes the box busy.
rc=0; run_gate own-worker "$WORK/quiet.ps" "$WORK/low.load" -- --max-load 1.5 --retries 1 || rc=$?
check own-worker 0 '"busy":[]' "$rc"

# 3. Another CI job on the box is busy, however low the load.
{ quiet_ps; printf '%s\n' '700   300 Runner.Worker'; } > "$WORK/other-worker.ps"
rc=0; run_gate other-worker "$WORK/other-worker.ps" "$WORK/low.load" -- --max-load 1.5 --retries 2 || rc=$?
check other-worker 3 '"quiet":false,"attempts":2' "$rc"
check other-worker 3 '"busy":["Runner.Worker:700"]' "$rc"

# 4. A cargo or rustc anywhere on the box (a dev session, another leg's container) is busy.
{ quiet_ps; printf '%s\n' '800     1 cargo' '801   800 rustc'; } > "$WORK/compile.ps"
rc=0; run_gate compile "$WORK/compile.ps" "$WORK/low.load" -- --max-load 1.5 --retries 1 || rc=$?
check compile 3 '"busy":["cargo:800","rustc:801"]' "$rc"

# 5. With no process to blame, a load over the limit is still busy.
rc=0; run_gate high-load "$WORK/quiet.ps" "$WORK/high.load" -- --max-load 1.5 --retries 2 || rc=$?
check high-load 3 '"quiet":false,"attempts":2,"waited_s":' "$rc"
check high-load 3 '"busy":["load1>1.5"]' "$rc"

# 6. A load exactly at the limit is not over it.
printf '1.50 1.00 1.00 1/400 9999\n' > "$WORK/edge.load"
rc=0; run_gate at-limit "$WORK/quiet.ps" "$WORK/edge.load" -- --max-load 1.5 --retries 1 || rc=$?
check at-limit 0 '"quiet":true' "$rc"

# 7. A box that goes quiet between attempts is measured, and the wait is recorded.
rc=0; run_gate settles "$WORK/quiet.ps" "$WORK/high.load" "$WORK/high.load" "$WORK/low.load" \
    -- --max-load 1.5 --retries 5 --interval 60 || rc=$?
check settles 0 '"quiet":true,"attempts":3,"waited_s":120,"load1":0.20' "$rc"

# 8. --retries 1 never sleeps: a busy box answers at once.
rc=0; run_gate busy-once "$WORK/other-worker.ps" "$WORK/low.load" -- --max-load 1.5 --retries 1 --interval 60 || rc=$?
check busy-once 3 '"attempts":1,"waited_s":0' "$rc"

# 9. An unreadable loadavg is an error, not a quiet box.
rc=0; run_gate no-loadavg "$WORK/quiet.ps" "$WORK/missing.load" -- --max-load 1.5 --retries 1 || rc=$?
TESTS_RUN=$((TESTS_RUN + 1))
if [ "$rc" -eq 2 ] && grep -qF 'cannot read' "$WORK/no-loadavg.err"; then
    echo "ok   [no-loadavg]"
else
    echo "FAIL [no-loadavg]: rc $rc, want 2 with 'cannot read' on stderr"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# 10. An unknown flag is a usage error.
rc=0; run_gate bad-flag "$WORK/quiet.ps" "$WORK/low.load" -- --bogus || rc=$?
TESTS_RUN=$((TESTS_RUN + 1))
if [ "$rc" -eq 2 ]; then echo "ok   [bad-flag]"; else
    echo "FAIL [bad-flag]: rc $rc, want 2"; TESTS_FAILED=$((TESTS_FAILED + 1)); fi

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
