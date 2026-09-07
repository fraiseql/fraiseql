#!/usr/bin/env bash
# Unit tests for the GATING side of tools/check-suite-coverage.py (#1289).
#
# Run directly:  bash tools/tests/suite_coverage_gating_test.sh
# Exits non-zero if any assertion fails.
#
# Coverage and gating are different questions, and until #1289 the gate only asked
# the first. Every `crates/*/tests/*_e2e_pg` suite was covered — by
# `Dagger — integration`, which ran on a push to `dev` and nowhere else — so two of
# them were red on `dev` for two weeks while all four required checks stayed green.
#
# The new dimension resolves a chain: a workflow job runs a leg, the job reports a
# check context, and a context either is or is not required on `dev`. Every link is
# a place the answer can be wrong in the SAFE-LOOKING direction — reporting a merge
# gate that does not exist — so each is asserted here against a fixture built to
# have exactly one defect.
#
# No Rust toolchain, no cargo, no network: the gate reads source and YAML as text.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-suite-coverage.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <workflow-yaml> <required-toml> [<exemptions-toml>] [<second-workflow-yaml>]
#
# A minimal repo: one crate with one plain test binary, a Dagger module whose
# `Test` leg runs it, and the workflow + mirror under test. The suite is always
# COVERED, so every verdict below is about gating and nothing else.
#
# The optional fifth argument writes a SECOND workflow, which the cases below use
# to build a job that runs no test leg at all — the shape whose required context
# escaped the trigger check entirely (#1296).
make_fixture() {
    local dir="$1" workflow="$2" mirror="$3" exemptions="${4:-}" workflow2="${5:-}"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/crates/demo/tests" "$dir/.github/workflows"
    cp "$GATE" "$dir/tools/check-suite-coverage.py"

    cat >"$dir/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/demo"]
TOML

    cat >"$dir/crates/demo/Cargo.toml" <<'TOML'
[package]
name = "demo"
version = "0.0.0"
edition = "2021"
TOML

    cat >"$dir/crates/demo/tests/probe.rs" <<'RS'
#[test]
fn a_thing_holds() {}
RS

    cat >"$dir/.dagger/main.go" <<'GO'
package main

func (m *FraiseqlCi) Test() string {
	script := []string{
		"cargo test -p demo --test probe",
	}
	return script[0]
}

func (m *FraiseqlCi) TestIntegration(suite string) string {
	switch suite {
	case "", "alpha":
		return m.integrationAlpha()
	case "beta":
		return m.integrationBeta()
	}
	return ""
}

func (m *FraiseqlCi) integrationAlpha() string {
	script := []string{
		"cargo test -p demo --test probe",
	}
	return script[0]
}

func (m *FraiseqlCi) integrationBeta() string { return "nothing" }
GO

    printf '%s\n' "$workflow" >"$dir/.github/workflows/probe.yml"
    if [ -n "$workflow2" ]; then
        printf '%s\n' "$workflow2" >"$dir/.github/workflows/probe2.yml"
    fi
    printf '%s\n' "$mirror" >"$dir/tools/required-checks.toml"
    if [ -n "$exemptions" ]; then
        printf '%s\n' "$exemptions" >"$dir/tools/suite-coverage-exemptions.toml"
    fi
}

# expect <label> <expected-exit> <fixture-dir> [<substring-that-must-appear>]
expect() {
    local label="$1" want="$2" dir="$3" needle="${4:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out rc
    set +e
    out="$(cd "$dir" && python3 tools/check-suite-coverage.py 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL  $label: exit $rc, wanted $want"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        echo "FAIL  $label: output did not mention '$needle'"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "PASS  $label"
}

WF_EVERY_BRANCH="name: Probe
on:
  push:
jobs:
  gates:
    name: workspace tests
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test --source=.
"

# ── 1. The shape #1289 asks for: a leg on every branch, required by name ────
make_fixture "$WORK/gating" "$WF_EVERY_BRANCH" 'required = ["workspace tests"]'
expect "a branch-triggered, required leg gates" 0 "$WORK/gating" "1 of 1 legs can fail a merge"

# ── 2. The defect itself: the leg runs only after the merge ─────────────────
#
# `branches: [dev]` was `Dagger — integration`'s trigger. The leg still runs, the
# suite is still covered, and a red test in it cannot stop anything.
make_fixture "$WORK/devonly" "name: Probe
on:
  push:
    branches: [dev]
jobs:
  gates:
    name: workspace tests
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test --source=.
" 'required = ["workspace tests"]'
expect "a dev-only leg cannot carry a required check" 1 "$WORK/devonly" \
    "does not run on a push to a working branch"

