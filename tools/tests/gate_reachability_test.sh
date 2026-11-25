#!/usr/bin/env bash
# Unit tests for tools/check-gate-reachability.py.
#
# Run directly:  bash tools/tests/gate_reachability_test.sh
# Exits non-zero if any assertion fails.
#
# The gate counts a gate script as run by CI only when a CI leg INVOKES it. Each red
# case below is a way a script can look covered and run nowhere: a mention instead of
# an invocation, a commented line, a self-test that runs it over fixtures, a Dagger
# function no workflow calls, a target only `make preflight` reaches. Each must be
# named. The green cases are the invocation shapes the real tree uses, so the gate is
# not passing by being strict about everything.
# shellcheck disable=SC2016 # Reason: the fixtures hold literal `$(...)` and backticks.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-gate-reachability.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <dir>: a workflow calling `dagger call preflight`, Preflight reaching
# ShellGates, ShellGates running a make target and a script, and an Unused function.
fixture() {
    local dir="$1"
    mkdir -p "$dir/.github/workflows" "$dir/.dagger" "$dir/tools/tests"
    cat >"$dir/.github/workflows/preflight.yml" <<'YML'
name: preflight
on: push
jobs:
  preflight:
    runs-on: ubuntu-latest
    steps:
      - run: dagger call preflight --source=.
YML
    cat >"$dir/.dagger/main.go" <<'GO'
package main

func (m *FraiseqlCi) Preflight(ctx context.Context) (string, error) {
	return m.ShellGates(ctx)
}

func (m *FraiseqlCi) ShellGates(ctx context.Context) (string, error) {
	script := strings.Join([]string{
		"make lint-a",
		"bash tools/check-b.sh",
		"make test-a-gate",
		"bash tools/tests/b_test.sh",
	}, "\n")
	return run(script)
}

func (m *FraiseqlCi) Unused(ctx context.Context) (string, error) {
	return run("true")
}
GO
    cat >"$dir/Makefile" <<'MK'
.PHONY: lint-a
lint-a:
	@python3 tools/check-a.py

test-a-gate:
	@bash tools/tests/a_test.sh
MK
    printf '#!/usr/bin/env python3\n' >"$dir/tools/check-a.py"
    printf '#!/usr/bin/env bash\ntrue\n' >"$dir/tools/check-b.sh"
    printf '#!/usr/bin/env bash\nbash tools/check-a.py\n' >"$dir/tools/tests/a_test.sh"
    printf '#!/usr/bin/env bash\nbash tools/check-b.sh\n' >"$dir/tools/tests/b_test.sh"
}

# case_dir <name>: a fresh fixture for one case.
case_dir() {
    local dir="$WORK/$1"
    fixture "$dir"
    printf '%s' "$dir"
}

run_gate() { GATE_REACHABILITY_ROOT="$1" python3 "$GATE" >"$1.out" 2>&1; }

# expect_red <name> <dir> <text the output must contain>
expect_red() {
    local name="$1" dir="$2" want="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    if run_gate "$dir"; then
        echo "FAIL: $name: the gate passed"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    elif ! grep -qF -- "$want" "$dir.out"; then
        echo "FAIL: $name: red, but the output does not say: $want"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "ok:   $name"
    fi
}

