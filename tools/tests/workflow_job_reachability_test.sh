#!/usr/bin/env bash
# Unit tests for tools/check-workflow-job-reachability.py (#1206).
#
# Run directly:  bash tools/tests/workflow_job_reachability_test.sh
# Exits non-zero if any assertion fails.
#
# The gate deletes jobs, so both of its directions have to be pinned:
#
#   * it must FLAG the shapes that cannot run — otherwise nine dead conditions
#     survive another migration, which is how they survived the last one;
#   * it must NOT flag the shapes that can — a false positive here costs a real
#     job. Every trigger form this repo uses gets a fixture below: `push` with
#     only `paths` (all branches AND all tags), `push` with only `tags`,
#     `release` (a tag ref), `workflow_dispatch` (any branch or tag), and
#     `workflow_call` (the caller's event, so nothing is decidable).
#
# Exit codes: 0 = clean, 1 = findings (or an empty scan), 2 = FATAL (a shape the
# gate cannot read). A shape it cannot read is never silently clean.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-workflow-job-reachability.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <name> <workflow-yaml> → prints the root dir to scan
fixture() {
    local dir="$WORK/$1"
    mkdir -p "$dir/.github/workflows"
    printf '%s\n' "$2" >"$dir/.github/workflows/probe.yml"
    printf '%s' "$dir"
}

# expect <label> <expected-exit> <root-dir> [<substring-that-must-appear>]
expect() {
    local label="$1" want="$2" dir="$3" needle="${4:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out rc
    set +e
    out="$(WORKFLOW_REACHABILITY_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL  $label: exit $rc, wanted $want"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        echo "FAIL  $label: output did not mention '$needle'"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "PASS  $label"
}

# ── 1. The finding this gate was written for: docker-build.yml's test-images ──
expect "a pull_request job in a tag-only workflow can never run" 1 "$(fixture pr-in-tag-only "$(cat <<'YML'
name: Probe
on:
  push:
    tags: ['v*']
  workflow_dispatch:
jobs:
  test-images:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - run: echo hi
YML
)")" "can never run"

# ── 2. …and the same job under a workflow that HAS the trigger is left alone ──
#
# The direction that costs a real job if the model is wrong.
expect "the same condition is fine where the trigger exists" 0 "$(fixture pr-present "$(cat <<'YML'
name: Probe
on:
  push:
    tags: ['v*']
  pull_request:
jobs:
  test-images:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 3. A branch ref under a tags-only push: the verify-deployment shape ───────
#
# `github.event_name == 'push'` is satisfiable on its own here. Only the
# CONJUNCTION is unsatisfiable, so a gate that evaluated atoms independently
# would pass this.
expect "push && a branch ref is dead when push is tag-only" 1 "$(fixture branch-under-tags "$(cat <<'YML'
name: Probe
on:
  push:
    tags: ['v*']
  workflow_dispatch:
jobs:
  verify-deployment:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - run: echo hi
YML
)")" "can never run"

# ── 4. A dead ARM inside a job that CAN run: the publish-to-docker-hub shape ──
#
# The job fires on the tag arm, so job-level reachability says nothing. Written
# as a folded `>-` scalar with more-indented continuation lines — the exact YAML
# that made the shared parser raise before #1206, i.e. this fixture also pins
# that a FATAL there is not mistaken for a clean scan.
expect "a dead arm is reported inside a reachable job" 1 "$(fixture dead-arm "$(cat <<'YML'
name: Probe
on:
  push:
    tags: ['v*']
  workflow_dispatch:
jobs:
  publish:
    runs-on: ubuntu-latest
    if: >-
      github.event_name == 'push' && (
        github.ref == 'refs/heads/main' ||
        startsWith(github.ref, 'refs/tags/v')
      )
    steps:
      - run: echo hi
YML
)")" "dead condition arm"

# ── 5. A constant arm: the mirror lie ────────────────────────────────────────
expect "an always-true event arm is reported as vacuous" 1 "$(fixture vacuous "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  cleanup:
    runs-on: ubuntu-latest
    if: github.event_name != 'pull_request'
    steps:
      - run: echo hi
YML
)")" "vacuous condition arm"

# ── 6. `github.event.pull_request` is null off a PR ──────────────────────────
#
# Without this, a job whose surviving arm is `contains(github.event.pull_request
# .labels.*.name, …)` reads as reachable in a workflow that can never deliver a
# PR — the perf-baseline.yml and benchmark-velocity.yml shape, where the sole job
# of the workflow never ran and pressing Run reported success.
expect "a PR-payload arm is dead where no PR can arrive" 1 "$(fixture pr-payload "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  bench:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' || contains(github.event.pull_request.labels.*.name, 'perf')
    steps:
      - run: echo hi
YML
)")" "can never run"