# ── 3. ...and the suite is reported by name, not only the workflow ──────────
#
# The two findings are deliberately separate. A reader fixing the workflow needs
# the first; a reader wondering what is unprotected needs the second, and #1289
# is precisely the case where nobody could answer the second question.
expect "...and names the suite left unprotected" 1 "$WORK/devonly" "UNGATED demo::probe"

# ── 3b. The same defect in a job that runs NO test leg ─────────────────────
#
# `required-checks.toml` says this gate "enforces the first half by reading the
# workflow's own `on:` block". It did not, for a whole class: the trigger check
# lived inside the per-leg loop, and `names` is populated only from cargo
# commands and `dagger call`s naming a known TEST leg. A job that runs neither
# belongs to no leg, so its required context was never asked the question.
#
# `feature matrix` (#1296) is the first such context in the tree — clippy under
# narrow feature sets, no `cargo test` anywhere in it — and listing it while its
# workflow was still `branches: [dev]` passed the gate silently. These two cases
# differ in the trigger and nothing else, so the verdict is attributable to it.
NON_TEST_JOB_STEPS="jobs:
  lint:
    name: lint only
    runs-on: ubuntu-latest
    steps:
      - run: make lint
"

make_fixture "$WORK/nontestdevonly" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
    branches: [dev]
$NON_TEST_JOB_STEPS"
expect "a dev-only NON-TEST job cannot carry a required check either" 1 \
    "$WORK/nontestdevonly" "\`lint only\` is required but its workflow"

make_fixture "$WORK/nontestbranch" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
$NON_TEST_JOB_STEPS"
expect "...and the same job on every branch does gate" 0 "$WORK/nontestbranch"

# ── 3c. A tags-only push trigger reaches no branch at all ──────────────────
#
# `on: push: tags: ['v*']` starts the workflow for a TAG ref and for nothing
# else — GitHub's rule is that defining only `tags`/`tags-ignore` stops the
# workflow running for events affecting branches, and this repository's own
# `release.yml` demonstrates it: 30 of its last 30 runs are `v*` tag refs, none
# a branch, across hundreds of branch pushes.
#
# The predicate read the ABSENCE of a `branches:` key as "no branch restriction"
# and answered True, so it would have waved through any of the sixteen publish
# contexts `release.yml` and `docker-build.yml` produce (#1298). Each one would
# have wedged `dev` permanently: a required check that reports on no branch push
# is strictly worse than the `branches: [dev]` case above, which at least
# reports after the merge.
make_fixture "$WORK/tagsonly" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
    tags: ['v*']
$NON_TEST_JOB_STEPS"
expect "a tags-only trigger cannot carry a required check" 1 \
    "$WORK/tagsonly" "\`lint only\` is required but its workflow"

# ── 3d. ...but a push carrying BOTH keys still answers on the branch half ───
#
# The rule has to be "tags present AND no branch key", not "tags present": a
# workflow that names `branches:` as well runs on those branches exactly as it
# always did, and refusing it would be a false failure on a real merge gate.
make_fixture "$WORK/tagsandbranches" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
    branches: ['**']
    tags: ['v*']
$NON_TEST_JOB_STEPS"
expect "...and a branches+tags push still gates on the branch half" 0 "$WORK/tagsandbranches"

# ── 3e. `tags-ignore:` alone is the same shape ─────────────────────────────
#
# The undefined-ref rule is symmetric: defining only `tags-ignore` leaves
# branches undefined, so the workflow runs for no branch push either. Nothing in
# the tree has this trigger today, which is exactly why it needs a case — the
# `tags:` half was also latent until it was not.
make_fixture "$WORK/tagsignore" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
    tags-ignore: ['v*']
$NON_TEST_JOB_STEPS"
expect "a tags-ignore-only trigger cannot carry a required check either" 1 \
    "$WORK/tagsignore" "\`lint only\` is required but its workflow"

# ── 3f. ...and an EXCLUSION list beside `tags:` still reaches branches ─────
#
# `branches-ignore:` names branches, so a push carrying it is not ref-kind
# filtered however many tag keys sit beside it. Both keys are present on purpose:
# `branches-ignore:` alone answers True whether or not the predicate handles it —
# the final fallthrough gets there anyway — so a case without `tags:` could not
# fail under any mutation, and would be a pin that pins nothing. This one is the
# input that discriminates.
make_fixture "$WORK/branchesignore" "$WF_EVERY_BRANCH" \
    'required = ["workspace tests", "lint only"]' "" \
    "name: Lint
on:
  push:
    branches-ignore: ['dependabot/**']
    tags: ['v*']
$NON_TEST_JOB_STEPS"
expect "a branches-ignore list beside tags still reaches working branches" 0 \
    "$WORK/branchesignore"