# expect_green <name> <dir>
expect_green() {
    local name="$1" dir="$2"
    TESTS_RUN=$((TESTS_RUN + 1))
    if run_gate "$dir" && grep -qF 'gate-reachability: OK' "$dir.out"; then
        echo "ok:   $name"
    else
        echo "FAIL: $name: the gate did not pass"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

ORPHAN='tools/check-c.sh: no CI leg runs it'
new_check() { printf '#!/usr/bin/env bash\ntrue\n' >"$1/tools/check-c.sh"; }

# --- green -------------------------------------------------------------------

d="$(case_dir baseline)"
expect_green "every script reached through ShellGates and its make targets" "$d"

d="$(case_dir go-string-list)"
new_check "$d"
sed -i 's|return run("true")|return run("true")\n}\n\nfunc (m *FraiseqlCi) Fmt() {\n\tWithExec([]string{"bash", "tools/check-c.sh"})|' "$d/.dagger/main.go"
sed -i 's|return m.ShellGates(ctx)|m.Fmt()\n\treturn m.ShellGates(ctx)|' "$d/.dagger/main.go"
expect_green "a script in a Go string list, in a function the root references" "$d"

d="$(case_dir raw-string)"
new_check "$d"
cat >>"$d/.dagger/main.go" <<'GO'

func (m *FraiseqlCi) Combo() {
	script := `set +e
out=$(bash tools/check-c.sh 2>&1)
exit 0`
	run(script)
}
GO
sed -i 's|return m.ShellGates(ctx)|m.Combo()\n\treturn m.ShellGates(ctx)|' "$d/.dagger/main.go"
expect_green "a script in a raw string spanning lines" "$d"

d="$(case_dir sibling-wrapper)"
printf '#!/usr/bin/env python3\n' >"$d/tools/check-c.py"
printf '#!/usr/bin/env bash\nexec python3 "$(dirname "${BASH_SOURCE[0]}")/check-c.py" "$@"\n' >"$d/tools/check-b.sh"
expect_green "a script its reached wrapper runs from its own directory" "$d"

d="$(case_dir workflow-direct)"
new_check "$d"
printf '      - run: bash tools/check-c.sh\n' >>"$d/.github/workflows/preflight.yml"
expect_green "a script a workflow step runs" "$d"

d="$(case_dir exempted)"
new_check "$d"
printf '[[exempt]]\npath = "tools/check-c.sh"\nreason = "a workstation tool"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_green "a script nothing runs, exempted with a reason" "$d"

# --- red: looks covered, runs nowhere ----------------------------------------

d="$(case_dir orphan)"
new_check "$d"
expect_red "a script nothing invokes" "$d" "$ORPHAN"

d="$(case_dir echo-mention)"
new_check "$d"
printf '\t@echo "see tools/check-c.sh"\n' >>"$d/Makefile"
expect_red "a script only named in an echo" "$d" "$ORPHAN"

d="$(case_dir case-arm)"
new_check "$d"
printf 'case "$f" in\n    tools/check-c.sh) return 0 ;;\nesac\n' >>"$d/tools/check-b.sh"
expect_red "a script only named in a case arm" "$d" "$ORPHAN"

d="$(case_dir self-test-only)"
new_check "$d"
printf 'bash tools/check-c.sh\n' >>"$d/tools/tests/a_test.sh"
expect_red "a script run only by a self-test, over its fixtures" "$d" "$ORPHAN"

d="$(case_dir unused-function)"
new_check "$d"
sed -i 's|return run("true")|return run("bash tools/check-c.sh")|' "$d/.dagger/main.go"
expect_red "a script run only by a Dagger function no workflow calls" "$d" "$ORPHAN"

d="$(case_dir go-comment)"
new_check "$d"
sed -i 's|		"make lint-a",|		// "bash tools/check-c.sh",\n		"make lint-a",|' "$d/.dagger/main.go"
expect_red "a script on a commented Go line" "$d" "$ORPHAN"

d="$(case_dir yaml-comment)"
new_check "$d"
printf '      # - run: bash tools/check-c.sh\n' >>"$d/.github/workflows/preflight.yml"
expect_red "a script on a commented workflow line" "$d" "$ORPHAN"

d="$(case_dir preflight-only)"
printf '#!/usr/bin/env bash\ntrue\n' >"$d/tools/tests/c_test.sh"
printf '\ntest-c-gate:\n\t@bash tools/tests/c_test.sh\n\npreflight: lint-a test-c-gate\n' >>"$d/Makefile"
expect_red "a self-test only \`make preflight\` runs" "$d" 'tools/tests/c_test.sh: no CI leg runs it'

# --- red: exemptions ---------------------------------------------------------

d="$(case_dir stale-exemption)"
printf '[[exempt]]\npath = "tools/check-b.sh"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a script CI runs" "$d" \
    'tools/check-b.sh: exempted, but a CI leg runs it'

d="$(case_dir missing-exemption)"
printf '[[exempt]]\npath = "tools/check-gone.sh"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a script that does not exist" "$d" \
    'tools/check-gone.sh: exempted, but no such gate script exists'

d="$(case_dir no-reason)"
new_check "$d"
printf '[[exempt]]\npath = "tools/check-c.sh"\nreason = " "\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption with no reason" "$d" 'exemption for tools/check-c.sh gives no reason'

# --- red: the parser went blind ----------------------------------------------

d="$(case_dir no-dagger-call)"
sed -i 's|dagger call preflight|echo preflight|' "$d/.github/workflows/preflight.yml"
expect_red "no workflow runs \`dagger call\`" "$d" 'no `dagger call` in any workflow'

d="$(case_dir unknown-root)"
sed -i 's|dagger call preflight|dagger call elsewhere|' "$d/.github/workflows/preflight.yml"
expect_red "no workflow's \`dagger call\` names a Go function" "$d" \
    "no workflow's \`dagger call\` names a function"

echo ""
echo "$TESTS_RUN cases, $TESTS_FAILED failed"
if [ "$TESTS_FAILED" -ne 0 ]; then
    exit 1
fi
echo "OK: check-gate-reachability.py names every script that runs in no CI leg."
