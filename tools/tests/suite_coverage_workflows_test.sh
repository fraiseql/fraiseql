#!/usr/bin/env bash
# Unit tests for the GitHub Actions side of tools/check-suite-coverage.py (#1120).
#
# Run directly:  bash tools/tests/suite_coverage_workflows_test.sh
# Exits non-zero if any assertion fails.
#
# Four codegen consumer suites shell out to language toolchains and the network,
# so they run in a GitHub-hosted job rather than an offline Dagger leg. They used
# to be carried as exemptions — a claim the gate did not check. Reading `run:`
# blocks removes the exemption, and this file is what stops that reading from
# being a worse lie than the exemption was.
#
# The risk is specific: a workflow can LOOK like coverage and provide none. Each
# fixture below is one of those ways, and each must be reported. Two of them are
# live in this repo today (`feature-flags.yml` and `bench.yml` lost their push
# triggers in the Dagger migration and kept their `cargo test` lines).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-suite-coverage.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <workflow-yaml>
# A minimal repo the gate can read: one crate holding one all-#[ignore]d test
# binary (the shape of the four consumer suites), a .dagger/main.go that runs
# NOTHING, and the workflow under test. With no workflow coverage the suite is
# an orphan, so every fixture's verdict is attributable to the workflow alone.
make_fixture() {
    local dir="$1" workflow="$2"
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

    cat >"$dir/crates/demo/tests/consumer.rs" <<'RS'
#[test]
#[ignore]
fn consumer_compiles_the_generated_client() {}
RS

    # A leg file with no cargo invocations at all: the Dagger side contributes
    # nothing, so the workflow side is the only thing that can cover the suite.
    cat >"$dir/.dagger/main.go" <<'GO'
package main

func (m *FraiseqlCi) Nothing() string { return "no invocations here" }
GO

    printf '%s\n' "$workflow" >"$dir/.github/workflows/probe.yml"
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
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    # `--` before the pattern: a needle like `--bench` is a pattern, not an option.
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        echo "FAIL  $label: output did not mention '$needle'"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "PASS  $label"
}

# ── 1. The positive case: a pushed, root-directory, --ignored run covers it ──
#
# Also the block-scalar case: the real step uses a folded `>` scalar, so a
# reader that mishandled folding would silently lose most of the command.
make_fixture "$WORK/covered" "$(cat <<'YML'
name: Probe
on:
  push:
    paths:
      - 'crates/demo/**'
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - name: Consumer-usage gates
        run: >
          cargo test -p demo
          --test consumer
          -- --ignored
YML
)"
expect "a pushed --ignored run covers an all-#[ignore]d suite" 0 "$WORK/covered" \
    "all covered"

# ── 2. workflow_dispatch-only: runs on no push and no PR ─────────────────────
make_fixture "$WORK/dispatch" "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test -p demo --test consumer -- --ignored
YML
)"
expect "workflow_dispatch-only is not coverage" 1 "$WORK/dispatch" \
    "workflow_dispatch-only"

# ── 3. `on:` must survive YAML 1.1 boolean coercion ──────────────────────────
#
# The plain scalar `on` is the boolean true in YAML 1.1. A key reader that
# coerces values turns every trigger block into a key named `True`, and the gate
# then reports that no workflow has an `on:` block — loud, and about the wrong
# thing. Fixture 1 already depends on this; this asserts the parse directly so
# the failure names the cause.
TESTS_RUN=$((TESTS_RUN + 1))
if python3 - "$GATE" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("sc", sys.argv[1])
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
doc = m.parse_yaml("name: x\non:\n  push:\n    paths: ['a/**']\njobs: {}\n")
assert "on" in doc, f"`on:` key was coerced away: {sorted(doc)}"
assert doc["on"]["push"]["paths"] == ["a/**"], doc["on"]
PY
then echo "PASS  \`on:\` is read as a key, not the boolean true"
else echo "FAIL  \`on:\` is read as a key, not the boolean true"; TESTS_FAILED=$((TESTS_FAILED + 1)); fi

# ── 3b. A folded scalar keeps its more-indented lines ────────────────────────
#
# `>` folds lines at the block indent into spaces, but a MORE-indented line is
# literal — that is plain YAML, and it is how a multi-line `if:` indents its
# parenthesised arms (docker-build.yml). The parser used to refuse that shape
# rather than reshape it, which was harmless here (this gate skips workflows with
# no `cargo test`) and FATAL for tools/check-workflow-job-reachability.py, which
# reads every workflow. Folding those lines into the run would silently rewrite a
# `run:` script, so what is asserted is that they are KEPT, not merely accepted.
TESTS_RUN=$((TESTS_RUN + 1))
if python3 - "$GATE" <<'PYX'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("sc", sys.argv[1])
m = importlib.util.module_from_spec(spec)
sys.modules["sc"] = m
spec.loader.exec_module(m)
doc = m.parse_yaml(
    "jobs:\n"
    "  a:\n"
    "    if: >-\n"
    "      one && (\n"
    "        two ||\n"
    "        three\n"
    "      )\n"
)
got = doc["jobs"]["a"]["if"]
assert got == "one && (\n  two ||\n  three\n)", repr(got)
folded = m.parse_yaml("k: >\n  a\n  b\n")["k"]
assert folded == "a b\n", repr(folded)
PYX
then echo "PASS  a folded scalar keeps more-indented lines and folds the rest"
else echo "FAIL  a folded scalar keeps more-indented lines and folds the rest"; TESTS_FAILED=$((TESTS_FAILED + 1)); fi

