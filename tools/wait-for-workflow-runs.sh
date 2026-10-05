#!/usr/bin/env bash
# Wait until every named workflow has a COMPLETED run for one commit.
#
# release-smoke.yml's `consume-published-artifacts` fires on the same `v*` push as
# release.yml and npm-publish.yml, so it started consuming before anything was
# published: on v2.15.0 it ran at 10:03Z, retried for ~135 s and gave up, while
# crates.io received 2.15.0 at 10:24–10:43Z (#1488). A fixed sleep only moves that
# race. This waits for the publishers themselves to finish.
#
# "Completed" is enough, whatever the conclusion: a release that published half its
# artifacts must still be consumed, so the consumer can report exactly what is missing.
# A workflow that never shows a run for the commit, or never completes, is a failure
# (exit 1), never a pass.
#
# Usage: wait-for-workflow-runs.sh <sha> <workflow-file> [<workflow-file> ...]
#
# Tunables (env):
#   WAIT_RUNS_MAX_ATTEMPTS  polls per workflow (default 160)
#   WAIT_RUNS_SLEEP_SECS    delay between polls (default 45)
# Default budget = 2 h per workflow: release.yml's slowest job (publish-crates) has a
# 90-minute timeout, and it waits on build/release jobs before it starts.
set -euo pipefail

if [ "$#" -lt 2 ]; then
    echo "usage: $(basename "$0") <sha> <workflow-file> [<workflow-file> ...]" >&2
    exit 2
fi

sha="$1"
shift
max_attempts="${WAIT_RUNS_MAX_ATTEMPTS:-160}"
sleep_secs="${WAIT_RUNS_SLEEP_SECS:-45}"

for workflow in "$@"; do
    state="none"
    for attempt in $(seq 1 "$max_attempts"); do
        line="$(gh run list --workflow "$workflow" --commit "$sha" --event push --limit 1 \
            --json status,conclusion,databaseId \
            --jq '.[0] | select(. != null) | [.status, (.conclusion // ""), (.databaseId | tostring)] | join("|")')"
        if [ -z "$line" ]; then
            state="none"
        else
            IFS='|' read -r status conclusion run_id <<<"$line"
            if [ "$status" = "completed" ]; then
                echo "$workflow: completed (${conclusion:-no conclusion}), run $run_id"
                continue 2
            fi
            state="$status"
        fi
        if [ "$attempt" -lt "$max_attempts" ]; then
            echo "$workflow: ${state} for $sha (poll ${attempt}/${max_attempts}); waiting ${sleep_secs}s"
            sleep "$sleep_secs"
        fi
    done
    if [ "$state" = "none" ]; then
        echo "::error::$workflow: no run for $sha after $max_attempts polls" >&2
    else
        echo "::error::$workflow: still $state for $sha after $max_attempts polls" >&2
    fi
    exit 1
done