# ── 4. A catch-all branch list is a branch trigger ──────────────────────────
#
# `branches-ignore:` and a bare `push:` both reach every branch; so does
# `branches: ['**']`. Refusing the last would be a false failure, and a gate that
# cries wolf gets an exemption written for it.
make_fixture "$WORK/catchall" "name: Probe
on:
  push:
    branches: ['**']
jobs:
  gates:
    name: workspace tests
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test --source=.
" 'required = ["workspace tests"]'
expect "a catch-all branches list still gates" 0 "$WORK/catchall"

# ── 5. A required context nothing produces gates on nothing ─────────────────
#
# The direction that lies: the mirror claims a merge gate, no job reports that
# name, and the ruleset waits forever for a check that never arrives.
make_fixture "$WORK/stale" "$WF_EVERY_BRANCH" 'required = ["workspace tests", "a job that does not exist"]'
expect "a required context no job produces is stale" 1 "$WORK/stale" \
    "STALE REQUIRED CONTEXT \`a job that does not exist\`"

# ── 6. A path-filtered workflow cannot be required ──────────────────────────
make_fixture "$WORK/pathfilter" "name: Probe
on:
  push:
    paths:
      - 'crates/demo/**'
jobs:
  gates:
    name: workspace tests
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test --source=.
" 'required = ["workspace tests"]'
expect "a paths-filtered workflow cannot be required" 1 "$WORK/pathfilter" \
    "blocks forever"

# ── 7. The published opt-out, and its expiry ───────────────────────────────
#
# `[[ungated]]` is a separate table from `[[exempt]]` on purpose: "runs nowhere"
# and "runs somewhere that cannot block a merge" have different remedies, and one
# claim must never answer the other's question.
make_fixture "$WORK/exempted" "$WF_EVERY_BRANCH" 'required = []' '[[ungated]]
target = "demo::probe"
reason = "needs a networked runner; tracked in #0000"
'
expect "an [[ungated]] row with a reason is the opt-out" 0 "$WORK/exempted"

make_fixture "$WORK/staleexempt" "$WF_EVERY_BRANCH" 'required = ["workspace tests"]' '[[ungated]]
target = "demo::probe"
reason = "needs a networked runner; tracked in #0000"
'
expect "an [[ungated]] row a required check made unnecessary is stale" 1 "$WORK/staleexempt" \
    "STALE [[ungated]] EXEMPTION demo::probe"

make_fixture "$WORK/noreason" "$WF_EVERY_BRANCH" 'required = []' '[[ungated]]
target = "demo::probe"
reason = ""
'
expect "an [[ungated]] row without a reason is FATAL" 2 "$WORK/noreason" "without target/reason"

# ── 8. `dagger call test-integration --suite=X` resolves through the switch ─
#
# The suite name is read out of `.dagger/main.go`'s switch rather than mirrored,
# so the workflow matrix and the Go dispatch cannot drift into agreement-by-luck.
make_fixture "$WORK/suite" "name: Probe
on:
  push:
jobs:
  gates:
    name: integration (\${{ matrix.suite }})
    runs-on: ubuntu-latest
    strategy:
      matrix:
        suite: [alpha, beta]
    steps:
      - run: dagger call test-integration --source=. --suite=\${{ matrix.suite }}
" 'required = ["integration (alpha)", "integration (beta)"]'
expect "a --suite= fan-out resolves to the Go method it reaches" 0 "$WORK/suite" \
    "integrationAlpha           → integration (alpha)"

# ── 9. ...and a suite the switch does not have is FATAL, not silent ────────
make_fixture "$WORK/badsuite" "name: Probe
on:
  push:
jobs:
  gates:
    name: integration (gamma)
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test-integration --source=. --suite=gamma
" 'required = ["integration (gamma)"]'
expect "a --suite= naming no case is FATAL" 2 "$WORK/badsuite" "is not a case in"

# ── 10. A `dagger call` naming no function is FATAL ────────────────────────
#
# Without this a typo reads as "that leg runs no tests", which is indistinguishable
# from the truth for every suite the leg was supposed to carry.
make_fixture "$WORK/badfn" "name: Probe
on:
  push:
jobs:
  gates:
    name: workspace tests
    runs-on: ubuntu-latest
    steps:
      - run: dagger call tset --source=.
" 'required = ["workspace tests"]'
expect "a dagger call naming no function is FATAL" 2 "$WORK/badfn" "names no"

# ── 11. A missing mirror is FATAL, not "nothing is required" ───────────────
make_fixture "$WORK/nomirror" "$WF_EVERY_BRANCH" 'required = ["workspace tests"]'
rm "$WORK/nomirror/tools/required-checks.toml"
expect "a missing required-checks.toml is FATAL" 2 "$WORK/nomirror" "is missing"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "suite-coverage gating self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-coverage gating self-test: $TESTS_RUN/$TESTS_RUN passed"
