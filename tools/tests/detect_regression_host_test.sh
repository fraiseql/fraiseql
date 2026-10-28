#!/usr/bin/env bash
# Unit tests for benchmarks/detect_regression.py's host guard.
#
# Run directly:  bash tools/tests/detect_regression_host_test.sh
# Exits non-zero if any assertion fails.
#
# Velocitybench numbers are only comparable on the box that measured them. A baseline
# from another host is never compared against (it reports that and passes), one from
# the same host still fails on a regression, and --update records the host it was
# measured on.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DETECT="$REPO_ROOT/benchmarks/detect_regression.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --update stamps the baseline with `git rev-parse HEAD`: run every case in a repository
# of its own, so the test holds in a checkout with no history (the Dagger preflight
# `git init`s its tree).
git init -q "$WORK/repo"
git -C "$WORK/repo" -c user.name=test -c user.email=test@example.invalid \
    commit -q --allow-empty -m fixture

baseline() {
    local host="$1"
    printf '{"version":"x","git_sha":"abc1234","recorded_at":"2026-10-28T00:00:00Z","config":{%s"dataset":"medium"},"thresholds":{"rps_regression_pct":5.0,"p95_regression_pct":10.0},"results":{"Q1":{"rps":1000,"p50_ms":1.0,"p95_ms":2.0,"p99_ms":3.0}}}\n' \
        "${host:+\"host\":\"$host\",}"
}
results() {
    printf '{"results":[{"framework":"fraiseql-tv","query":"Q1","rps":%s,"p50_ms":1.0,"p95_ms":2.0,"p99_ms":3.0}]}\n' "$1"
}

# check <name> <expected-rc> <expected-substring> <file-to-search> -- <detect args...>
check() {
    local name="$1" want_rc="$2" want="$3" where="$4"
    shift 5
    local rc=0
    (cd "$WORK/repo" && python3 "$DETECT" "$@") > "$WORK/$name.out" 2>&1 || rc=$?
    TESTS_RUN=$((TESTS_RUN + 1))
    if [ "$rc" -ne "$want_rc" ] || ! grep -qF -- "$want" "$where"; then
        echo "FAIL [$name]: rc $rc (want $want_rc), '$want' in $(basename "$where")?"
        sed 's/^/  | /' "$WORK/$name.out" "$where" 2>/dev/null || true
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "ok   [$name]"
    fi
}

baseline archbox > "$WORK/archbox.json"
baseline forge > "$WORK/forge.json"
baseline "" > "$WORK/unlabelled.json"
results 500 > "$WORK/slow.json"
results 1000 > "$WORK/same.json"

# 1. Another host's baseline is not compared against, even when the numbers are worse.
check other-host 0 'not measured on forge' "$WORK/other-host.md" -- \
    --results "$WORK/slow.json" --baseline "$WORK/archbox.json" \
    --output "$WORK/other-host.md" --require-host forge

# 2. A baseline that names no host is not comparable either.
check unlabelled 0 'not measured on forge' "$WORK/unlabelled.md" -- \
    --results "$WORK/slow.json" --baseline "$WORK/unlabelled.json" \
    --output "$WORK/unlabelled.md" --require-host forge

# 3. The same host's baseline still fails on a regression.
check same-host-slow 1 '❌' "$WORK/same-host-slow.md" -- \
    --results "$WORK/slow.json" --baseline "$WORK/forge.json" \
    --output "$WORK/same-host-slow.md" --require-host forge

# 4. ...and passes when nothing moved.
check same-host-same 0 '✅' "$WORK/same-host-same.md" -- \
    --results "$WORK/same.json" --baseline "$WORK/forge.json" \
    --output "$WORK/same-host-same.md" --require-host forge

# 5. --update records the host the numbers were measured on.
cp "$WORK/archbox.json" "$WORK/updated.json"
check update-host 0 '"host": "forge"' "$WORK/updated.json" -- \
    --results "$WORK/same.json" --baseline "$WORK/updated.json" --update --host forge

# 6. --update without --host refuses: a baseline must say where it was measured.
cp "$WORK/archbox.json" "$WORK/nohost.json"
check update-no-host 2 '--host' "$WORK/update-no-host.out" -- \
    --results "$WORK/same.json" --baseline "$WORK/nohost.json" --update

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
