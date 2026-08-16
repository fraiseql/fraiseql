#!/usr/bin/env bash
# Unit tests for tools/check-publish-parity.py.
#
# Run directly:  bash tools/tests/publish_parity_test.sh
# Exits non-zero if any assertion fails.
#
# The gate reads four files and compares them, so its fixtures are just a tree with
# those four files in it — no toolchain, no git history. It therefore runs in Dagger
# ShellGates alongside the gate itself.
#
# What is worth pinning here, in order of how badly each would hurt:
#
#   1. **A crate published nowhere.** This is the defect the gate was written for:
#      `fraiseql-cdc-sinks` reached `legacyPublishOrder` and the workspace but never
#      reached release.yml, and because it is an *optional* dependency of
#      fraiseql-server the pre-tag dry-run tolerated it while the real publish could
#      not. A gate that misses this direction is the gate we already had.
#   2. **Order drift.** Membership parity alone is not enough: PublishOrderSelftest
#      proves `legacyPublishOrder` is topologically valid, and only order *equality*
#      carries that proof onto the steps that actually publish.
#   3. **A missing index wait.** The sparse index lags; a dependent that resolves
#      before its dependency is indexed fails the publish mid-run, which is the
#      expensive, half-published failure mode.
#   4. **An unchecked outcome.** The roll-up decides whether the job succeeded. A
#      publish step missing from it reports a failed publish as a success — how
#      `publish-guard` sat unchecked until this gate was written.
#   5. **Green must be reachable.** A gate that fails on everything teaches people
#      to skip it, so the untouched fixture must pass.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-publish-parity.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── Fixture tree ─────────────────────────────────────────────────────────────
# A miniature of the real layout: three publishable crates (a → b → c, published
# in that order) plus one publish = false crate that must be ignored throughout.
mk_tree() {
    local dir="$WORK/$1"
    mkdir -p "$dir/.github/workflows" "$dir/.dagger"
    mkdir -p "$dir/crates/crate-a" "$dir/crates/crate-b" "$dir/crates/crate-c" "$dir/crates/crate-priv"

    cat > "$dir/Cargo.toml" <<'EOF'
[workspace]
members = [
  "crates/crate-a",
  "crates/crate-b",
  "crates/crate-c",
  "crates/crate-priv"
]
EOF

    for c in a b c; do
        cat > "$dir/crates/crate-$c/Cargo.toml" <<EOF
[package]
name = "crate-$c"
version = "1.0.0"
EOF
    done
    cat > "$dir/crates/crate-priv/Cargo.toml" <<'EOF'
[package]
name = "crate-priv"
version = "1.0.0"
publish = false
EOF

    cat > "$dir/.dagger/release.go" <<'EOF'
var legacyPublishOrder = []string{
	// Tier 1.
	"crate-a", "crate-b",
	// Tier 2.
	"crate-c",
}
EOF

    cat > "$dir/.github/workflows/release.yml" <<'EOF'
jobs:
  validate-release:
    steps:
      - name: Dry-run publish for every publishable crate
        run: |
          CRATES="crate-a crate-b \
                  crate-c"
  publish-crates:
    steps:
      - name: Publish crate-a
        id: publish-a
        run: cargo publish --package crate-a --token X
      - name: Wait for crates.io indexing (Tier 1)
        run: bash tools/wait-for-crates-index.sh "$V" crate-a
      - name: Publish crate-b
        id: publish-b
        run: cargo publish --package crate-b --token X
      - name: Wait for crates.io indexing (Tier 2)
        run: bash tools/wait-for-crates-index.sh "$V" crate-b
      - name: Publish crate-c
        id: publish-c
        run: cargo publish --package crate-c --token X
      - name: Verify all crates published
        run: |
          for step_outcome in \
            "${{ steps.publish-a.outcome }}" \
            "${{ steps.publish-b.outcome }}" \
            "${{ steps.publish-c.outcome }}"; do
            echo "$step_outcome"
          done
      - name: Report crates.io publishing results
        run: echo done
EOF
    echo "$dir"
}

# assert_gate <expect-pass|expect-fail> <name> <tree-dir> [expected-substring]
assert_gate() {
    local mode="$1" name="$2" dir="$3" needle="${4:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out status
    out="$(python3 "$GATE" "$dir" 2>&1)" && status=0 || status=$?

    if [ "$mode" = "expect-pass" ] && [ "$status" -ne 0 ]; then
        echo "FAIL: $name — expected exit 0, got $status"
        echo "$out" | sed 's/^/       /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if [ "$mode" = "expect-fail" ]; then
        if [ "$status" -eq 0 ]; then
            echo "FAIL: $name — expected a non-zero exit, gate passed"
            TESTS_FAILED=$((TESTS_FAILED + 1))
            return
        fi
        # The gate must fail for the stated reason, not incidentally.
        if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF "$needle"; then
            echo "FAIL: $name — failed, but not for the expected reason"
            echo "       wanted substring: $needle"
            echo "$out" | sed 's/^/       /'
            TESTS_FAILED=$((TESTS_FAILED + 1))
            return
        fi
    fi
    echo "ok: $name"
}

