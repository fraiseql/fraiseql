#!/usr/bin/env bash
# Unit tests for tools/wait-for-workflow-runs.sh.
#
# Run directly:  bash tools/tests/wait_for_workflow_runs_test.sh
# Exits non-zero if any assertion fails.
#
# `gh` is replaced by a stub that answers `gh run list` from a script of per-call
# responses, so each case drives the poll loop through a known sequence: completed at
# once, in progress then completed, a run that never appears, a run that never ends, a
# failed run (still "finished": the consumer reports what is missing), two workflows.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SUBJECT="$REPO_ROOT/tools/wait-for-workflow-runs.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# stub_gh <dir> <workflow>=<resp1>,<resp2>,... : each `gh run list --workflow <wf>` call
# prints the next response (the last repeats). A response is `none`, or `<status>:<conclusion>`.
stub_gh() {
    local dir="$1"; shift
    mkdir -p "$dir/bin" "$dir/state"
    for spec in "$@"; do
        printf '%s\n' "${spec#*=}" | tr ',' '\n' > "$dir/state/${spec%%=*}.responses"
    done
    cat > "$dir/bin/gh" <<EOF
#!/usr/bin/env bash
state="$dir/state"
wf=""
while [ "\$#" -gt 0 ]; do
    case "\$1" in --workflow) wf="\$2"; shift 2 ;; *) shift ;; esac
done
n=\$(cat "\$state/\$wf.calls" 2>/dev/null || echo 0); n=\$((n + 1)); echo "\$n" > "\$state/\$wf.calls"
total=\$(wc -l < "\$state/\$wf.responses")
line=\$(( n > total ? total : n ))
resp=\$(sed -n "\${line}p" "\$state/\$wf.responses")
if [ "\$resp" = none ]; then echo ""; else echo "\${resp%%:*}|\${resp#*:}|4242"; fi
EOF
    chmod +x "$dir/bin/gh"
}

# assert_run <name> <expected-rc> <expected-substring> <dir> <args...>
assert_run() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" dir="$4"; shift 4
    local out rc
    set +e
    out="$(PATH="$dir/bin:$PATH" WAIT_RUNS_MAX_ATTEMPTS=4 WAIT_RUNS_SLEEP_SECS=0 bash "$SUBJECT" "$@" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "wait-for-workflow-runs.sh:"

d="$WORK/done"; stub_gh "$d" "release.yml=completed:success"
assert_run "a completed run returns at once" 0 "release.yml: completed (success)" "$d" abc123 release.yml

d="$WORK/later"; stub_gh "$d" "release.yml=none,in_progress:,completed:success"
assert_run "absent, then running, then completed is waited out" 0 "release.yml: completed (success)" "$d" abc123 release.yml
[[ "$(cat "$d/state/release.yml.calls")" -eq 3 ]] || { echo "  FAIL: expected 3 polls" >&2; TESTS_FAILED=$((TESTS_FAILED + 1)); }

d="$WORK/never"; stub_gh "$d" "release.yml=none"
assert_run "a run that never appears is a failure, not a pass" 1 "release.yml: no run for abc123" "$d" abc123 release.yml

d="$WORK/stuck"; stub_gh "$d" "release.yml=in_progress:"
assert_run "a run that never completes times out red" 1 "release.yml: still in_progress" "$d" abc123 release.yml

d="$WORK/failed"; stub_gh "$d" "release.yml=completed:failure"
assert_run "a failed release still counts as finished" 0 "release.yml: completed (failure)" "$d" abc123 release.yml

d="$WORK/two"; stub_gh "$d" "release.yml=completed:success" "npm-publish.yml=queued:,completed:success"
assert_run "every named workflow is waited for" 0 "npm-publish.yml: completed (success)" "$d" abc123 release.yml npm-publish.yml

d="$WORK/two-stuck"; stub_gh "$d" "release.yml=completed:success" "npm-publish.yml=queued:"
assert_run "one finished workflow does not excuse another" 1 "npm-publish.yml: still queued" "$d" abc123 release.yml npm-publish.yml

d="$WORK/usage"; stub_gh "$d" "release.yml=completed:success"
assert_run "no workflow named is a usage error" 2 "usage:" "$d" abc123

echo ""
echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[[ "$TESTS_FAILED" -eq 0 ]]
