#!/usr/bin/env bash
# Unit tests for tools/check-publish-secrets.py.
#
# Run directly:  bash tools/tests/publish_secrets_test.sh
# Exits non-zero if any assertion fails.
#
# The gate reads .github/workflows/*.yml only, so each fixture is a tree holding a
# release.yml (with its `Validate required secrets` step) and, where a case needs one, a
# second publishing workflow. No toolchain, no git: it runs in Dagger ShellGates.
#
# Pinned, in order of how badly each would hurt:
#
#   1. **A publish job reading a secret nobody checks** — #1518: `CARGO_REGISTRY_TOKEN`
#      did not exist, read as "", and the Rust SDK upload failed after every other
#      package was already public. Both in release.yml and in a separate workflow.
#   2. **The platform token passes**; a checked secret passes; green is reachable.
#   3. **Not every `secrets.` is a secret**: `steps.publish-secrets.outcome` is a step
#      output, and a comment naming a secret is prose.
#   4. **The gate cannot go vacuous**: no publish job found, or no validate step, fails.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-publish-secrets.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A release.yml whose prerequisite step checks CARGO_TOKEN, and whose publish job reads
# the secret named by $2 (default CARGO_TOKEN).
mk_tree() {
    local dir="$WORK/$1" secret="${2:-CARGO_TOKEN}"
    mkdir -p "$dir/.github/workflows"
    cat > "$dir/.github/workflows/release.yml" <<EOF
name: Release
on:
  push:
    tags: ['v*']
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - name: Validate required secrets
        run: |
          if [ -z "\${{ secrets.CARGO_TOKEN }}" ]; then exit 1; fi
      - name: Next step
        run: echo "\${{ secrets.NOT_CHECKED_BY_THE_STEP_ABOVE }}"
  publish-crates:
    runs-on: ubuntu-latest
    steps:
      - name: Publish
        id: publish-secrets
        run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: \${{ secrets.$secret }}
      - name: Roll up
        run: echo "\${{ steps.publish-secrets.outcome }}"
EOF
}

expect() {
    local name="$1" want="$2" dir="$3" pattern="${4:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out rc=0
    out="$(python3 "$GATE" "$dir" 2>&1)" || rc=$?
    if [ "$want" = pass ] && [ "$rc" -ne 0 ]; then
        echo "FAIL [$name]: expected pass, got rc=$rc: $out"; TESTS_FAILED=$((TESTS_FAILED + 1)); return
    fi
    if [ "$want" = fail ] && [ "$rc" -eq 0 ]; then
        echo "FAIL [$name]: expected failure, got pass: $out"; TESTS_FAILED=$((TESTS_FAILED + 1)); return
    fi
    if [ -n "$pattern" ] && ! grep -qF -- "$pattern" <<<"$out"; then
        echo "FAIL [$name]: output lacks '$pattern': $out"; TESTS_FAILED=$((TESTS_FAILED + 1)); return
    fi
    echo "  ok   $name"
}

mk_tree green
expect "a publish job reading the checked secret passes" pass "$WORK/green"

mk_tree unchecked CARGO_REGISTRY_TOKEN
expect "#1518: a publish job reading an unchecked secret fails" fail "$WORK/unchecked" \
    "job \`publish-crates\` publishes with \`secrets.CARGO_REGISTRY_TOKEN\`"

mk_tree platform GITHUB_TOKEN
expect "the platform token needs no check" pass "$WORK/platform"

mk_tree other
cat > "$WORK/other/.github/workflows/sdk.yml" <<'EOF'
name: SDK
on:
  push:
    tags: ['sdk/v*']
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      # The old spelling was secrets.CARGO_REGISTRY_TOKEN; this comment is prose.
      - run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.SDK_ONLY_TOKEN }}
EOF
expect "#1518: a separate publishing workflow is checked too" fail "$WORK/other" \
    "sdk.yml: job \`publish\` publishes with \`secrets.SDK_ONLY_TOKEN\`"

mk_tree prose
cat > "$WORK/prose/.github/workflows/sdk.yml" <<'EOF'
name: SDK
on: workflow_dispatch
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      # Never secrets.CARGO_REGISTRY_TOKEN: the repository has no such secret.
      - run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_TOKEN }}
EOF
expect "a comment naming a secret is not a reference" pass "$WORK/prose"

mk_tree nonpublish
cat > "$WORK/nonpublish/.github/workflows/ci.yml" <<'EOF'
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: cargo build
        env:
          SOME_TOKEN: ${{ secrets.SOME_TOKEN }}
EOF
expect "a job that publishes nothing is out of scope" pass "$WORK/nonpublish"

mk_tree vacuous
sed -i 's/cargo publish/cargo build/' "$WORK/vacuous/.github/workflows/release.yml"
expect "no publish job found fails (the patterns stopped matching)" fail "$WORK/vacuous" \
    "no publish job found"

mk_tree nostep
sed -i 's/Validate required secrets/Validate things/' "$WORK/nostep/.github/workflows/release.yml"
expect "no validate step fails" fail "$WORK/nostep" "has no \`Validate required secrets\` step"

echo ""
echo "publish secrets gate: $TESTS_RUN cases, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