# ── 7. workflow_call: the caller's event, so nothing is decidable ────────────
#
# A reusable workflow must never be flagged. Its `github.event_name` is whatever
# the CALLER received, which this file cannot see.
expect "a reusable workflow is not mis-flagged" 0 "$(fixture reusable "$(cat <<'YML'
name: Probe
on:
  workflow_call:
jobs:
  build:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request' && github.ref == 'refs/heads/main'
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 8. `push:` with only `paths` matches every branch AND every tag ──────────
#
# The nine SDK workflows' shape. Reading `paths` as a ref filter would flag every
# one of their publish jobs.
expect "push with only a paths filter still delivers tags" 0 "$(fixture paths-only "$(cat <<'YML'
name: Probe
on:
  push:
    paths: ['sdks/official/fraiseql-go/**']
  pull_request:
    paths: ['sdks/official/fraiseql-go/**']
jobs:
  publish:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/go-sdk/v')
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 9. `release:` carries a tag ref ─────────────────────────────────────────
#
# csharp-sdk.yml's publish job: `branches: ['**']` on push (never a tag), and the
# tag ref arrives on `release`. Model the release event and it is reachable;
# forget it and a real publish job is deleted.
expect "a release event supplies the tag ref" 0 "$(fixture release-tag "$(cat <<'YML'
name: Probe
on:
  push:
    branches: ['**']
  release:
    types: [published]
jobs:
  publish:
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/csharp-sdk/v')
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 10. workflow_dispatch can be pointed at any branch ──────────────────────
expect "a dispatch can target any branch" 0 "$(fixture dispatch-branch "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  publish:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/dev'
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 11. A branch pattern that cannot produce the named branch ───────────────
expect "a ref outside the push branch filter is dead" 1 "$(fixture branch-filter "$(cat <<'YML'
name: Probe
on:
  push:
    branches: ['release/*']
jobs:
  deploy:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - run: echo hi
YML
)")" "can never run"

# ── 12. …and one it CAN produce is left alone ──────────────────────────────
expect "a ref inside the push branch filter is fine" 0 "$(fixture branch-filter-ok "$(cat <<'YML'
name: Probe
on:
  push:
    branches: ['release/*', 'dev']
jobs:
  deploy:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/dev' || startsWith(github.ref, 'refs/heads/release/')
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 13. An unmodelled event decides nothing, and flags nothing ─────────────
#
# `schedule` runs on the default branch, whose name is repo configuration rather
# than a property of this file. MAYBE, never a finding.
expect "a schedule-triggered branch condition is not flagged" 0 "$(fixture scheduled "$(cat <<'YML'
name: Probe
on:
  schedule:
    - cron: '0 3 * * 0'
jobs:
  scan:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - run: echo hi
YML
)")" "every job can run"

# ── 14. An `if:` the gate cannot parse is FATAL, never a pass ──────────────
expect "an unreadable condition is FATAL" 2 "$(fixture unreadable "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  build:
    runs-on: ubuntu-latest
    if: github.event_name == = 'push'
    steps:
      - run: echo hi
YML
)")" "cannot parse"

# ── 15. A workflow with no `on:` block is FATAL ───────────────────────────
expect "a workflow with no triggers is FATAL" 2 "$(fixture no-on "$(cat <<'YML'
name: Probe
jobs:
  build:
    runs-on: ubuntu-latest
    if: github.event_name == 'push'
    steps:
      - run: echo hi
YML
)")" "no \`on:\` block"

# ── 16. A ref filter pattern the gate cannot translate is FATAL ──────────
#
# `+` and character classes are legal in GitHub filter patterns. Guessing at
# them produces a wrong verdict on a real job; refusing produces a fix.
expect "an untranslatable ref pattern is FATAL" 2 "$(fixture odd-pattern "$(cat <<'YML'
name: Probe
on:
  push:
    branches: ['releas+e']
jobs:
  build:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - run: echo hi
YML
)")" "teach the gate this shape"

# ── 17. The vacuous-scan guard, both ways ────────────────────────────────
#
# A gate that reports success having read nothing is the exact failure it
# exists to catch: it would go quiet the moment the directory moved.
EMPTY="$WORK/empty"
mkdir -p "$EMPTY/.github/workflows"
expect "an empty workflow directory is not a pass" 1 "$EMPTY" "no workflows found"

NOTHING="$WORK/nothing"
mkdir -p "$NOTHING"
expect "a missing workflow directory is not a pass" 1 "$NOTHING" "no workflow directory"

# ── 18. The real tree is green ──────────────────────────────────────────
#
# Runs the gate against this repository, so the self-test cannot pass while the
# checked-in workflows are red.
expect "this repository's workflows are all reachable" 0 "$REPO_ROOT" "every job can run"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "workflow job reachability self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "workflow job reachability self-test: $TESTS_RUN/$TESTS_RUN passed"