# ── 5. The untouched fixture passes ──────────────────────────────────────────
base="$(mk_tree base)"
assert_gate expect-pass "an aligned tree passes" "$base"

# ── 1. A crate in the workspace + legacyPublishOrder but not in release.yml ──
# This is #382 exactly: the crate exists and is ordered, but nothing publishes it.
t="$(mk_tree missing_publish)"
python3 - "$t" <<'PY'
import re, sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
text = p.read_text()
text = text.replace("""      - name: Publish crate-b
        id: publish-b
        run: cargo publish --package crate-b --token X
""", "")
text = text.replace('            "${{ steps.publish-b.outcome }}" \\\n', "")
text = text.replace("crate-a crate-b \\", "crate-a \\")
p.write_text(text)
PY
assert_gate expect-fail "a crate published nowhere is caught" "$t" \
    "release.yml publish steps is missing: crate-b"

# ── 1b. …and the mirror: a crate dropped from legacyPublishOrder ─────────────
t="$(mk_tree missing_legacy)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".dagger/release.go"
p.write_text(p.read_text().replace('"crate-a", "crate-b",', '"crate-a",'))
PY
assert_gate expect-fail "a crate missing from legacyPublishOrder is caught" "$t" \
    "legacyPublishOrder is missing: crate-b"

# ── 1c. A CRATES= list that forgot one ───────────────────────────────────────
t="$(mk_tree missing_crates_list)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
p.write_text(p.read_text().replace('CRATES="crate-a crate-b \\', 'CRATES="crate-a \\'))
PY
assert_gate expect-fail "a CRATES= list that forgot a crate is caught" "$t" \
    "CRATES list at line"

# ── 2. Order drift between release.yml and legacyPublishOrder ────────────────
t="$(mk_tree order_drift)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".dagger/release.go"
p.write_text(p.read_text().replace('"crate-a", "crate-b",', '"crate-b", "crate-a",'))
PY
assert_gate expect-fail "order drift is caught" "$t" \
    "different order than legacyPublishOrder"

# ── 3. A published crate with no index wait after it ─────────────────────────
t="$(mk_tree missing_wait)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
text = p.read_text().replace("""      - name: Wait for crates.io indexing (Tier 1)
        run: bash tools/wait-for-crates-index.sh "$V" crate-a
""", "")
p.write_text(text)
PY
assert_gate expect-fail "a publish with no index wait is caught" "$t" \
    "crate-a is published but never waited on"

# ── 3b. A wait that exists but runs BEFORE the publish it claims to cover ────
# Ordering matters: a wait above its publish step observes the previous release's
# version and returns immediately.
t="$(mk_tree wait_before_publish)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
text = p.read_text()
text = text.replace("""      - name: Publish crate-a
        id: publish-a
        run: cargo publish --package crate-a --token X
      - name: Wait for crates.io indexing (Tier 1)
        run: bash tools/wait-for-crates-index.sh "$V" crate-a
""", """      - name: Wait for crates.io indexing (Tier 1)
        run: bash tools/wait-for-crates-index.sh "$V" crate-a
      - name: Publish crate-a
        id: publish-a
        run: cargo publish --package crate-a --token X
""")
p.write_text(text)
PY
assert_gate expect-fail "an index wait placed before its publish is caught" "$t" \
    "crate-a is published but never waited on"

# ── 4. A publish step the success roll-up never checks ───────────────────────
t="$(mk_tree unchecked_outcome)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
p.write_text(p.read_text().replace('            "${{ steps.publish-b.outcome }}" \\\n', ""))
PY
assert_gate expect-fail "an unchecked publish outcome is caught" "$t" \
    "publish-b"

# ── 4b. The summary listing it does NOT satisfy the roll-up ──────────────────
# The human-readable summary names the same ids; unioning the two would let the
# deciding list stay incomplete while the gate reported green.
t="$(mk_tree outcome_only_in_summary)"
python3 - "$t" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / ".github/workflows/release.yml"
text = p.read_text()
text = text.replace('            "${{ steps.publish-b.outcome }}" \\\n', "")
text = text.replace("      - name: Report crates.io publishing results\n        run: echo done",
                    '      - name: Report crates.io publishing results\n'
                    '        run: echo "${{ steps.publish-b.outcome }}"')
p.write_text(text)
PY
assert_gate expect-fail "an outcome named only in the summary is still caught" "$t" \
    "publish-b"

# ── A publish = false crate is not demanded anywhere ─────────────────────────
# The fixture's crate-priv is absent from every list and the base case passes, so
# this is already covered; assert it explicitly so a parser change cannot start
# demanding that private crates be published.
t="$(mk_tree private_crate_ignored)"
assert_gate expect-pass "a publish = false crate is not demanded" "$t"

echo ""
echo "publish-parity gate self-test: $((TESTS_RUN - TESTS_FAILED))/$TESTS_RUN passed"
[ "$TESTS_FAILED" -eq 0 ] || exit 1
