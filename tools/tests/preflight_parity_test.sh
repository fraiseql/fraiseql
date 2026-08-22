#!/usr/bin/env bash
# Unit tests for tools/check-preflight-parity.py.
#
# Run directly:  bash tools/tests/preflight_parity_test.sh
# Exits non-zero if any assertion fails.
#
# The gate exists because two hand-maintained copies of one gate list — the
# Makefile's `preflight:` target and `.dagger/main.go`'s ShellGates script —
# drifted twice while `make preflight` kept printing "Safe to push" (#1135).
# A parity gate that cannot itself go red would just be a third copy of the
# same false assurance, so its red capability is pinned here: each fixture
# below is a way the two lists can diverge, and each must be reported.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-preflight-parity.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <makefile-preflight-prereqs> <shellgates-entries>
# Builds a minimal tree with the same shapes the real files use.
make_fixture() {
    local dir="$1" prereqs="$2" entries="$3"
    mkdir -p "$dir/.dagger"

    cat >"$dir/Makefile" <<EOF
.PHONY: lint-routes
lint-routes:
	@bash tools/check-route-syntax.sh

.PHONY: lint-feature-chains
lint-feature-chains:
	@bash tools/check-feature-chains.sh

.PHONY: test-deadline-gate
test-deadline-gate:
	@bash tools/tests/check_deadlines_test.sh

.PHONY: lint-unwrap
lint-unwrap:
	@echo counting

.PHONY: preflight
preflight: ${prereqs}
	@\$(MAKE) --no-print-directory lint-unwrap UNWRAP_ALLOW_LIMIT=3
	@bash tools/check-audit-lockstep.sh
EOF

    cat >"$dir/.dagger/main.go" <<EOF
package main

const (
	unwrapAllowLimit = "3"
)

func (m *FraiseqlCi) ShellGates(
	ctx context.Context,
	source *dagger.Directory,
) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"git init -q . >/dev/null",
${entries}
	}, "\n")
	return nil, nil
}
EOF
}

# assert_gate <name> <expected-exit> <expected-substring> <dir>
assert_gate() {
    local name="$1" want_exit="$2" want_text="$3" dir="$4"
    TESTS_RUN=$((TESTS_RUN + 1))

    local out rc
    set +e
    out="$(python3 "$GATE" --root "$dir" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if ! printf '%s' "$out" | grep -qF "$want_text"; then
        echo "  FAIL: $name — output did not contain: $want_text"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "  ok: $name"
}

echo "=== tools/check-preflight-parity.py ==="

# A ShellGates command with no local counterpart at all — the #1135 shape.
make_fixture "$WORK/missing" "lint-routes" \
'		"make lint-unwrap UNWRAP_ALLOW_LIMIT=" + unwrapAllowLimit,
		"bash tools/check-route-syntax.sh",
		"bash tools/check-audit-lockstep.sh",
		"make test-deadline-gate",'
assert_gate "a ShellGates make target absent from preflight is reported" \
    1 "make test-deadline-gate" "$WORK/missing"

# A bare script invocation with no wrapping target — the check-feature-chains shape.
make_fixture "$WORK/missing-script" "lint-routes" \
'		"make lint-unwrap UNWRAP_ALLOW_LIMIT=" + unwrapAllowLimit,
		"bash tools/check-route-syntax.sh",
		"bash tools/check-audit-lockstep.sh",
		"bash tools/check-feature-chains.sh",'
assert_gate "a ShellGates script absent from preflight is reported" \
    1 "bash tools/check-feature-chains.sh" "$WORK/missing-script"

# Both lists agree — via a make target locally and the bare script in CI, which
# must compare equal or every wrapped gate would report as missing.
make_fixture "$WORK/ok" "lint-routes lint-feature-chains test-deadline-gate" \
'		"make lint-unwrap UNWRAP_ALLOW_LIMIT=" + unwrapAllowLimit,
		"bash tools/check-route-syntax.sh",
		"bash tools/check-audit-lockstep.sh",
		"bash tools/check-feature-chains.sh",
		"make test-deadline-gate",'
assert_gate "matching lists pass, across both spellings" \
    0 "preflight-parity: OK" "$WORK/ok"

# The same gate with a different budget on each side. This is #990's shape: a
# local harness pinned to a stale limit reads red (or green) against a rule CI
# does not enforce.
make_fixture "$WORK/drift" "lint-routes lint-feature-chains test-deadline-gate" \
'		"make lint-unwrap UNWRAP_ALLOW_LIMIT=99",
		"bash tools/check-route-syntax.sh",
		"bash tools/check-audit-lockstep.sh",
		"bash tools/check-feature-chains.sh",
		"make test-deadline-gate",'
assert_gate "the same gate with a drifted budget is reported" \
    1 "UNWRAP_ALLOW_LIMIT" "$WORK/drift"

# If the Go literal is reshaped so the parser extracts nothing, the gate must
# fail loudly rather than vacuously pass over an empty requirement set — the
# failure mode that makes a gate worth less than no gate.
mkdir -p "$WORK/blind/.dagger"
cp "$WORK/ok/Makefile" "$WORK/blind/Makefile"
cat >"$WORK/blind/.dagger/main.go" <<'EOF'
package main

func (m *FraiseqlCi) ShellGates(
	ctx context.Context,
	source *dagger.Directory,
) (string, error) {
	script := strings.Join([]string{
		"set -e",
	}, "\n")
	return nil, nil
}
EOF
assert_gate "an unparseable ShellGates literal fails loudly, not vacuously" \
    1 "went blind" "$WORK/blind"

echo ""
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