# ── 4. A path filter that does not reach the suite's own source ──────────────
#
# The suite can then be broken in the same commit that fails to run it.
make_fixture "$WORK/paths" "$(cat <<'YML'
name: Probe
on:
  push:
    paths:
      - 'docs/**'
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test -p demo --test consumer -- --ignored
YML
)"
expect "a paths filter missing the suite's crate is not coverage" 1 "$WORK/paths"

# ── 5. A job-level defaults.run.working-directory (the rust-sdk.yml shape) ───
make_fixture "$WORK/jobdir" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: sdks/official/fraiseql-rust
    steps:
      - run: cargo test -p demo --test consumer -- --ignored
YML
)"
expect "a job-level working-directory leaves this workspace" 1 "$WORK/jobdir" \
    "outside this workspace"

# ── 6. A per-step working-directory (the rust-sdk-client.yml shape) ──────────
make_fixture "$WORK/stepdir" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test -p demo --test consumer -- --ignored
        working-directory: sdks/official/fraiseql-rust/fraiseql-client
YML
)"
expect "a per-step working-directory leaves this workspace" 1 "$WORK/stepdir" \
    "outside this workspace"

# ── 7. A `cd` inside the run script moves the cwd just as well ───────────────
make_fixture "$WORK/cd" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: |
          cd sdks/official/fraiseql-rust
          cargo test -p demo --test consumer -- --ignored
YML
)"
expect "a \`cd\` in the script leaves this workspace" 1 "$WORK/cd" \
    "outside this workspace"

# ── 8. `--bench` selects a benchmark target, not a test binary ───────────────
make_fixture "$WORK/bench" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --bench consumer -p demo -- --ignored
YML
)"
expect "\`--bench\` is not a test binary" 1 "$WORK/bench" "--bench"

# ── 9. A plain run does not execute an all-#[ignore]d suite ──────────────────
make_fixture "$WORK/noignored" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test -p demo --test consumer
YML
)"
expect "a run without \`-- --ignored\` does not cover an #[ignore]d suite" 1 "$WORK/noignored"

# ── 10. An include-only strategy.matrix expands to its entries ───────────────
#
# Seeding the product with `[{}]` instead of `[]` leaves a phantom
# variable-less combination, against which every `${{ matrix.* }}` is
# unresolvable — the gate would go FATAL on a workflow it can read.
make_fixture "$WORK/matrix" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        include:
          - name: a
            test: consumer
          - name: b
            test: consumer
    steps:
      - run: cargo test --test "${{ matrix.test }}" -p demo -- --ignored
YML
)"
expect "an include-only matrix resolves \${{ matrix.* }}" 0 "$WORK/matrix" "all covered"

# ── 11. An axis matrix expands to the product ───────────────────────────────
make_fixture "$WORK/axes" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        suite: [consumer]
        rust: [stable, beta]
    steps:
      - run: cargo test --test "${{ matrix.suite }}" -p demo -- --ignored
YML
)"
expect "an axis matrix resolves \${{ matrix.* }}" 0 "$WORK/axes" "all covered"

# ── 12. An expression the gate cannot resolve is FATAL, never dropped ───────
#
# The whole point: a parser that silently drops what it cannot read reports
# coverage that does not exist, which is worse than the exemption it replaced.
make_fixture "$WORK/unresolvable" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --test "${{ env.SUITE_NAME }}" -p demo -- --ignored
YML
)"
expect "an unresolvable expression is FATAL" 2 "$WORK/unresolvable" "cannot resolve"

# ── 13. …and it is FATAL even where the result would have been discarded ────
#
# `feature-flags.yml` is dispatch-only, so nothing it says can become coverage.
# Resolving before filtering is what keeps the parser from rotting there: a new
# shape must be taught to the gate even in a workflow whose verdict is thrown
# away.
make_fixture "$WORK/unresolvable-dispatch" "$(cat <<'YML'
name: Probe
on:
  workflow_dispatch:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --test "${{ env.SUITE_NAME }}" -p demo -- --ignored
YML
)"
expect "unresolvable is FATAL even in a discarded workflow" 2 "$WORK/unresolvable-dispatch" \
    "cannot resolve"

# ── 14. A `--test` naming a binary that does not exist is a GHOST ───────────
#
# Direction C reaches workflow invocations too, so a renamed suite cannot leave
# a dangling flag behind in a workflow any more than in a leg.
make_fixture "$WORK/ghost" "$(cat <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test -p demo --test consumer --test departed -- --ignored
YML
)"
expect "a workflow \`--test\` naming no binary is a GHOST" 1 "$WORK/ghost" "GHOST"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "suite-coverage workflow self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-coverage workflow self-test: $TESTS_RUN/$TESTS_RUN passed"
